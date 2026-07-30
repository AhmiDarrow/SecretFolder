import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

import { api, fileToBase64 } from "./api";

describe("api surface", () => {
  it("exposes the invoke wrappers used by screens", () => {
    const keys = Object.keys(api).sort();
    expect(keys).toEqual(
      [
        "changePassword",
        "createFolder",
        "createText",
        "deleteItem",
        "exportPath",
        "folderContentCount",
        "folderPath",
        "getItem",
        "hideMain",
        "importBytes",
        "importPath",
        "listItems",
        "lock",
        "maxFileBytes",
        "moveItem",
        "openExternal",
        "quitApp",
        "renameItem",
        "setIdleLockSecs",
        "setup",
        "showMain",
        "status",
        "touch",
        "unlock",
        "unlockRecovery",
        "updateText",
      ].sort(),
    );
    // exportBytesB64 must stay removed (plaintext IPC channel)
    expect(keys).not.toContain("exportBytesB64");
  });
});

describe("api invoke wrappers", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("status / setup / unlock / lock", async () => {
    invoke.mockResolvedValue({ initialized: true, unlocked: false });
    await api.status();
    expect(invoke).toHaveBeenCalledWith("get_status");

    invoke.mockResolvedValue("RECOVERY-KEY");
    await expect(api.setup("hunter22")).resolves.toBe("RECOVERY-KEY");
    expect(invoke).toHaveBeenCalledWith("setup_vault", { password: "hunter22" });

    invoke.mockResolvedValue(undefined);
    await api.unlock("pw");
    expect(invoke).toHaveBeenCalledWith("unlock_vault", { password: "pw" });
    await api.unlockRecovery("rk");
    expect(invoke).toHaveBeenCalledWith("unlock_with_recovery", {
      recoveryKey: "rk",
    });
    await api.lock();
    expect(invoke).toHaveBeenCalledWith("lock_vault");
  });

  it("list / get / folder path", async () => {
    invoke.mockResolvedValue([]);
    await api.listItems();
    expect(invoke).toHaveBeenCalledWith("list_items", { parentId: null });
    await api.listItems("folder-1");
    expect(invoke).toHaveBeenCalledWith("list_items", { parentId: "folder-1" });
    await api.folderPath("folder-1");
    expect(invoke).toHaveBeenCalledWith("folder_path", { folderId: "folder-1" });
    invoke.mockResolvedValue({ id: "i1" });
    await api.getItem("i1");
    expect(invoke).toHaveBeenCalledWith("get_item", { id: "i1" });
  });

  it("create / update / rename / move / delete", async () => {
    invoke.mockResolvedValue({ id: "f1" });
    await api.createFolder("Docs");
    expect(invoke).toHaveBeenCalledWith("create_folder", {
      name: "Docs",
      parentId: null,
    });
    await api.createText("a.txt", "hi", "f1");
    expect(invoke).toHaveBeenCalledWith("create_text", {
      name: "a.txt",
      body: "hi",
      parentId: "f1",
    });
    await api.updateText("i1", "body", "renamed.txt");
    expect(invoke).toHaveBeenCalledWith("update_text", {
      id: "i1",
      name: "renamed.txt",
      body: "body",
    });
    await api.renameItem("i1", "new-name");
    expect(invoke).toHaveBeenCalledWith("rename_item", {
      id: "i1",
      name: "new-name",
    });
    await api.moveItem("i1", "f1");
    expect(invoke).toHaveBeenCalledWith("move_item", {
      id: "i1",
      parentId: "f1",
    });
    invoke.mockResolvedValue(undefined);
    await api.deleteItem("i1", true);
    expect(invoke).toHaveBeenCalledWith("delete_item", {
      id: "i1",
      cascade: true,
    });
  });

  it("import / export / max / password / window helpers", async () => {
    invoke.mockResolvedValue({ id: "imp" });
    await api.importPath("C:\\Users\\x\\Desktop\\a.bin", "a.bin", null);
    expect(invoke).toHaveBeenCalledWith("import_path", {
      path: "C:\\Users\\x\\Desktop\\a.bin",
      name: "a.bin",
      parentId: null,
    });
    await api.importBytes("b.bin", "application/octet-stream", "AQID", null);
    expect(invoke).toHaveBeenCalledWith("import_bytes", {
      name: "b.bin",
      mime: "application/octet-stream",
      dataB64: "AQID",
      parentId: null,
    });
    invoke.mockResolvedValue(undefined);
    await api.exportPath("i1", "C:\\Users\\x\\Desktop\\out.bin");
    expect(invoke).toHaveBeenCalledWith("export_path", {
      id: "i1",
      path: "C:\\Users\\x\\Desktop\\out.bin",
    });
    invoke.mockResolvedValue(52_428_800);
    await api.maxFileBytes();
    expect(invoke).toHaveBeenCalledWith("max_file_bytes");
    invoke.mockResolvedValue(undefined);
    await api.changePassword("old", "new");
    expect(invoke).toHaveBeenCalledWith("change_password", {
      currentPassword: "old",
      newPassword: "new",
    });
    await api.setIdleLockSecs(900);
    expect(invoke).toHaveBeenCalledWith("set_idle_lock_secs", { secs: 900 });
    await api.touch();
    expect(invoke).toHaveBeenCalledWith("touch_activity");
    await api.folderContentCount("f1");
    expect(invoke).toHaveBeenCalledWith("folder_content_count", { id: "f1" });
    await api.showMain();
    expect(invoke).toHaveBeenCalledWith("show_main_window");
    await api.hideMain();
    expect(invoke).toHaveBeenCalledWith("hide_main_window");
    await api.quitApp();
    expect(invoke).toHaveBeenCalledWith("quit_app");
    await api.openExternal("https://github.com/AhmiDarrow");
    expect(invoke).toHaveBeenCalledWith("open_external_url", {
      url: "https://github.com/AhmiDarrow",
    });
  });
});

describe("fileToBase64", () => {
  it("strips data URL prefix and round-trips bytes", async () => {
    const file = new File([new Uint8Array([1, 2, 3, 4])], "t.bin", {
      type: "application/octet-stream",
    });
    const b64 = await fileToBase64(file);
    expect(b64).not.toMatch(/^data:/);
    expect(b64.length).toBeGreaterThan(0);
    const bin = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    expect(Array.from(bin)).toEqual([1, 2, 3, 4]);
  });
});
