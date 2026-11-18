//! The Settings tab: how long until the vault locks itself, and the master password.

use remedy_ui::core::Insets;
use remedy_ui::reactive::{Effect, Signal};
use remedy_ui::{El, Ui};
use remedy_ui_widgets_core as w;
use remedy_ui_widgets_forms::{Decimal, NumberOptions, number_input};

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

pub fn settings(st: &St, ui: &Ui) -> El {
    let unit = ui.theme().spacing.unit;
    let msg: Signal<Option<(bool, String)>> = Signal::new(None);

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
    let options = NumberOptions {
        min: Some(Decimal::from_int(1)),
        max: Some(Decimal::from_int(MAX_IDLE_MINUTES)),
        unit: Some("min".into()),
        ..NumberOptions::default()
    };
    let s1 = st.clone();
    let save_idle = move || {
        let m = minutes(mins.get_untracked().unwrap_or(Decimal::from_int(15)));
        if m < 1 {
            msg.set(Some((false, "Idle lock must be at least 1 minute.".into())));
            return;
        }
        let secs = (m.min(MAX_IDLE_MINUTES) * 60) as u64;
        if checked(&s1, s1.with(|v| v.set_idle_lock_secs(secs))).is_some() {
            state::refresh(&s1);
            msg.set(Some((
                true,
                format!("Idle lock set to {} min.", m.min(MAX_IDLE_MINUTES)),
            )));
        }
    };
    let idle = w::section(ui, "Idle lock")
        .child(
            w::row(ui)
                .gap(unit * 2.0)
                .child(number_input(ui, "Idle lock (minutes)", mins, options))
                .child(w::button(ui, "Save", save_idle)),
        )
        .child(w::label(
            ui,
            "Locks after this long with no input. Default is 15 minutes.",
            "muted",
        ));

    // ---- password
    let current = w::secret_input(ui, "Current password");
    let new = w::secret_input(ui, "New password");
    let again = w::secret_input(ui, "Confirm new password");
    let (s2, c2, n2, a2) = (st.clone(), current.clone(), new.clone(), again.clone());
    let change = move || {
        let (cur, pw, pw2) = (c2.take_secret(), n2.take_secret(), a2.take_secret());
        if pw.expose().chars().count() < MIN_PASSWORD {
            msg.set(Some((
                false,
                format!("New password must be at least {MIN_PASSWORD} characters."),
            )));
            return;
        }
        if pw.expose() != pw2.expose() {
            msg.set(Some((false, "New passwords do not match.".into())));
            return;
        }
        match s2.with(|v| v.change_password(cur.expose(), pw.expose())) {
            Ok(()) => msg.set(Some((true, "Password changed.".into()))),
            Err(e) => msg.set(Some((false, e.to_string()))),
        }
    };
    let password = w::section(ui, "Change master password")
        .child(w::field(ui, "Current password", current.el().fill_width()))
        .child(w::field(ui, "New password", new.el().fill_width()))
        .child(w::field(
            ui,
            "Confirm new password",
            again.el().fill_width(),
        ))
        .child(w::button(ui, "Change password", change).variant("primary"))
        .child(w::label(
            ui,
            "Recovery key is unchanged when you change the master password.",
            "muted",
        ));

    let line = w::label(ui, "", "").text_dyn(move || msg.get().map(|(_, m)| m).unwrap_or_default());
    let l2 = line.clone();
    let line = line.bind(move || {
        let m = msg.get();
        let _ = l2
            .clone()
            .variant(if m.as_ref().is_some_and(|(ok, _)| *ok) {
                "ok"
            } else {
                "error"
            })
            .hidden(m.is_none());
    });
    w::scroll(ui).grow(1.0).fill_width().child(
        w::column(ui)
            .gap(unit * 6.0)
            .padding(Insets::all(unit * 6.0))
            .width(560.0)
            .layout(|l| l.max_width = remedy_ui::layout::Length::Percent(1.0))
            .child(w::heading(ui, 1, "Settings"))
            .child(idle)
            .child(password)
            .child(line),
    )
}
