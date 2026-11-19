//! The tray icon (the app's folder-and-lock mark) and its menu: Show, Lock, Quit.

use remedy_ui::host::{TrayEvent, TrayIcon, TrayImage, TrayItem};

use crate::state::{self, St};

const SHOW: u32 = 1;
const LOCK: u32 = 2;
const QUIT: u32 = 9;

/// The mark at tray size, decoded from the icon the Tauri app ships.
pub fn icon() -> Option<TrayImage> {
    const PNG: &[u8] = include_bytes!("../../src-tauri/icons/32x32.png");
    let d = remedy_ui_media_image::decode(PNG, None).ok()?;
    (d.image.width == d.image.height).then(|| TrayImage {
        size: d.image.width,
        rgba: d.image.pixels.to_vec(),
    })
}

/// Shows the tray icon and wires its menu.
pub fn install(st: &St) {
    let Some(image) = icon() else { return };
    let item = |id, label: &str| TrayItem::Item {
        id,
        label: label.to_owned(),
        enabled: true,
    };
    st.app.set_tray(Some(TrayIcon::new(
        "SecretFolder",
        image,
        vec![
            item(SHOW, "Show SecretFolder"),
            item(LOCK, "Lock vault"),
            TrayItem::Separator,
            item(QUIT, "Quit SecretFolder"),
        ],
    )));
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
}
