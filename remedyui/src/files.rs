//! The Files tab: the folder's items (search, multi-select, breadcrumbs, drag and drop
//! inside the vault and from other apps), and the open item (text editor, image preview,
//! binary details) with rename, copy, export and delete.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use remedy_ui::core::Insets;
use remedy_ui::host::FileDialog;
use remedy_ui::input::Key;
use remedy_ui::layout::{Axis, Length};
use remedy_ui::reactive::{Effect, Signal, untrack};
use remedy_ui::view::{Align, Drag, DragData, DropEffect};
use remedy_ui::{El, TextField, Ui};
use remedy_ui_media_image::{Fit, Source};
use remedy_ui_widgets_core::{self as w, DialogButton};
use remedy_ui_widgets_data::{Column, SortValue, TableState, table};

use crate::error::AppError;
use crate::state::{self, St, checked};
use crate::vault::{ItemKind, ItemPreview, MAX_FILE_BYTES, guess_mime};

/// How long copied text stays on the clipboard.
const COPY_KEEP: Duration = Duration::from_secs(30);

/// Vault items being dragged inside the app.
struct Moving(Vec<String>);

/// The open item.
#[derive(Clone)]
struct Detail {
    item: ItemPreview,
    text: Option<String>,
    image: Option<Arc<[u8]>>,
}

/// A size the shipped way (`formatBytes`) in `id`.
pub fn size_text(id: &str, n: u64) -> String {
    crate::lang::bytes(id, n)
}

fn icon_for(kind: ItemKind) -> w::Icon {
    match kind {
        ItemKind::Folder => w::Icon::Folder,
        ItemKind::Text => w::Icon::Note,
        ItemKind::Image => w::Icon::Image,
        ItemKind::Binary => w::Icon::File,
    }
}

/// The kind as 0.2.0 names it (`files.kind*`), capitalized for the Kind column.
fn kind_name(id: &str, kind: ItemKind) -> String {
    let key = match kind {
        ItemKind::Folder => "files.kindFolder",
        ItemKind::Text => "files.kindText",
        ItemKind::Image => "files.kindImage",
        ItemKind::Binary => "files.kindBinary",
    };
    crate::lang::cap(&crate::lang::tr(id, key))
}

