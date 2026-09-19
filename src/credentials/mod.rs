//! Credential storage.
//!
//! Claude Code keeps its active credential in the macOS Keychain, and on every
//! other platform in an owner-only JSON file next to its settings. This module
//! picks the matching backend so no caller has to branch on the platform, and a
//! future platform is added here rather than in command logic.

#[cfg(target_os = "macos")]
mod keychain;
#[cfg(target_os = "macos")]
pub(crate) use keychain::{
    describe, delete_saved, read_active, read_saved, write_active, write_saved,
};

// The file backend is compiled on every platform so its behavior stays under
// test on a macOS development machine, and it is selected as the backend only
// where there is no Keychain.
#[cfg_attr(
    target_os = "macos",
    allow(dead_code, reason = "selected only on platforms without a Keychain")
)]
mod file;
#[cfg(not(target_os = "macos"))]
pub(crate) use file::{describe, delete_saved, read_active, read_saved, write_active, write_saved};
