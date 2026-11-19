//! The whole app on the headless host, driven the way a screen reader would (by the
//! accessibility tree's labels and actions) and, for dragging, by the pointer. Every test
//! uses a vault in a temporary folder.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use remedy_ui::core::Point;
use remedy_ui::host::accesskit::{Action, ActionRequest, Node, NodeId, TreeId};
use remedy_ui::host::{
    AccessibilityEvent, DragData, DragEvent, Host, HostEvent, TrayEvent, WindowId,
};
use remedy_ui::input::{
    Buttons, Key, KeyEvent, Modifiers, PointerEvent, PointerKind, PointerPhase,
};
use remedy_ui::{App, AppConfig, El, GeometryStore, Runner};
use remedy_ui_bridge_headless::HeadlessHost;

use crate::state::{Screen, St};
use crate::vault::ItemKind;

const PASSWORD: &str = "correct horse battery staple";

struct Driver {
    host: HeadlessHost,
    runner: Runner,
    st: St,
    trees: HashMap<WindowId, HashMap<NodeId, Node>>,
    roots: HashMap<WindowId, NodeId>,
    _dir: tempfile::TempDir,
}

impl Driver {
    fn new() -> Self {
        Self::on(true)
    }

    /// The app as `main` starts it, on the vault `SECRETFOLDER_VAULT` names (tests always set
    /// it, to a temporary copy).
    fn from_env(dir: tempfile::TempDir) -> Self {
        assert!(crate::state::vault_override().is_some(), "tests never open the person's own vault");
        let slot: Rc<RefCell<Option<St>>> = Rc::default();
        let s2 = slot.clone();
        let config = crate::fonts::with_inter(
            AppConfig::new("SecretFolder test").cpu_only().theme(crate::state::theme(), remedy_ui_themes::sheet),
        );
        let mut runner = Runner::with_geometry(config, GeometryStore::in_memory(), move |app: &App| {
            *s2.borrow_mut() = Some(crate::state::start_at(app, crate::state::vault_override()));
        });
        let mut host = HeadlessHost::new();
        host.run_frames(&mut runner, 1);
        let st = slot.borrow().clone().expect("the app started");
        let mut d = Driver { host, runner, st, trees: HashMap::new(), roots: HashMap::new(), _dir: dir };
        d.frames(2);
        d
    }

