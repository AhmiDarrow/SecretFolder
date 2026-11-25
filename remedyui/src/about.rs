//! The About tab: what SecretFolder is, its links, and signed updates from GitHub releases
//! (the same minisign key the Tauri app uses), in the interface language.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use remedy_ui::core::Insets;
use remedy_ui::reactive::{Signal, untrack};
use remedy_ui::view::Align;
use remedy_ui::{El, Ui};
use remedy_ui::image::{Fit, Source};
use remedy_ui::update::{self as up, Apply, BannerText, Feed, Release, UpdateConfig, UpdateState};
use remedy_ui::widgets as w;

use crate::lang::{Lang, tr};
use crate::state::{self, St};

/// The release signing key (minisign public key line).
const PUBLIC_KEY: &str = "RWTqUvUHbMZIlnLI8UP0R02Z7Fyw6l7ZjEDIX/5jNQKtif7D+Zi20u8Y";

/// This platform's release asset and how it is put in place: Windows runs the signed
/// installer (which closes the app and replaces it); Linux replaces the program itself
/// (`SecretFolder_<version>_linux-x86_64`, the executable the archive also holds).
#[cfg(windows)]
const ASSET: (&str, fn() -> Apply) = ("_x64-setup.exe", || Apply::RunInstaller(Vec::new()));
#[cfg(not(windows))]
const ASSET: (&str, fn() -> Apply) = ("_linux-x86_64", || Apply::ReplaceExecutable);

const LINKS: [(&str, &str); 4] = [
    ("about.github", "https://github.com/AhmiDarrow"),
    ("about.repo", "https://github.com/AhmiDarrow/SecretFolder"),
    ("about.releases", "https://github.com/AhmiDarrow/SecretFolder/releases"),
    ("about.patreon", "https://www.patreon.com/cw/AhmiDarrow"),
];