/// What a batch did: `(failed-all key, partial key, the partial key's count name)`.
struct Batch(&'static str, &'static str, &'static str);
const IMPORT: Batch = Batch("files.importFailed", "files.importedPartial", "imported");
const MOVE: Batch = Batch("files.moveFailed", "files.movedPartial", "moved");
const DELETE: Batch = Batch("files.deleteFailed", "files.deletedPartial", "deleted");

/// A batch's outcome, as 0.2.0 words it (and its first few reasons): nothing when all went
/// well.
fn batch_message(st: &St, b: &Batch, ok: usize, failures: &[String]) -> Option<String> {
    if failures.is_empty() {
        return None;
    }
    let l = st.lang;
    let head = if ok == 0 {
        l.t(b.0)
    } else {
        l.tv(b.1, &[(b.2, &l.num(ok as u64)), ("failed", &l.num(failures.len() as u64))])
    };
    Some(format!("{head} {}", failures.iter().take(3).cloned().collect::<Vec<_>>().join(" · ")))
}

/// Imports files from disk into `parent`, with the Tauri app's rules (where files may come
/// from, how large they may be).
pub fn import_paths(st: &St, paths: &[PathBuf], parent: Option<String>) {
    let (mut ok, mut failures) = (0usize, Vec::new());
    for p in paths {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let r = (|| {
            if !p.is_file() {
                return Err(AppError::Message("path is not a file".into()));
            }
            crate::policy::validate_import_path(p)?;
            if std::fs::metadata(p)?.len() > MAX_FILE_BYTES {
                return Err(AppError::TooLarge(MAX_FILE_BYTES));
            }
            let data = std::fs::read(p)?;
            let mime = guess_mime(&name);
            st.with(|v| v.import_bytes(name.clone(), mime, data, parent.clone()))
        })();
        match r {
            Ok(_) => ok += 1,
            Err(AppError::Locked) => {
                state::lock(st);
                return;
            }
            Err(e) => failures.push(format!("{name}: {}", st.lang.err(&e.to_string()))),
        }
    }
    state::changed(st);
    if let Some(m) = batch_message(st, &IMPORT, ok, &failures) {
        st.error.set(Some(m));
    }
}

/// Moves `ids` into `parent` (`None`: the top level).
fn move_items(st: &St, ids: &[String], parent: Option<String>) {
    let (mut ok, mut failures) = (0usize, Vec::new());
    for id in ids.iter().filter(|id| Some(*id) != parent.as_ref()) {
        match st.with(|v| v.move_item(id, parent.clone())) {
            Ok(_) => ok += 1,
            Err(AppError::Locked) => {
                state::lock(st);
                return;
            }
            Err(e) => failures.push(st.lang.err(&e.to_string())),
        }
    }
    state::changed(st);
    if let Some(m) = batch_message(st, &MOVE, ok, &failures) {
        st.error.set(Some(m));
    }
}

/// A drop of vault items or files on `target` (a folder id; `None` with `current` for the
/// folder shown): what it would do.
fn accept(st: &St, d: &Drag, target: Option<&ItemPreview>, here: Option<String>) -> DropEffect {
    let folder = match target {
        Some(t) if t.kind == ItemKind::Folder => Some(t.id.clone()),
        Some(_) => return DropEffect::None,
        None => here,
    };
    match d.local::<Moving>() {
        // Into a folder that is not one of them, and not where they already are.
        Some(Moving(ids)) => {
            let already = st
                .items
                .with_untracked(|items| {
                    ids.iter()
                        .all(|id| items.iter().any(|i| i.id == *id && i.parent_id == folder))
                })
                .unwrap_or(false);
            if folder.as_ref().is_some_and(|f| ids.contains(f)) || (target.is_none() && already) {
                DropEffect::None
            } else {
                DropEffect::Move
            }
        }
        None if !d.data.files.is_empty() => DropEffect::Copy,
        None => DropEffect::None,
    }
}

fn take_drop(st: &St, d: &Drag, folder: Option<String>) {
    match d.local::<Moving>() {
        Some(Moving(ids)) => move_items(st, ids, folder),
        None => import_paths(st, &d.data.files, folder),
    }
}

pub fn files(st: &St, ui: &Ui) -> El {
    let unit = ui.theme().spacing.unit;
    let query = Signal::new(String::new());
    let shown: Signal<Vec<ItemPreview>> = Signal::new(Vec::new());
    let (items, q2) = (st.items, query);
    let _ = Effect::new(move || {
        let q = q2.get().trim().to_lowercase();
        let list = items.get();
        shown.set(if q.is_empty() {
            list
        } else {
            list.into_iter()
                .filter(|i| i.name.to_lowercase().contains(&q))
                .collect()
        });
    });
    let tstate: TableState<String> = TableState::new(true);
    let detail: Signal<Option<Detail>> = Signal::new(None);

    // ---- the list
    tstate.set_drag(|ids| {
        Some(
            Drag::new(DragData::default())
                .with_local(Moving(ids.to_vec()))
                .allow(&[DropEffect::Move]),
        )
    });
    let (s1, s2) = (st.clone(), st.clone());
    let row_of = move |id: &String| {
        shown
            .with_untracked(|v| v.iter().find(|i| i.id == *id).cloned())
            .ok()
            .flatten()
    };
    let row_of2 = row_of;
    let here = st.folder;
    tstate.set_drop(
        move |d, k| {
            let target = k.and_then(&row_of);
            if k.is_some() && target.as_ref().is_none_or(|t| t.kind != ItemKind::Folder) {
                return DropEffect::None;
            }
            accept(&s1, d, target.as_ref(), here.get_untracked().ok().flatten())
        },
        move |d, k, _| {
            let folder = match k.and_then(&row_of2) {
                Some(t) => Some(t.id),
                None => here.get_untracked().ok().flatten(),
            };
            take_drop(&s2, d, folder);
        },
    );
    // The list is built again in the new language after a switch (its headings are fixed
    // when built); the selection, sort and scroll state live in `tstate` and are kept.
    let slot = w::column(ui)
        .grow(1.0)
        .fill_width()
        .layout(|l| l.min_height = Length::Px(0.0));
    let (sl, st_l, ui_l, ts_l, l) = (slot.clone(), st.clone(), ui.clone(), tstate.clone(), st.lang);
    let list = slot.bind(move || {
        let id = l.id();
        untrack(|| {
            sl.clear();
            let _ = sl.clone().child(item_table(&st_l, &ui_l, &ts_l, shown, &id));
        });
    });

    // ---- the open item follows the selection's cursor; moving away from unsaved text asks
    // first (Stay goes back to it).
    let (s4, sel, u4) = (st.clone(), tstate.selection.clone(), ui.clone());
    let _ = Effect::new(move || {
        let cursor = sel.cursor.get();
        let _ = s4.changed.get();
        untrack(|| {
            let open = detail
                .with_untracked(|d| d.as_ref().map(|d| d.item.id.clone()))
                .ok()
                .flatten();
            let leaving =
                open.is_some() && open != cursor && s4.dirty.get_untracked().unwrap_or(false);
            if !leaving {
                load_detail(&s4, cursor.as_deref(), detail);
                return;
            }
            let (s5, s6, sel2, open) = (
                s4.clone(),
                s4.clone(),
                sel.clone(),
                open.unwrap_or_default(),
            );
            let target = cursor.clone();
            let l = s4.lang;
            let _ = w::dialog(
                &u4,
                l.t("port.unsavedTitle"),
                l.t("files.unsavedConfirm"),
                vec![
                    DialogButton::new(l.t("port.stay"), "", move || sel2.select_only(open.clone())),
                    DialogButton::new(l.t("port.leave"), "danger", move || {
                        s5.dirty.set(false);
                        load_detail(&s6, target.as_deref(), detail);
                    }),
                ],
            );
        });
    });

    // ---- toolbar, selection bar, breadcrumbs, empty state
    let l = st.lang;
    let search = w::search(ui, l.t("files.search"), move |s| query.set(s));
    let (s5, s6, s7) = (st.clone(), st.clone(), st.clone());
    let (u1, sel2) = (ui.clone(), tstate.selection.clone());
    let toolbar = w::wrap(ui)
        .gap(unit * 2.0)
        .child(w::button(ui, l.k("files.newText"), move || new_text(&s5, &sel2)).variant("primary"))
        .child(w::button(ui, l.k("files.newFolder"), move || {
            name_dialog(&s6, &u1, None)
        }))
        .child(w::button(ui, l.k("files.import"), move || import_dialog(&s7)));
    let selbar = selection_bar(st, ui, &tstate, detail);
    let crumbs = crumbs(st, ui, &tstate);
    let (f, ue) = (st.folder, ui.clone());
    let empty = remedy_ui::show(ui, move || shown.with(Vec::is_empty), move || {
        crate::state::label(&ue, "", "muted").text(move || {
            let key = if f.get().is_some() { "files.emptyChild" } else { "files.emptyRoot" };
            crate::lang::tr(&l.id(), key)
        })
    })
    .el();

    let (s8, ui8, ts8) = (st.clone(), ui.clone(), tstate.clone());
    let left = w::column(ui)
        .gap(unit * 2.0)
        .padding(Insets::all(unit * 3.0))
        .fill_width()
        .grow(1.0)
        .layout(|l| l.min_height = Length::Px(0.0))
        .child(search)
        .child(toolbar)
        .child(selbar)
        .child(crumbs)
        .child(parent_row(st, ui))
        .child(list)
        .child(empty)
        .on_key(move |e| {
            if !e.pressed {
                return false;
            }
            match e.key {
                Key::Delete | Key::Backspace => {
                    let ids = ts8.selection.selected.get_untracked().unwrap_or_default();
                    if ids.is_empty() {
                        return false;
                    }
                    delete_dialog(&s8, &ui8, ids, &ts8.selection);
                    true
                }
                Key::F(2) => {
                    let id = ts8.selection.cursor.get_untracked().ok().flatten();
                    let item = id.and_then(|id| {
                        s8.items
                            .with_untracked(|v| v.iter().find(|i| i.id == id).cloned())
                            .ok()
                            .flatten()
                    });
                    if let Some(i) = item {
                        name_dialog(&s8, &ui8, Some(i));
                        return true;
                    }
                    false
                }
                Key::Escape if s8.error.get_untracked().ok().flatten().is_some() => {
                    s8.error.set(None);
                    true
                }
                _ => false,
            }
        });

    // ---- the open item
    let right = detail_pane(st, ui, detail, &tstate);
    let split = w::split(ui, Axis::Row, left, right, Signal::new(0.45));
    w::column(ui)
        .grow(1.0)
        .fill_width()
        .layout(|l| l.min_height = Length::Px(0.0))
        .child(split)
}

/// The item list with its headings in `id`.
fn item_table(st: &St, ui: &Ui, tstate: &TableState<String>, shown: Signal<Vec<ItemPreview>>, id: &str) -> El {
    let (id1, id2) = (id.to_owned(), id.to_owned());
    let t = |key: &str| crate::lang::tr(id, key);
    let columns = vec![
        Column::new("name", t("dialog.name"), 220.0, |ui: &Ui, i: &ItemPreview| {
            w::row(ui)
                .gap(ui.theme().spacing.unit * 2.0)
                .align(Align::Center)
                .child(w::icon(ui, icon_for(i.kind), ui.theme().type_scale.body + 2.0))
                .child(crate::state::label(ui, i.name.clone(), "").data_text().nowrap())
        })
        .sort_by(|i: &ItemPreview| SortValue::Text(format!("{}{}", u8::from(i.kind != ItemKind::Folder), i.name))),
        {
            let (st, sel) = (st.clone(), tstate.selection.clone());
            Column::new("actions", t("port.colActions"), 140.0, move |ui: &Ui, i: &ItemPreview| {
                row_actions(&st, ui, i, &sel)
            })
        },
        Column::new("size", t("port.colSize"), 90.0, move |ui: &Ui, i: &ItemPreview| {
            let s = if i.kind == ItemKind::Folder { "—".to_owned() } else { size_text(&id1, i.size) };
            // Body text like Kind and Modified: muted text on a selected (amber) row falls
            // just under 4.5:1.
            crate::state::label(ui, s, "").nowrap()
        })
        .sort_by(|i: &ItemPreview| {
            if i.kind == ItemKind::Folder { SortValue::Missing } else { SortValue::Number(i.size as f64) }
        })
        .align_end(),
        Column::text("kind", t("port.colKind"), 80.0, move |i: &ItemPreview| kind_name(&id2, i.kind)),
        Column::text("modified", t("port.colModified"), 150.0, |i: &ItemPreview| {
            chrono::DateTime::parse_from_rfc3339(&i.updated_at)
                .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default()
        }),
    ];
    let (s3, u3) = (st.clone(), ui.clone());
    table(ui, t("files.listLabel"), shown, |i: &ItemPreview| i.id.clone(), columns, tstate, move |id| {
        open_item(&s3, &u3, id)
    })
}

/// Opens a folder, or the item in the detail pane (asking first if the editor has unsaved
/// edits).
fn open_item(st: &St, ui: &Ui, id: &str) {
    let item = st
        .items
        .with_untracked(|v| v.iter().find(|i| i.id == id).cloned())
        .ok()
        .flatten();
    let Some(item) = item else { return };
    if item.kind == ItemKind::Folder {
        let (s, id) = (st.clone(), item.id.clone());
        guard(st, ui, move || go_to(&s, Some(id.clone())));
    }
}

/// Shows `folder` (`None`: the top level).
fn go_to(st: &St, folder: Option<String>) {
    st.folder.set(folder);
    st.dirty.set(false);
    state::reload(st);
}

/// Runs `then` now, or after the user agrees to drop unsaved text edits.
fn guard(st: &St, ui: &Ui, then: impl Fn() + 'static) {
    if !st.dirty.get_untracked().unwrap_or(false) {
        then();
        return;
    }
    let (d, l) = (st.dirty, st.lang);
    let _ = w::dialog(
        ui,
        l.t("port.unsavedTitle"),
        l.t("files.unsavedConfirm"),
        vec![
            DialogButton::new(l.t("port.stay"), "", || {}),
            DialogButton::new(l.t("port.leave"), "danger", move || {
                d.set(false);
                then();
            }),
        ],
    );
}

fn load_detail(st: &St, id: Option<&str>, detail: Signal<Option<Detail>>) {
    let item = id.and_then(|id| {
        st.items
            .with_untracked(|v| v.iter().find(|i| i.id == id).cloned())
            .ok()
            .flatten()
    });
    let Some(item) = item.filter(|i| i.kind != ItemKind::Folder) else {
        detail.set(None);
        return;
    };
    // The same item still open with unsaved edits stays as the user left it.
    let same = detail
        .with_untracked(|d| d.as_ref().is_some_and(|d| d.item.id == item.id))
        .unwrap_or(false);
    if same && st.dirty.get_untracked().unwrap_or(false) {
        return;
    }
    let loaded = match item.kind {
        ItemKind::Text => checked(st, st.with(|v| v.get_item(&item.id))).map(|d| Detail {
            item: item.clone(),
            text: d.text,
            image: None,
        }),
        ItemKind::Image => {
            checked(st, st.with(|v| v.export_bytes(&item.id))).map(|(_, bytes)| Detail {
                item: item.clone(),
                text: None,
                image: Some(Arc::from(bytes)),
            })
        }
        _ => Some(Detail {
            item: item.clone(),
            text: None,
            image: None,
        }),
    };
    st.dirty.set(false);
    detail.set(loaded);
}

fn selection_bar(
    st: &St,
    ui: &Ui,
    tstate: &TableState<String>,
    _detail: Signal<Option<Detail>>,
) -> El {
    let unit = ui.theme().spacing.unit;
    let sel = tstate.selection.clone();
    let slot = w::row(ui).gap(unit * 2.0).align(Align::Center).fill_width();
    let (s2, st2, u2, sel2, l) = (slot.clone(), st.clone(), ui.clone(), sel.clone(), st.lang);
    slot.bind(move || {
        let n = sel2.selected.with(Vec::len);
        let _ = l.id();
        untrack(|| {
            s2.clear();
            if n == 0 {
                let _ = s2.clone().child(crate::state::label(&u2, l.t("files.selectHint"), "muted"));
                return;
            }
            let (st3, u3, sel3, sel4) = (st2.clone(), u2.clone(), sel2.clone(), sel2.clone());
            let _ = s2
                .clone()
                .child(crate::state::label(&u2, l.tv("files.selected", &[("count", &l.num(n as u64))]), "strong"))
                .child(
                    w::button(&u2, l.t("files.delete"), move || {
                        let ids = sel3.selected.get_untracked().unwrap_or_default();
                        delete_dialog(&st3, &u3, ids, &sel3);
                    })
                    .variant("danger"),
                )
                .child(w::button(&u2, l.t("files.clear"), move || sel4.clear()).variant("quiet"))
                .child(crate::state::label(&u2, l.t("files.multiHint"), "muted"));
        });
    })
    .semantics(
        remedy_ui_a11y::Semantics::new(remedy_ui_a11y::Role::Status)
            .live(remedy_ui_a11y::Live::Polite),
    )
}

/// The path from the top to the folder shown; each part opens its folder and takes items
/// dragged onto it (moving them there).
fn crumbs(st: &St, ui: &Ui, tstate: &TableState<String>) -> El {
    let unit = ui.theme().spacing.unit;
    let bar = w::wrap(ui)
        .gap(unit)
        .align(Align::Center)
        .fill_width()
        .semantics(
            remedy_ui_a11y::Semantics::new(remedy_ui_a11y::Role::Group).label(st.lang.t("files.pathLabel")),
        );
    let (b2, st2, u2, _sel) = (
        bar.clone(),
        st.clone(),
        ui.clone(),
        tstate.selection.clone(),
    );
    let l = st.lang;
    bar.bind(move || {
        let crumbs = st2.crumbs.get();
        let here = st2.folder.get();
        let _ = l.id();
        untrack(|| {
            b2.clear();
            let mut parts: Vec<(Option<String>, String)> = vec![(None, l.t("files.vault"))];
            parts.extend(crumbs.iter().map(|c| (Some(c.id.clone()), c.name.clone())));
            let last = parts.len() - 1;
            for (i, (id, name)) in parts.into_iter().enumerate() {
                if i > 0 {
                    let _ = b2.clone().child(crate::state::label(&u2, "/", "muted"));
                }
                let (s, id2, id3, id4) = (st2.clone(), id.clone(), id.clone(), id.clone());
                let (sa, sd) = (st2.clone(), st2.clone());
                let u4 = u2.clone();
                let btn = w::button(&u2, name, move || {
                    let (s2, id) = (s.clone(), id2.clone());
                    guard(&s, &u4, move || go_to(&s2, id.clone()));
                })
                .variant(if i == last { "primary" } else { "quiet" })
                .on_drop(
                    move |d, _| {
                        if id3 == here_of(&sa) && d.local::<Moving>().is_some() {
                            DropEffect::None
                        } else {
                            accept(&sa, d, None, id3.clone())
                        }
                    },
                    move |d, _, _| take_drop(&sd, d, id4.clone()),
                );
                let _ = b2.clone().child(btn);
            }
            if here.is_some() {
                let (s, up, u4) = (
                    st2.clone(),
                    crumbs
                        .len()
                        .checked_sub(2)
                        .and_then(|i| crumbs.get(i))
                        .map(|c| c.id.clone()),
                    u2.clone(),
                );
                let _ = b2.clone().child(
                    w::button(&u2, l.t("files.up"), move || {
                        let (s2, up) = (s.clone(), up.clone());
                        guard(&s, &u4, move || go_to(&s2, up.clone()));
                    })
                    .variant("quiet"),
                );
            }
        });
    })
}

/// The open and remove buttons at the end of a row (the shipped list's row actions).
fn row_actions(st: &St, ui: &Ui, i: &ItemPreview, sel: &remedy_ui_widgets_data::Selection<String>) -> El {
    let unit = ui.theme().spacing.unit;
    // Small buttons that fit a table row.
    let small = remedy_ui::style::StylePatch::new()
        .text_size(ui.theme().type_scale.small)
        .padding(Insets::symmetric(unit * 0.5, unit * 2.0));
    let l = st.lang;
    let mut row = w::row(ui).gap(unit).align(Align::Center);
    if i.kind == ItemKind::Folder {
        let (s, u, id) = (st.clone(), ui.clone(), i.id.clone());
        row = row.child(
            w::button(ui, l.t("files.open"), move || {
                let (s2, id) = (s.clone(), id.clone());
                guard(&s, &u, move || go_to(&s2, Some(id.clone())));
            })
            .variant("quiet")
            .style(small.clone())
            .height(24.0)
            .semantics(
                remedy_ui_a11y::Semantics::new(remedy_ui_a11y::Role::Button)
                    .label(l.tv("files.openFolderTitle", &[("name", &i.name)]))
                    .action(remedy_ui_a11y::Action::Click),
            ),
        );
    }
    let (s, u, id, sel) = (st.clone(), ui.clone(), i.id.clone(), sel.clone());
    let label = if i.kind == ItemKind::Folder {
        l.tv("files.removeFolderAria", &[("name", &i.name)])
    } else {
        l.tv("files.removeItemAria", &[("name", &i.name)])
    };
    row.child(
        w::button(ui, l.t("files.remove"), move || delete_dialog(&s, &u, vec![id.clone()], &sel))
            .variant("danger")
            .style(small)
            .height(24.0)
            .semantics(
                remedy_ui_a11y::Semantics::new(remedy_ui_a11y::Role::Button)
                    .label(label)
                    .action(remedy_ui_a11y::Action::Click),
            ),
    )
}

/// Inside a folder, the ".." row: opens the parent, and takes items dragged onto it (moving
/// them up) or files from other apps (imported into the parent). Not there at the top.
fn parent_row(st: &St, ui: &Ui) -> El {
    let folder = st.folder;
    let (s, u) = (st.clone(), ui.clone());
    remedy_ui::show(ui, move || folder.with(Option::is_some), move || parent_row_built(&s, &u)).el()
}

fn parent_row_built(st: &St, ui: &Ui) -> El {
    let unit = ui.theme().spacing.unit;
    let crumbs = st.crumbs;
    let parent = move || {
        crumbs
            .with_untracked(|c| c.len().checked_sub(2).and_then(|i| c.get(i)).map(|p| (p.id.clone(), p.name.clone())))
            .ok()
            .flatten()
    };
    let l = st.lang;
    let hint = w::text(ui, "").variant("muted").text(move || {
        let _ = crumbs.get();
        let id = l.id();
        let name = parent().map_or_else(|| crate::lang::tr(&id, "files.vault"), |(_, n)| n);
        crate::lang::trv(&id, "files.parentMeta", &[("extra", &format!(" · {name}"))])
    });
    let (s1, u1) = (st.clone(), ui.clone());
    let (sa, sd) = (st.clone(), st.clone());
    let up = move || parent().map(|(id, _)| id);
    let row = ui
        .el("list_item")
        .row()
        .gap(unit * 3.0)
        .align(Align::Center)
        .fill_width()
        .padding(Insets::symmetric(unit * 2.0, unit * 3.0))
        .child(w::text(ui, "⬆"))
        .child(w::column(ui).gap(unit * 0.5).child(w::text(ui, "..").variant("strong")).child(hint))
        .on_click(move || {
            let s2 = s1.clone();
            guard(&s1, &u1, move || go_to(&s2, up()));
        })
        .on_drop(
            move |d, _| accept(&sa, d, None, parent().map(|(id, _)| id)),
            move |d, _, _| take_drop(&sd, d, parent().map(|(id, _)| id)),
        )
        .semantics(move || {
            remedy_ui_a11y::Semantics::new(remedy_ui_a11y::Role::Button)
                .label(crate::lang::tr(&l.id(), "port.parentFolder"))
                .action(remedy_ui_a11y::Action::Click)
        });
    w::tooltip(&row, l.k("files.parentTitle"));
    row
}

fn here_of(st: &St) -> Option<String> {
    st.folder.get_untracked().ok().flatten()
}

fn new_text(st: &St, sel: &remedy_ui_widgets_data::Selection<String>) {
    let folder = here_of(st);
    let name = st.lang.t("files.defaultTextName");
    if let Some(item) = checked(st, st.with(|v| v.create_text(&name, "", folder))) {
        state::changed(st);
        sel.select_only(item.id);
    }
}

fn import_dialog(st: &St) {
    let s = st.clone();
    let win = st.window.borrow().clone();
    st.app.file_dialog(
        win.as_ref(),
        {
            let mut d = FileDialog::default();
            d.title = st.lang.t("port.importDialog");
            d.multiple = true;
            d
        },
        move |paths| {
            if !paths.is_empty() {
                import_paths(&s, &paths, here_of(&s));
            }
        },
    );
}

fn export(st: &St, item: &ItemPreview) {
    let (s, id) = (st.clone(), item.id.clone());
    let win = st.window.borrow().clone();
    let name = if item.name.is_empty() {
        "export.bin".to_owned()
    } else {
        item.name.clone()
    };
    st.app.file_dialog(
        win.as_ref(),
        {
            let mut d = FileDialog::default();
            d.title = st.lang.t("files.exportDialog");
            d.save = true;
            d.file_name = Some(name);
            d
        },
        move |paths| {
            let Some(dest) = paths.into_iter().next() else {
                return;
            };
            let r = (|| {
                crate::policy::validate_export_path(&dest)?;
                let (_, data) = s.with(|v| v.export_bytes(&id))?;
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&dest, data)?;
                Ok::<(), AppError>(())
            })();
            let _ = checked(&s, r);
        },
    );
}

