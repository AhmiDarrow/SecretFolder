import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import App from "./App";

const { mockListen, mockGetCurrentWindow } = vi.hoisted(() => ({
  mockListen: vi.fn(async () => () => {}),
  mockGetCurrentWindow: vi.fn(() => ({ label: "main" })),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => mockListen(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => mockGetCurrentWindow(),
}));

const {
  mockStatus,
  mockListItems,
  mockFolderPath,
  mockSetup,
  mockUnlock,
  mockUnlockRecovery,
  mockLock,
  mockCreateText,
  mockCreateFolder,
  mockGetItem,
  mockUpdateText,
  mockRenameItem,
  mockDeleteItem,
  mockMoveItem,
  mockImportPath,
  mockExportPath,
  mockChangePassword,
} = vi.hoisted(() => ({
  mockStatus: vi.fn(),
  mockListItems: vi.fn(),
  mockFolderPath: vi.fn(),
  mockSetup: vi.fn(),
  mockUnlock: vi.fn(),
  mockUnlockRecovery: vi.fn(),
  mockLock: vi.fn(),
  mockCreateText: vi.fn(),
  mockCreateFolder: vi.fn(),
  mockGetItem: vi.fn(),
  mockUpdateText: vi.fn(),
  mockRenameItem: vi.fn(),
  mockDeleteItem: vi.fn(),
  mockMoveItem: vi.fn(),
  mockImportPath: vi.fn(),
  mockExportPath: vi.fn(),
  mockChangePassword: vi.fn(),
}));

vi.mock("./api", () => ({
  api: {
    status: (...args: unknown[]) => mockStatus(...args),
    listItems: (...args: unknown[]) => mockListItems(...args),
    folderPath: (...args: unknown[]) => mockFolderPath(...args),
    setup: (...args: unknown[]) => mockSetup(...args),
    unlock: (...args: unknown[]) => mockUnlock(...args),
    unlockRecovery: (...args: unknown[]) => mockUnlockRecovery(...args),
    lock: (...args: unknown[]) => mockLock(...args),
    createText: (...args: unknown[]) => mockCreateText(...args),
    createFolder: (...args: unknown[]) => mockCreateFolder(...args),
    getItem: (...args: unknown[]) => mockGetItem(...args),
    updateText: (...args: unknown[]) => mockUpdateText(...args),
    renameItem: (...args: unknown[]) => mockRenameItem(...args),
    deleteItem: (...args: unknown[]) => mockDeleteItem(...args),
    moveItem: (...args: unknown[]) => mockMoveItem(...args),
    importPath: (...args: unknown[]) => mockImportPath(...args),
    exportPath: (...args: unknown[]) => mockExportPath(...args),
    changePassword: (...args: unknown[]) => mockChangePassword(...args),
  },
}));

// Import React inside the factory so the mock uses the same React instance
// Vitest resolves for the test file (avoids dual-React element identity errors).
vi.mock("./screens/UnlockScreen", async () => {
  const R = await import("react");
  return {
    UnlockScreen: ({
      initialized,
      pendingRecoveryKey,
    }: {
      initialized: boolean;
      pendingRecoveryKey: string | null;
    }) =>
      R.createElement(
        "div",
        { "data-testid": "unlock-screen" },
        `unlock:${initialized ? "yes" : "no"}${
          pendingRecoveryKey ? ` key:${pendingRecoveryKey}` : ""
        }`,
      ),
  };
});

vi.mock("./screens/ExplorerScreen", async () => {
  const R = await import("react");
  return {
    ExplorerScreen: () =>
      R.createElement(
        "div",
        { "data-testid": "explorer-screen" },
        "explorer",
      ),
  };
});

describe("App routing", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListItems.mockResolvedValue([]);
    mockFolderPath.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
  });

  it("shows boot error when status fails", async () => {
    mockStatus.mockRejectedValue(new Error("vault offline"));
    render(<App />);
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/vault offline/i);
    });
  });

  it("routes locked vault to unlock screen", async () => {
    mockStatus.mockResolvedValue({
      initialized: true,
      unlocked: false,
      itemCount: 0,
      idleLockSecs: 900,
      hasRecoveryKey: true,
    });
    render(<App />);
    await waitFor(() => {
      expect(screen.getByTestId("unlock-screen").textContent).toContain(
        "unlock:yes",
      );
    });
  });

  it("routes unlocked vault to explorer", async () => {
    mockStatus.mockResolvedValue({
      initialized: true,
      unlocked: true,
      itemCount: 2,
      idleLockSecs: 900,
      hasRecoveryKey: true,
    });
    render(<App />);
    await waitFor(() => {
      expect(screen.getByTestId("explorer-screen")).toBeTruthy();
    });
  });

  it("routes uninitialized vault to unlock/setup", async () => {
    mockStatus.mockResolvedValue({
      initialized: false,
      unlocked: false,
      itemCount: 0,
      idleLockSecs: 900,
      hasRecoveryKey: false,
    });
    render(<App />);
    await waitFor(() => {
      expect(screen.getByTestId("unlock-screen").textContent).toContain(
        "unlock:no",
      );
    });
  });
});
