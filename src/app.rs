//! Application use cases.
//!
//! The `App` type is the orchestration layer. It coordinates modules with
//! side effects but leaves their implementation details in their own files.

use crate::claude;
use crate::cli::Commands;
use crate::context::AppContext;
use crate::domain::{
    AccountEntry, AccountKind, AccountName, Config, RateLimitSnapshot, RoutingMode, State,
};
use crate::keychain::Keychain;
use crate::routing;
use crate::settings;
use crate::statusline;
use crate::storage;
use crate::switcher::{self, SwitchOptions};
use crate::time::{now_epoch, now_epoch_i64};
use crate::util::{display_pct, humanize};
use anyhow::{Context, Result, anyhow, bail};

pub(crate) struct App {
    ctx: AppContext,
    keychain: Keychain,
}

impl App {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            ctx: AppContext::new()?,
            keychain: Keychain,
        })
    }

    pub(crate) fn handle(&self, command: Commands) -> Result<()> {
        match command {
            Commands::Setup { name, kind } => self.setup(name, kind),
            Commands::Switch { name, yes } => self.switch(&name, yes),
            Commands::Toggle { yes } => self.toggle(yes),
            Commands::List => self.list(),
            Commands::Remove { name } => self.remove(&name),
            Commands::Current => self.current(),
            Commands::Status => self.status(),
            Commands::Config {
                alert_at,
                mode,
                priority,
            } => self.config(alert_at, mode, priority),
            Commands::Install => settings::install_statusline(&self.ctx),
            Commands::Uninstall => settings::uninstall_statusline(&self.ctx),
            Commands::Statusline => statusline::handle(&self.ctx, &self.keychain),
        }
    }

    /// Save the active credential under an account name.
    ///
    /// The name defaults to the account's email address so that two logins are
    /// told apart by the thing that actually differs between them.
    fn setup(&self, name: Option<String>, kind: Option<AccountKind>) -> Result<()> {
        let status = claude::auth_status();
        let name = match name {
            Some(value) => AccountName::parse(&value)?,
            None => {
                let email = status.as_ref().and_then(|status| status.email.as_deref());
                let email = email.ok_or_else(|| {
                    anyhow!("claude auth status reported no email; pass an account name")
                })?;
                AccountName::parse(email)?
            }
        };
        self.ctx.ensure_app_dir()?;

        let mut state = storage::load_state(&self.ctx)?;
        // The active Keychain account is stable for a given Claude Code
        // installation. Cache it after first detection so later commands do not
        // need to parse Keychain metadata unless the state file is missing.
        let active_account = match &state.active_account {
            Some(account) => account.clone(),
            None => self.keychain.detect_active_account()?,
        };
        let credential = self
            .keychain
            .read_active(&active_account)
            .context("failed to read active Claude Code credential from Keychain")?;
        // Prefer an explicit CLI override for import and migration cases,
        // then what Claude Code reports, then the credential JSON shape.
        let kind = kind
            .or_else(|| status.as_ref().and_then(|status| status.kind))
            .or_else(|| claude::detect_account_kind_from_credential(&credential))
            .unwrap_or(AccountKind::Other);

        self.keychain
            .upsert_account(&name, &credential)
            .with_context(|| format!("failed to save account credential for {name}"))?;

        let now = now_epoch();
        let created_at = state
            .accounts
            .get(name.as_str())
            .map(|entry| entry.created_at)
            .unwrap_or(now);
        state.active_account = Some(active_account);
        state.current_account = Some(name.to_string());
        state.accounts.insert(
            name.clone().into_string(),
            AccountEntry {
                kind,
                created_at,
                updated_at: now,
            },
        );
        storage::save_state(&self.ctx, &state)?;

        println!("saved account: {name} ({})", kind.as_str());
        Ok(())
    }

    fn switch(&self, name: &str, yes: bool) -> Result<()> {
        let name = AccountName::parse(name)?;
        switcher::switch_to(
            &self.ctx,
            &self.keychain,
            &name,
            SwitchOptions { yes, emit: true },
        )
    }

    fn toggle(&self, yes: bool) -> Result<()> {
        let state = storage::load_state(&self.ctx)?;
        let previous = state
            .previous_account
            .as_deref()
            .ok_or_else(|| anyhow!("no previous account recorded"))?;
        if !state.accounts.contains_key(previous) {
            bail!("previous account is not saved: {previous}");
        }
        self.switch(previous, yes)
    }

    /// Print saved accounts in routing order with their last known quota.
    fn list(&self) -> Result<()> {
        let state = storage::load_state(&self.ctx)?;
        if state.accounts.is_empty() {
            println!("no accounts saved");
            return Ok(());
        }

        let config = storage::load_config(&self.ctx)?;
        let cache = storage::load_rate_limits(&self.ctx)?;
        let now = now_epoch_i64();

        for (index, name) in routing::ordered_accounts(&state, &config)
            .into_iter()
            .enumerate()
        {
            let marker = if state.current_account.as_deref() == Some(name.as_str()) {
                "*"
            } else {
                " "
            };
            let kind = state
                .accounts
                .get(&name)
                .map(|entry| entry.kind.as_str())
                .unwrap_or("other");
            let quota = cache
                .accounts
                .get(&name)
                .map(|snapshot| describe_quota(snapshot, now))
                .unwrap_or_else(|| "-".to_string());
            println!("{marker} {}. {name}\t{kind}\t{quota}", index + 1);
        }
        Ok(())
    }

    fn remove(&self, name: &str) -> Result<()> {
        let name = AccountName::parse(name)?;
        let mut state = storage::load_state(&self.ctx)?;
        if state.current_account.as_deref() == Some(name.as_str()) {
            bail!("cannot remove the current account; switch to another account first");
        }
        if state.accounts.remove(name.as_str()).is_none() {
            bail!("account is not saved: {name}");
        }
        self.keychain.delete_account(&name).ok();
        if state.previous_account.as_deref() == Some(name.as_str()) {
            state.previous_account = None;
        }
        storage::save_state(&self.ctx, &state)?;

        let mut cache = storage::load_rate_limits(&self.ctx)?;
        if cache.accounts.remove(name.as_str()).is_some() {
            storage::save_rate_limits(&self.ctx, &cache)?;
        }
        println!("removed account: {name}");
        Ok(())
    }

    fn current(&self) -> Result<()> {
        let mut state = storage::load_state(&self.ctx)?;
        if let Some(current) = state.current_account {
            println!("{current}");
            return Ok(());
        }

        if let Some(detected) =
            switcher::detect_current_account_by_credential(&self.keychain, &mut state)?
        {
            storage::save_state(&self.ctx, &state)?;
            println!("{detected}");
            return Ok(());
        }

        println!("unknown");
        Ok(())
    }

    fn status(&self) -> Result<()> {
        let state = storage::load_state(&self.ctx)?;
        let config = storage::load_config(&self.ctx)?;
        let cache = storage::load_rate_limits(&self.ctx)?;
        let now = now_epoch_i64();

        println!(
            "current: {}",
            state.current_account.as_deref().unwrap_or("unknown")
        );
        println!(
            "previous: {}",
            state.previous_account.as_deref().unwrap_or("none")
        );
        println!(
            "active keychain account: {}",
            state.active_account.as_deref().unwrap_or("unknown")
        );
        print_config(&config, &state);
        println!("accounts: {}", state.accounts.len());

        if cache.accounts.is_empty() {
            println!("quota: none");
            return Ok(());
        }
        println!("quota:");
        for (name, snapshot) in &cache.accounts {
            println!("  {name}\t{}", describe_quota(snapshot, now));
        }
        Ok(())
    }

    fn config(
        &self,
        alert_at: Option<u8>,
        mode: Option<RoutingMode>,
        priority: Option<Vec<String>>,
    ) -> Result<()> {
        self.ctx.ensure_app_dir()?;
        let state = storage::load_state(&self.ctx)?;
        let mut config = storage::load_config(&self.ctx)?;
        if let Some(alert_at) = alert_at {
            if alert_at > 100 {
                bail!("--alert-at must be between 0 and 100");
            }
            config.alert_at = alert_at;
        }
        if let Some(mode) = mode {
            config.mode = mode;
        }
        if let Some(priority) = priority {
            config.priority = clean_priority(priority, &state)?;
        }
        storage::save_config(&self.ctx, &config)?;
        print_config(&config, &state);
        Ok(())
    }
}