/// New folder (`item` is `None`) or rename `item`.
fn name_dialog(st: &St, ui: &Ui, item: Option<ItemPreview>) {
    let unit = ui.theme().spacing.unit;
    let rename = item.is_some();
    let l = st.lang;
    let label = l.t(if rename { "port.newName" } else { "port.folderName" });
    let field = w::text_input(ui, label.clone());
    let default_name = l.t("files.defaultFolderName");
    field.set_value(item.as_ref().map_or(default_name.as_str(), |i| i.name.as_str()));
    let modal_slot: Rc<RefCell<Option<w::Modal>>> = Rc::default();
    let submit: Rc<dyn Fn()> = {
        let (st, field, item, slot) = (st.clone(), field.clone(), item.clone(), modal_slot.clone());
        Rc::new(move || {
            let name = field.value().trim().to_owned();
            if name.is_empty() {
                return;
            }
            let ok = match &item {
                Some(i) if i.name == name => true,
                Some(i) => checked(&st, st.with(|v| v.rename_item(&i.id, &name))).is_some(),
                None => checked(&st, st.with(|v| v.create_folder(&name, here_of(&st)))).is_some(),
            };
            if ok {
                state::changed(&st);
                if let Some(m) = slot.borrow().as_ref() {
                    m.close();
                }
            }
        })
    };
    let s2 = submit.clone();
    let field = field.on_submit(move || s2());
    let slot2 = modal_slot.clone();
    let go = w::button(ui, l.t(if rename { "dialog.rename" } else { "dialog.create" }), move || {
        submit()
    })
    .variant("primary");
    let content = w::column(ui)
        .gap(unit * 3.0)
        .child(w::field(ui, label, field.el().fill_width()))
        .child(
            w::row(ui)
                .gap(unit * 2.0)
                .justify(remedy_ui::view::Justify::End)
                .child(w::button(ui, l.t("dialog.cancel"), move || {
                    if let Some(m) = slot2.borrow().as_ref() {
                        m.close();
                    }
                }))
                .child(go),
        );
    let m = w::modal(
        ui,
        l.t(if rename { "dialog.renameTitle" } else { "dialog.newFolderTitle" }),
        content,
        true,
    );
    *modal_slot.borrow_mut() = Some(m);
    field.el().focus();
    field.select_all();
}

