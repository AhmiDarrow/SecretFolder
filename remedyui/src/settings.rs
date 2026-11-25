//! The Settings tab: the language, how long until the vault locks itself, and the master
//! password.

use remedy_ui::core::Insets;
use remedy_ui::reactive::{Effect, Signal};
use remedy_ui::{El, Ui};
use remedy_ui::widgets as w;
use remedy_ui::forms::{Decimal, NumberOptions, number_input};

use crate::lang::tr;
use crate::state::{self, St, checked};

const MIN_PASSWORD: usize = 12;
/// The longest idle lock the setting offers, in minutes.
const MAX_IDLE_MINUTES: i64 = 240;

/// Whole minutes from a decimal (the field keeps no fraction digits).
fn minutes(d: Decimal) -> i64 {
    let (m, scale) = d.round(0).parts();
    let m = m / 10i128.pow(u32::from(scale));
    i64::try_from(m).unwrap_or(i64::MAX)
}

/// A message under the settings: a catalog key with its placeholders (drawn in the language
/// shown, so it follows a switch), or an error already in words.
#[derive(Clone, PartialEq)]
enum Note {
    Key(&'static str, Vec<(&'static str, String)>),
    Raw(String),
}

impl Note {
    fn text(&self, id: &str) -> String {
        match self {
            Note::Key(k, vars) => {
                let vars: Vec<(&str, &str)> = vars.iter().map(|(n, v)| (*n, v.as_str())).collect();
                crate::lang::trv(id, k, &vars)
            }
            Note::Raw(s) => crate::lang::err_in(id, s),
        }
    }
}

/// A section whose heading (and accessible name) follows the language.
fn section(ui: &Ui, l: crate::lang::Lang, key: &'static str) -> El {
    let s = w::section(ui, &l.t(key));
    let s2 = s.clone();
    let _ = s.clone().bind(move || {
        let title = tr(&l.id(), key);
        let _ = s2.clone().semantics(remedy_ui::a11y::Semantics::new(remedy_ui::a11y::Role::Group).label(title.clone()));
        if let Some(h) = s2.child_elements().into_iter().next() {
            let _ = h.text(title);
        }
    });
    s
}

pub fn settings(st: &St, ui: &Ui) -> El {
    let unit = ui.theme().spacing.unit;
    let l = st.lang;
    let msg: Signal<Option<(bool, Note)>> = Signal::new(None);

    // ---- language (0.2.0's first setting): saved beside the vault, applies at once.
    let language = section(ui, l, "settings.language")
        .child(crate::gate::language_select(st, ui))
        .child(w::text(ui, l.k("settings.languageHelp")).variant("muted"));

    // ---- idle lock
    let mins = Signal::new(Decimal::from_int(15));
    let status = st.status;
    let _ = Effect::new(move || {
        if let Some(s) = status.get() {
            mins.set_if_changed(Decimal::from_int(
                i64::try_from((s.idle_lock_secs / 60).max(1)).unwrap_or(15),
            ));
        }
    });
    let options = NumberOptions::default()
        .min(Decimal::from_int(1))
        .max(Decimal::from_int(MAX_IDLE_MINUTES))
        .unit(l.t("port.min"));
    let s1 = st.clone();
    let save_idle = move || {
        let m = minutes(mins.get_untracked().unwrap_or(Decimal::from_int(15)));
        if m < 1 {
            msg.set(Some((false, Note::Key("settings.idleMin", Vec::new()))));
            return;
        }
        let m = m.min(MAX_IDLE_MINUTES);
        let secs = (m * 60) as u64;
        if checked(&s1, s1.with(|v| v.set_idle_lock_secs(secs))).is_some() {
            state::refresh(&s1);
            msg.set(Some((true, Note::Key("settings.idleSet", vec![("count", s1.lang.num(m as u64))]))));
        }
    };
    let idle_field = number_input(ui, l.t("settings.idle"), mins, options);
    let idle = section(ui, l, "port.idleTitle")
        .child(
            w::row(ui)
                .gap(unit * 2.0)
                .child(idle_field)
                .child(w::button(ui, l.k("settings.save"), save_idle)),
        )
        .child(w::text(ui, l.k("settings.idleHelp")).variant("muted"));

    // ---- password
    let field = |key: &'static str| {
        let f = w::secret_input(ui, l.t(key));
        l.name_field(&f, key, false);
        f
    };
    let current = field("settings.currentPw");
    let new = field("settings.newPw");
    let again = field("settings.confirmPw");
    let (s2, c2, n2, a2) = (st.clone(), current.clone(), new.clone(), again.clone());
    let change = move || {
        let (cur, pw, pw2) = (c2.take_secret(), n2.take_secret(), a2.take_secret());
        if pw.expose().chars().count() < MIN_PASSWORD {
            msg.set(Some((false, Note::Key("settings.pwShort", Vec::new()))));
            return;
        }
        if pw.expose() != pw2.expose() {
            msg.set(Some((false, Note::Key("settings.pwMismatch", Vec::new()))));
            return;
        }
        let Some(vault) = s2.shared() else { return };
        if s2.busy.get_untracked().unwrap_or(false) {
            return;
        }
        s2.busy.set(true);
        let s3 = s2.clone();
        s2.app.background(
            move || vault.lock().unwrap_or_else(|e| e.into_inner()).change_password(cur.expose(), pw.expose()),
            move |r| {
                s3.busy.set(false);
                match r {
                    Ok(()) => msg.set(Some((true, Note::Key("settings.pwChanged", Vec::new())))),
                    Err(e) => msg.set(Some((false, Note::Raw(e.to_string())))),
                }
            },
        );
    };
    let password = section(ui, l, "settings.changePw")
        .child(w::field(ui, l.k("settings.currentPw"), current.el().fill_width()))
        .child(w::field(ui, l.k("settings.newPw"), new.el().fill_width()))
        .child(w::field(ui, l.k("settings.confirmPw"), again.el().fill_width()))
        .child({
            let busy = st.busy;
            w::button(
                ui,
                move || tr(&l.id(), if busy.get() { "unlock.working" } else { "settings.changePwBtn" }),
                change,
            )
            .variant("primary")
            .disabled(move || busy.get())
        })
        .child(w::text(ui, l.k("settings.recoveryUnchanged")).variant("muted"));

    let u = ui.clone();
    let line = remedy_ui::show(ui, move || msg.with(Option::is_some), move || {
        let l2 = l;
        let t = w::text(&u, "").text(move || {
            let id = l2.id();
            msg.get().map(|(_, m)| m.text(&id)).unwrap_or_default()
        });
        let t2 = t.clone();
        t.bind(move || {
            let ok = msg.with(|m| m.as_ref().is_some_and(|(ok, _)| *ok));
            let _ = t2.clone().variant(if ok { "ok" } else { "error" });
        })
    })
    .el();
    // Screen capture protection is asked for; some systems (Wayland, X11) cannot give it.
    let protected = st.window.borrow().as_ref().is_none_or(remedy_ui::Window::is_capture_protected);
    let capture = section(ui, l, "port.captureTitle").child(w::text(ui, l.k("port.captureHelp")).variant("muted"));
    w::scroll(ui).grow(1.0).fill_width().child(
        w::column(ui)
            .gap(unit * 6.0)
            .padding(Insets::all(unit * 6.0))
            .width(560.0)
            .layout(|l| l.max_width = remedy_ui::layout::Length::Percent(1.0))
            .child(w::heading(ui, 1, l.k("settings.title")))
            .child(language)
            .child(idle)
            .child(password)
            .child(capture.hidden(protected))
            .child(line),
    )
}