/// Validate a user supplied account order.
///
/// Names are accepted before the account they refer to is saved, because the
/// order is easier to write once than to revisit after every `setup`. An unsaved
/// name is reported and then ignored by routing until it exists.
fn clean_priority(priority: Vec<String>, state: &State) -> Result<Vec<String>> {
    let mut cleaned: Vec<String> = Vec::with_capacity(priority.len());
    for value in priority {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let name = AccountName::parse(value)?;
        if !state.accounts.contains_key(name.as_str()) {
            eprintln!("warning: account is not saved yet: {name}");
        }
        if !cleaned.iter().any(|seen| seen == name.as_str()) {
            cleaned.push(name.into_string());
        }
    }
    Ok(cleaned)
}

fn describe_quota(snapshot: &RateLimitSnapshot, now: i64) -> String {
    let usage = match snapshot.peak_usage() {
        Some((label, pct)) => format!("{}({label})", display_pct(Some(pct))),
        None => "-".to_string(),
    };
    match snapshot.blocking_reset(now) {
        Some((label, reset)) => format!("{usage} reset in {}({label})", humanize(reset - now)),
        None => usage,
    }
}

fn print_config(config: &Config, state: &State) {
    println!("alert_at={}", config.alert_at);
    println!("mode={}", config.mode.as_str());
    if config.priority.is_empty() {
        println!("priority=(plan kind order: personal, team, enterprise, other)");
    } else {
        println!("priority={}", config.priority.join(","));
    }
    let order = routing::ordered_accounts(state, config);
    if !order.is_empty() {
        println!("order={}", order.join(" -> "));
    }
}