/// Asks before deleting `ids` (saying how much a non-empty folder still holds), then
/// deletes them.
fn delete_dialog(
    st: &St,
    ui: &Ui,
    ids: Vec<String>,
    sel: &remedy_ui_widgets_data::Selection<String>,
) {
    let items: Vec<ItemPreview> = st
        .items
        .with_untracked(|v| v.iter().filter(|i| ids.contains(&i.id)).cloned().collect())
        .unwrap_or_default();
    if items.is_empty() {
        return;
    }
    let mut nested: Vec<(String, usize)> = Vec::new();
    for i in items.iter().filter(|i| i.kind == ItemKind::Folder) {
        if let Some(n) = checked(st, st.with(|v| v.folder_content_count(&i.id)))
            && n > 0
        {
            nested.push((i.id.clone(), n));
        }
    }
    // Worded as 0.2.0's delete dialog (`ExplorerScreen.tsx`), from its catalog.
    let l = st.lang;
    let n = items.len();
    let quoted: Vec<String> = items.iter().map(|i| l.tv("delete.quote", &[("name", &i.name)])).collect();
    let joiner = l.t("delete.join");
    let names = if n <= 3 {
        quoted.join(&joiner)
    } else {
        l.tv("delete.andMore", &[("names", &quoted[..2].join(&joiner)), ("count", &l.num((n - 2) as u64))])
    };
    let single_folder = n == 1 && items[0].kind == ItemKind::Folder;
    let title = if n > 1 {
        l.tv("delete.titleItems", &[("count", &l.num(n as u64))])
    } else if single_folder && !nested.is_empty() {
        l.t("delete.titleNotEmpty")
    } else if single_folder {
        l.t("delete.titleEmptyFolder")
    } else {
        l.t("delete.titleItem")
    };
    let (message, button) = if nested.is_empty() {
        (
            format!("{} {}", l.tv("delete.simple", &[("names", &names)]), l.t("delete.cannotUndo")),
            if n > 1 {
                l.tv("delete.count", &[("count", &l.num(n as u64))])
            } else if single_folder {
                l.t("delete.folder")
            } else {
                l.t("delete.one")
            },
        )
    } else {
        let total: usize = nested.iter().map(|(_, c)| c).sum();
        let head = if n == 1 {
            l.tv("delete.oneCascade", &[("name", &items[0].name), ("items", &l.count("child", total as u64))])
        } else {
            l.tv(
                "delete.multiCascade",
                &[
                    ("names", &names),
                    ("folders", &l.count("folders", nested.len() as u64)),
                    ("nested", &l.count("nested", total as u64)),
                ],
            )
        };
        (
            format!("{} {}", crate::lang::plain(&head), l.t("delete.cascadeHelp")),
            if n == 1 { l.t("delete.folderContents") } else { l.t("delete.allContents") },
        )
    };
    let (s, sel) = (st.clone(), sel.clone());
    let _ = w::dialog(
        ui,
        title,
        message,
        vec![
            DialogButton::new(l.t("dialog.cancel"), "", || {}),
            DialogButton::new(button, "danger_solid", move || {
                let (mut ok, mut failures) = (0usize, Vec::new());
                let here = here_of(&s);
                for i in &items {
                    let cascade = nested.iter().any(|(id, _)| *id == i.id);
                    match s.with(|v| v.delete_item(&i.id, cascade)) {
                        Ok(()) => ok += 1,
                        Err(AppError::Locked) => {
                            state::lock(&s);
                            return;
                        }
                        Err(e) => failures.push(format!("{}: {}", i.name, s.lang.err(&e.to_string()))),
                    }
                    if here.as_deref() == Some(i.id.as_str()) {
                        s.folder.set(i.parent_id.clone());
                    }
                }
                sel.clear();
                s.dirty.set(false);
                state::changed(&s);
                if let Some(m) = batch_message(&s, &DELETE, ok, &failures) {
                    s.error.set(Some(m));
                }
            }),
        ],
    );
}

