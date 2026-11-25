//! The app's state and the actions every part shares: unlock, lock, the current folder,
//! and the vault calls behind them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use remedy_ui::core::Color;
use remedy_ui::reactive::Signal;
use remedy_ui::style::Theme;
use remedy_ui::{App, Window};
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};
use crate::lang::Lang;
use crate::policy::UnlockThrottle;
use crate::vault::{ItemPreview, Vault, VaultStatus};

/// What the window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// No vault yet: choose a master password.
    Setup,
    /// Locked: enter the password or recovery key.
    Unlock,
    /// Just created: the recovery key, shown once.
    Recovery,
    /// Unlocked: the explorer.
    Unlocked,
    /// The vault could not be opened.
    Broken,
}

/// Everything the windows share. Lives as long as the app.
pub struct Folder {
    pub app: App,
    vault: Option<Arc<Mutex<Vault>>>,
    /// The vault's folder, where `ui.json` (the language) lives beside `vault.json`.
    pub root: Option<std::path::PathBuf>,
    /// Why the vault could not be opened (the shipped code's English; shown localized).
    pub load_error: String,
    /// The interface language.
    pub lang: Lang,
    pub throttle: RefCell<UnlockThrottle>,
    pub screen: Signal<Screen>,
    pub status: Signal<Option<VaultStatus>>,
    pub busy: Signal<bool>,
    /// The last error worth showing.
    pub error: Signal<Option<String>>,
    pub recovery_key: RefCell<Option<Zeroizing<String>>>,
    pub window: RefCell<Option<Window>>,
    /// The folder the explorer shows (`None`: the vault's top level), its items, and the
    /// folders from the top down to it.
    pub folder: Signal<Option<String>>,
    pub items: Signal<Vec<ItemPreview>>,
    pub crumbs: Signal<Vec<ItemPreview>>,
    /// The text editor has edits not yet saved.
    pub dirty: Signal<bool>,
    /// Bumped whenever the vault's contents change (the explorer reloads what it shows).
    pub changed: Signal<u64>,
    locking: Cell<bool>,
}

pub type St = Rc<Folder>;

/// SecretFolder's look: dark chrome with an amber accent.
pub fn theme() -> Theme {
    let mut t = Theme::dark_forest();
    t.name = "SecretFolder".to_owned();
    // The shipped typeface first (`font-family: "Inter", system-ui, …` in `src/App.css`); the
    // files come with `fonts::with_inter`.
    t.font_families.insert(0, crate::fonts::FAMILY.to_owned());
    let c = &mut t.colors;
    let hex = |s: &str| Color::from_hex(s).unwrap_or(Color::BLACK);
    c.bg = hex("#1a1b1e");
    c.panel = hex("#24262b");
    c.card = hex("#1e1f23");
    c.text = hex("#f3f4f6");
    c.muted = hex("#9ca3af");
    c.accent = hex("#fbbf24");
    c.on_accent = hex("#1a1b1e");
    c.danger = hex("#f87171");
    c.ok = hex("#86efac");
    c.line = Some(hex("#3f3f46"));
    t
}

impl Folder {
    /// Runs `f` on the vault. Blocks while a background unlock holds it, so callers only use
    /// it when no unlock is running (the gate shows "Working…" meanwhile).
    pub fn with<T>(&self, f: impl FnOnce(&mut Vault) -> AppResult<T>) -> AppResult<T> {
        let Some(v) = &self.vault else {
            return Err(AppError::Message(self.load_error()));
        };
        let mut g = v.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut g)
    }

    /// The vault, for work on a background thread.
    pub fn shared(&self) -> Option<Arc<Mutex<Vault>>> {
        self.vault.clone()
    }

    /// Why the vault could not be opened, in the interface language.
    pub fn load_error(&self) -> String {
        let l = self.lang;
        l.tv("port.openFailed", &[("detail", &l.err(&self.load_error))])
    }

    /// Shows an error from the shipped code (English) in the interface language.
    pub fn fail(&self, raw: impl AsRef<str>) {
        self.error.set(Some(self.lang.err(raw.as_ref())));
    }
}

/// The language to start in: the one saved in `ui.json` beside the vault (0.2.0 wrote it
/// there; read by the shipped `prefs.rs`), else `SECRETFOLDER_LOCALE`, else the system's.
pub fn initial_locale(root: Option<&std::path::Path>) -> String {
    root.and_then(crate::prefs::read_ui_locale)
        .or_else(crate::prefs::env_locale)
        .unwrap_or_else(|| crate::lang::detect(&crate::lang::system_tags()).to_owned())
}

/// Switches the interface language: written to `ui.json` by the shipped `prefs.rs` (which
/// keeps every other key; `vault.json` is never touched), then every label, the tray and
/// the engine's own words follow at once.
pub fn set_language(st: &St, id: &str) {
    if !crate::prefs::is_ui_locale(id) {
        return;
    }
    if let Some(root) = &st.root
        && let Err(e) = crate::prefs::write_ui_locale(root, id)
    {
        st.fail(e.to_string());
        return;
    }
    st.app.set_locale(Some(id.to_owned()));
    st.lang.0.set_if_changed(id.to_owned());
    crate::tray::install_menu(st);
}

/// `SECRETFOLDER_VAULT` points the app at another vault folder (tests, a second vault).
pub fn vault_override() -> Option<std::path::PathBuf> {
    std::env::var_os("SECRETFOLDER_VAULT").map(Into::into)
}

/// Builds the app on the default vault, or the one `SECRETFOLDER_VAULT` names.
pub fn start(app: &App) {
    start_at(app, vault_override());
}

