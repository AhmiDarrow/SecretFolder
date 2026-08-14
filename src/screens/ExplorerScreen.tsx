import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type DragEvent as ReactDragEvent,
} from "react";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { save } from "@tauri-apps/plugin-dialog";
import { api, fileToBase64 } from "../api";
import { copySecret } from "../clipboard";
import { formatError } from "../errors";
import type { ItemDetail, ItemPreview, VaultStatus } from "../types";
import { MAX_FILE_BYTES } from "../types";
import {
  checkForAppUpdate,
  downloadAndInstallUpdate,
} from "../updater";
import appMark from "../assets/app-mark.png";

const GITHUB_PROFILE = "https://github.com/AhmiDarrow";
const GITHUB_REPO = "https://github.com/AhmiDarrow/SecretFolder";
const GITHUB_RELEASES = "https://github.com/AhmiDarrow/SecretFolder/releases";
const PATREON_URL = "https://www.patreon.com/cw/AhmiDarrow";

/** Basename from an absolute OS path (Windows or POSIX). */
function pathFileName(p: string): string {
  const norm = p.replace(/\\/g, "/");
  const i = norm.lastIndexOf("/");
  return i >= 0 ? norm.slice(i + 1) : p;
}

/** MIME type for in-vault item drags (not OS files). */
const VAULT_ITEM_MIME = "application/x-secretfolder-item";

type DropTarget =
  | { kind: "folder"; id: string; name: string }
  | { kind: "parent" }
  | { kind: "current" };

/** Resolve drop target under a CSS-pixel point (folder row, parent-up, or list). */
function dropTargetAtPoint(clientX: number, clientY: number): DropTarget {
  const el = document.elementFromPoint(clientX, clientY);
  if (!el) return { kind: "current" };
  const folderEl = el.closest<HTMLElement>("[data-drop-folder-id]");
  if (folderEl?.dataset.dropFolderId) {
    return {
      kind: "folder",
      id: folderEl.dataset.dropFolderId,
      name: folderEl.dataset.dropFolderName || "folder",
    };
  }
  if (el.closest("[data-drop-parent]")) {
    return { kind: "parent" };
  }
  return { kind: "current" };
}

/** Tauri reports physical pixels; DOM hit-tests need CSS pixels. */
function physicalToCss(x: number, y: number): { x: number; y: number } {
  const dpr = window.devicePixelRatio || 1;
  return { x: x / dpr, y: y / dpr };
}

interface Props {
  status: VaultStatus;
  onLocked: () => void;
}

type Panel = "files" | "settings" | "about";

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

function kindIcon(kind: ItemPreview["kind"]): string {
  switch (kind) {
    case "folder":
      return "📁";
    case "text":
      return "📄";
    case "image":
      return "🖼";
    default:
      return "📦";
  }
}