fn detail_pane(
    st: &St,
    ui: &Ui,
    detail: Signal<Option<Detail>>,
    tstate: &TableState<String>,
) -> El {
    let unit = ui.theme().spacing.unit;
    let pane = w::column(ui)
        .gap(unit * 3.0)
        .padding(Insets::all(unit * 4.0))
        .grow(1.0)
        .fill_width()
        .layout(|l| l.min_height = Length::Px(0.0));
    let (p2, st2, u2, sel) = (
        pane.clone(),
        st.clone(),
        ui.clone(),
        tstate.selection.clone(),
    );
    pane.bind(move || {
        let d = detail.get();
        untrack(|| {
            p2.clear();
            let content = match d {
                None => {
                    let folder_picked = sel
                        .cursor
                        .get_untracked()
                        .ok()
                        .flatten()
                        .and_then(|id| st2.items.with_untracked(|v| v.iter().any(|i| i.id == id && i.kind == ItemKind::Folder)).ok())
                        .unwrap_or(false);
                    let l = st2.lang;
                    let key = if folder_picked { "files.folderSelected" } else { "files.selectFile" };
                    w::column(&u2)
                        .gap(unit * 2.0)
                        .child(crate::state::label(&u2, l.k(key), ""))
                        .child(crate::state::label(&u2, l.k("files.detailHelp"), "muted"))
                }
                Some(d) => match d.item.kind {
                    ItemKind::Text => text_editor(&st2, &u2, d, &sel),
                    _ => preview(&st2, &u2, d, &sel),
                },
            };
            let _ = p2.clone().child(content);
        });
    })
}