    /// The app on a system that can (or cannot) keep windows out of screen capture.
    fn on(capture_supported: bool) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("vault");
        let slot: Rc<RefCell<Option<St>>> = Rc::default();
        let s2 = slot.clone();
        let config = crate::fonts::with_inter(
            AppConfig::new("SecretFolder test").cpu_only().theme(crate::state::theme(), remedy_ui_themes::sheet),
        );
        let mut runner =
            Runner::with_geometry(config, GeometryStore::in_memory(), move |app: &App| {
                *s2.borrow_mut() = Some(crate::state::start_at(app, Some(root)));
            });
        let mut host = HeadlessHost::new();
        host.set_capture_supported(capture_supported);
        host.run_frames(&mut runner, 1);
        let st = slot.borrow().clone().expect("the app started");
        let mut d = Driver {
            host,
            runner,
            st,
            trees: HashMap::new(),
            roots: HashMap::new(),
            _dir: dir,
        };
        d.frames(2);
        d
    }

    fn frames(&mut self, n: usize) {
        self.host.run_frames(&mut self.runner, n);
        for id in self.host.window_ids() {
            let tree = self.trees.entry(id).or_default();
            if tree.is_empty() {
                self.host
                    .send(HostEvent::Accessibility(id, AccessibilityEvent::Requested));
                self.host.run_frames(&mut self.runner, 1);
            }
            for u in self.host.take_accessibility(id) {
                if let Some(t) = &u.tree {
                    self.roots.insert(id, t.root);
                }
                for (nid, node) in u.nodes {
                    tree.insert(nid, node);
                }
            }
            if let Some(root) = self.roots.get(&id).copied() {
                let mut live = std::collections::HashSet::new();
                let mut todo = vec![root];
                while let Some(n) = todo.pop() {
                    if live.insert(n)
                        && let Some(node) = tree.get(&n)
                    {
                        todo.extend(node.children().iter().copied());
                    }
                }
                tree.retain(|k, _| live.contains(k));
            }
        }
    }

    /// Runs frames until `done`, letting background work (Argon2, image decoding) finish.
    fn until(&mut self, what: &str, done: impl Fn(&Driver) -> bool) {
        for _ in 0..3000 {
            self.frames(1);
            if done(self) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!(
            "timed out waiting for {what}: screen {:?}, error {:?}",
            self.screen(),
            self.st.error.get_untracked()
        );
    }

    fn win(&self) -> WindowId {
        self.host.window_ids().first().copied().expect("the window")
    }

    fn ui(&self) -> remedy_ui::Ui {
        self.st
            .window
            .borrow()
            .as_ref()
            .and_then(|w| w.ui())
            .unwrap_or_else(|| panic!("the window is open"))
    }

    fn node(&self, label: &str, action: Action) -> NodeId {
        let tree = self.trees.get(&self.win()).expect("a tree for the window");
        let found = tree
            .iter()
            .find(|(_, n)| n.label() == Some(label) && n.supports_action(action));
        match found {
            Some((id, _)) => *id,
            None => {
                let labels: Vec<_> = tree.values().filter_map(|n| n.label()).collect();
                panic!("no {label:?} with {action:?} in {labels:?}")
            }
        }
    }

    fn has(&self, label: &str) -> bool {
        self.trees
            .get(&self.win())
            .is_some_and(|t| t.values().any(|n| n.label() == Some(label)))
    }

    fn act(&mut self, label: &str, action: Action) {
        let target = self.node(label, action);
        let req = ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: target,
            data: None,
        };
        let w = self.win();
        self.host
            .send(HostEvent::Accessibility(w, AccessibilityEvent::Action(req)));
        self.frames(2);
    }

    fn click(&mut self, label: &str) {
        self.act(label, Action::Click);
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) {
        let w = self.win();
        self.host.send(HostEvent::Key(
            w,
            KeyEvent::press(key).with_modifiers(modifiers).with_text(None),
        ));
        self.frames(2);
    }

    /// Types into a field by focusing it, selecting what is there and pasting.
    fn type_into(&mut self, label: &str, text: &str) {
        self.act(label, Action::Focus);
        self.key(
            Key::Character("a".into()),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
        );
        let _ = self.host.set_clipboard_text(text, false);
        self.key(
            Key::Character("v".into()),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
        );
    }

    fn screen(&self) -> Screen {
        self.st.screen.get_untracked().expect("screen signal")
    }

    fn error(&self) -> Option<String> {
        self.st.error.get_untracked().ok().flatten()
    }

    /// Every control assistive technology can press or focus has a name, and the engine's
    /// accessibility check finds nothing.
    fn assert_accessible(&self) {
        let tree = self.trees.get(&self.win()).expect("a tree");
        let nameless: Vec<String> = tree
            .values()
            .filter(|n| n.supports_action(Action::Click) || n.supports_action(Action::Focus))
            // A grid row is read through its cells (ARIA and UIA name it from them).
            .filter(|n| n.role() != remedy_ui::host::accesskit::Role::Row)
            .filter(|n| n.label().is_none_or(|l| l.trim().is_empty()))
            .map(|n| format!("{:?}", n.role()))
            .collect();
        assert!(nameless.is_empty(), "unnamed controls: {nameless:?}");
        let ui = self.ui();
        let found: Vec<String> = remedy_ui_devtools::audit(&ui)
            .iter()
            .map(|p| {
                format!(
                    "{} [{}]",
                    p.message(),
                    ui.element(p.element)
                        .info()
                        .map(|i| remedy_ui_devtools::describe(&i))
                        .unwrap_or_default()
                )
            })
            .collect();
        assert!(found.is_empty(), "{found:#?}");
    }

    /// Creates the vault and goes through the recovery key to the explorer.
    fn set_up(&mut self) {
        self.type_into("Master password", PASSWORD);
        self.type_into("Confirm password", PASSWORD);
        self.click("Create vault");
        self.until("the recovery key", |d| d.screen() == Screen::Recovery);
        self.frames(2);
        self.click("I saved this recovery key somewhere safe");
        self.click("Continue to vault");
        self.until("the explorer", |d| d.screen() == Screen::Unlocked);
        self.frames(2);
    }

    fn names(&self) -> Vec<String> {
        self.st
            .items
            .get_untracked()
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.name)
            .collect()
    }

    fn id_of(&self, name: &str) -> String {
        self.st
            .items
            .get_untracked()
            .unwrap_or_default()
            .into_iter()
            .find(|i| i.name == name)
            .map(|i| i.id)
            .expect("the item")
    }

    /// The table row showing `name`.
    fn row(&self, name: &str) -> El {
        let ui = self.ui();
        let mut todo = vec![ui.root()];
        while let Some(e) = todo.pop() {
            if e.info().is_some_and(|i| i.widget == "table_row")
                && texts(&e).iter().any(|t| t == name)
            {
                return e;
            }
            todo.extend(e.child_elements());
        }
        panic!("no row for {name}")
    }

    fn pointer(&mut self, at: Point, phase: PointerPhase, down: bool) {
        let w = self.win();
        self.host.send(HostEvent::Pointer(
            w,
            PointerEvent::new(phase, at).with_kind(PointerKind::Mouse).with_buttons(if down { Buttons::PRIMARY } else { Buttons::default() }),
        ));
    }
}

