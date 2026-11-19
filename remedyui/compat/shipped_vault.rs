//! Drives a shipped release's own vault code (`vault.rs`, `crypto.rs`, `error.rs` taken from a
//! release tag by `compat/shipped.sh`), so the RemedyUI build's vault handling is checked
//! against the exact code people's vaults were written with.
//!
//!   make <vault folder> <password>      a vault with sample folders and files; prints the
//!                                        recovery key
//!   dump <vault folder> <password>      unlocks and prints every item (path, kind, type,
//!                                        size, contents) and the idle lock as JSON
//!   dump-recovery <vault folder> <key>  the same, unlocked with the recovery key

mod crypto;
mod error;
mod vault;

use std::path::PathBuf;

use base64::Engine;
use vault::{ItemKind, Vault};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (cmd, root, secret) = match args.as_slice() {
        [_, c, p, s] => (c.as_str(), PathBuf::from(p), s.as_str()),
        _ => {
            eprintln!("usage: shipped-vault make|dump|dump-recovery <vault folder> <password|key>");
            std::process::exit(2);
        }
    };
    let result = match cmd {
        "make" => make(root, secret),
        "dump" => dump(root, secret, false),
        "dump-recovery" => dump(root, secret, true),
        _ => Err(format!("unknown command {cmd}")),
    };
    if let Err(e) = result {
        eprintln!("shipped-vault: {e}");
        std::process::exit(1);
    }
}

fn s<T, E: std::fmt::Display>(r: Result<T, E>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

/// A small PNG (2×2, opaque red, green, blue, white).
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00,
    0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xfd, 0xd4, 0x9a, 0x73, 0x00, 0x00, 0x00, 0x12, 0x49,
    0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xc0, 0x00, 0xc2, 0x0c, 0xff, 0x81, 0x00, 0x00, 0x1f, 0xee,
    0x05, 0xfb, 0x0b, 0xd9, 0x68, 0x8b, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn make(root: PathBuf, password: &str) -> Result<(), String> {
    let mut v = s(Vault::open_root(root))?;
    let recovery = s(v.setup(password))?;
    let docs = s(v.create_folder("Documents", None))?;
    let taxes = s(v.create_folder("Taxes 2026", Some(docs.id.clone())))?;
    s(v.create_folder("Empty folder", None))?;
    s(v.create_text("Passwords.txt", "bank: hunter2hunter2\nemail: correct-horse\n", None))?;
    s(v.create_text("Ünïcödé 日本語 — notes.md", "line one\r\nline two\twith a tab\n🔑", Some(docs.id.clone())))?;
    s(v.create_text("Empty.txt", "", Some(taxes.id.clone())))?;
    s(v.import_bytes("photo.png".into(), "image/png".into(), PNG.to_vec(), Some(docs.id.clone())))?;
    let blob: Vec<u8> = (0..70_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect();
    s(v.import_bytes("backup.bin".into(), "application/octet-stream".into(), blob, Some(taxes.id.clone())))?;
    // Made, renamed, edited, moved and deleted: what the release did must stick.
    let t = s(v.create_text("draft.txt", "first", None))?;
    s(v.update_text(&t.id, Some("final.txt".into()), "second version".into()))?;
    s(v.move_item(&t.id, Some(docs.id.clone())))?;
    let gone = s(v.create_text("deleted.txt", "must not come back", None))?;
    s(v.delete_item(&gone.id, false))?;
    s(v.set_idle_lock_secs(1800))?;
    v.lock();
    println!("{recovery}");
    Ok(())
}

fn walk(v: &mut Vault, parent: Option<String>, prefix: &str, out: &mut Vec<serde_json::Value>) -> Result<(), String> {
    for i in s(v.list_items_in(parent))? {
        let path = format!("{prefix}/{}", i.name);
        if i.kind == ItemKind::Folder {
            out.push(serde_json::json!({ "path": path, "kind": i.kind, "id": i.id }));
            walk(v, Some(i.id.clone()), &path, out)?;
        } else {
            let (_, bytes) = s(v.export_bytes(&i.id))?;
            out.push(serde_json::json!({
                "path": path, "kind": i.kind, "id": i.id, "mime": i.mime, "size": i.size,
                "data_b64": base64::engine::general_purpose::STANDARD.encode(bytes),
            }));
        }
    }
    Ok(())
}

fn dump(root: PathBuf, secret: &str, recovery: bool) -> Result<(), String> {
    let mut v = s(Vault::open_root(root))?;
    if recovery {
        s(v.unlock_with_recovery(secret))?;
    } else {
        s(v.unlock(secret))?;
    }
    let mut items = Vec::new();
    walk(&mut v, None, "", &mut items)?;
    items.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let out = serde_json::json!({ "idle_lock_secs": v.status().idle_lock_secs, "items": items });
    println!("{}", s(serde_json::to_string_pretty(&out))?);
    v.lock();
    Ok(())
}
