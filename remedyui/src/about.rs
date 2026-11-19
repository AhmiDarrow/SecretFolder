//! The About tab: what SecretFolder is, its links, and signed updates from GitHub releases
//! (the same minisign key the Tauri app uses).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use remedy_ui::core::Insets;
use remedy_ui::reactive::Signal;
use remedy_ui::view::Align;
use remedy_ui::{El, Ui};
use remedy_ui_media_image::{Fit, Source};
use remedy_ui_services_update::{self as up, Apply, Feed, Release, UpdateConfig, UpdateState};
use remedy_ui_widgets_core as w;

use crate::state::{self, St};

/// The release signing key (minisign public key line).
const PUBLIC_KEY: &str = "RWTqUvUHbMZIlnLI8UP0R02Z7Fyw6l7ZjEDIX/5jNQKtif7D+Zi20u8Y";

const LINKS: [(&str, &str); 4] = [
    ("GitHub", "https://github.com/AhmiDarrow"),
    ("Repo", "https://github.com/AhmiDarrow/SecretFolder"),
    (
        "Releases",
        "https://github.com/AhmiDarrow/SecretFolder/releases",
    ),
    ("Patreon", "https://www.patreon.com/cw/AhmiDarrow"),
];

fn config() -> UpdateConfig {
    UpdateConfig::new(
        env!("CARGO_PKG_VERSION"),
        Feed::GitHub {
            owner: "AhmiDarrow".into(),
            repo: "SecretFolder".into(),
            asset: "_x64-setup.exe".into(),
        },
        PUBLIC_KEY,
    )
}

/// Where an update stands.
struct Updater {
    state: Signal<UpdateState>,
    release: RefCell<Option<Release>>,
    file: RefCell<Option<PathBuf>>,
}

fn check(st: &St, u: &Rc<Updater>) {
    if matches!(
        u.state.get_untracked().ok(),
        Some(UpdateState::Checking | UpdateState::Downloading(_))
    ) {
        return;
    }
    u.state.set(UpdateState::Checking);
    let u = u.clone();
    st.app.background(
        || up::check(&config()),
        move |r| match r {
            Ok(Some(rel)) => {
                *u.release.borrow_mut() = Some(rel.clone());
                u.state.set(UpdateState::Available(rel));
            }
            Ok(None) => u.state.set(UpdateState::UpToDate),
            Err(e) => u.state.set(UpdateState::Failed(e.to_string())),
        },
    );
}

fn install(st: &St, u: &Rc<Updater>) {
    let Some(rel) = u.release.borrow().clone() else {
        return;
    };
    u.state.set(UpdateState::Downloading(None));
    let u = u.clone();
    let dir = std::env::temp_dir().join("SecretFolder-update");
    st.app.background(
        move || up::download(&config(), &rel, &dir, |_, _| {}),
        move |r| match r {
            Ok(file) => {
                *u.file.borrow_mut() = Some(file);
                u.state.set(UpdateState::Ready);
            }
            Err(e) => u.state.set(UpdateState::Failed(e.to_string())),
        },
    );
}

fn restart(st: &St, u: &Rc<Updater>) {
    let Some(file) = u.file.borrow().clone() else {
        return;
    };
    // Lock first: the installer replaces this program.
    state::lock(st);
    match up::apply(&file, &Apply::RunInstaller(Vec::new())) {
        Ok(()) => st.app.quit(),
        Err(e) => u.state.set(UpdateState::Failed(e.to_string())),
    }
}

pub fn about(st: &St, ui: &Ui) -> El {
    let unit = ui.theme().spacing.unit;
    let u = Rc::new(Updater {
        state: Signal::new(UpdateState::Idle),
        release: RefCell::new(None),
        file: RefCell::new(None),
    });
    let (s1, u1, s2, u2, s3, u3) = (
        st.clone(),
        u.clone(),
        st.clone(),
        u.clone(),
        st.clone(),
        u.clone(),
    );
    let banner = up::update_banner(
        ui,
        u.state,
        move || install(&s1, &u1),
        move || restart(&s2, &u2),
    );
    let link_error: Signal<Option<String>> = Signal::new(None);
    let mut links = w::wrap(ui).gap(unit * 2.0);
    for (i, (label, url)) in LINKS.iter().enumerate() {
        let s = st.clone();
        let b = w::button(ui, *label, move || {
            s.app.open_url(*url, move |r| link_error.set(r.err().map(|e| e.reason)));
        });
        links = links.child(if i == 0 { b.variant("primary") } else { b });
    }
    let u = ui.clone();
    let err = remedy_ui::show(ui, move || link_error.with(Option::is_some), move || {
        w::text(&u, "").variant("error").text(move || link_error.get().unwrap_or_default())
    })
    .el();
    w::scroll(ui).grow(1.0).fill_width().child(
        w::column(ui)
            .gap(unit * 4.0)
            .padding(Insets::all(unit * 6.0))
            .width(560.0)
            .layout(|l| l.max_width = remedy_ui::layout::Length::Percent(1.0))
            .child(
                w::row(ui)
                    .gap(unit * 3.0)
                    .align(Align::Center)
                    .child(
                        remedy_ui_media_image::image(
                            ui,
                            Source::Bytes(Arc::from(crate::explorer::MARK)),
                            "",
                            Fit::Contain,
                        )
                        .width(36.0)
                        .height(36.0),
                    )
                    .child(
                        w::column(ui)
                            .child(w::heading(ui, 1, "About"))
                            .child(w::text(ui, "Hi I'm Ahmi, hope this helps!").variant("")),
                    ),
            )
            .child(w::text(ui, format!(
                    "Local-only encrypted folder vault · MIT · v{} · built with RemedyUI",
                    env!("CARGO_PKG_VERSION")
                )).variant("muted"))
            .child(w::text(ui, "Vault data stays on this PC. Network is only for signed GitHub updates.").variant("muted"))
            .child(links)
            .child(err)
            .child(w::button(ui, "Check for updates", move || check(&s3, &u3)))
            .child(banner),
    )
}
