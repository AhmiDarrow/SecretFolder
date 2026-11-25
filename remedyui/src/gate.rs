//! Before the explorer: create a vault, unlock it (password or recovery key), and the
//! recovery key shown once after setup. Every card has the language picker, so the app can
//! be read before any vault exists.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use remedy_ui::reactive::{Signal, untrack};
use remedy_ui::style::StylePatch;
use remedy_ui::view::{Align, Justify};
use remedy_ui::{El, TextField, Ui};
use remedy_ui::widgets as w;
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};
use crate::lang::{LOCALES, Lang};
use crate::state::{self, St};

/// How long a copied recovery key stays on the clipboard.
const RECOVERY_COPY_KEEP: Duration = Duration::from_secs(30);
const MIN_PASSWORD: usize = 12;

/// Error text that follows the shared error signal; there (and taking room) only while there
/// is one.
pub fn error_line(st: &St, ui: &Ui) -> El {
    let e = st.error;
    let u = ui.clone();
    remedy_ui::show(ui, move || e.with(Option::is_some), move || {
        w::text(&u, "").variant("error").text(move || e.get().unwrap_or_default()).semantics(move || {
            remedy_ui::a11y::Semantics::new(remedy_ui::a11y::Role::Alert).label(e.get().unwrap_or_default())
        })
    })
    .el()
}

/// A button whose label follows `f`.
pub fn dyn_label(b: El, f: impl Fn() -> String + 'static) -> El {
    if let Some(l) = b.child_elements().into_iter().next() {
        let _ = l.text(f);
    }
    b
}

pub fn fineprint(ui: &Ui, s: impl remedy_ui::view::IntoProp<String>) -> El {
    w::text(ui, s).variant("muted").style(StylePatch::new().text_size(ui.theme().type_scale.small))
}

/// The language picker (every shipped language in its own name). A choice is saved to
/// `ui.json` and applies at once; a switch made elsewhere moves the picker too.
pub fn language_select(st: &St, ui: &Ui) -> El {
    let l = st.lang;
    let index = |id: &str| LOCALES.iter().position(|(x, _)| *x == id).unwrap_or(0);
    let chosen = Signal::new(index(&l.now()));
    let names = LOCALES.iter().map(|(_, n)| (*n).to_owned()).collect();
    let pick = w::select(ui, l.k("settings.language"), names, chosen).fill_width().justify(Justify::SpaceBetween);
    let st2 = st.clone();
    let _ = pick.clone().bind(move || {
        let (id, _) = LOCALES[chosen.get().min(LOCALES.len() - 1)];
        if l.now() != id {
            untrack(|| state::set_language(&st2, id));
        }
    });
    let _ = pick.clone().bind(move || {
        let i = index(&l.id());
        untrack(|| chosen.set_if_changed(i));
    });
    pick
}

/// The centered card every gate screen sits in, with the app's mark; it scrolls when the
/// window is shorter than the card (Inter's line height makes the setup card taller than the
/// smallest window).
fn card(ui: &Ui, content: El) -> El {
    let unit = ui.theme().spacing.unit;
    let inner = w::column(ui)
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
        );
    w::scroll(ui).grow(1.0).fill_width().child(inner)
}

/// A secret field named in the current language (again after a switch).
fn secret(ui: &Ui, l: Lang, key: &'static str) -> TextField {
    let f = w::secret_input(ui, l.t(key));
    l.name_field(&f, key, false);
    f
}