fn texts(e: &El) -> Vec<String> {
    let mut out = Vec::new();
    let mut todo = vec![e.clone()];
    while let Some(e) = todo.pop() {
        if let Some(t) = e.info().and_then(|i| i.text) {
            out.push(t);
        }
        todo.extend(e.child_elements());
    }
    out
}

/// A point on a row's name (away from the row's buttons), half way down.
fn centre(e: &El) -> Point {
    let b = e.bounds().expect("laid out");
    Point::new(b.min.x + 80.0, (b.min.y + b.max.y) / 2.0)
}

#[test]
fn setup_shows_the_recovery_key_once_and_opens_the_vault() {
    let mut d = Driver::new();
    assert_eq!(d.screen(), Screen::Setup);
    d.assert_accessible();
    assert!(
        d.host
            .window(d.win())
            .is_some_and(|w| w.options().capture_protected),
        "hidden from screen capture"
    );
    assert!(
        d.host.tray().is_some_and(|t| t.tooltip == "SecretFolder"),
        "tray icon shown"
    );
    // Short and mismatched passwords are refused.
    d.type_into("Master password", "short");
    d.type_into("Confirm password", "short");
    d.click("Create vault");
    assert_eq!(
        d.error().as_deref(),
        Some("Password must be at least 12 characters.")
    );
    d.type_into("Master password", PASSWORD);
    d.type_into("Confirm password", "something else entirely");
    d.click("Create vault");
    assert_eq!(d.error().as_deref(), Some("Passwords do not match."));
    d.type_into("Master password", PASSWORD);
    d.type_into("Confirm password", PASSWORD);
    d.click("Create vault");
    d.until("the recovery key", |d| d.screen() == Screen::Recovery);
    d.frames(2);
    d.assert_accessible();
    let key =
        d.st.recovery_key
            .borrow()
            .as_ref()
            .map(|k| k.to_string())
            .expect("a recovery key");
    d.click("Copy recovery key");
    assert_eq!(
        d.host.clipboard(),
        Some((key.as_str(), true)),
        "copied as a secret"
    );
    assert!(d.has("Copied"));
    // Continue waits for the confirmation.
    d.click("I saved this recovery key somewhere safe");
    d.click("Continue to vault");
    d.until("the explorer", |d| d.screen() == Screen::Unlocked);
    assert!(d.st.recovery_key.borrow().is_none(), "the key is forgotten");
    d.frames(2);
    d.assert_accessible();
    // Locking clears the copied key from the clipboard.
    d.click("Lock");
    assert_eq!(d.screen(), Screen::Unlock);
    assert!(
        d.host.clipboard().is_none_or(|(t, _)| t.is_empty()),
        "the key left the clipboard"
    );
    // The recovery key unlocks.
    d.click("Use recovery key");
    d.type_into("Recovery key", &format!("  {key}  "));
    d.click("Unlock with recovery key");
    d.until("unlock", |d| d.screen() == Screen::Unlocked);
}

#[test]
fn folders_text_import_export_rename_and_delete() {
    let mut d = Driver::new();
    d.set_up();
    // A folder, through the name dialog.
    d.click("New folder");
    d.type_into("Folder name", "Taxes");
    d.click("Create");
    assert_eq!(d.names(), vec!["Taxes"]);
    // A text note: written, saved with Ctrl+S, encrypted on disk.
    d.click("New text");
    assert!(d.names().contains(&"Untitled.txt".to_owned()));
    d.type_into("Text", "the safe code is 4711");
    assert!(d.st.dirty.get_untracked().unwrap_or(false));
    d.key(
        Key::Character("s".into()),
        Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    );
    assert!(!d.st.dirty.get_untracked().unwrap_or(true), "saved");
    let id = d.id_of("Untitled.txt");
    let text = d.st.with(|v| v.get_item(&id)).expect("item").text;
    assert_eq!(text.as_deref(), Some("the safe code is 4711"));
    let on_disk: Vec<u8> = walk(&d._dir.path().join("vault"))
        .into_iter()
        .flat_map(|p| std::fs::read(p).unwrap_or_default())
        .collect();
    assert!(
        !String::from_utf8_lossy(&on_disk).contains("safe code"),
        "nothing plain on disk"
    );
    // Copy clears itself.
    d.click("Copy");
    assert_eq!(d.host.clipboard(), Some(("the safe code is 4711", true)));
    d.host.advance(Duration::from_secs(31));
    d.frames(1);
    assert!(
        d.host.clipboard().is_none_or(|(t, _)| t.is_empty()),
        "cleared after 30 s"
    );
    // Import through the dialog; export through the save dialog.
    let src = d._dir.path().join("photo-notes.txt");
    std::fs::write(&src, b"imported bytes").expect("write");
    d.host.answer_dialog(vec![src.clone()]);
    d.click("Import…");
    assert!(
        d.names().contains(&"photo-notes.txt".to_owned()),
        "{:?}",
        d.names()
    );
    let out = d._dir.path().join("out.txt");
    d.host.answer_dialog(vec![out.clone()]);
    d.click("Export");
    assert_eq!(
        std::fs::read_to_string(&out).ok().as_deref(),
        Some("the safe code is 4711")
    );
    // Rename with F2 (on the list), through the dialog.
    let row = d.row("photo-notes.txt");
    let c = centre(&row);
    d.pointer(c, PointerPhase::Down, true);
    d.pointer(c, PointerPhase::Up, false);
    d.frames(2);
    d.key(Key::F(2), Modifiers::NONE);
    d.type_into("New name", "renamed.txt");
    d.click("Rename");
    assert!(
        d.names().contains(&"renamed.txt".to_owned()),
        "{:?}",
        d.names()
    );
    // Delete asks first; a folder with things in it says so.
    let taxes = d.id_of("Taxes");
    let note = d.id_of("Untitled.txt");
    d.st.with(|v| v.move_item(&note, Some(taxes.clone())))
        .expect("move");
    crate::state::changed(&d.st);
    d.frames(2);
    let row = d.row("Taxes");
    let c = centre(&row);
    d.pointer(c, PointerPhase::Down, true);
    d.pointer(c, PointerPhase::Up, false);
    d.frames(2);
    d.key(Key::Delete, Modifiers::NONE);
    assert!(d.has("Folder is not empty"), "the warning");
    d.click("Delete folder & contents");
    assert_eq!(d.names(), vec!["renamed.txt"]);
    assert_eq!(
        d.st.with(|v| Ok(v.status().item_count)).ok(),
        Some(1),
        "the note inside went too"
    );
}

