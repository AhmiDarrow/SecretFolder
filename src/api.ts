import { invoke } from "@tauri-apps/api/core";
import type { ItemDetail, ItemPreview, VaultStatus } from "./types";

/** Thin invoke wrappers — names match screen call sites. */
export const api = {
  status: () => invoke<VaultStatus>("get_status"),
  setup: (password: string) => invoke<string>("setup_vault", { password }),
  unlock: (password: string) => invoke<void>("unlock_vault", { password }),
  unlockRecovery: (recoveryKey: string) =>
    invoke<void>("unlock_with_recovery", { recoveryKey }),
  lock: () => invoke<void>("lock_vault"),
  touch: () => invoke<void>("touch_activity"),
  setIdleLockSecs: (secs: number) =>
    invoke<void>("set_idle_lock_secs", { secs }),
  changePassword: (current: string, newPassword: string) =>
    invoke<void>("change_password", { currentPassword: current, newPassword }),

  listItems: (parentId?: string | null) =>
    invoke<ItemPreview[]>("list_items", { parentId: parentId ?? null }),
  folderPath: (folderId?: string | null) =>
    invoke<ItemPreview[]>("folder_path", { folderId: folderId ?? null }),
  getItem: (id: string) => invoke<ItemDetail>("get_item", { id }),
  createFolder: (name: string, parentId?: string | null) =>
    invoke<ItemPreview>("create_folder", {
      name,
      parentId: parentId ?? null,
    }),
  moveItem: (id: string, parentId?: string | null) =>
    invoke<ItemPreview>("move_item", { id, parentId: parentId ?? null }),
  createText: (name: string, body = "", parentId?: string | null) =>
    invoke<ItemPreview>("create_text", {
      name,
      body,
      parentId: parentId ?? null,
    }),
  updateText: (id: string, body: string, name?: string | null) =>
    invoke<ItemPreview>("update_text", {
      id,
      name: name ?? null,
      body,
    }),
  renameItem: (id: string, name: string) =>
    invoke<ItemPreview>("rename_item", { id, name }),
  folderContentCount: (id: string) =>
    invoke<number>("folder_content_count", { id }),
  deleteItem: (id: string, cascade = false) =>
    invoke<void>("delete_item", { id, cascade }),

  importPath: (path: string, name?: string | null, parentId?: string | null) =>
    invoke<ItemPreview>("import_path", {
      path,
      name: name ?? null,
      parentId: parentId ?? null,
    }),
  importBytes: (
    name: string,
    mime: string,
    dataB64: string,
    parentId?: string | null,
  ) =>
    invoke<ItemPreview>("import_bytes", {
      name,
      mime,
      dataB64,
      parentId: parentId ?? null,
    }),
  exportPath: (id: string, path: string) =>
    invoke<void>("export_path", { id, path }),
  maxFileBytes: () => invoke<number>("max_file_bytes"),

  showMain: () => invoke<void>("show_main_window"),
  hideMain: () => invoke<void>("hide_main_window"),
  quitApp: () => invoke<void>("quit_app"),
  openExternal: (url: string) => invoke<void>("open_external_url", { url }),
};

/** Read a browser File as base64 (no data: prefix). */
export function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result;
      if (typeof result !== "string") {
        reject(new Error("failed to read file"));
        return;
      }
      const comma = result.indexOf(",");
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.onerror = () => reject(reader.error ?? new Error("read failed"));
    reader.readAsDataURL(file);
  });
}