fn action_row(
    st: &St,
    ui: &Ui,
    item: &ItemPreview,
    sel: &remedy_ui_widgets_data::Selection<String>,
    rename: bool,
) -> El {
    let unit = ui.theme().spacing.unit;
    let l = st.lang;
    let mut row = w::wrap(ui).gap(unit * 2.0).align(Align::Center);
    if rename {
        let (s, u, i) = (st.clone(), ui.clone(), item.clone());
        row = row.child(w::button(ui, l.k("files.rename"), move || {
            name_dialog(&s, &u, Some(i.clone()))
        }));
    }
    let (s, i) = (st.clone(), item.clone());
    row = row.child(w::button(ui, l.k("files.export"), move || export(&s, &i)));
    let (s, u, id, sel) = (st.clone(), ui.clone(), item.id.clone(), sel.clone());
    row.child(
        w::button(ui, l.k("files.delete"), move || {
            // The open item, unless several are selected: then all of them.
            let picked = sel.selected.get_untracked().unwrap_or_default();
            let ids = if picked.len() > 1 {
                picked
            } else {
                vec![id.clone()]
            };
            delete_dialog(&s, &u, ids, &sel);
        })
        .variant("danger"),
    )
}

fn text_editor(st: &St, ui: &Ui, d: Detail, sel: &remedy_ui_widgets_data::Selection<String>) -> El {
    let unit = ui.theme().spacing.unit;
    let l = st.lang;
    let name: TextField = w::text_input(ui, l.t("port.fileName"));
    l.name_field(&name, "port.fileName", false);
    name.set_value(&d.item.name);
    // Copies out of the text go to the secret clipboard (out of history, cleared after 30 s),
    // as the Copy button does.
    let body: TextField = w::text_area(ui, l.t("port.text")).copy_as_secret(COPY_KEEP);
    l.name_field(&body, "port.text", false);
    body.set_value(d.text.as_deref().unwrap_or_default());
    let dirty = st.dirty;
    let name = name.on_change(move || dirty.set(true));
    let body = body.on_change(move || dirty.set(true));
    let save: Rc<dyn Fn()> = {
        let (st, name, body, id, old) = (
            st.clone(),
            name.clone(),
            body.clone(),
            d.item.id.clone(),
            d.item.name.clone(),
        );
        Rc::new(move || {
            let new_name = name.value().trim().to_owned();
            let rename = (!new_name.is_empty() && new_name != old).then_some(new_name);
            if checked(
                &st,
                st.with(|v| v.update_text(&id, rename.clone(), body.value())),
            )
            .is_some()
            {
                st.dirty.set(false);
                state::changed(&st);
            }
        })
    };
    let s1 = save.clone();
    let save_btn = w::button(ui, l.k("files.save"), move || s1()).variant("primary");
    let sb = save_btn.clone();
    let save_btn = save_btn.bind(move || {
        let _ = sb.clone().disabled(!dirty.get());
    });
    let (s, b) = (st.clone(), body.clone());
    let copy = w::button(ui, l.k("files.copy"), move || s.app.copy_secret(b.value(), COPY_KEEP));
    let s2 = save.clone();
    let editor = body
        .el()
        .fill_width()
        .grow(1.0)
        .layout(|l| l.min_height = Length::Px(120.0))
        .on_key(move |e| {
            // Ctrl+S (Command+S on Apple platforms) saves.
            let m = e.modifiers;
            if e.pressed && (m.ctrl || m.meta) && e.key == Key::Character("s".into()) {
                s2();
                return true;
            }
            false
        });
    w::column(ui)
        .gap(unit * 3.0)
        .grow(1.0)
        .fill_width()
        .layout(|l| l.min_height = Length::Px(0.0))
        .child(
            w::wrap(ui)
                .gap(unit * 2.0)
                .align(Align::Center)
                .child(name.el().width(260.0))
                .child(save_btn)
                .child(copy)
                .child(action_row(st, ui, &d.item, sel, false)),
        )
        .child(editor)
}