/// Setup (new vault) or unlock (password or recovery key).
pub fn gate(st: &St, ui: &Ui, setup: bool, use_recovery: Signal<bool>) -> El {
    let unit = ui.theme().spacing.unit;
    let l = st.lang;
    let recovery = !setup && use_recovery.get_untracked().unwrap_or(false);
    let (title, hint) = if setup {
        ("unlock.titleSetup", "unlock.leadSetup")
    } else if recovery {
        ("unlock.titleRecovery", "unlock.leadRecovery")
    } else {
        ("unlock.titleUnlock", "unlock.leadUnlock")
    };
    let label = if recovery { "unlock.recoveryKey" } else { "unlock.masterPassword" };
    let pass = secret(ui, l, label);
    if recovery {
        pass.set_reveal(true);
    }
    let confirm = setup.then(|| secret(ui, l, "unlock.confirmPassword"));

    let submit: Rc<dyn Fn()> = {
        let (st, pass, confirm) = (st.clone(), pass.clone(), confirm.clone());
        Rc::new(move || submit(&st, &pass, confirm.as_ref(), setup, recovery))
    };
    let s2 = submit.clone();
    let pass = pass.on_submit(move || s2());
    let busy = st.busy;
    let idle_label = if setup {
        "unlock.createVault"
    } else if recovery {
        "unlock.unlockRecovery"
    } else {
        "unlock.unlock"
    };
    // A live label: the button's text and its accessible name both follow the language.
    let go_label = move || crate::lang::tr(&l.id(), if busy.get() { "unlock.working" } else { idle_label });
    let go = w::button(ui, go_label, {
        let s = submit.clone();
        move || {
            if !busy.get_untracked().unwrap_or(false) {
                s();
            }
        }
    })
    .variant("primary")
    .fill_width();

    let mut form = w::column(ui)
        .gap(unit * 3.0)
        .fill_width()
        .child(mark(ui))
        .child(w::heading(ui, 1, l.k(title)))
        .child(w::text(ui, l.k(hint)).variant("muted"))
        .child(w::field(ui, l.k(label), pass.el().fill_width()));
    if let Some(c) = &confirm {
        let s3 = submit.clone();
        let c = c.clone().on_submit(move || s3());
        form = form.child(w::field(ui, l.k("unlock.confirmPassword"), c.el().fill_width()));
    }
    form = form.child(error_line(st, ui)).child(go);
    let initialized = st
        .status
        .get_untracked()
        .ok()
        .flatten()
        .is_some_and(|s| s.initialized);
    if !setup && initialized {
        let (key, next) = if recovery {
            ("unlock.usePassword", false)
        } else {
            ("unlock.useRecovery", true)
        };
        let e = st.error;
        form = form.child(
            w::row(ui).justify(Justify::Center).fill_width().child(
                w::button(ui, l.k(key), move || {
                    e.set(None);
                    use_recovery.set(next);
                })
                .variant("quiet"),
            ),
        );
    }
    form = form.child(fineprint(ui, l.k("unlock.fineprint"))).child(language_select(st, ui));
    pass.el().focus();
    card(ui, form)
}

fn submit(st: &St, pass: &TextField, confirm: Option<&TextField>, setup: bool, recovery: bool) {
    if st.busy.get_untracked().unwrap_or(false) {
        return;
    }
    let l = st.lang;
    st.error.set(None);
    let pw = pass.take_secret();
    if setup {
        let again = confirm.map(TextField::take_secret);
        if pw.expose().chars().count() < MIN_PASSWORD {
            st.error.set(Some(l.t("unlock.pwShort")));
            return;
        }
        if again.as_ref().map(|a| a.expose()) != Some(pw.expose()) {
            st.error.set(Some(l.t("unlock.pwMismatch")));
            return;
        }
    } else if pw.is_empty() {
        st.error.set(Some(l.t(if recovery { "port.enterRecovery" } else { "port.enterPassword" })));
        return;
    } else if let Err(e) = st.throttle.borrow_mut().check() {
        st.fail(e.to_string());
        return;
    }
    let Some(vault) = st.shared() else {
        st.error.set(Some(st.load_error()));
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
                        s.error.set(Some(s.lang.t("unlock.noKey")));
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
                    s.fail(e.to_string());
                }
            }
        },
    );
}

