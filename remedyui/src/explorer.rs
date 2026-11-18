//! The window: the gate screens until the vault is open, then the header (brand, Files,
//! Settings and About tabs, Lock and Hide) over the chosen tab.

use std::sync::Arc;

use remedy_ui::core::Insets;
use remedy_ui::reactive::{Signal, untrack};
use remedy_ui::view::{Align, Justify};
use remedy_ui::{El, Ui, Window, WindowSpec};
use remedy_ui_media_image::{Fit, Source};
use remedy_ui_widgets_core as w;
use remedy_ui_widgets_nav::{self as nav, Tab};

use crate::state::{self, Screen, St};

/// The app's mark (a folder with a lock), as the Tauri app ships it.
pub const MARK: &[u8] = include_bytes!("../../src-tauri/icons/64x64.png");

/// Opens the window.
pub fn open(st: &St) {
    let spec = WindowSpec::new("SecretFolder")
        .size(1000.0, 680.0)
        .min_size(560.0, 420.0)
        .hide_on_close()
        .persist_geometry("main")
        .capture_protected(true);
    let s = st.clone();
    let win = st.app.open(spec, move |ui, win| build(&s, ui, win));
    *st.window.borrow_mut() = Some(win);
}

fn build(st: &St, ui: &Ui, win: &Window) {
    let use_recovery = Signal::new(false);
    let tab: Signal<Option<String>> = Signal::new(Some("files".to_owned()));
    let page = w::column(ui)
        .grow(1.0)
        .fill_width()
        .layout(|l| l.min_height = remedy_ui::layout::Length::Px(0.0));
    let p = page.clone();
    let (st2, ui2, win2) = (st.clone(), ui.clone(), win.clone());
    let page = page.bind(move || {
        let screen = st2.screen.get();
        let _ = use_recovery.get();
        untrack(|| {
            p.clear();
            st2.error.set(None);
            let content = match screen {
                Screen::Setup | Screen::Unlock => {
                    crate::gate::gate(&st2, &ui2, screen == Screen::Setup, use_recovery)
                }
                Screen::Recovery => crate::gate::recovery_key(&st2, &ui2),
                Screen::Unlocked => unlocked(&st2, &ui2, &win2, tab),
                Screen::Broken => crate::gate::broken(&st2, &ui2),
            };
            let _ = p.clone().child(content);
        });
    });
    ui.root().child(page);
}

/// The brand: the mark, the name, and how many items the vault holds.
fn brand(st: &St, ui: &Ui) -> El {
    let unit = ui.theme().spacing.unit;
    let status = st.status;
    let count = w::label(ui, "", "muted").nowrap().text_dyn(move || {
        let n = status.get().map_or(0, |s| s.item_count);
        if n == 1 {
            "1 item".to_owned()
        } else {
            format!("{n} items")
        }
    });
    w::row(ui)
        .gap(unit * 2.0)
        .align(Align::Center)
        .child(
            remedy_ui_media_image::image(ui, Source::Bytes(Arc::from(MARK)), "", Fit::Contain)
                .width(28.0)
                .height(28.0),
        )
        .child(w::label(ui, "SecretFolder", "strong").nowrap())
        .child(count)
}

fn unlocked(st: &St, ui: &Ui, win: &Window, tab: Signal<Option<String>>) -> El {
    let unit = ui.theme().spacing.unit;
    let tabs = Signal::new(vec![
        Tab {
            id: "files".into(),
            title: "Files".into(),
            closable: false,
        },
        Tab {
            id: "settings".into(),
            title: "Settings".into(),
            closable: false,
        },
        Tab {
            id: "about".into(),
            title: "About".into(),
            closable: false,
        },
    ]);
    let (s1, w1) = (st.clone(), win.clone());
    let header = w::row(ui)
        .fill_width()
        .align(Align::Center)
        .gap(unit * 3.0)
        .padding(Insets::symmetric(unit * 2.0, unit * 4.0))
        .style(remedy_ui::style::StylePatch {
            background: Some(ui.theme().colors.panel),
            ..Default::default()
        })
        .child(brand(st, ui).grow(1.0))
        .child(nav::tabs(ui, "Sections", tabs, tab, |_| {}))
        .child(
            w::row(ui)
                .gap(unit)
                .justify(Justify::End)
                .grow(1.0)
                .child(w::button(ui, "Lock", move || state::lock(&s1)).variant("quiet"))
                .child(w::button(ui, "Hide", move || w1.set_visible(false)).variant("quiet")),
        );
    let body = w::column(ui)
        .grow(1.0)
        .fill_width()
        .layout(|l| l.min_height = remedy_ui::layout::Length::Px(0.0));
    // Each tab is built once and kept, so switching tabs keeps unsaved edits and the
    // selection (as the original does).
    let files = crate::files::files(st, ui);
    let settings = crate::settings::settings(st, ui);
    let about = crate::about::about(st, ui);
    let (f2, s2, a2) = (files.clone(), settings.clone(), about.clone());
    let body = body
        .child(files)
        .child(settings)
        .child(about)
        .bind(move || {
            let t = tab.get();
            let t = t.as_deref().unwrap_or("files");
            let _ = f2.clone().hidden(t != "files");
            let _ = s2.clone().hidden(t != "settings");
            let _ = a2.clone().hidden(t != "about");
        });
    w::column(ui)
        .grow(1.0)
        .fill_width()
        .layout(|l| l.min_height = remedy_ui::layout::Length::Px(0.0))
        .child(header)
        .child(banner(st, ui))
        .child(body)
}

/// The error banner under the header, dismissed with its button or Escape.
fn banner(st: &St, ui: &Ui) -> El {
    let unit = ui.theme().spacing.unit;
    let e = st.error;
    let text = w::label(ui, "", "error")
        .grow(1.0)
        .text_dyn(move || e.get().unwrap_or_default());
    let bar = w::row(ui)
        .fill_width()
        .align(Align::Center)
        .gap(unit * 2.0)
        .padding(Insets::symmetric(unit * 2.0, unit * 4.0))
        .child(text)
        .child(w::button(ui, "Dismiss", move || e.set(None)).variant("quiet"))
        .on_key(move |k| {
            if k.pressed
                && k.key == remedy_ui::input::Key::Escape
                && e.get_untracked().ok().flatten().is_some()
            {
                e.set(None);
                return true;
            }
            false
        });
    let b2 = bar.clone();
    bar.bind(move || {
        let msg = e.get();
        let _ = b2.clone().hidden(msg.is_none()).semantics(
            remedy_ui_a11y::Semantics::new(remedy_ui_a11y::Role::Alert)
                .label(msg.unwrap_or_default()),
        );
    })
}
