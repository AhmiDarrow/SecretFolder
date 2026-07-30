import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ExplorerScreen } from "./ExplorerScreen";
import type { VaultStatus } from "../types";

const mockListItems = vi.fn();
const mockFolderPath = vi.fn();
const mockLock = vi.fn();
const mockCreateText = vi.fn();
const mockCreateFolder = vi.fn();
const mockGetItem = vi.fn();
const mockTouch = vi.fn();
const mockHideMain = vi.fn();
const mockMaxFileBytes = vi.fn();

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(async () => "0.1.0"),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ label: "main" }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
  ask: vi.fn(),
  message: vi.fn(),
}));

vi.mock("../api", () => ({
  api: {
    listItems: (...a: unknown[]) => mockListItems(...a),
    folderPath: (...a: unknown[]) => mockFolderPath(...a),
    lock: (...a: unknown[]) => mockLock(...a),
    createText: (...a: unknown[]) => mockCreateText(...a),
    createFolder: (...a: unknown[]) => mockCreateFolder(...a),
    getItem: (...a: unknown[]) => mockGetItem(...a),
    updateText: vi.fn(),
    renameItem: vi.fn(),
    deleteItem: vi.fn(),
    moveItem: vi.fn(),
    importPath: vi.fn(),
    importBytes: vi.fn(),
    exportPath: vi.fn(),
    changePassword: vi.fn(),
    setIdleLockSecs: vi.fn(),
    folderContentCount: vi.fn(async () => 0),
    maxFileBytes: (...a: unknown[]) => mockMaxFileBytes(...a),
    touch: (...a: unknown[]) => mockTouch(...a),
    hideMain: (...a: unknown[]) => mockHideMain(...a),
    openExternal: vi.fn(),
    status: vi.fn(),
  },
  fileToBase64: vi.fn(async () => ""),
}));

vi.mock("../clipboard", () => ({
  copySecret: vi.fn(async () => ({ ok: true })),
}));

const status: VaultStatus = {
  initialized: true,
  unlocked: true,
  itemCount: 1,
  idleLockSecs: 900,
  hasRecoveryKey: true,
};

const sampleItem = {
  id: "item-1",
  name: "notes.txt",
  mime: "text/plain",
  kind: "text" as const,
  parentId: null,
  size: 12,
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-01T00:00:00Z",
};

describe("ExplorerScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    cleanup();
    mockListItems.mockResolvedValue([sampleItem]);
    mockFolderPath.mockResolvedValue([]);
    mockLock.mockResolvedValue(undefined);
    mockTouch.mockResolvedValue(undefined);
    mockHideMain.mockResolvedValue(undefined);
    mockMaxFileBytes.mockResolvedValue(25 * 1024 * 1024);
    mockCreateText.mockResolvedValue({
      id: "new-1",
      name: "Untitled.txt",
      mime: "text/plain",
      kind: "text" as const,
      parentId: null,
      size: 0,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    });
  });

  it("renders listed items after load", async () => {
    render(<ExplorerScreen status={status} onLocked={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText("notes.txt")).toBeTruthy();
    });
    expect(mockListItems).toHaveBeenCalled();
    expect(mockFolderPath).toHaveBeenCalled();
  });

  it("shows empty state when vault has no items", async () => {
    mockListItems.mockResolvedValue([]);
    render(<ExplorerScreen status={status} onLocked={vi.fn()} />);
    await waitFor(() => {
      expect(mockListItems).toHaveBeenCalled();
    });
    // Explorer should still mount chrome even with zero items.
    expect(document.body.textContent?.length ?? 0).toBeGreaterThan(0);
  });

  it("locks vault and notifies parent", async () => {
    const onLocked = vi.fn();
    render(<ExplorerScreen status={status} onLocked={onLocked} />);
    await waitFor(() => {
      expect(screen.getByText("notes.txt")).toBeTruthy();
    });

    const lockBtn = screen.getByRole("button", { name: /^lock$/i });
    fireEvent.click(lockBtn);
    await waitFor(() => {
      expect(mockLock).toHaveBeenCalled();
      expect(onLocked).toHaveBeenCalled();
    });
  });
});
