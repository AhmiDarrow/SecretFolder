//! The tray icon (the app's folder-and-lock mark) and its menu: Show, Lock, Quit, in the
//! interface language (0.2.0's own `tray_labels`), again after every language switch.

use remedy_ui::host::{TrayEvent, TrayIcon, TrayImage, TrayItem};

use crate::state::{self, St};

const SHOW: u32 = 1;
const LOCK: u32 = 2;
const QUIT: u32 = 9;

/// The mark at tray size, decoded from the icon the Tauri app ships.
pub fn icon() -> Option<TrayImage> {
    const PNG: &[u8] = include_bytes!("../../src-tauri/icons/32x32.png");
    let d = remedy_ui::image::decode(PNG, None).ok()?;
    (d.image.width == d.image.height).then(|| TrayImage {
        size: d.image.width,
        rgba: d.image.pixels.to_vec(),
    })
}

/// The menu in `locale`, worded as 0.2.0's tray (`prefs::tray_labels`, clamped as it did).
pub fn menu(locale: &str) -> Vec<TrayItem> {
    let (show, lock, quit) = crate::prefs::tray_labels(locale);
    // Plain entries, none in bold: the shipped tray had no default item.
    vec![
        TrayItem::item(SHOW, crate::prefs::clamp_tray_label(show, "Show")),
        TrayItem::item(LOCK, crate::prefs::clamp_tray_label(lock, "Lock")),
        TrayItem::Separator,
        TrayItem::item(QUIT, crate::prefs::clamp_tray_label(quit, "Quit")),
    ]
}

/// Shows the tray icon with its menu in the current language.
pub fn install_menu(st: &St) {
    let Some(image) = icon() else { return };
    st.app.set_tray(Some(TrayIcon::new("SecretFolder", image, menu(&st.lang.now()))));
}

/// Shows the tray icon and wires its menu.
pub fn install(st: &St) {
    install_menu(st);
    let st = st.clone();
    st.app.clone().on_tray(move |e| match e {
        TrayEvent::Activate | TrayEvent::Menu(SHOW) => state::show(&st),
        TrayEvent::Menu(LOCK) => state::lock(&st),
        TrayEvent::Menu(QUIT) => state::quit(&st),
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_tray_icon_is_the_shipped_mark() {
        let i = super::icon().unwrap_or_else(|| unreachable!("the icon decodes"));
        assert_eq!(i.size, 32);
        assert_eq!(i.rgba.len(), 32 * 32 * 4);
        assert!(
            i.rgba.chunks(4).any(|p| p.get(3).is_some_and(|a| *a > 0)),
            "not blank"
        );
    }

    #[test]
    fn the_menu_speaks_every_shipped_language() {
        for (id, _) in crate::lang::LOCALES {
            let m = super::menu(id);
            assert_eq!(m[0].label(), crate::lang::tr(id, "tray.show"), "{id}");
            assert_eq!(m[1].label(), crate::lang::tr(id, "tray.lock"), "{id}");
            assert_eq!(m[3].label(), crate::lang::tr(id, "tray.quit"), "{id}");
        }
    }
}