fn preview(st: &St, ui: &Ui, d: Detail, sel: &remedy_ui_widgets_data::Selection<String>) -> El {
    let unit = ui.theme().spacing.unit;
    let l = st.lang;
    let (mime, size) = (d.item.mime.clone(), d.item.size);
    let meta = move || format!("{mime} · {}", size_text(&l.id(), size));
    let body = match &d.image {
        Some(bytes) => remedy_ui_media_image::image(
            ui,
            Source::Bytes(bytes.clone()),
            d.item.name.clone(),
            Fit::Contain,
        )
        .fill_width()
        .grow(1.0)
        .layout(|l| l.min_height = Length::Px(0.0)),
        None if d.item.kind == ItemKind::Image => crate::state::label(ui, l.k("files.noPreview"), "muted"),
        None => crate::state::label(ui, l.k("files.binaryHint"), ""),
    };
    w::column(ui)
        .gap(unit * 3.0)
        .grow(1.0)
        .fill_width()
        .layout(|l| l.min_height = Length::Px(0.0))
        .child(
            w::wrap(ui)
                .gap(unit * 2.0)
                .align(Align::Center)
                .child(w::heading(ui, 2, d.item.name.clone()))
                .child(action_row(st, ui, &d.item, sel, true)),
        )
        .child(body)
        .child(crate::state::label(ui, meta, "muted"))
}