#[test]
fn dragging_moves_items_and_files_from_other_apps_import() {
    let mut d = Driver::new();
    d.set_up();
    d.click("New folder");
    d.type_into("Folder name", "Inbox");
    d.click("Create");
    d.click("New text");
    d.frames(2);
    // Drag the note onto the folder row.
    let (from, to) = (centre(&d.row("Untitled.txt")), centre(&d.row("Inbox")));
    d.pointer(from, PointerPhase::Down, true);
    d.pointer(Point::new(from.x, from.y + 10.0), PointerPhase::Move, true);
    d.pointer(to, PointerPhase::Move, true);
    d.pointer(to, PointerPhase::Up, false);
    d.frames(2);
    assert_eq!(d.names(), vec!["Inbox"], "moved into the folder");
    let inbox = d.id_of("Inbox");
    let inside =
        d.st.with(|v| v.list_items_in(Some(inbox.clone())))
            .expect("list");
    assert_eq!(
        inside.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        vec!["Untitled.txt"]
    );
    // Files dropped from another app on the folder row are imported into it.
    let file = d._dir.path().join("receipt.txt");
    std::fs::write(&file, b"paid").expect("write");
    let at = centre(&d.row("Inbox"));
    let w = d.win();
    d.host.send(HostEvent::Drag(
        w,
        DragEvent::Enter {
            position: Some(at),
            data: DragData::files(vec![file.clone()]),
        },
    ));
    d.frames(1);
    assert_eq!(d.host.drop_effect(w), remedy_ui::host::DropEffect::Copy);
    d.host.send(HostEvent::Drag(
        w,
        DragEvent::Drop {
            position: Some(at),
            data: DragData::files(vec![file]),
        },
    ));
    d.frames(2);
    let inside =
        d.st.with(|v| v.list_items_in(Some(inbox.clone())))
            .expect("list");
    assert!(
        inside
            .iter()
            .any(|i| i.name == "receipt.txt" && i.kind == ItemKind::Text),
        "{inside:?}"
    );
    // Open the folder (Enter on the row), then drag the note up onto "Vault" in the path.
    let c = centre(&d.row("Inbox"));
    d.pointer(c, PointerPhase::Down, true);
    d.pointer(c, PointerPhase::Up, false);
    d.frames(1);
    d.key(Key::Enter, Modifiers::NONE);
    assert_eq!(d.st.folder.get_untracked().ok().flatten(), Some(inbox));
    let note = centre(&d.row("Untitled.txt"));
    let vault_crumb = d.node("Vault", Action::Click);
    let b = d
        .trees
        .get(&d.win())
        .and_then(|t| t.get(&vault_crumb))
        .and_then(|n| n.bounds())
        .expect("crumb bounds");
    let crumb = Point::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
    d.pointer(note, PointerPhase::Down, true);
    d.pointer(Point::new(note.x, note.y + 10.0), PointerPhase::Move, true);
    d.pointer(crumb, PointerPhase::Move, true);
    d.pointer(crumb, PointerPhase::Up, false);
    d.frames(2);
    let top = d.st.with(|v| v.list_items_in(None)).expect("list");
    assert!(
        top.iter().any(|i| i.name == "Untitled.txt"),
        "moved up: {top:?}"
    );
}