/// The recovery key, shown once after setup; the vault opens only once the user says it is
/// saved.
pub fn recovery_key(st: &St, ui: &Ui) -> El {
    let unit = ui.theme().spacing.unit;
    let l = st.lang;
    let key = st
        .recovery_key
        .borrow()
        .as_ref()
        .map(|k| k.to_string())
        .unwrap_or_default();
    let saved = Signal::new(false);
    let shown = w::text(ui, Zeroizing::new(key).as_str()).variant("strong")
        .data_text()
        .style(StylePatch::new().mono(true))
        .semantics(move || {
            remedy_ui::a11y::Semantics::new(remedy_ui::a11y::Role::Status).label(crate::lang::tr(&l.id(), "unlock.recoveryKey"))
        });
    let copied = Rc::new(Cell::new(0u64));
    // Whether "Copied" shows (for two seconds after a copy).
    let just_copied = Signal::new(false);
    let copy = {
        let (st, copied) = (st.clone(), copied.clone());
        move || {
            if let Some(k) = st.recovery_key.borrow().as_ref() {
                st.app.copy_secret(k.to_string(), RECOVERY_COPY_KEEP);
                just_copied.set(true);
                let n = copied.get() + 1;
                copied.set(n);
                let c2 = copied.clone();
                st.app.after(Duration::from_secs(2), move || {
                    if c2.get() == n {
                        just_copied.set(false);
                    }
                });
            }
        }
    };
    let copy_btn = dyn_label(
        w::button(ui, l.t("recovery.copy"), copy).variant("primary"),
        move || crate::lang::tr(&l.id(), if just_copied.get() { "recovery.copied" } else { "recovery.copy" }),
    );
    let done = {
        let st = st.clone();
        move || {
            if !saved.get_untracked().unwrap_or(false) {
                st.error.set(Some(st.lang.t("unlock.confirmSaveError")));
                return;
            }
            st.error.set(None);
            st.recovery_key.borrow_mut().take();
            state::refresh(&st);
        }
    };
    let e = st.error;
    let check = w::check(ui, l.k("recovery.savedCheck"), saved);
    let check = check.bind(move || {
        let _ = saved.get();
        e.set(None);
    });
    let go = w::button(ui, l.k("recovery.continue"), done)
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
            .child(mark(ui))
            .child(w::heading(ui, 1, l.k("recovery.title")))
            .child(w::text(ui, move || crate::lang::plain(&crate::lang::tr(&l.id(), "recovery.body"))).variant("muted"))
            .child(w::card(ui).fill_width().child(shown))
            .child(copy_btn)
            .child(check)
            .child(error_line(st, ui))
            .child(go)
            .child(fineprint(ui, l.k("recovery.fineprint")))
            .child(language_select(st, ui)),
    )
}

/// The vault could not be opened.
pub fn broken(st: &St, ui: &Ui) -> El {
    let (s, s2, l) = (st.clone(), st.clone(), st.lang);
    card(
        ui,
        w::column(ui)
            .gap(ui.theme().spacing.unit * 3.0)
            .child(mark(ui))
            .child(w::heading(ui, 1, l.k("port.cantStart")))
            .child(w::text(ui, move || {
                let _ = l.id();
                s2.load_error()
            })
            .variant("error"))
            .child(w::button(ui, l.k("tray.quit"), move || state::quit(&s)).variant("danger"))
            .child(language_select(st, ui)),
    )
}

/// The app mark at the top of a gate card (decorative: the heading names the screen).
fn mark(ui: &Ui) -> El {
    remedy_ui::image::image(
        ui,
        remedy_ui::image::Source::Bytes(std::sync::Arc::from(crate::explorer::MARK)),
        "",
        remedy_ui::image::Fit::Contain,
    )
    .width(44.0)
    .height(44.0)
    .semantics(remedy_ui::a11y::Semantics::new(remedy_ui::a11y::Role::Image).hidden(true))
}
