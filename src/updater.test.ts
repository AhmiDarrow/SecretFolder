import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock Tauri updater plugin
const mockCheck = vi.fn();

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: () => mockCheck(),
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(),
}));

import { checkForAppUpdate, downloadAndInstallUpdate } from "./updater";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("checkForAppUpdate", () => {
  it("returns up-to-date when check returns null", async () => {
    mockCheck.mockResolvedValue(null);
    const result = await checkForAppUpdate();
    expect(result).toEqual({ kind: "up-to-date" });
  });

  it("returns available with version and body", async () => {
    mockCheck.mockResolvedValue({
      version: "2.0.0",
      body: "Security fixes",
    });
    const result = await checkForAppUpdate();
    expect(result).toEqual({
      kind: "available",
      version: "2.0.0",
      body: "Security fixes",
    });
  });

  it("returns available with null body", async () => {
    mockCheck.mockResolvedValue({
      version: "1.5.0",
      body: null,
    });
    const result = await checkForAppUpdate();
    expect(result).toEqual({
      kind: "available",
      version: "1.5.0",
      body: null,
    });
  });

  it("returns error on exception", async () => {
    mockCheck.mockRejectedValue(new Error("network failure"));
    const result = await checkForAppUpdate();
    expect(result.kind).toBe("error");
    expect((result as { kind: "error"; message: string }).message).toContain(
      "network failure",
    );
  });

  it("returns error on non-Error thrown value", async () => {
    mockCheck.mockRejectedValue("just a string");
    const result = await checkForAppUpdate();
    expect(result.kind).toBe("error");
  });
});

describe("downloadAndInstallUpdate", () => {
  it("returns false when no update available", async () => {
    mockCheck.mockResolvedValue(null);
    const result = await downloadAndInstallUpdate();
    expect(result).toBe(false);
  });

  it("downloads, installs, calls progress, and returns true", async () => {
    // Build a mock update with a working downloadAndInstall
    const updateMock = {
      version: "2.0.0",
      downloadAndInstall: vi.fn(
        async (
          cb: (event: {
            event: string;
            data: { contentLength?: number; chunkLength?: number };
          }) => void,
        ) => {
          cb({ event: "Started", data: { contentLength: 200 } });
          cb({ event: "Progress", data: { chunkLength: 80 } });
          cb({ event: "Progress", data: { chunkLength: 80 } });
          cb({ event: "Progress", data: { chunkLength: 40 } });
          cb({ event: "Finished", data: {} });
        },
      ),
    };

    mockCheck.mockResolvedValue(updateMock);

    const progressCb = vi.fn();
    const result = await downloadAndInstallUpdate(progressCb);

    expect(result).toBe(true);
    expect(updateMock.downloadAndInstall).toHaveBeenCalledTimes(1);
    // Progress: Started→0, 80/200=40, 160/200=80, 200/200=100, Finished→100
    expect(progressCb).toHaveBeenCalledTimes(5);
    expect(progressCb).toHaveBeenNthCalledWith(1, 0);
    expect(progressCb).toHaveBeenNthCalledWith(2, 40);
    expect(progressCb).toHaveBeenNthCalledWith(3, 80);
    expect(progressCb).toHaveBeenNthCalledWith(4, 100);
    expect(progressCb).toHaveBeenNthCalledWith(5, 100);
  });

  it("calls progress with null when contentLength unknown", async () => {
    const updateMock = {
      version: "2.0.0",
      downloadAndInstall: vi.fn(
        async (
          cb: (event: {
            event: string;
            data: { contentLength?: number; chunkLength?: number };
          }) => void,
        ) => {
          cb({ event: "Started", data: {} }); // no contentLength
          cb({ event: "Progress", data: { chunkLength: 50 } });
          cb({ event: "Finished", data: {} });
        },
      ),
    };

    mockCheck.mockResolvedValue(updateMock);

    const progressCb = vi.fn();
    await downloadAndInstallUpdate(progressCb);

    // Started → 0, Progress → null (no contentLength), Finished → 100
    expect(progressCb).toHaveBeenNthCalledWith(1, 0);
    expect(progressCb).toHaveBeenNthCalledWith(2, null);
    expect(progressCb).toHaveBeenNthCalledWith(3, 100);
  });
});