#[test]
fn wrong_passwords_are_throttled_and_settings_tray_and_links_work() {
    let mut d = Driver::new();
    d.set_up();
    // Settings: the idle lock and the password.
    d.click("Settings");
    d.type_into("Idle lock (minutes)", "30");
    d.click("Save");
    assert!(d.has("Idle lock set to 30 min."));
    assert_eq!(
        d.st.with(|v| Ok(v.status().idle_lock_secs)).ok(),
        Some(1800)
    );
    d.type_into("Current password", PASSWORD);
    d.type_into("New password", "a brand new passphrase");
    d.type_into("Confirm new password", "a brand new passphrase");
    d.click("Change password");
    d.until("the change", |d| d.has("Password changed."));
    // About: links open through the system, only to the project's pages.
    d.click("About");
    d.click("Repo");
    assert_eq!(
        d.host.opened_urls(),
        ["https://github.com/AhmiDarrow/SecretFolder"]
    );
    // The tray locks; five wrong passwords bring a pause.
    d.host.send(HostEvent::Tray(TrayEvent::Menu(2)));
    d.frames(2);
    assert_eq!(d.screen(), Screen::Unlock);
    for _ in 0..5 {
        d.type_into("Master password", "not the password at all");
        d.click("Unlock");
        d.until("the refusal", |d| {
            d.error().is_some() && !d.st.busy.get_untracked().unwrap_or(true)
        });
    }
    d.type_into("Master password", "a brand new passphrase");
    d.click("Unlock");
    d.frames(2);
    assert!(
        d.error()
            .is_some_and(|e| e.contains("too many unlock attempts")),
        "{:?}",
        d.error()
    );
    assert_eq!(d.screen(), Screen::Unlock);
    // Idle time locks too (once unlocked again after the pause).
    d.st.throttle.borrow_mut().record_success();
    d.type_into("Master password", "a brand new passphrase");
    d.click("Unlock");
    d.until("unlock", |d| d.screen() == Screen::Unlocked);
    d.host.advance(Duration::from_secs(1801));
    d.frames(2);
    assert_eq!(d.screen(), Screen::Unlock, "locked after 30 idle minutes");
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                todo.push(p)
            } else {
                out.push(p)
            }
        }
    }
    out
}

#[test]
fn images_preview_search_filters_and_unsaved_text_is_never_lost_silently() {
    let mut d = Driver::new();
    d.set_up();
    // A PNG imported shows as an image, decoded off the UI thread.
    let png = d._dir.path().join("scan.png");
    let mut raw = Vec::new();
    for y in 0..8u8 {
        for x in 0..8u8 {
            raw.extend_from_slice(&[x * 30, y * 30, 200, 255]);
        }
    }
    let mut bytes = Vec::new();
    let enc = image::codecs::png::PngEncoder::new(&mut bytes);
    image::ImageEncoder::write_image(enc, &raw, 8, 8, image::ExtendedColorType::Rgba8)
        .expect("encode");
    std::fs::write(&png, &bytes).expect("write");
    d.host.answer_dialog(vec![png]);
    d.click("Import…");
    assert_eq!(
        d.st.items
            .get_untracked()
            .ok()
            .and_then(|v| v.first().map(|i| i.kind)),
        Some(ItemKind::Image)
    );
    let c = centre(&d.row("scan.png"));
    d.pointer(c, PointerPhase::Down, true);
    d.pointer(c, PointerPhase::Up, false);
    d.until("the preview", |d| {
        d.trees.get(&d.win()).is_some_and(|t| {
            t.values().any(|n| {
                n.role() == remedy_ui::host::accesskit::Role::Image && n.label() == Some("scan.png")
            })
        })
    });
    // And its pixels decoded (off the UI thread, delivered to the app by the runner).
    d.until("the decoded pixels", |d| {
        let ui = d.ui();
        let mut todo = vec![ui.root()];
        while let Some(e) = todo.pop() {
            if e.info().is_some_and(|i| {
                i.widget == "image"
                    && i.variant.is_empty()
                    && i.semantics
                        .as_ref()
                        .is_some_and(|s| s.name() == Some("scan.png"))
            }) {
                return true;
            }
            todo.extend(e.child_elements());
        }
        false
    });
    assert!(
        d.has("image/png · 0.1 KB")
            || d.trees.get(&d.win()).is_some_and(|t| t
                .values()
                .any(|n| n.label().is_some_and(|l| l.starts_with("image/png"))))
    );
    // Search narrows the folder by name.
    d.click("New text");
    d.type_into("Search this folder…", "SCAN");
    d.frames(2);
    assert!(d.row("scan.png").bounds().is_some());
    let shown_rows = {
        let ui = d.ui();
        let mut n = 0;
        let mut todo = vec![ui.root()];
        while let Some(e) = todo.pop() {
            if e.info().is_some_and(|i| i.widget == "table_row") && !texts(&e).is_empty() {
                n += 1;
            }
            todo.extend(e.child_elements());
        }
        n
    };
    assert_eq!(shown_rows, 1, "only the match shows");
    d.type_into("Search this folder…", "");
    d.frames(2);
    // Unsaved text: picking another item asks; Stay keeps the edits.
    let c = centre(&d.row("Untitled.txt"));
    d.pointer(c, PointerPhase::Down, true);
    d.pointer(c, PointerPhase::Up, false);
    d.frames(2);
    d.type_into("Text", "draft that must survive");
    let other = centre(&d.row("scan.png"));
    d.pointer(other, PointerPhase::Down, true);
    d.pointer(other, PointerPhase::Up, false);
    d.frames(2);
    assert!(d.has("Unsaved changes"), "asked before leaving");
    d.click("Stay");
    assert!(d.st.dirty.get_untracked().unwrap_or(false), "still dirty");
    d.click("Save");
    let id = d.id_of("Untitled.txt");
    assert_eq!(
        d.st.with(|v| v.get_item(&id))
            .expect("item")
            .text
            .as_deref(),
        Some("draft that must survive")
    );
}

