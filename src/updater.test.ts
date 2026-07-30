import { describe, it, expect, vi, beforeEach } from "vitest";

const check = vi.fn();
const relaunch = vi.fn();
const lock = vi.fn();

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: (...a: unknown[]) => check(...a),
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: (...a: unknown[]) => relaunch(...a),
}));

vi.mock("./api", () => ({
  api: {
    lock: (...a: unknown[]) => lock(...a),
  },
}));

describe("updater helpers", () => {
  beforeEach(() => {
    vi.resetModules();
    check.mockReset();
    relaunch.mockReset();
    lock.mockReset();
    lock.mockResolvedValue(undefined);
  });

  it("reports up-to-date when check returns null", async () => {
    check.mockResolvedValue(null);
    const { checkForAppUpdate } = await import("./updater");
    await expect(checkForAppUpdate()).resolves.toEqual({ kind: "up-to-date" });
  });

  it("reports available version", async () => {
    check.mockResolvedValue({ version: "9.9.9", body: "notes" });
    const { checkForAppUpdate } = await import("./updater");
    await expect(checkForAppUpdate()).resolves.toEqual({
      kind: "available",
      version: "9.9.9",
      body: "notes",
    });
  });

  it("maps check errors", async () => {
    check.mockRejectedValue(new Error("network down"));
    const { checkForAppUpdate } = await import("./updater");
    await expect(checkForAppUpdate()).resolves.toEqual({
      kind: "error",
      message: "Error: network down",
    });
  });

  it("downloadAndInstallUpdate returns false when nothing available", async () => {
    check.mockResolvedValue(null);
    const { downloadAndInstallUpdate } = await import("./updater");
    await expect(downloadAndInstallUpdate()).resolves.toBe(false);
    expect(lock).not.toHaveBeenCalled();
    expect(relaunch).not.toHaveBeenCalled();
  });

  it("locks vault before install even if lock fails", async () => {
    lock.mockRejectedValue(new Error("already locked"));
    const downloadAndInstall = vi.fn(async () => {});
    check.mockResolvedValue({
      version: "9.9.9",
      downloadAndInstall,
    });
    relaunch.mockResolvedValue(undefined);

    const { downloadAndInstallUpdate } = await import("./updater");
    await expect(downloadAndInstallUpdate()).resolves.toBe(true);
    expect(lock).toHaveBeenCalledOnce();
    expect(downloadAndInstall).toHaveBeenCalledOnce();
    expect(relaunch).toHaveBeenCalledOnce();
  });

  it("downloadAndInstallUpdate locks, installs, then relaunches", async () => {
    const order: string[] = [];
    lock.mockImplementation(async () => {
      order.push("lock");
    });
    const downloadAndInstall = vi.fn(
      async (
        cb: (e: {
          event: string;
          data?: { contentLength?: number; chunkLength?: number };
        }) => void,
      ) => {
        order.push("install");
        cb({ event: "Started", data: { contentLength: 100 } });
        cb({ event: "Progress", data: { chunkLength: 50 } });
        cb({ event: "Progress", data: { chunkLength: 50 } });
        cb({ event: "Finished" });
      },
    );
    check.mockResolvedValue({
      version: "9.9.9",
      downloadAndInstall,
    });
    relaunch.mockImplementation(async () => {
      order.push("relaunch");
    });

    const pcts: Array<number | null> = [];
    const { downloadAndInstallUpdate } = await import("./updater");
    await expect(
      downloadAndInstallUpdate((p) => {
        pcts.push(p);
      }),
    ).resolves.toBe(true);

    expect(order).toEqual(["lock", "install", "relaunch"]);
    expect(pcts).toEqual([0, 50, 100, 100]);
  });
});