export function ExplorerScreen({ status, onLocked }: Props) {
  const [panel, setPanel] = useState<Panel>("files");
  const [items, setItems] = useState<ItemPreview[]>([]);
  const [folderId, setFolderId] = useState<string | null>(null);
  const [crumbs, setCrumbs] = useState<ItemPreview[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  /** Multi-select set (click / Ctrl / Shift — no checkboxes). Primary drives detail. */
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
  const [detail, setDetail] = useState<ItemDetail | null>(null);
  const [editName, setEditName] = useState("");
  const [editBody, setEditBody] = useState("");
  const [dirty, setDirty] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [query, setQuery] = useState("");
  /** OS-file drag over the list (Tauri native drop). */
  const [osDragOver, setOsDragOver] = useState(false);
  /** Highlighted drop target while dragging OS files or vault items. */
  const [dropHighlight, setDropHighlight] = useState<DropTarget | null>(null);
  /** Item id currently being dragged inside the vault. */
  const [draggingId, setDraggingId] = useState<string | null>(null);
  /** Ids included in the active vault drag (multi-move). */
  const draggingIdsRef = useRef<string[]>([]);
  const [pendingDelete, setPendingDelete] = useState<{
    /** One or more items to delete. */
    items: Array<{
      id: string;
      name: string;
      kind: ItemPreview["kind"];
      childCount: number;
    }>;
  } | null>(null);
  const [nameDialog, setNameDialog] = useState<{
    mode: "folder" | "rename";
    title: string;
    value: string;
    itemId?: string;
  } | null>(null);
  const nameInputRef = useRef<HTMLInputElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  /** Always-current folder for Tauri drop handler (avoids stale closure). */
  const folderIdRef = useRef<string | null>(null);
  folderIdRef.current = folderId;
  const crumbsRef = useRef<ItemPreview[]>([]);
  crumbsRef.current = crumbs;
  const importInFlight = useRef(false);
  const moveInFlight = useRef(false);
  /** Live drop target while dragging a vault item (ref so dragend always sees it). */
  const dropHighlightRef = useRef<DropTarget | null>(null);
  /** Live id of the vault item being dragged. */
  const draggingIdRef = useRef<string | null>(null);
  /** Suppress the click that WebView2 fires after a drag. */
  const skipClickAfterDragRef = useRef(false);

  const [idleMins, setIdleMins] = useState(
    Math.max(1, Math.round(status.idleLockSecs / 60)),
  );
  const [curPw, setCurPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [newPw2, setNewPw2] = useState("");
  const [settingsMsg, setSettingsMsg] = useState<string | null>(null);

  const [version, setVersion] = useState("…");
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [pendingVersion, setPendingVersion] = useState<string | null>(null);

  // Focus/select only when the dialog opens — not on every keystroke
  // (nameDialog is a new object each onChange; depending on it re-selects all text).
  const nameDialogOpen = nameDialog !== null;
  useEffect(() => {
    if (!nameDialogOpen) return;
    const t = window.setTimeout(() => {
      nameInputRef.current?.focus();
      nameInputRef.current?.select();
    }, 0);
    return () => window.clearTimeout(t);
  }, [nameDialogOpen]);

  const refreshList = useCallback(async () => {
    const [list, path] = await Promise.all([
      api.listItems(folderId),
      api.folderPath(folderId),
    ]);
    setItems(list);
    setCrumbs(path);
    // Drop multi-select ids that no longer exist in this view.
    setSelectedIds((prev) => {
      if (prev.size === 0) return prev;
      const alive = new Set(list.map((i) => i.id));
      const next = new Set<string>();
      for (const id of prev) {
        if (alive.has(id)) next.add(id);
      }
      return next.size === prev.size ? prev : next;
    });
  }, [folderId]);

  // Escape dismisses delete confirm, then error banner (name dialog handles its own Escape).
  useEffect(() => {
    /* polish:delete-escape */
    if (nameDialog || (!pendingDelete && !error)) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      if (pendingDelete) {
        setPendingDelete(null);
        return;
      }
      setError(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pendingDelete, error, nameDialog]);

  // Changing folders clears multi-select + detail focus.
  useEffect(() => {
    setSelectedIds(new Set());
    setSelectedId(null);
    setDetail(null);
  }, [folderId]);

  useEffect(() => {
    void refreshList().catch((e) =>
      setError(formatError(e)),
    );
  }, [refreshList]);

  useEffect(() => {
    void getVersion().then(setVersion).catch(() => setVersion("?"));
  }, []);

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      setEditName("");
      setEditBody("");
      setDirty(false);
      return;
    }
    // Folders are navigated only via openItem (row click / double-click / Enter).
    // Checkbox and multi-select set selectedId without entering the folder.
    const preview = items.find((i) => i.id === selectedId);
    if (preview?.kind === "folder") {
      setDetail(null);
      setEditName("");
      setEditBody("");
      setDirty(false);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const d = await api.getItem(selectedId);
        if (cancelled) return;
        if (d.kind === "folder") {
          // Don't auto-navigate — selection alone must not open folders.
          setDetail(null);
          setEditName("");
          setEditBody("");
          setDirty(false);
          return;
        }
        setDetail(d);
        setEditName(d.name);
        setEditBody(d.text ?? "");
        setDirty(false);
      } catch (e) {
        if (!cancelled) setError(formatError(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selectedId, items]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((i) => i.name.toLowerCase().includes(q));
  }, [items, query]);

  const selectedCount = selectedIds.size;

  function clearSelection() {
    setSelectedIds(new Set());
    setSelectedId(null);
    setDetail(null);
  }

  function selectOnly(id: string) {
    setSelectedIds(new Set([id]));
    setSelectedId(id);
  }

  function toggleSelected(id: string) {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
    setSelectedId(id);
  }

  function selectRange(toId: string) {
    const ids = filtered.map((i) => i.id);
    const toIdx = ids.indexOf(toId);
    if (toIdx < 0) {
      selectOnly(toId);
      return;
    }
    const fromId = selectedId && ids.includes(selectedId) ? selectedId : toId;
    const fromIdx = ids.indexOf(fromId);
    const [a, b] = fromIdx <= toIdx ? [fromIdx, toIdx] : [toIdx, fromIdx];
    setSelectedIds(new Set(ids.slice(a, b + 1)));
    setSelectedId(toId);
  }

  function selectAllFiltered() {
    if (!filtered.length) return;
    setSelectedIds(new Set(filtered.map((i) => i.id)));
    setSelectedId(filtered[filtered.length - 1]?.id ?? null);
  }

  /** Ids to move: multi-selection if the dragged row is in it, else just that row. */
  function idsForDrag(item: ItemPreview): string[] {
    if (selectedIds.has(item.id) && selectedIds.size > 1) {
      return Array.from(selectedIds);
    }
    return [item.id];
  }

  async function lockNow() {
    await api.lock();
    onLocked();
  }

  async function onNewText() {
    setBusy(true);
    setError(null);
    try {
      const item = await api.createText("Untitled.txt", "", folderId);
      await refreshList();
      setSelectedId(item.id);
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
    }
  }

  function onNewFolder() {
    setNameDialog({
      mode: "folder",
      title: "New folder",
      value: "New folder",
    });
  }
  async function importOneFile(file: File, parentId: string | null) {
    if (file.size > MAX_FILE_BYTES) {
      throw new Error(
        `"${file.name}" is larger than ${formatBytes(MAX_FILE_BYTES)}`,
      );
    }
    const dataB64 = await fileToBase64(file);
    await api.importBytes(
      file.name,
      file.type || "application/octet-stream",
      dataB64,
      parentId,
    );
  }

  /** Browser FileList path (Import… picker). */
  async function importFiles(fileList: FileList | File[]) {
    const files = Array.from(fileList);
    if (!files.length) return;
    if (importInFlight.current) return;
    importInFlight.current = true;
    setBusy(true);
    setError(null);
    try {
      const parent = folderIdRef.current;
      for (const f of files) {
        await importOneFile(f, parent);
      }
      await refreshList();
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
      importInFlight.current = false;
    }
  }

  /** Resolve parent id for a drop target (folder / parent-up / current view). */
  const resolveDropParentId = useCallback((target: DropTarget): string | null => {
    if (target.kind === "folder") return target.id;
    if (target.kind === "parent") {
      const path = crumbsRef.current;
      if (path.length <= 1) return null;
      return path[path.length - 2]?.id ?? null;
    }
    return folderIdRef.current;
  }, []);

  /**
   * OS paths from Tauri's native drag-drop (required on Windows —
   * HTML5 dataTransfer.files is empty while dragDropEnabled is true).
   * `parentId` is the folder under the cursor (or current view).
   */
  const importOsPaths = useCallback(
    async (paths: string[], parentId: string | null) => {
      const files = paths.filter((p) => {
        const base = pathFileName(p);
        // Skip obvious directories / empty names; backend also rejects non-files.
        return Boolean(base) && !base.endsWith("/") && !base.endsWith("\\");
      });
      if (!files.length) return;
      if (importInFlight.current) return;
      importInFlight.current = true;
      setBusy(true);
      setError(null);
      try {
        let imported = 0;
        const errors: string[] = [];
        for (const p of files) {
          try {
            await api.importPath(p, pathFileName(p), parentId);
            imported += 1;
          } catch (e) {
            const msg = formatError(e);
            errors.push(`${pathFileName(p)}: ${msg}`);
          }
        }
        await refreshList();
        if (errors.length) {
          const head =
            imported > 0
              ? `Imported ${imported}, ${errors.length} failed. `
              : "Import failed. ";
          setError(head + errors.slice(0, 3).join(" · "));
        }
      } catch (e) {
        setError(formatError(e));
      } finally {
        setBusy(false);
        importInFlight.current = false;
      }
    },
    [refreshList],
  );

  function onFileInput(e: ChangeEvent<HTMLInputElement>) {
    const files = e.target.files;
    e.target.value = "";
    if (files?.length) void importFiles(files);
  }

  // Tauri native file drop — paths land here, not in HTML5 drop events.
  // Position hit-tests folder rows so OS drops can target a specific folder.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void (async () => {
      try {
        const webview = getCurrentWebview();
        unlisten = await webview.onDragDropEvent((event) => {
          const ev = event.payload;
          // Ignore native OS drag events while an in-vault item drag is active
          // so WebView2 doesn't clear our folder/parent highlight mid-move.
          if (draggingIdRef.current != null) return;

          if (ev.type === "enter" || ev.type === "over") {
            setOsDragOver(true);
            const { x, y } = physicalToCss(ev.position.x, ev.position.y);
            setVaultDropHighlight(dropTargetAtPoint(x, y));
          } else if (ev.type === "leave") {
            setOsDragOver(false);
            setVaultDropHighlight(null);
          } else if (ev.type === "drop") {
            const { x, y } = physicalToCss(ev.position.x, ev.position.y);
            const target = dropTargetAtPoint(x, y);
            setOsDragOver(false);
            setVaultDropHighlight(null);
            const paths = ev.paths ?? [];
            if (paths.length) {
              void importOsPaths(paths, resolveDropParentId(target));
            }
          }
        });
        if (cancelled) {
          unlisten?.();
          unlisten = undefined;
        }
      } catch {
        // Non-Tauri (vitest / plain browser) — Import… picker still works.
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [importOsPaths, resolveDropParentId]);

  async function moveVaultItems(ids: string[], parentId: string | null) {
    const unique = Array.from(new Set(ids.filter(Boolean)));
    if (!unique.length) return;
    if (moveInFlight.current) return;

    // Never move a folder into itself; skip those ids.
    const toMove = unique.filter((id) => id !== parentId);
    if (!toMove.length) return;

    moveInFlight.current = true;
    setBusy(true);
    setError(null);
    try {
      const errors: string[] = [];
      let moved = 0;
      for (const id of toMove) {
        try {
          await api.moveItem(id, parentId);
          moved += 1;
        } catch (e) {
          const msg = formatError(e);
          errors.push(msg);
        }
      }
      if (selectedId && toMove.includes(selectedId)) {
        setSelectedId(null);
        setDetail(null);
      }
      setSelectedIds((prev) => {
        const next = new Set(prev);
        for (const id of toMove) next.delete(id);
        return next;
      });
      await refreshList();
      if (errors.length) {
        const head =
          moved > 0
            ? `Moved ${moved}, ${errors.length} failed. `
            : "Move failed. ";
        setError(head + errors.slice(0, 3).join(" · "));
      }
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
      moveInFlight.current = false;
    }
  }

  function setVaultDropHighlight(target: DropTarget | null) {
    dropHighlightRef.current = target;
    setDropHighlight(target);
  }

  /**
   * In-vault moves complete on dragend (not HTML5 drop).
   * Tauri's native OS drag-drop often swallows drop events for internal drags
   * on Windows/WebView2 — highlight tracks the target; dragend commits the move.
   */
  function onVaultDragStart(e: ReactDragEvent, item: ItemPreview) {
    const ids = idsForDrag(item);
    draggingIdsRef.current = ids;
    e.dataTransfer.setData(VAULT_ITEM_MIME, ids.join(","));
    e.dataTransfer.setData("text/plain", ids.join(","));
    e.dataTransfer.effectAllowed = "move";
    // Empty drag image keeps hit-testing stable under the cursor.
    try {
      const img = document.createElement("canvas");
      img.width = 1;
      img.height = 1;
      e.dataTransfer.setDragImage(img, 0, 0);
    } catch {
      /* ignore */
    }
    draggingIdRef.current = item.id;
    setDraggingId(item.id);
    setVaultDropHighlight(null);
    skipClickAfterDragRef.current = true;
  }

  function onVaultDragEnd(e: ReactDragEvent) {
    const ids =
      draggingIdsRef.current.length > 0
        ? draggingIdsRef.current
        : draggingIdRef.current
          ? [draggingIdRef.current]
          : [];
    // Prefer live hit-test at release point; fall back to last highlight.
    const atPoint = dropTargetAtPoint(e.clientX, e.clientY);
    const highlighted = dropHighlightRef.current;
    let target: DropTarget = atPoint;
    // If release point is ambiguous ("current") but we were highlighting a
    // folder/parent, commit that move — WebView2 often reports stale coords.
    if (atPoint.kind === "current" && highlighted && highlighted.kind !== "current") {
      target = highlighted;
    }

    draggingIdRef.current = null;
    draggingIdsRef.current = [];
    setDraggingId(null);
    setVaultDropHighlight(null);
    // Swallow the synthetic click that follows dragend.
    window.setTimeout(() => {
      skipClickAfterDragRef.current = false;
    }, 0);

    if (!ids.length) return;
    if (target.kind === "current") return; // no-op drop on open folder
    const parentId = resolveDropParentId(target);
    // Don't drop a multi-selection into one of its own folders.
    if (target.kind === "folder" && ids.includes(target.id)) return;

    void moveVaultItems(ids, parentId);
  }

  /** True when the event originated on Remove / Open / other controls. */
  function isRowControlTarget(t: EventTarget | null): boolean {
    return (
      t instanceof Element &&
      !!t.closest("button, .row-actions, a, textarea, select, input")
    );
  }

  function onRowClick(e: React.MouseEvent, item: ItemPreview) {
    if (skipClickAfterDragRef.current) return;
    // Open / Remove must never change selection via the row handler.
    if (isRowControlTarget(e.target)) return;

    if (e.shiftKey) {
      e.preventDefault();
      selectRange(item.id);
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      toggleSelected(item.id);
      return;
    }
    // Plain single-click: select only (Windows Explorer style).
    // Folders/files open on double-click or Enter / Open.
    selectOnly(item.id);
  }

  function confirmLeaveIfDirty(): boolean {
    if (!dirty) return true;
    return window.confirm(
      "You have unsaved text changes. Leave without saving?",
    );
  }

  function openItemGuarded(item: ItemPreview) {
    if (skipClickAfterDragRef.current) return;
    if (!confirmLeaveIfDirty()) return;
    selectOnly(item.id);
    openItem(item);
  }

  function goUpGuarded() {
    if (skipClickAfterDragRef.current) return;
    if (!confirmLeaveIfDirty()) return;
    goUpOneLevel();
  }

  function onVaultDragOverTarget(e: ReactDragEvent, target: DropTarget) {
    // Accept vault-item drags; also accept when types are empty mid-drag (WebView2).
    const types = Array.from(e.dataTransfer.types || []);
    const isVault =
      types.includes(VAULT_ITEM_MIME) ||
      types.includes("text/plain") ||
      draggingIdRef.current != null;
    if (!isVault) return;
    e.preventDefault();
    e.stopPropagation();
    e.dataTransfer.dropEffect = "move";
    setVaultDropHighlight(target);
  }

  function onVaultDropOnTarget(e: ReactDragEvent, target: DropTarget) {
    // Best-effort if the browser does fire drop; primary path is dragend.
    const types = Array.from(e.dataTransfer.types || []);
    const isVault =
      types.includes(VAULT_ITEM_MIME) ||
      types.includes("text/plain") ||
      draggingIdRef.current != null;
    if (!isVault) return;
    e.preventDefault();
    e.stopPropagation();
    setVaultDropHighlight(target);
    const raw =
      e.dataTransfer.getData(VAULT_ITEM_MIME) ||
      e.dataTransfer.getData("text/plain") ||
      "";
    const ids = raw
      ? raw.split(",").map((s) => s.trim()).filter(Boolean)
      : draggingIdsRef.current.length
        ? draggingIdsRef.current
        : draggingIdRef.current
          ? [draggingIdRef.current]
          : [];
    draggingIdRef.current = null;
    draggingIdsRef.current = [];
    setDraggingId(null);
    setVaultDropHighlight(null);
    if (!ids.length) return;
    if (target.kind === "folder" && ids.includes(target.id)) return;
    if (target.kind === "current") return;
    void moveVaultItems(ids, resolveDropParentId(target));
  }

  function goUpOneLevel() {
    const path = crumbs;
    if (path.length === 0) return;
    const parentId = path.length === 1 ? null : path[path.length - 2].id;
    goCrumb(parentId);
  }

  async function onSaveText() {
    if (!detail || detail.kind !== "text") return;
    setBusy(true);
    setError(null);
    try {
      const updated = await api.updateText(
        detail.id,
        editBody,
        editName !== detail.name ? editName : null,
      );
      setDirty(false);
      setSelectedId(updated.id);
      await refreshList();
      const d = await api.getItem(updated.id);
      setDetail(d);
      setEditName(d.name);
      setEditBody(d.text ?? "");
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
    }
  }

  function onRename() {
    if (!detail) return;
    setNameDialog({
      mode: "rename",
      title: "Rename",
      value: detail.name,
      itemId: detail.id,
    });
  }

  async function submitNameDialog() {
    if (!nameDialog) return;
    const name = nameDialog.value.trim();
    if (!name) return;
    if (nameDialog.mode === "rename") {
      if (!nameDialog.itemId || name === detail?.name) {
        setNameDialog(null);
        return;
      }
    }
    const dialog = nameDialog;
    setNameDialog(null);
    setBusy(true);
    setError(null);
    try {
      if (dialog.mode === "folder") {
        await api.createFolder(name, folderId);
        await refreshList();
      } else if (dialog.itemId) {
        await api.renameItem(dialog.itemId, name);
        setEditName(name);
        await refreshList();
        const d = await api.getItem(dialog.itemId);
        setDetail(d);
      }
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
    }
  }

  async function onDelete(target?: ItemPreview) {
    // Prefer explicit target; else multi-selection; else open detail.
    let candidates: ItemPreview[] = [];
    if (target) {
      candidates = [target];
    } else if (selectedIds.size > 0) {
      candidates = items.filter((i) => selectedIds.has(i.id));
    } else if (detail) {
      candidates = [
        {
          id: detail.id,
          name: detail.name,
          mime: detail.mime,
          size: detail.size,
          kind: detail.kind,
          parentId: detail.parentId,
          createdAt: detail.createdAt,
          updatedAt: detail.updatedAt,
        },
      ];
    }
    if (!candidates.length) return;

    setBusy(true);
    setError(null);
    try {
      const prepared: Array<{
        id: string;
        name: string;
        kind: ItemPreview["kind"];
        childCount: number;
      }> = [];
      for (const item of candidates) {
        let childCount = 0;
        if (item.kind === "folder") {
          childCount = await api.folderContentCount(item.id);
        }
        prepared.push({
          id: item.id,
          name: item.name,
          kind: item.kind,
          childCount,
        });
      }
      setPendingDelete({ items: prepared });
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
    }
  }

  async function confirmDelete(cascade: boolean) {
    if (!pendingDelete) return;
    const batch = pendingDelete.items;
    setPendingDelete(null);
    setBusy(true);
    setError(null);
    try {
      const errors: string[] = [];
      let deleted = 0;
      let leftCurrentFolder = false;
      for (const item of batch) {
        try {
          const needsCascade = item.kind === "folder" && item.childCount > 0;
          await api.deleteItem(item.id, needsCascade ? cascade : false);
          deleted += 1;
          if (folderId === item.id) leftCurrentFolder = true;
        } catch (e) {
          const msg = formatError(e);
          errors.push(`${item.name}: ${msg}`);
        }
      }
      const deletedIds = new Set(batch.map((i) => i.id));
      if (
        (selectedId && deletedIds.has(selectedId)) ||
        (detail && deletedIds.has(detail.id))
      ) {
        setSelectedId(null);
        setDetail(null);
      }
      setSelectedIds((prev) => {
        const next = new Set(prev);
        for (const id of deletedIds) next.delete(id);
        return next;
      });
      // If we deleted the folder we're currently inside, go up.
      if (leftCurrentFolder) {
        const parent = crumbs.length >= 2 ? crumbs[crumbs.length - 2].id : null;
        setFolderId(parent);
      }
      await refreshList();
      if (errors.length) {
        const head =
          deleted > 0
            ? `Deleted ${deleted}, ${errors.length} failed. `
            : "Delete failed. ";
        setError(head + errors.slice(0, 3).join(" · "));
      }
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
    }
  }

  async function onExport() {
    if (!detail || detail.kind === "folder") return;
    setBusy(true);
    setError(null);
    try {
      const defaultName = detail.name || "export.bin";
      const dest = await save({
        defaultPath: defaultName,
        title: "Export from SecretFolder",
      });
      if (dest === null) return; // user cancelled
      await api.exportPath(detail.id, dest);
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
    }
  }

  async function onCopyText() {
    if (!detail || detail.kind !== "text") return;
    try {
      const res = await copySecret(editBody);
      if (!res.ok) throw new Error(res.error ?? "copy failed");
    } catch (e) {
      setError(formatError(e));
    }
  }

  function openItem(item: ItemPreview) {
    // Navigation into folders is always guarded by openItemGuarded / goCrumb.
    if (item.kind === "folder") {
      setSelectedId(null);
      setDetail(null);
      setDirty(false);
      setFolderId(item.id);
      return;
    }
    setSelectedId(item.id);
  }

  function goCrumb(id: string | null) {
    if (!confirmLeaveIfDirty()) return;
    setSelectedId(null);
    setDetail(null);
    setFolderId(id);
  }

  async function saveIdle() {
    setSettingsMsg(null);
    try {
      const secs = Math.max(60, Math.round(idleMins) * 60);
      await api.setIdleLockSecs(secs);
      setSettingsMsg(`Idle lock set to ${Math.round(secs / 60)} min.`);
    } catch (e) {
      setSettingsMsg(formatError(e));
    }
  }

  async function savePassword() {
    setSettingsMsg(null);
    if (newPw.length < 12) {
      setSettingsMsg("New password must be at least 12 characters.");
      return;
    }
    if (newPw !== newPw2) {
      setSettingsMsg("New passwords do not match.");
      return;
    }
    try {
      await api.changePassword(curPw, newPw);
      setCurPw("");
      setNewPw("");
      setNewPw2("");
      setSettingsMsg("Password changed.");
    } catch (e) {
      setSettingsMsg(formatError(e));
    }
  }

  async function openAboutLink(url: string) {
    try {
      await api.openExternal(url);
    } catch (e) {
      setUpdateMsg(formatError(e));
    }
  }

  async function onCheckUpdate() {
    setUpdateBusy(true);
    setUpdateMsg("Checking…");
    setPendingVersion(null);
    try {
      const res = await checkForAppUpdate();
      if (res.kind === "up-to-date") {
        setUpdateMsg(`You're on the latest version (${version}).`);
      } else if (res.kind === "available") {
        setPendingVersion(res.version);
        setUpdateMsg(`Update ${res.version} is ready to install.`);
      } else {
        setUpdateMsg(res.message);
      }
    } catch (e) {
      setUpdateMsg(formatError(e));
    } finally {
      setUpdateBusy(false);
    }
  }

  async function onInstallUpdate() {
    setUpdateBusy(true);
    setUpdateMsg("Downloading update…");
    try {
      const ok = await downloadAndInstallUpdate((pct) => {
        if (pct == null) {
          setUpdateMsg("Downloading update…");
        } else {
          setUpdateMsg(`Downloading update… ${pct}%`);
        }
      });
      if (!ok) {
        setUpdateMsg(`You're on the latest version (${version}).`);
        setPendingVersion(null);
      }
      // relaunch() exits the process on success
    } catch (e) {
      setUpdateMsg(formatError(e));
    } finally {
      setUpdateBusy(false);
    }
  }
  return (
    <div className="explorer">
      <header className="explorer-top">
        <div className="brand-inline">
          <span className="logo-mark brand-mark sm" aria-hidden>
            <img src={appMark} alt="" width={28} height={28} draggable={false} />
          </span>
          <strong>SecretFolder</strong>
          <span className="muted small">
            {status.itemCount} item{status.itemCount === 1 ? "" : "s"}
          </span>
        </div>
        <nav className="top-nav">
          <button
            type="button"
            className={panel === "files" ? "tab active" : "tab"}
            onClick={() => setPanel("files")}
          >
            Files
          </button>
          <button
            type="button"
            className={panel === "settings" ? "tab active" : "tab"}
            onClick={() => setPanel("settings")}
          >
            Settings
          </button>
          <button
            type="button"
            className={panel === "about" ? "tab active" : "tab"}
            onClick={() => setPanel("about")}
          >
            About
          </button>
        </nav>
        <div className="top-actions">
          <button type="button" className="btn ghost" onClick={() => void lockNow()}>
            Lock
          </button>
          <button
            type="button"
            className="btn ghost"
            onClick={() => void api.hideMain()}
          >
            Hide
          </button>
        </div>
      </header>

      {error && (
        <div className="banner error" role="alert">
          {error}
          <button type="button" className="linkish" onClick={() => setError(null)}>
            dismiss
          </button>
        </div>
      )}

      {panel === "files" && (
        <div
          className={
            osDragOver || draggingId
              ? "explorer-body drop-active"
              : "explorer-body"
          }
        >
          <aside className="file-list-pane">
            <div className="list-toolbar">
              <input
                className="search"
                placeholder="Search this folder…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              <div className="row gap wrap">
                <button
                  type="button"
                  className="btn primary sm"
                  disabled={busy}
                  onClick={() => void onNewText()}
                >
                  New text
                </button>
                <button
                  type="button"
                  className="btn sm"
                  disabled={busy}
                  onClick={() => void onNewFolder()}
                >
                  New folder
                </button>
                <button
                  type="button"
                  className="btn sm"
                  disabled={busy}
                  onClick={() => fileInputRef.current?.click()}
                >
                  Import…
                </button>
                <input
                  ref={fileInputRef}
                  type="file"
                  multiple
                  hidden
                  onChange={onFileInput}
                />
              </div>
              <div className="row gap wrap selection-bar" aria-live="polite">
                {selectedCount > 0 ? (
                  <>
                    <span className="muted small selection-count">
                      {selectedCount} selected
                    </span>
                    <button
                      type="button"
                      className="btn danger sm"
                      disabled={busy}
                      onClick={() => void onDelete()}
                    >
                      Delete
                    </button>
                    <button
                      type="button"
                      className="btn sm ghost"
                      disabled={busy}
                      onClick={() => clearSelection()}
                    >
                      Clear
                    </button>
                    <span className="muted small">
                      Ctrl+click · Shift+click · drag onto folder or ..
                    </span>
                  </>
                ) : (
                  <span className="muted small">
                    Click to select · double-click to open · Ctrl/Shift multi-select
                  </span>
                )}
              </div>
            </div>

            <nav className="breadcrumbs" aria-label="Folder path">
              <button
                type="button"
                className={!folderId ? "crumb active" : "crumb"}
                onClick={() => goCrumb(null)}
              >
                Vault
              </button>
              {crumbs.map((c) => (
                <span key={c.id} className="crumb-seg">
                  <span className="crumb-sep">/</span>
                  <button
                    type="button"
                    className={folderId === c.id ? "crumb active" : "crumb"}
                    onClick={() => goCrumb(c.id)}
                  >
                    {c.name}
                  </button>
                </span>
              ))}
              {folderId && (
                <button
                  type="button"
                  className="btn sm ghost crumb-up"
                  title="Go up one level"
                  onClick={() => goUpOneLevel()}
                >
                  ↑ Up
                </button>
              )}
            </nav>

            <ul
              className={
                dropHighlight?.kind === "current" && (osDragOver || draggingId)
                  ? "file-list drop-target-active"
                  : "file-list"
              }
              role="listbox"
              aria-multiselectable="true"
              aria-label="Vault items"
              tabIndex={0}
              onKeyDown={(e) => {
                const tag = (e.target as HTMLElement | null)?.tagName;
                if (
                  tag === "INPUT" ||
                  tag === "TEXTAREA" ||
                  tag === "SELECT" ||
                  tag === "BUTTON"
                ) {
                  return;
                }
                if ((e.ctrlKey || e.metaKey) && (e.key === "a" || e.key === "A")) {
                  e.preventDefault();
                  selectAllFiltered();
                  return;
                }
                if (
                  (e.key === "Delete" || e.key === "Backspace") &&
                  selectedIds.size > 0
                ) {
                  e.preventDefault();
                  void onDelete();
                }
              }}
              onDragOver={(e) => {
                if (draggingIdRef.current == null && !osDragOver) {
                  const types = Array.from(e.dataTransfer.types || []);
                  if (
                    !types.includes(VAULT_ITEM_MIME) &&
                    !types.includes("text/plain")
                  ) {
                    return;
                  }
                }
                // Track folder / parent under cursor for vault moves.
                const t = dropTargetAtPoint(e.clientX, e.clientY);
                if (draggingIdRef.current != null) {
                  onVaultDragOverTarget(e, t);
                }
              }}
              onDrop={(e) => {
                if (draggingIdRef.current == null) return;
                const t = dropTargetAtPoint(e.clientX, e.clientY);
                onVaultDropOnTarget(e, t);
              }}
              onDragLeave={(e) => {
                // Leaving the list entirely clears highlight (not entering a child).
                if (!e.currentTarget.contains(e.relatedTarget as Node)) {
                  if (!osDragOver && draggingIdRef.current == null) {
                    setVaultDropHighlight(null);
                  }
                }
              }}
            >
              {folderId && (
                <li
                  className={
                    dropHighlight?.kind === "parent"
                      ? "parent-up-row drop-target-active"
                      : "parent-up-row"
                  }
                  data-drop-parent="1"
                  onDragOver={(e) => onVaultDragOverTarget(e, { kind: "parent" })}
                  onDrop={(e) => onVaultDropOnTarget(e, { kind: "parent" })}
                  onClick={() => goUpGuarded()}
                  title="Go to parent folder — drop items here to move up"
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      goUpGuarded();
                    }
                  }}
                >
                  <div className="file-row parent-up">
                    <span className="file-ico" aria-hidden>
                      ⬆
                    </span>
                    <span className="file-meta">
                      <span className="file-name">..</span>
                      <span className="muted small">
                        Parent folder
                        {crumbs.length
                          ? ` · ${
                              crumbs.length === 1
                                ? "Vault"
                                : crumbs[crumbs.length - 2]?.name
                            }`
                          : ""}
                        {" · drop here to move up"}
                      </span>
                    </span>
                  </div>
                </li>
              )}
              {filtered.length === 0 && !folderId && (
                <li className="empty muted">
                  {osDragOver
                    ? "Drop files to import…"
                    : "Empty folder — drop files here or use Import."}
                </li>
              )}
              {filtered.length === 0 && folderId && (
                <li className="empty muted">
                  {osDragOver
                    ? dropHighlight?.kind === "folder"
                      ? `Drop into ${dropHighlight.name}…`
                      : dropHighlight?.kind === "parent"
                        ? "Drop to move/import to parent…"
                        : "Drop files to import…"
                    : "Empty — drop files here, or drag items onto .. to move up."}
                </li>
              )}
              {filtered.map((item) => {
                const isFolder = item.kind === "folder";
                const isSelected = selectedIds.has(item.id);
                const isPrimary = selectedId === item.id;
                const isDropTarget =
                  isFolder &&
                  dropHighlight?.kind === "folder" &&
                  dropHighlight.id === item.id;
                const isDragging =
                  draggingId === item.id ||
                  (draggingId != null &&
                    selectedIds.has(item.id) &&
                    selectedIds.has(draggingId));
                return (
                  <li
                    key={item.id}
                    className={[
                      isDropTarget ? "drop-target-active" : "",
                      isDragging ? "is-dragging" : "",
                      isSelected ? "is-multi-selected" : "",
                    ]
                      .filter(Boolean)
                      .join(" ") || undefined}
                    data-drop-folder-id={isFolder ? item.id : undefined}
                    data-drop-folder-name={isFolder ? item.name : undefined}
                    draggable={!busy}
                    onDragStart={(e) => onVaultDragStart(e, item)}
                    onDragEnd={onVaultDragEnd}
                    onDragOver={
                      isFolder
                        ? (e) =>
                            onVaultDragOverTarget(e, {
                              kind: "folder",
                              id: item.id,
                              name: item.name,
                            })
                        : undefined
                    }
                    onDrop={
                      isFolder
                        ? (e) =>
                            onVaultDropOnTarget(e, {
                              kind: "folder",
                              id: item.id,
                              name: item.name,
                            })
                        : undefined
                    }
                    onClick={(e) => onRowClick(e, item)}
                    onDoubleClick={(e) => {
                      if (isRowControlTarget(e.target)) {
                        e.preventDefault();
                        e.stopPropagation();
                        return;
                      }
                      openItemGuarded(item);
                    }}
                    role="option"
                    aria-selected={isSelected || isPrimary}
                    tabIndex={0}
                    onKeyDown={(e) => {
                      if (isRowControlTarget(e.target)) return;
                      if (e.key === "Enter") {
                        e.preventDefault();
                        openItemGuarded(item);
                      } else if (e.key === " ") {
                        // Space toggles multi-select (does not open).
                        e.preventDefault();
                        toggleSelected(item.id);
                      } else if (
                        (e.ctrlKey || e.metaKey) &&
                        (e.key === "a" || e.key === "A")
                      ) {
                        e.preventDefault();
                        selectAllFiltered();
                      } else if (e.key === "Delete" || e.key === "Backspace") {
                        if (selectedIds.size > 0 || isPrimary) {
                          e.preventDefault();
                          void onDelete();
                        }
                      }
                    }}
                  >
                    <div
                      className={
                        isPrimary || isSelected ? "file-row active" : "file-row"
                      }
                    >
                      <span className="file-ico" aria-hidden>
                        {kindIcon(item.kind)}
                      </span>
                      <span className="file-meta">
                        <span className="file-name">{item.name}</span>
                        <span className="muted small">
                          {item.kind === "folder"
                            ? "Folder · double-click or Open · drop to move/import"
                            : `${item.kind} · ${formatBytes(item.size)}`}
                        </span>
                      </span>
                      <span className="row-actions">
                        {isFolder && (
                          <button
                            type="button"
                            className="btn ghost sm row-action-btn"
                            disabled={busy}
                            title={`Open folder ${item.name}`}
                            aria-label={`Open folder ${item.name}`}
                            onClick={(e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              openItemGuarded(item);
                            }}
                            onPointerDown={(e) => e.stopPropagation()}
                            onMouseDown={(e) => e.stopPropagation()}
                          >
                            Open
                          </button>
                        )}
                        <button
                          type="button"
                          className="btn danger sm row-action-btn"
                          disabled={busy}
                          title={
                            item.kind === "folder"
                              ? "Remove folder"
                              : "Remove item"
                          }
                          aria-label={
                            item.kind === "folder"
                              ? `Remove folder ${item.name}`
                              : `Remove ${item.name}`
                          }
                          onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            void onDelete(item);
                          }}
                          onPointerDown={(e) => e.stopPropagation()}
                          onMouseDown={(e) => e.stopPropagation()}
                        >
                          Remove
                        </button>
                      </span>
                    </div>
                  </li>
                );
              })}
            </ul>
            {osDragOver && dropHighlight?.kind === "current" && (
              <div className="drop-overlay" aria-hidden>
                Drop to import into{" "}
                {crumbs.length ? crumbs[crumbs.length - 1].name : "Vault"}
              </div>
            )}
            {osDragOver && dropHighlight?.kind === "folder" && (
              <div className="drop-overlay" aria-hidden>
                Drop to import into {dropHighlight.name}
              </div>
            )}
            {osDragOver && dropHighlight?.kind === "parent" && (
              <div className="drop-overlay" aria-hidden>
                Drop to import into parent folder
              </div>
            )}
          </aside>
          <main className="detail-pane">
            {!detail && (
              <div className="detail-empty muted">
                <p>
                  {selectedId &&
                  items.find((i) => i.id === selectedId)?.kind === "folder"
                    ? "Folder selected — double-click or press Open to enter."
                    : "Select a file to view or edit."}
                </p>
                <p className="small">
                  Click selects · double-click opens · Ctrl/Shift multi-select.
                  Drop OS files onto the list or a folder row to import. Drag
                  vault items onto a folder or <strong>..</strong> to move them.
                </p>
              </div>
            )}

            {detail && detail.kind === "text" && (
              <div className="detail-editor">
                <div className="detail-toolbar">
                  <input
                    className="name-input"
                    value={editName}
                    onChange={(e) => {
                      setEditName(e.target.value);
                      setDirty(true);
                    }}
                  />
                  <div className="row gap">
                    <button
                      type="button"
                      className="btn primary sm"
                      disabled={busy || !dirty}
                      onClick={() => void onSaveText()}
                    >
                      Save
                    </button>
                    <button
                      type="button"
                      className="btn sm"
                      onClick={() => void onCopyText()}
                    >
                      Copy
                    </button>
                    <button
                      type="button"
                      className="btn sm"
                      onClick={() => void onExport()}
                    >
                      Export
                    </button>
                    <button
                      type="button"
                      className="btn danger sm"
                      onClick={() => void onDelete()}
                    >
                      Delete
                    </button>
                  </div>
                </div>
                <textarea
                  className="editor"
                  value={editBody}
                  onChange={(e) => {
                    setEditBody(e.target.value);
                    setDirty(true);
                  }}
                  spellCheck={false}
                />
              </div>
            )}

            {detail && detail.kind === "image" && (
              <div className="detail-media">
                <div className="detail-toolbar">
                  <h2 className="detail-title">{detail.name}</h2>
                  <div className="row gap">
                    <button
                      type="button"
                      className="btn sm"
                      onClick={() => void onRename()}
                    >
                      Rename
                    </button>
                    <button
                      type="button"
                      className="btn sm"
                      onClick={() => void onExport()}
                    >
                      Export
                    </button>
                    <button
                      type="button"
                      className="btn danger sm"
                      onClick={() => void onDelete()}
                    >
                      Delete
                    </button>
                  </div>
                </div>
                {detail.dataUrl ? (
                  <img
                    className="preview-img"
                    src={detail.dataUrl}
                    alt={detail.name}
                  />
                ) : (
                  <p className="muted">No preview</p>
                )}
                <p className="muted small">
                  {detail.mime} · {formatBytes(detail.size)}
                </p>
              </div>
            )}

            {detail && detail.kind === "binary" && (
              <div className="detail-media">
                <div className="detail-toolbar">
                  <h2 className="detail-title">{detail.name}</h2>
                  <div className="row gap">
                    <button
                      type="button"
                      className="btn sm"
                      onClick={() => void onRename()}
                    >
                      Rename
                    </button>
                    <button
                      type="button"
                      className="btn sm"
                      onClick={() => void onExport()}
                    >
                      Export
                    </button>
                    <button
                      type="button"
                      className="btn danger sm"
                      onClick={() => void onDelete()}
                    >
                      Delete
                    </button>
                  </div>
                </div>
                <p className="muted">
                  Binary file — export to open with another app.
                </p>
                <p className="muted small">
                  {detail.mime} · {formatBytes(detail.size)}
                </p>
              </div>
            )}
          </main>
        </div>
      )}

      {panel === "settings" && (
        <div className="settings-panel card">
          <h2>Settings</h2>
          <label className="field">
            <span>Idle lock (minutes)</span>
            <div className="row gap">
              <input
                type="number"
                min={1}
                max={240}
                value={idleMins}
                onChange={(e) => setIdleMins(Number(e.target.value))}
              />
              <button type="button" className="btn sm" onClick={() => void saveIdle()}>
                Save
              </button>
            </div>
          </label>
          <p className="muted small">
            Default is 15 minutes of inactivity. Set and save to change.
          </p>

          <h3>Change master password</h3>
          <label className="field">
            <span>Current password</span>
            <input
              type="password"
              value={curPw}
              onChange={(e) => setCurPw(e.target.value)}
            />
          </label>
          <label className="field">
            <span>New password</span>
            <input
              type="password"
              value={newPw}
              onChange={(e) => setNewPw(e.target.value)}
            />
          </label>
          <label className="field">
            <span>Confirm new password</span>
            <input
              type="password"
              value={newPw2}
              onChange={(e) => setNewPw2(e.target.value)}
            />
          </label>
          <button type="button" className="btn primary" onClick={() => void savePassword()}>
            Change password
          </button>
          {settingsMsg && <p className="muted">{settingsMsg}</p>}
          <p className="muted small">
            Recovery key is unchanged when you change the master password.
          </p>
        </div>
      )}

      {panel === "about" && (
        <div
          className="settings-panel card about-panel-compact"
          aria-label="About SecretFolder"
        >
          <div className="about-head">
            <div className="logo-mark about-mark" aria-hidden>
              <img src={appMark} alt="" width={36} height={36} draggable={false} />
            </div>
            <div>
              <h2>About</h2>
              <p className="about-hello">Hi I&apos;m Ahmi, hope this helps!</p>
            </div>
          </div>
          <p className="muted small">
            Local-only encrypted folder vault for Windows · MIT · v{version}
          </p>
          <p className="muted small">
            Vault data stays on this PC. Network is only for signed GitHub updates.
          </p>
          <div className="about-links">
            <button
              type="button"
              className="btn sm primary"
              onClick={() => void openAboutLink(GITHUB_PROFILE)}
            >
              GitHub
            </button>
            <button
              type="button"
              className="btn sm"
              onClick={() => void openAboutLink(GITHUB_REPO)}
            >
              Repo
            </button>
            <button
              type="button"
              className="btn sm"
              onClick={() => void openAboutLink(GITHUB_RELEASES)}
            >
              Releases
            </button>
            <button
              type="button"
              className="btn sm"
              onClick={() => void openAboutLink(PATREON_URL)}
            >
              Patreon
            </button>
          </div>
          <div className="about-update">
            <button
              type="button"
              className="btn sm"
              disabled={busy || updateBusy}
              onClick={() => void onCheckUpdate()}
            >
              {updateBusy && !pendingVersion
                ? "Checking…"
                : "Check for updates"}
            </button>
            {pendingVersion && (
              <button
                type="button"
                className="btn sm primary"
                disabled={busy || updateBusy}
                onClick={() => void onInstallUpdate()}
              >
                {updateBusy
                  ? "Installing…"
                  : `Install ${pendingVersion} & restart`}
              </button>
            )}
          </div>
          {updateMsg && (
            <p
              className={
                updateMsg.startsWith("You're on") ||
                updateMsg.startsWith("Update ")
                  ? "muted ok"
                  : "muted"
              }
            >
              {updateMsg}
            </p>
          )}
        </div>
      )}

      {pendingDelete && (() => {
        const batch = pendingDelete.items;
        const multi = batch.length > 1;
        const nonEmptyFolders = batch.filter(
          (i) => i.kind === "folder" && i.childCount > 0,
        );
        const needsCascade = nonEmptyFolders.length > 0;
        const totalNested = nonEmptyFolders.reduce(
          (n, i) => n + i.childCount,
          0,
        );
        const title = multi
          ? needsCascade
            ? `Delete ${batch.length} items?`
            : `Delete ${batch.length} items?`
          : needsCascade
            ? "Folder is not empty"
            : batch[0].kind === "folder"
              ? "Delete empty folder?"
              : "Delete item?";
        const nameList =
          batch.length <= 3
            ? batch.map((i) => `“${i.name}”`).join(", ")
            : `${batch
                .slice(0, 2)
                .map((i) => `“${i.name}”`)
                .join(", ")} and ${batch.length - 2} more`;
        return (
          <div
            className="modal-backdrop"
            role="presentation"
            onClick={() => !busy && setPendingDelete(null)}
          >
            <div
              className="modal-dialog"
              role="alertdialog"
              aria-modal="true"
              aria-labelledby="delete-dialog-title"
              onClick={(e) => e.stopPropagation()}
            >
              <h2 id="delete-dialog-title">{title}</h2>
              {needsCascade ? (
                <>
                  <p>
                    {multi ? (
                      <>
                        Delete{" "}
                        <span className="modal-note-name">{nameList}</span>
                        ?{" "}
                        <strong>
                          {nonEmptyFolders.length} folder
                          {nonEmptyFolders.length === 1 ? "" : "s"}
                        </strong>{" "}
                        still hold{" "}
                        <strong>
                          {totalNested} nested item
                          {totalNested === 1 ? "" : "s"}
                        </strong>
                        .
                      </>
                    ) : (
                      <>
                        <span className="modal-note-name">
                          “{batch[0].name}”
                        </span>{" "}
                        still has{" "}
                        <strong>
                          {batch[0].childCount} item
                          {batch[0].childCount === 1 ? "" : "s"}
                        </strong>{" "}
                        inside.
                      </>
                    )}
                  </p>
                  <p className="muted small">
                    Empty folders first, or permanently delete everything
                    selected (including nested contents). This cannot be undone.
                  </p>
                  <div className="modal-actions">
                    <button
                      type="button"
                      className="btn sm"
                      disabled={busy}
                      onClick={() => setPendingDelete(null)}
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      className="btn danger-solid sm"
                      disabled={busy}
                      onClick={() => void confirmDelete(true)}
                    >
                      {multi
                        ? "Delete all & contents"
                        : "Delete folder & contents"}
                    </button>
                  </div>
                </>
              ) : (
                <>
                  <p>
                    Delete{" "}
                    <span className="modal-note-name">{nameList}</span>?
                  </p>
                  <p className="muted small">This cannot be undone.</p>
                  <div className="modal-actions">
                    <button
                      type="button"
                      className="btn sm"
                      disabled={busy}
                      onClick={() => setPendingDelete(null)}
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      className="btn danger-solid sm"
                      disabled={busy}
                      onClick={() => void confirmDelete(false)}
                    >
                      {multi
                        ? `Delete ${batch.length}`
                        : batch[0].kind === "folder"
                          ? "Delete folder"
                          : "Delete"}
                    </button>
                  </div>
                </>
              )}
            </div>
          </div>
        );
      })()}

      {nameDialog && (
        <div
          className="modal-backdrop"
          role="presentation"
          onClick={() => !busy && setNameDialog(null)}
        >
          <div
            className="modal-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="name-dialog-title"
            onClick={(e) => e.stopPropagation()}
          >
            <h2 id="name-dialog-title">{nameDialog.title}</h2>
            <label className="field">
              <span className="muted small">Name</span>
              <input
                ref={nameInputRef}
                className="name-input"
                value={nameDialog.value}
                onChange={(e) =>
                  setNameDialog({ ...nameDialog, value: e.target.value })
                }
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void submitNameDialog();
                  } else if (e.key === "Escape") {
                    e.preventDefault();
                    setNameDialog(null);
                  }
                }}
              />
            </label>
            <div className="modal-actions">
              <button
                type="button"
                className="btn sm"
                disabled={busy}
                onClick={() => setNameDialog(null)}
              >
                Cancel
              </button>
              <button
                type="button"
                className="btn primary sm"
                disabled={busy || !nameDialog.value.trim()}
                onClick={() => void submitNameDialog()}
              >
                {nameDialog.mode === "folder" ? "Create" : "Rename"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