#[test]
fn every_screen_fits_the_smallest_window() {
    let mut d = Driver::new();
    let w = d.win();
    d.host.user_resize(w, remedy_ui::core::Size::new(560.0, 420.0));
    d.frames(3);
    d.assert_accessible();
    d.set_up();
    d.frames(2);
    d.assert_accessible();
    for tab in ["Settings", "About", "Files"] {
        d.click(tab);
        d.frames(2);
        d.assert_accessible();
    }
}

#[test]
fn settings_say_when_the_system_cannot_hide_the_vault_from_capture() {
    for supported in [true, false] {
        let mut d = Driver::on(supported);
        d.set_up();
        d.click("Settings");
        d.frames(2);
        let note = "This system cannot keep SecretFolder out of screenshots or screen sharing. Lock the vault before sharing your screen.";
        assert_eq!(d.has(note), !supported, "capture supported: {supported}");
    }
}

/// Writes what the window shows to `SECRETFOLDER_SHOTS` (a folder), for looking at the
/// design: `SECRETFOLDER_SHOTS=dir cargo test shots -- --ignored`.
#[test]
#[ignore = "writes screenshots for review"]
fn shots() {
    let Some(dir) = std::env::var_os("SECRETFOLDER_SHOTS").map(std::path::PathBuf::from) else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let shoot = |d: &mut Driver, name: &str, size: Option<(f32, f32)>| {
        if let Some((w, h)) = size {
            let win = d.win();
            d.host.user_resize(win, remedy_ui::core::Size::new(w, h));
            d.frames(3);
        }
        let w = d.host.window(d.win()).expect("window");
        let (width, height, _) = w.size();
        let rgba = w.surface().to_vec();
        let _ = image::save_buffer(dir.join(format!("{name}.png")), &rgba, width, height, image::ExtendedColorType::Rgba8);
    };
    // The shipped app's sample vault (the same one the before-pictures show).
    let mut d = Driver::new();
    let win = d.win();
    d.host.user_resize(win, remedy_ui::core::Size::new(920.0, 640.0));
    d.frames(3);
    shoot(&mut d, "01-setup", None);
    d.type_into("Master password", "short");
    d.type_into("Confirm password", "short");
    d.click("Create vault");
    shoot(&mut d, "01-setup-short-password", None);
    d.type_into("Master password", PASSWORD);
    d.type_into("Confirm password", PASSWORD);
    d.click("Create vault");
    d.until("the recovery key", |d| d.screen() == Screen::Recovery);
    d.frames(3);
    shoot(&mut d, "02-recovery-key", None);
    d.click("I saved this recovery key somewhere safe");
    d.click("Continue to vault");
    d.until("the explorer", |d| d.screen() == Screen::Unlocked);
    d.until("the mark", |_| remedy_ui_media_image::cached() > 0);
    d.frames(3);
    shoot(&mut d, "04-files-empty", None);
    let png = include_bytes!("../tests/fixtures/sample.png");
    let docs = d.st.with(|v| v.create_folder("Documents", None)).expect("folder");
    let taxes = d.st.with(|v| v.create_folder("Taxes 2026", Some(docs.id.clone()))).expect("folder");
    d.st.with(|v| v.create_folder("Empty folder", None)).expect("folder");
    d.st.with(|v| v.create_text("Passwords.txt", "bank: sample-not-real\nemail: sample\n", None)).expect("text");
    d.st.with(|v| v.create_text("Wi-Fi notes.md", "Guest network: sample\n", Some(docs.id.clone()))).expect("text");
    d.st.with(|v| v.import_bytes("photo.png".into(), "image/png".into(), png.to_vec(), Some(docs.id.clone()))).expect("png");
    d.st.with(|v| v.import_bytes("backup.bin".into(), "application/octet-stream".into(), vec![7; 70_000], Some(taxes.id.clone())))
        .expect("bin");
    crate::state::changed(&d.st);
    d.frames(3);
    shoot(&mut d, "04-files", None);
    shoot(&mut d, "04-files-narrow", Some((720.0, 480.0)));
    shoot(&mut d, "04-files-wide", Some((1400.0, 900.0)));
    let win = d.win();
    d.host.user_resize(win, remedy_ui::core::Size::new(920.0, 640.0));
    d.frames(3);
    let c = centre(&d.row("Passwords.txt"));
    d.pointer(c, PointerPhase::Down, true);
    d.pointer(c, PointerPhase::Up, false);
    d.frames(3);
    shoot(&mut d, "06-text-open", None);
    d.click("Open folder Documents");
    d.frames(3);
    shoot(&mut d, "05-folder", None);
    let c = centre(&d.row("photo.png"));
    d.pointer(c, PointerPhase::Down, true);
    d.pointer(c, PointerPhase::Up, false);
    d.until("the preview", |_| remedy_ui_media_image::cached() > 1);
    d.frames(3);
    shoot(&mut d, "07-image-open", None);
    d.click("Open folder Taxes 2026");
    let c = centre(&d.row("backup.bin"));
    d.pointer(c, PointerPhase::Down, true);
    d.pointer(c, PointerPhase::Up, false);
    d.frames(3);
    shoot(&mut d, "08-binary-open", None);
    d.click("Go to parent folder");
    d.click("Go to parent folder");
    d.click("New folder");
    shoot(&mut d, "09-new-folder-dialog", None);
    d.click("Cancel");
    d.click("Remove folder Documents");
    shoot(&mut d, "10-delete-folder-dialog", None);
    d.click("Cancel");
    d.click("Remove Passwords.txt");
    shoot(&mut d, "10-delete-item-dialog", None);
    d.click("Cancel");
    d.click("Settings");
    d.frames(2);
    shoot(&mut d, "11-settings", None);
    d.click("About");
    d.frames(2);
    shoot(&mut d, "12-about", None);
    d.click("Lock");
    d.frames(2);
    shoot(&mut d, "03-unlock", None);
    d.type_into("Master password", "not the password at all");
    d.click("Unlock");
    d.until("the refusal", |d| d.error().is_some());
    d.frames(2);
    shoot(&mut d, "03-unlock-wrong-password", None);
    d.click("Use recovery key");
    shoot(&mut d, "03-unlock-recovery-key", None);
}

