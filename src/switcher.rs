//! Account switching transaction.
//!
//! Switching touches both secret storage and local metadata. Keeping the flow in
//! one module makes it easier to see the ordering: refresh the current backup,
//! activate the target credential, update state, then reset quota side effects.

use crate::claude;
use crate::context::AppContext;
use crate::credentials;
use crate::domain::{AccountName, State};
use crate::notification;
use crate::storage;
use crate::time::now_epoch;
use crate::util::confirm;
use anyhow::{Context, Result, bail};

#[derive(Clone, Copy, Debug)]
pub(crate) struct SwitchOptions {
    pub(crate) yes: bool,
    pub(crate) emit: bool,
}

pub(crate) fn switch_to(
    ctx: &AppContext,
    name: &AccountName,
    options: SwitchOptions,
) -> Result<()> {
    let mut state = storage::load_state(ctx)?;
    if !state.accounts.contains_key(name.as_str()) {
        bail!("account is not saved: {name}");
    }
    confirm(&format!("switch to {name}?"), options.yes)?;

    let active_credential = credentials::read_active(ctx).unwrap_or_default();
    refresh_current_backup(ctx, &state, name, &active_credential)?;

    let target_credential = credentials::read_saved(ctx, name)
        .with_context(|| format!("failed to read saved credential for {name}"))?;
    if !claude::has_oauth_token(&target_credential) {
        bail!(
            "the saved credential for {name} carries no token, so activating it would log you out; \
             sign in as that account and run `setup {name}` again"
        );
    }
    let old_current = state.current_account.clone();

    if active_credential == target_credential {
        state.current_account = Some(name.to_string());
        storage::save_state(ctx, &state)?;
        if options.emit {
            println!("already using account: {name}");
        }
        return Ok(());
    }

    credentials::write_active(ctx, &target_credential)
        .with_context(|| format!("failed to activate account {name}"))?;

    if old_current.as_deref() != Some(name.as_str()) {
        state.previous_account = old_current;
    }
    state.current_account = Some(name.to_string());
    state.switched_at = Some(now_epoch());
    storage::save_state(ctx, &state)?;

    clear_quota_side_effects(ctx, name)?;

    if options.emit {
        println!("switched to account: {name}");
        println!("restart Claude Code if the running session does not pick up the new credential");
    }
    Ok(())
}

pub(crate) fn detect_current_account_by_credential(
    ctx: &AppContext,
    state: &mut State,
) -> Result<Option<String>> {
    let active = credentials::read_active(ctx)?;
    let found = saved_account_for_credential(ctx, state, &active)?;
    if let Some(name) = found.as_ref() {
        state.current_account = Some(name.clone());
    }
    Ok(found)
}

/// The saved account holding exactly this credential, if one does.
///
/// Credentials are compared rather than trusted from a name, because the only
/// thing that reliably identifies a login here is the token itself.
pub(crate) fn saved_account_for_credential(
    ctx: &AppContext,
    state: &State,
    credential: &str,
) -> Result<Option<String>> {
    for name in state.accounts.keys() {
        let account_name = AccountName::parse(name)?;
        if let Ok(saved) = credentials::read_saved(ctx, &account_name)
            && saved == credential
        {
            return Ok(Some(name.clone()));
        }
    }
    Ok(None)
}

/// Refresh the saved credential for the currently active account before switching.
///
/// Claude Code may refresh OAuth tokens while the account is active. Capturing the
/// current credential immediately before switching prevents restoring an older
/// token the next time the user switches back to this account.
fn refresh_current_backup(
    ctx: &AppContext,
    state: &State,
    target: &AccountName,
    active_credential: &str,
) -> Result<()> {
    if let Some(current_account) = state.current_account.as_deref()
        && current_account != target.as_str()
        && state.accounts.contains_key(current_account)
        // After `claude auth logout` the active record is still there with its
        // tokens blanked. Copying that over the saved account would destroy the
        // only working copy of that login.
        && claude::has_oauth_token(active_credential)
    {
        let current_name = AccountName::parse(current_account)?;
        credentials::write_saved(ctx, &current_name, active_credential).with_context(|| {
            format!("failed to update current account backup for {current_account}")
        })?;
    }
    Ok(())
}

/// Drop the quota state that the switch invalidates.
///
/// The target's cached reading describes the moment the router left it, so it is
/// discarded and re-observed. Readings for every other account are kept: they are
/// what the statusline counts down and what selection skips over. Notification
/// markers are cleared so the next quota event is announced again.
fn clear_quota_side_effects(ctx: &AppContext, target: &AccountName) -> Result<()> {
    let mut cache = storage::load_rate_limits(ctx)?;
    if cache.accounts.remove(target.as_str()).is_some() {
        storage::save_rate_limits(ctx, &cache)?;
    }
    notification::clear_notified(ctx)
}