fn config() -> UpdateConfig {
    UpdateConfig::new(
        env!("CARGO_PKG_VERSION"),
        Feed::GitHub {
            owner: "AhmiDarrow".into(),
            repo: "SecretFolder".into(),
            asset: ASSET.0.into(),
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

/// The update strip in 0.2.0's words for `id`; the install button restarts the app.
fn texts(id: &str) -> BannerText {
    let version = env!("CARGO_PKG_VERSION");
    let mut t = BannerText::localized(id);
    t.checking = tr(id, "about.checking");
    t.up_to_date = crate::lang::trv(id, "about.latest", &[("version", version)]);
    // The banner fills `{version}` with the release's.
    t.available = tr(id, "about.available");
    t.downloading = tr(id, "about.downloading");
    t.ready = tr(id, "about.installing");
    t.install = tr(id, "about.install");
    t.restart = None;
    let l = id.to_owned();
    t.failure = Rc::new(move |e| crate::lang::err_in(&l, e));
    t
}

fn busy(u: &Updater) -> bool {
    matches!(
        u.state.get_untracked().ok(),
        Some(UpdateState::Checking | UpdateState::Downloading(_) | UpdateState::Ready)
    )
}

fn check(st: &St, u: &Rc<Updater>) {
    if busy(u) {
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

/// Downloads (showing how far it got), verifies the signature, then installs and restarts.
fn install(st: &St, u: &Rc<Updater>) {
    let Some(rel) = u.release.borrow().clone() else {
        return;
    };
    u.state.set(UpdateState::Downloading(None));
    let progress = Arc::new((AtomicU64::new(0), AtomicU64::new(0)));
    poll(st, u, progress.clone());
    let (u2, s) = (u.clone(), st.clone());
    let dir = std::env::temp_dir().join("SecretFolder-update");
    st.app.background(
        move || {
            up::download(&config(), &rel, &dir, |got, total| {
                progress.0.store(got, Ordering::Relaxed);
                progress.1.store(total.unwrap_or(0), Ordering::Relaxed);
            })
        },
        move |r| match r {
            Ok(file) => {
                *u2.file.borrow_mut() = Some(file);
                u2.state.set(UpdateState::Ready);
                restart(&s, &u2);
            }
            Err(e) => u2.state.set(UpdateState::Failed(e.to_string())),
        },
    );
}

/// Shows the download's progress while it runs.
fn poll(st: &St, u: &Rc<Updater>, progress: Arc<(AtomicU64, AtomicU64)>) {
    if !matches!(u.state.get_untracked().ok(), Some(UpdateState::Downloading(_))) {
        return;
    }
    let (got, total) = (progress.0.load(Ordering::Relaxed), progress.1.load(Ordering::Relaxed));
    let pct = (total > 0).then(|| (got as f32 / total as f32).min(1.0));
    u.state.set_if_changed(UpdateState::Downloading(pct));
    let (s, u) = (st.clone(), u.clone());
    st.app.after(Duration::from_millis(150), move || poll(&s, &u, progress));
}

fn restart(st: &St, u: &Rc<Updater>) {
    let Some(file) = u.file.borrow().clone() else {
        return;
    };
    // Lock first: the update replaces this program.
    state::lock(st);
    match up::apply(&file, &(ASSET.1)()) {
        Ok(()) => state::quit(st),
        Err(e) => u.state.set(UpdateState::Failed(e.to_string())),
    }
}

/// The update strip, built again in the new language after a switch (the update's state is
/// kept).
fn banner(st: &St, ui: &Ui, u: &Rc<Updater>) -> El {
    let slot = w::column(ui).fill_width();
    let (s2, st2, u2, up2, l) = (slot.clone(), st.clone(), ui.clone(), u.clone(), st.lang);
    slot.bind(move || {
        let id = l.id();
        untrack(|| {
            s2.clear();
            let (a, b, c, d) = (st2.clone(), up2.clone(), st2.clone(), up2.clone());
            let strip = up::update_banner_with(&u2, up2.state, &texts(&id), move || install(&a, &b), move || restart(&c, &d));
            let _ = s2.clone().child(strip);
        });
    })
}

pub fn about(st: &St, ui: &Ui) -> El {
    let unit = ui.theme().spacing.unit;
    let l: Lang = st.lang;
    let u = Rc::new(Updater {
        state: Signal::new(UpdateState::Idle),
        release: RefCell::new(None),
        file: RefCell::new(None),
    });
    let link_error: Signal<Option<String>> = Signal::new(None);
    let mut links = w::wrap(ui).gap(unit * 2.0);
    for (i, (key, url)) in LINKS.iter().enumerate() {
        let s = st.clone();
        let b = w::button(ui, l.k(key), move || {
            let s2 = s.clone();
            s.app.open_url(*url, move |r| link_error.set(r.err().map(|e| s2.lang.err(&e.reason))));
        });
        links = links.child(if i == 0 { b.variant("primary") } else { b });
    }
    let uc = ui.clone();
    let err = remedy_ui::show(ui, move || link_error.with(Option::is_some), move || {
        w::text(&uc, "").variant("error").text(move || link_error.get().unwrap_or_default())
    })
    .el();
    let state = u.state;
    let (s3, u3) = (st.clone(), u.clone());
    let check_label = move || {
        tr(&l.id(), if matches!(state.get(), UpdateState::Checking) { "about.checking" } else { "about.check" })
    };
    let check_btn = w::button(ui, check_label, move || check(&s3, &u3))
        .disabled(move || matches!(state.get(), UpdateState::Checking | UpdateState::Downloading(_) | UpdateState::Ready));
    let tagline = move || {
        let id = l.id();
        format!(
            "{} · {}",
            crate::lang::trv(&id, "port.tagline", &[("version", env!("CARGO_PKG_VERSION"))]),
            tr(&id, "port.builtWith")
        )
    };
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
                        remedy_ui::image::image(ui, Source::Bytes(Arc::from(crate::explorer::MARK)), "", Fit::Contain)
                            .width(36.0)
                            .height(36.0),
                    )
                    .child(
                        w::column(ui)
                            .child(w::heading(ui, 1, l.k("about.title")))
                            .child(w::text(ui, l.k("about.hello"))),
                    ),
            )
            .child(w::text(ui, tagline).variant("muted"))
            .child(w::text(ui, l.k("about.network")).variant("muted"))
            .child(links)
            .child(err)
            .child(check_btn)
            .child(banner(st, ui, &u)),
    )
}