/// Copies a folder tree.
fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from).expect("read dir").flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).expect("copy");
        }
    }
}

/// Every item under `parent`, by path, read through the vault code.
fn tree_of(v: &mut crate::vault::Vault, parent: Option<String>, prefix: &str, out: &mut Vec<(String, ItemKind, String, Vec<u8>)>) {
    for i in v.list_items_in(parent).expect("list") {
        let path = format!("{prefix}/{}", i.name);
        if i.kind == ItemKind::Folder {
            out.push((path.clone(), i.kind, i.mime.clone(), Vec::new()));
            tree_of(v, Some(i.id.clone()), &path, out);
        } else {
            let (_, bytes) = v.export_bytes(&i.id).expect("export");
            out.push((path, i.kind, i.mime.clone(), bytes));
        }
    }
}

/// Vaults written by shipped releases (0.1.0, the first, and 0.1.7, the last Tauri release)
/// open in this build through `SECRETFOLDER_VAULT`, show their folders and files, take edits,
/// and read back afterwards through the shipped vault code with nothing lost. The fixtures and
/// their expected contents were made by `compat/shipped.sh` with the release tags' own code.
#[test]
fn shipped_vaults_open_edit_and_reopen_without_loss() {
    for (tag, pw) in [("v0.1.0", "old-pass9"), ("v0.1.7", PASSWORD)] {
        shipped_vault_round_trip(tag, pw);
    }
}

