//! SecretFolder on RemedyUI: an encrypted folder vault with the feel of a file explorer.
//!
//! The vault (Argon2id + XChaCha20-Poly1305, encrypted names), the unlock throttle and the
//! import and export rules are the same code the Tauri app uses; this crate is only the
//! interface: setup, unlock and the one-time recovery key, the explorer (folders, search,
//! multi-select, drag and drop inside the vault and from and to other apps, text editing,
//! image preview, import and export), settings, About with signed updates, the tray, idle
//! lock and clipboard clearing.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[path = "../../src-tauri/src/crypto.rs"]
#[allow(dead_code)]
#[rustfmt::skip]
mod crypto;
#[path = "../../src-tauri/src/error.rs"]
#[allow(dead_code)]
#[rustfmt::skip]
mod error;
#[path = "../../src-tauri/src/policy.rs"]
#[allow(dead_code)]
#[rustfmt::skip]
mod policy;
// 0.2.0's `ui.json` beside the vault (the language), read and written by its own code.
#[path = "../../src-tauri/src/prefs.rs"]
#[allow(dead_code)]
#[rustfmt::skip]
mod prefs;
#[path = "../../src-tauri/src/vault.rs"]
#[allow(dead_code)]
#[rustfmt::skip]
mod vault;

mod about;
mod explorer;
mod files;
mod fonts;
mod gate;
mod lang;
mod settings;
mod state;
mod tray;

#[cfg(test)]
mod flow_tests;

fn main() {
    // Another vault is another app: its own single-instance lock and window geometry.
    let name = if state::vault_override().is_some() {
        "SecretFolder (other vault)"
    } else {
        "SecretFolder"
    };
    let config = fonts::with_inter(
        remedy_ui::AppConfig::new(name).single_instance().theme(state::theme(), remedy_ui_themes::sheet),
    );
    if let Err(e) = remedy_ui::run(config, |app| {
        state::start(app);
    }) {
        eprintln!("SecretFolder: {e:?}");
        std::process::exit(1);
    }
}
