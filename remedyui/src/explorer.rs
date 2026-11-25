//! The window: the gate screens until the vault is open, then the header (brand, Files,
//! Settings and About tabs, Lock and Hide) over the chosen tab.

use std::sync::Arc;

use remedy_ui::core::Insets;
use remedy_ui::reactive::{Signal, untrack};
use remedy_ui::view::{Align, Justify};
use remedy_ui::{El, Ui, Window, WindowSpec};
use remedy_ui::image::{Fit, Source};
use remedy_ui::widgets as w;
use remedy_ui::nav::{self as nav, Tab};

use crate::state::{self, Screen, St};

/// The app's mark (a folder with a lock), as the Tauri app ships it.
pub const MARK: &[u8] = include_bytes!("../../src-tauri/icons/64x64.png");
/// `src-tauri/icons/128x128@2x.png` (256 px), the largest of the shipped window icons; each
/// platform scales it to the sizes it shows.
const WINDOW_ICON: &[u8] = include_bytes!("../../src-tauri/icons/128x128@2x.png");

/// The shipped app's icon for the window (taskbar, Alt+Tab, the title bar).
pub fn window_icon() -> Option<remedy_ui::host::WindowIcon> {
    let d = remedy_ui::image::decode(WINDOW_ICON, None).ok()?;
    let img = d.image;
    if img.width != img.height {
        return None;
    }
    remedy_ui::host::WindowIcon::new(img.width, img.pixels.to_vec())
}

/// Opens the window.
pub fn open(st: &St) {
    let spec = WindowSpec::new("SecretFolder")
        .size(920.0, 640.0)
        .min_size(560.0, 420.0)
        .hide_on_close()
        .persist_geometry("main")
        .capture_protected(true);
    let spec = match window_icon() {
        Some(icon) => spec.icon(icon),
        None => spec,
    };
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
    let (status, l) = (st.status, st.lang);
    let count = w::text(ui, "").variant("muted").nowrap().text(move || {
        let n = status.get().map_or(0, |s| s.item_count);
        crate::lang::count_in(&l.id(), "items", n as u64)
    });
    w::row(ui)
        .gap(unit * 2.0)
        .align(Align::Center)
        .child(
            remedy_ui::image::image(ui, Source::Bytes(Arc::from(MARK)), "", Fit::Contain)
                .width(28.0)
                .height(28.0),
        )
        .child(w::text(ui, "SecretFolder").variant("strong").nowrap())
        .child(count)
}

fn unlocked(st: &St, ui: &Ui, win: &Window, tab: Signal<Option<String>>) -> El {
    let unit = ui.theme().spacing.unit;
    let l = st.lang;
    let tab_list = move |id: &str| {
        ["files", "settings", "about"]
            .iter()
            .map(|t| Tab {
                id: (*t).into(),
                title: crate::lang::tr(id, &format!("nav.{t}")),
                closable: false,
            })
            .collect::<Vec<_>>()
    };
    let tabs = Signal::new(tab_list(&l.now()));
    // The tab titles follow a language switch.
    let _ = remedy_ui::reactive::Effect::new(move || {
        let id = l.id();
        untrack(|| tabs.set(tab_list(&id)));
    });
    let (s1, w1) = (st.clone(), win.clone());
    let header = w::row(ui)
        .fill_width()
        .align(Align::Center)
        .gap(unit * 3.0)
        .padding(Insets::symmetric(unit * 2.0, unit * 4.0))
        .style(remedy_ui::style::StylePatch::new().background(ui.theme().colors.panel))
        .child(brand(st, ui).grow(1.0))
        .child(nav::tabs(ui, l.t("port.sections"), tabs, tab, |_| {}))
        .child(
            w::row(ui)
                .gap(unit)
                .justify(Justify::End)
                .grow(1.0)
                .child(w::button(ui, l.k("nav.lock"), move || state::lock(&s1)).variant("quiet"))
                .child(w::button(ui, l.k("nav.hide"), move || w1.set_visible(false)).variant("quiet")),
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

/// The error banner under the header, there while there is an error, dismissed with its
/// button or Escape.
fn banner(st: &St, ui: &Ui) -> El {
    let (e, l) = (st.error, st.lang);
    let u = ui.clone();
    remedy_ui::show(ui, move || e.with(Option::is_some), move || banner_bar(e, l, &u)).el()
}

fn banner_bar(e: Signal<Option<String>>, l: crate::lang::Lang, ui: &Ui) -> El {
    let unit = ui.theme().spacing.unit;
    let text = w::text(ui, "").variant("error")
        .grow(1.0)
        .text(move || e.get().unwrap_or_default());
    let bar = w::row(ui)
        .fill_width()
        .align(Align::Center)
        .gap(unit * 2.0)
        .padding(Insets::symmetric(unit * 2.0, unit * 4.0))
        .child(text)
        .child(w::button(ui, move || crate::lang::cap(&crate::lang::tr(&l.id(), "nav.dismiss")), move || e.set(None)).variant("quiet"))
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
    bar.semantics(move || remedy_ui::a11y::Semantics::new(remedy_ui::a11y::Role::Alert).label(e.get().unwrap_or_default()))
}
