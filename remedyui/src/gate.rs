//! Before the explorer: create a vault, unlock it (password or recovery key), and the
//! recovery key shown once after setup.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use remedy_ui::reactive::Signal;
use remedy_ui::style::StylePatch;
use remedy_ui::view::{Align, Justify};
use remedy_ui::{El, TextField, Ui};
use remedy_ui_widgets_core as w;
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};
use crate::state::{self, St};

/// How long a copied recovery key stays on the clipboard.
const RECOVERY_COPY_KEEP: Duration = Duration::from_secs(30);
const MIN_PASSWORD: usize = 12;

/// Error text that follows the shared error signal; takes no space while there is none.
pub fn error_line(st: &St, ui: &Ui) -> El {
    let e = st.error;
    let label = w::label(ui, "", "error").text_dyn(move || e.get().unwrap_or_default());
    let l2 = label.clone();
    label.bind(move || {
        let msg = e.get();
        let _ = l2.clone().hidden(msg.is_none()).semantics(
            remedy_ui_a11y::Semantics::new(remedy_ui_a11y::Role::Alert)
                .label(msg.unwrap_or_default()),
        );
    })
}

/// A button whose label follows `f`.
pub fn dyn_label(b: El, f: impl Fn() -> String + 'static) -> El {
    if let Some(l) = b.child_elements().into_iter().next() {
        let _ = l.text_dyn(f);
    }
    b
}

pub fn fineprint(ui: &Ui, s: &str) -> El {
    w::label(ui, s, "muted").style(StylePatch {
        text_size: Some(ui.theme().type_scale.small),
        ..StylePatch::default()
    })
}

/// The centered card every gate screen sits in, with the app's mark.
fn card(ui: &Ui, content: El) -> El {
    let unit = ui.theme().spacing.unit;
    w::column(ui)
        .grow(1.0)
        .fill_width()
        .align(Align::Center)
        .justify(Justify::Center)
        .padding(remedy_ui::core::Insets::all(unit * 6.0))
        .child(
            w::card(ui)
                .width(420.0)
                .layout(|l| l.max_width = remedy_ui::layout::Length::Percent(1.0))
                .child(content),
        )
}

/// Setup (new vault) or unlock (password or recovery key).
pub fn gate(st: &St, ui: &Ui, setup: bool, use_recovery: Signal<bool>) -> El {
    let unit = ui.theme().spacing.unit;
    let recovery = !setup && use_recovery.get_untracked().unwrap_or(false);
    let (title, hint) = if setup {
        (
            "Create your vault",
            "Choose a strong master password. Files are encrypted on disk.",
        )
    } else if recovery {
        ("Recovery unlock", "Unlock with your recovery key.")
    } else {
        (
            "Unlock SecretFolder",
            "Enter your master password to open your encrypted vault.",
        )
    };
    let label = if recovery {
        "Recovery key"
    } else {
        "Master password"
    };
    let pass = w::secret_input(ui, label);
    if recovery {
        pass.set_reveal(true);
    }
    let confirm = setup.then(|| w::secret_input(ui, "Confirm password"));

    let submit: Rc<dyn Fn()> = {
        let (st, pass, confirm) = (st.clone(), pass.clone(), confirm.clone());
        Rc::new(move || submit(&st, &pass, confirm.as_ref(), setup, recovery))
    };
    let s2 = submit.clone();
    let pass = pass.on_submit(move || s2());
    let busy = st.busy;
    let idle_label = if setup {
        "Create vault"
    } else if recovery {
        "Unlock with recovery key"
    } else {
        "Unlock"
    };
    let go = w::button(ui, idle_label, {
        let s = submit.clone();
        move || {
            if !busy.get_untracked().unwrap_or(false) {
                s();
            }
        }
    })
    .variant("primary")
    .fill_width();
    let go = dyn_label(go, move || {
        if busy.get() {
            "Working…".to_owned()
        } else {
            idle_label.to_owned()
        }
    });

    let mut form = w::column(ui)
        .gap(unit * 3.0)
        .fill_width()
        .child(w::heading(ui, 1, title))
        .child(w::label(ui, hint, "muted"))
        .child(w::field(ui, label, pass.el().fill_width()));
    if let Some(c) = &confirm {
        let s3 = submit.clone();
        let c = c.clone().on_submit(move || s3());
        form = form.child(w::field(ui, "Confirm password", c.el().fill_width()));
    }
    form = form.child(error_line(st, ui)).child(go);
    let initialized = st
        .status
        .get_untracked()
        .ok()
        .flatten()
        .is_some_and(|s| s.initialized);
    if !setup && initialized {
        let (label, next) = if recovery {
            ("Use master password", false)
        } else {
            ("Use recovery key", true)
        };
        let e = st.error;
        form = form.child(
            w::row(ui).justify(Justify::Center).fill_width().child(
                w::button(ui, label, move || {
                    e.set(None);
                    use_recovery.set(next);
                })
                .variant("quiet"),
            ),
        );
    }
    form = form.child(fineprint(
        ui,
        "Local-only encrypted vault. No cloud sync. Signed app updates only.",
    ));
    pass.el().focus();
    card(ui, form)
}

