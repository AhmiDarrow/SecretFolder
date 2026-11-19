// Before-evidence harness: answers the shipped app's Tauri commands with sample data so
// each screen renders in a headless browser. Query parameters pick the state:
//   state=setup|locked|unlocked   folder=<id> (open that folder first via act)
//   act=Label1,Label2 (buttons, rows or tabs clicked in order; "dbl:Name" double-clicks a
//   row)   fill=1|short (types sample text into inputs)   empty=1 (no items)
//   w=,h= (window size; the page is laid out at it and the screenshot cropped)
const q = new URLSearchParams(location.search);
const state = q.get("state") || "unlocked";
const now = "2026-11-01T10:00:00Z";
const item = (id, name, kind, parentId, size = 0, mime = "") => ({
  id, name, kind, parentId, size, mime: mime || (kind === "folder" ? "inode/directory" : "text/plain"),
  createdAt: now, updatedAt: now,
});
// A small inline PNG for the image preview (16x16 amber).
const PNG =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAAAFklEQVR4nGP4v1+FJMQwqmFUw/DVAAAZXeIQHjfMeQAAAABJRU5ErkJggg==";
const items = q.get("empty") ? [] : [
  item("f1", "Documents", "folder", null),
  item("f2", "Taxes 2026", "folder", "f1"),
  item("f3", "Empty folder", "folder", null),
  item("t1", "Passwords.txt", "text", null, 42),
  item("t2", "Wi-Fi notes.md", "text", "f1", 34, "text/markdown"),
  item("i1", "photo.png", "image", "f1", 75, "image/png"),
  item("b1", "backup.bin", "binary", "f2", 70000, "application/octet-stream"),
];
const texts = { t1: "bank: sample-not-real\nemail: sample\n", t2: "Guest network: sample\n" };
const status = () => ({
  initialized: state !== "setup", unlocked: state === "unlocked",
  itemCount: items.length, idleLockSecs: 900, hasRecoveryKey: true,
});
const pathOf = (id) => {
  const out = [];
  let cur = items.find((i) => i.id === id);
  while (cur) { out.unshift(cur); cur = items.find((i) => i.id === cur.parentId); }
  return out;
};
const callbacks = new Map();
let next = 1;
window.__TAURI_INTERNALS__ = {
  metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
  plugins: { path: { sep: "/", delimiter: ";" } },
  transformCallback(cb) { const id = next++; callbacks.set(id, cb); return id; },
  unregisterCallback(id) { callbacks.delete(id); },
  runCallback(id, data) { callbacks.get(id)?.(data); },
  callbacks,
  convertFileSrc: (p) => p,
  async invoke(cmd, args) {
    switch (cmd) {
      case "get_status": return status();
      case "setup_vault": return "ABCD-EFGH-JKLM-NPQR-STUV-WXYZ-2345-6789";
      case "unlock_vault": case "unlock_with_recovery": throw "wrong password or corrupted vault";
      case "list_items": return items.filter((i) => (i.parentId ?? null) === (args.parentId ?? null));
      case "folder_path": return args.folderId ? pathOf(args.folderId) : [];
      case "get_item": {
        const i = items.find((x) => x.id === args.id);
        return { ...i, text: texts[i.id] ?? (i.kind === "text" ? "" : null), dataUrl: i.kind === "image" ? PNG : null };
      }
      case "folder_content_count": return items.filter((i) => i.parentId === args.id).length;
      case "max_file_bytes": return 25 * 1024 * 1024;
      case "plugin:app|version": return "0.1.7";
      case "plugin:event|listen": return next++;
      case "plugin:updater|check": return null;
      default: return null;
    }
  },
};
if (q.get("w") && q.get("h")) {
  const css = `width:${q.get("w")}px;height:${q.get("h")}px;overflow:hidden;`;
  document.documentElement.style.cssText += css;
  document.body.style.cssText += css;
}
const acts = (q.get("act") || "").split(",").filter(Boolean);
const fill = q.get("fill");
async function run() {
  await new Promise((r) => setTimeout(r, 500));
  if (fill) {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    for (const el of document.querySelectorAll("input[type=password], input:not([type])")) {
      if (el.classList.contains("search")) continue;
      set.call(el, fill === "short" ? "short" : "correct horse battery");
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }
  }
  for (const a of acts) {
    await new Promise((r) => setTimeout(r, 300));
    const dbl = a.startsWith("dbl:");
    const label = dbl ? a.slice(4) : a;
    const all = [...document.querySelectorAll("button, li[role=option], [role=button], label")];
    const el = all.find((x) => {
      const name = x.querySelector?.(".file-name")?.textContent?.trim();
      return name === label || x.textContent.trim() === label || x.getAttribute("aria-label") === label || x.title === label;
    });
    if (!el) continue;
    if (dbl) el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    else el.click();
  }
}
run();