fn shipped_vault_round_trip(tag: &str, pw: &str) {
    use base64::Engine;
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("vault");
    copy_tree(&fixtures.join(format!("vault-{tag}")), &root);
    let recovery = std::fs::read_to_string(fixtures.join(format!("{tag}.recovery"))).expect("recovery key");
    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures.join(format!("vault-{tag}.expected.json"))).expect("expected"),
    )
    .expect("json");
    let want: Vec<(String, String, Vec<u8>)> = expected["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|i| {
            let data = i["data_b64"].as_str().map(|b| base64::engine::general_purpose::STANDARD.decode(b).expect("b64"));
            (i["path"].as_str().unwrap_or_default().to_owned(), i["mime"].as_str().unwrap_or_default().to_owned(), data.unwrap_or_default())
        })
        .collect();

    // SAFETY: the only test that touches SECRETFOLDER_VAULT; set and read on this thread
    // before the app starts.
    unsafe { std::env::set_var("SECRETFOLDER_VAULT", &root) };
    let mut d = Driver::from_env(dir);
    unsafe { std::env::remove_var("SECRETFOLDER_VAULT") };
    assert_eq!(d.screen(), Screen::Unlock, "{tag}: an existing vault asks for its password");
    d.type_into("Master password", pw);
    d.click("Unlock");
    d.until("unlock", |d| d.screen() == Screen::Unlocked);
    d.frames(2);
    let mut top = d.names();
    top.sort();
    assert_eq!(top, vec!["Documents", "Empty folder", "Passwords.txt"], "{tag}: the top level");
    d.assert_accessible();
    assert_eq!(d.st.status.get_untracked().ok().flatten().map(|s| s.idle_lock_secs), Some(1800), "{tag}: idle lock kept");

    // Into Documents with its row's Open button; the image and text show.
    d.click("Open folder Documents");
    let mut docs = d.names();
    docs.sort();
    assert_eq!(docs, vec!["Taxes 2026", "final.txt", "photo.png", "Ünïcödé 日本語 — notes.md"], "{tag}: Documents");
    d.assert_accessible();
    // Back up through the ".." row.
    d.click("Go to parent folder");
    assert!(d.names().contains(&"Passwords.txt".to_owned()));

    // Edits: Passwords.txt gets new text; "Empty folder" is renamed; a file is imported;
    // the idle lock and the password change.
    let row = d.row("Passwords.txt");
    let c = centre(&row);
    d.pointer(c, PointerPhase::Down, true);
    d.pointer(c, PointerPhase::Up, false);
    d.frames(2);
    d.type_into("Text", "bank: changed-in-remedyui\n");
    d.key(Key::Character("s".into()), Modifiers { ctrl: true, ..Modifiers::NONE });
    assert!(!d.st.dirty.get_untracked().unwrap_or(true), "{tag}: saved");
    let row = d.row("Empty folder");
    let c = centre(&row);
    d.pointer(c, PointerPhase::Down, true);
    d.pointer(c, PointerPhase::Up, false);
    d.frames(2);
    d.key(Key::F(2), Modifiers::NONE);
    d.type_into("New name", "Archive");
    d.click("Rename");
    let src = d._dir.path().join("new-in-remedyui.txt");
    std::fs::write(&src, b"made after the move to RemedyUI").expect("write");
    d.host.answer_dialog(vec![src]);
    d.click("Import…");
    d.click("Settings");
    d.type_into("Idle lock (minutes)", "45");
    d.click("Save");
    let new_pw = "the new master password 2026";
    d.type_into("Current password", pw);
    d.type_into("New password", new_pw);
    d.type_into("Confirm new password", new_pw);
    d.click("Change password");
    d.until("the change", |d| d.has("Password changed."));
    d.click("Lock");
    assert_eq!(d.screen(), Screen::Unlock);
    let dir = std::mem::replace(&mut d._dir, tempfile::tempdir().expect("temp dir"));
    drop(d);

    // Read back with the shipped vault code (src-tauri/src/vault.rs), by the new password
    // and by the original recovery key.
    for by_recovery in [false, true] {
        let mut v = crate::vault::Vault::open_root(root.clone()).expect("the shipped code opens it");
        if by_recovery {
            v.unlock_with_recovery(recovery.trim()).expect("the recovery key still opens it");
        } else {
            v.unlock(new_pw).expect("the new password opens it");
        }
        assert_eq!(v.status().idle_lock_secs, 45 * 60, "{tag}: idle lock saved");
        let mut got = Vec::new();
        tree_of(&mut v, None, "", &mut got);
        got.sort_by(|a, b| a.0.cmp(&b.0));
        let paths: Vec<&str> = got.iter().map(|g| g.0.as_str()).collect();
        let mut want_paths: Vec<String> = want
            .iter()
            .map(|w| w.0.replace("/Empty folder", "/Archive"))
            .chain(["/new-in-remedyui.txt".to_owned()])
            .collect();
        want_paths.sort();
        assert_eq!(paths, want_paths.iter().map(String::as_str).collect::<Vec<_>>(), "{tag}: the tree");
        for (path, _, mime, bytes) in &got {
            match path.as_str() {
                "/Passwords.txt" => assert_eq!(bytes.as_slice(), b"bank: changed-in-remedyui\n"),
                "/new-in-remedyui.txt" => assert_eq!(bytes.as_slice(), b"made after the move to RemedyUI"),
                "/Archive" => {}
                p => {
                    let w = want.iter().find(|w| w.0 == p).unwrap_or_else(|| panic!("{tag}: {p}"));
                    assert_eq!(bytes, &w.2, "{tag}: {p} contents unchanged");
                    if !w.1.is_empty() {
                        assert_eq!(mime, &w.1, "{tag}: {p} type");
                    }
                }
            }
        }
        v.lock();
    }
    if let Some(out) = std::env::var_os("SECRETFOLDER_COMPAT_OUT") {
        copy_tree(&root, &std::path::PathBuf::from(out).join(format!("after-{tag}")));
    }
    drop(dir);
}