fn submit(st: &St, pass: &TextField, confirm: Option<&TextField>, setup: bool, recovery: bool) {
    if st.busy.get_untracked().unwrap_or(false) {
        return;
    }
    st.error.set(None);
    let pw = pass.take_secret();
    if setup {
        let again = confirm.map(TextField::take_secret);
        if pw.expose().chars().count() < MIN_PASSWORD {
            st.error.set(Some(format!(
                "Password must be at least {MIN_PASSWORD} characters."
            )));
            return;
        }
        if again.as_ref().map(|a| a.expose()) != Some(pw.expose()) {
            st.error.set(Some("Passwords do not match.".to_owned()));
            return;
        }
    } else if pw.is_empty() {
        st.error.set(Some(
            if recovery {
                "Enter your recovery key."
            } else {
                "Enter your master password."
            }
            .to_owned(),
        ));
        return;
    } else if let Err(e) = st.throttle.borrow_mut().check() {
        st.error.set(Some(e.to_string()));
        return;
    }
    let Some(vault) = st.shared() else {
        st.error.set(Some(st.load_error.clone()));
        return;
    };
    st.busy.set(true);
    let s = st.clone();
    // Argon2 takes a moment: off the UI thread.
    st.app.background(
        move || -> AppResult<Option<Zeroizing<String>>> {
            let mut v = vault.lock().unwrap_or_else(|e| e.into_inner());
            if setup {
                v.setup(pw.expose()).map(|k| Some(Zeroizing::new(k)))
            } else if recovery {
                v.unlock_with_recovery(pw.expose().trim()).map(|()| None)
            } else {
                v.unlock(pw.expose()).map(|()| None)
            }
        },
        move |r| {
            s.busy.set(false);
            match r {
                Ok(key) => {
                    if setup && key.as_ref().is_none_or(|k| k.trim().is_empty()) {
                        s.error.set(Some(
                            "Setup succeeded but no recovery key was returned.".to_owned(),
                        ));
                        return;
                    }
                    if !setup {
                        s.throttle.borrow_mut().record_success();
                    }
                    *s.recovery_key.borrow_mut() = key;
                    state::refresh(&s);
                }
                Err(e) => {
                    if !setup && matches!(e, AppError::BadPassword | AppError::Crypto(_)) {
                        s.throttle.borrow_mut().record_failure();
                    }
                    s.error.set(Some(e.to_string()));
                }
            }
        },
    );
}

/// The recovery key, shown once after setup; the vault opens only once the user says it is
/// saved.
pub fn recovery_key(st: &St, ui: &Ui) -> El {
    let unit = ui.theme().spacing.unit;
    let key = st
        .recovery_key
        .borrow()
        .as_ref()
        .map(|k| k.to_string())
        .unwrap_or_default();
    let saved = Signal::new(false);
    let shown = w::label(ui, Zeroizing::new(key).as_str(), "strong")
        .data_text()
        .style(StylePatch {
            mono: Some(true),
            ..StylePatch::default()
        })
        .semantics(
            remedy_ui_a11y::Semantics::new(remedy_ui_a11y::Role::Status).label("Recovery key"),
        );
    let copied = Rc::new(Cell::new(0u64));
    let copy_label = Signal::new("Copy recovery key".to_owned());
    let copy = {
        let (st, copied) = (st.clone(), copied.clone());
        move || {
            if let Some(k) = st.recovery_key.borrow().as_ref() {
                st.app.copy_secret(k.to_string(), RECOVERY_COPY_KEEP);
                copy_label.set("Copied".to_owned());
                let n = copied.get() + 1;
                copied.set(n);
                let c2 = copied.clone();
                st.app.after(Duration::from_secs(2), move || {
                    if c2.get() == n {
                        copy_label.set("Copy recovery key".to_owned());
                    }
                });
            }
        }
    };
    let copy_btn = dyn_label(
        w::button(ui, "Copy recovery key", copy).variant("primary"),
        move || copy_label.get(),
    );
    let done = {
        let st = st.clone();
        move || {
            if !saved.get_untracked().unwrap_or(false) {
                st.error.set(Some(
                    "Confirm that you saved the recovery key before continuing.".to_owned(),
                ));
                return;
            }
            st.error.set(None);
            st.recovery_key.borrow_mut().take();
            state::refresh(&st);
        }
    };
    let e = st.error;
    let check = w::check(ui, "I saved this recovery key somewhere safe", saved);
    let check = check.bind(move || {
        let _ = saved.get();
        e.set(None);
    });
    let go = w::button(ui, "Continue to vault", done)
        .variant("primary")
        .fill_width();
    let g2 = go.clone();
    let go = go.bind(move || {
        let _ = g2.clone().disabled(!saved.get());
    });
    card(
        ui,
        w::column(ui)
            .gap(unit * 3.0)
            .fill_width()
            .child(w::heading(ui, 1, "Save your recovery key"))
            .child(w::label(
                ui,
                "This is the only time SecretFolder will show this key. Store it offline — it unlocks the vault if \
                 you forget the master password. Anyone with this key can open your vault.",
                "muted",
            ))
            .child(w::card(ui).fill_width().child(shown))
            .child(copy_btn)
            .child(check)
            .child(error_line(st, ui))
            .child(go)
            .child(fineprint(ui, "Protects data at rest on disk. Does not protect a fully compromised PC while unlocked.")),
    )
}

/// The vault could not be opened.
pub fn broken(st: &St, ui: &Ui) -> El {
    let s = st.clone();
    card(
        ui,
        w::column(ui)
            .gap(ui.theme().spacing.unit * 3.0)
            .child(w::heading(ui, 1, "SecretFolder can't start"))
            .child(w::label(ui, st.load_error.clone(), "error"))
            .child(w::button(ui, "Quit", move || state::quit(&s)).variant("danger")),
    )
}