/// Builds the app: state, tray, the window, second-launch and idle handling.
pub fn start_at(app: &App, vault_root: Option<std::path::PathBuf>) -> St {
    // Where the vault lives, even when it cannot be opened (the language is read from there).
    let root = vault_root.clone().or_else(|| crate::vault::default_vault_root().ok());
    let opened = match vault_root {
        Some(p) => Vault::open_root(p),
        None => Vault::open_default(),
    };
    let (vault, load_error, root) = match opened {
        Ok(v) => {
            let r = v.data_root().to_path_buf();
            (Some(Arc::new(Mutex::new(v))), String::new(), Some(r))
        }
        Err(e) => (None, e.to_string(), root),
    };
    let locale = initial_locale(root.as_deref());
    app.set_locale(Some(locale.clone()));
    let st: St = Rc::new(Folder {
        app: app.clone(),
        vault,
        root,
        load_error,
        lang: Lang::new(&locale),
        throttle: RefCell::new(UnlockThrottle::new()),
        screen: Signal::new(Screen::Unlock),
        status: Signal::new(None),
        busy: Signal::new(false),
        error: Signal::new(None),
        recovery_key: RefCell::new(None),
        window: RefCell::new(None),
        folder: Signal::new(None),
        items: Signal::new(Vec::new()),
        crumbs: Signal::new(Vec::new()),
        dirty: Signal::new(false),
        changed: Signal::new(0),
        locking: Cell::new(false),
    });
    // The tray keeps the app running with the window hidden.
    app.keep_running(true);
    refresh(&st);
    crate::tray::install(&st);
    crate::explorer::open(&st);
    let s = st.clone();
    app.on_second_instance(move |_| show(&s));
    // Real input keeps the vault's own idle clock in step with the app's.
    let s = st.clone();
    app.on_activity(move || {
        if let Some(v) = &s.vault
            && let Ok(mut g) = v.try_lock()
        {
            g.touch();
        }
    });
    let s = st.clone();
    app.idle_lock(None, move || lock(&s));
    st
}

/// Reads the vault's status and picks the screen.
pub fn refresh(st: &St) {
    let status = match st.with(|v| Ok(v.status())) {
        Ok(s) => s,
        Err(_) => {
            st.screen.set_if_changed(Screen::Broken);
            return;
        }
    };
    let screen = if !status.initialized {
        Screen::Setup
    } else if status.unlocked {
        if st.recovery_key.borrow().is_some() {
            Screen::Recovery
        } else {
            Screen::Unlocked
        }
    } else {
        Screen::Unlock
    };
    let unlocked = status.unlocked;
    st.app.set_idle_timeout(if unlocked {
        idle_timeout(status.idle_lock_secs)
    } else {
        None
    });
    st.status.set(Some(status));
    st.screen.set_if_changed(screen);
    if unlocked {
        reload(st);
    } else {
        st.items.set(Vec::new());
        st.crumbs.set(Vec::new());
    }
}

/// The idle timeout for a vault setting.
pub fn idle_timeout(secs: u64) -> Option<Duration> {
    (secs > 0).then(|| Duration::from_secs(secs))
}

/// Re-reads the current folder's items and path (and the vault's item count).
pub fn reload(st: &St) {
    let folder = st.folder.get_untracked().ok().flatten();
    let listed = st.with(|v| {
        Ok((
            v.list_items_in(folder.clone())?,
            v.folder_path(folder.as_deref())?,
            v.status(),
        ))
    });
    match listed {
        Ok((items, crumbs, status)) => {
            st.items.set(items);
            st.crumbs.set(crumbs);
            st.status.set(Some(status));
        }
        // The folder went away (deleted elsewhere): back to the top.
        Err(AppError::NotFound) if folder.is_some() => {
            st.folder.set(None);
            reload(st);
        }
        Err(e) => {
            let _ = checked(st, Err::<(), _>(e));
        }
    }
}

/// The vault changed: reload and tell everything that shows its contents.
pub fn changed(st: &St) {
    reload(st);
    st.changed.update(|n| *n = n.wrapping_add(1));
}

/// The value of `r`, or `None` after handling the error: a locked vault locks the UI, other
/// errors are shown.
pub fn checked<T>(st: &St, r: AppResult<T>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(AppError::Locked) => {
            lock(st);
            None
        }
        Err(e) => {
            st.fail(e.to_string());
            None
        }
    }
}

/// Locks the vault: forgets the key and what was shown, clears a copied secret, and shows the
/// unlock screen.
pub fn lock(st: &St) {
    if st.locking.replace(true) {
        return;
    }
    let _ = st.with(|v| {
        v.lock();
        Ok(())
    });
    st.app.clear_secret();
    st.recovery_key.borrow_mut().take();
    st.dirty.set(false);
    st.folder.set(None);
    st.error.set(None);
    st.locking.set(false);
    refresh(st);
}

/// Shows and focuses the window, opening it if it was closed.
pub fn show(st: &St) {
    let w = st.window.borrow().clone();
    match w {
        Some(w) if w.is_open() => {
            w.set_visible(true);
            w.set_minimized(false);
            w.focus();
        }
        _ => crate::explorer::open(st),
    }
}

/// Quits the app (the vault locks as it goes).
pub fn quit(st: &St) {
    let _ = st.with(|v| {
        v.lock();
        Ok(())
    });
    st.app.quit();
}

/// Text in a variant of the label style ("" for the plain one).
pub fn label(ui: &remedy_ui::Ui, s: impl remedy_ui::view::IntoProp<String>, variant: &'static str) -> remedy_ui::El {
    remedy_ui::widgets::text(ui, s).variant(variant)
}
