import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// Mock clipboard
const mockCopySecret = vi.hoisted(() => vi.fn());
vi.mock("../clipboard", () => ({ copySecret: (...args: unknown[]) => mockCopySecret(...args) }));

// Mock api
const mockApi = vi.hoisted(() => ({
  setup: vi.fn<(_: string) => Promise<string>>(),
  unlock: vi.fn<(pw: string) => Promise<void>>(),
  unlockRecovery: vi.fn<(pw: string) => Promise<void>>(),
}));
vi.mock("../api", () => ({ api: mockApi }));

// Mock asset import
vi.mock("../assets/app-mark.png", () => ({ default: "app-mark.png" }));

import { UnlockScreen } from "./UnlockScreen";

type UnlockProps = {
  initialized: boolean;
  pendingRecoveryKey: string | null;
  onSetupComplete: (recoveryKey: string) => void;
  onRecoveryConfirmed: () => void;
  onUnlocked: () => void;
};

const defaultProps: UnlockProps = {
  initialized: false,
  pendingRecoveryKey: null,
  onSetupComplete: vi.fn(),
  onRecoveryConfirmed: vi.fn(),
  onUnlocked: vi.fn(),
};

function renderScreen(overrides: Partial<UnlockProps> = {}) {
  return render(<UnlockScreen {...defaultProps} {...overrides} />);
}

beforeEach(() => {
  vi.clearAllMocks();
  mockApi.setup.mockResolvedValue("RECOVERY-KEY-12345");
  mockApi.unlock.mockResolvedValue(undefined);
  mockApi.unlockRecovery.mockResolvedValue(undefined);
  mockCopySecret.mockResolvedValue({ ok: true });
});

afterEach(() => {
  cleanup();
});

describe("UnlockScreen — setup mode", () => {
  it('shows setup form when not initialized and no pending key', () => {
    renderScreen();
    expect(screen.getByText(/create your vault/i)).toBeTruthy();
    expect(screen.getByText(/choose a strong master password/i)).toBeTruthy();
    expect(screen.getByLabelText(/confirm password/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: /create vault/i })).toBeTruthy();
  });

  it("rejects short passwords", async () => {
    renderScreen();
    const pwInput = screen.getByLabelText(/master password/i);
    const pw2Input = screen.getByLabelText(/confirm password/i);

    await userEvent.type(pwInput, "short");
    await userEvent.type(pw2Input, "short");

    fireEvent.click(screen.getByRole("button", { name: /create vault/i }));

    await waitFor(() => {
      expect(screen.getByText(/at least 12 characters/i)).toBeTruthy();
    });
    expect(mockApi.setup).not.toHaveBeenCalled();
  });

  it("rejects mismatched passwords", async () => {
    renderScreen();
    const pwInput = screen.getByLabelText(/master password/i);
    const pw2Input = screen.getByLabelText(/confirm password/i);

    await userEvent.type(pwInput, "password1234");
    await userEvent.type(pw2Input, "differentpw1");

    fireEvent.click(screen.getByRole("button", { name: /create vault/i }));

    await waitFor(() => {
      expect(screen.getByText(/do not match/i)).toBeTruthy();
    });
    expect(mockApi.setup).not.toHaveBeenCalled();
  });

  it("shows recovery key after successful setup", async () => {
    mockApi.setup.mockResolvedValue("RECOVERY-ABC-789");
    const onSetupComplete = vi.fn();

    renderScreen({ onSetupComplete });
    const pwInput = screen.getByLabelText(/master password/i);
    const pw2Input = screen.getByLabelText(/confirm password/i);

    await userEvent.type(pwInput, "goodpassword123");
    await userEvent.type(pw2Input, "goodpassword123");

    fireEvent.click(screen.getByRole("button", { name: /create vault/i }));

    await waitFor(() => {
      expect(mockApi.setup).toHaveBeenCalledWith("goodpassword123");
      expect(onSetupComplete).toHaveBeenCalledWith("RECOVERY-ABC-789");
      // Should now show the recovery key display
      expect(screen.getByText(/save your recovery key/i)).toBeTruthy();
      expect(screen.getByText("RECOVERY-ABC-789")).toBeTruthy();
    });
  });

  it("displays error when no recovery key returned", async () => {
    mockApi.setup.mockResolvedValue("");
    renderScreen();
    const pwInput = screen.getByLabelText(/master password/i);
    const pw2Input = screen.getByLabelText(/confirm password/i);

    await userEvent.type(pwInput, "goodpassword123");
    await userEvent.type(pw2Input, "goodpassword123");

    fireEvent.click(screen.getByRole("button", { name: /create vault/i }));

    await waitFor(() => {
      expect(screen.getByText(/no recovery key/i)).toBeTruthy();
    });
  });
});

describe("UnlockScreen — unlock mode", () => {
  it("shows unlock form when initialized", () => {
    renderScreen({ initialized: true });
    expect(screen.getByText(/unlock secretfolder/i)).toBeTruthy();
    expect(screen.getByText(/enter your master password/i)).toBeTruthy();
    // Confirm password should NOT be shown
    expect(screen.queryByLabelText(/confirm password/i)).toBeNull();
    expect(screen.getByRole("button", { name: /unlock/i })).toBeTruthy();
  });

  it("calls api.unlock on submit", async () => {
    const onUnlocked = vi.fn();
    renderScreen({ initialized: true, onUnlocked });
    const pwInput = screen.getByLabelText(/master password/i);

    await userEvent.type(pwInput, "mypassword");
    fireEvent.click(screen.getByRole("button", { name: /unlock/i }));

    await waitFor(() => {
      expect(mockApi.unlock).toHaveBeenCalledWith("mypassword");
      expect(onUnlocked).toHaveBeenCalled();
    });
  });

  it("shows error on failed unlock", async () => {
    mockApi.unlock.mockRejectedValue(new Error("Wrong password"));
    renderScreen({ initialized: true });
    const pwInput = screen.getByLabelText(/master password/i);

    await userEvent.type(pwInput, "wrongpw");
    fireEvent.click(screen.getByRole("button", { name: /unlock/i }));

    await waitFor(() => {
      expect(screen.getByText(/wrong password/i)).toBeTruthy();
    });
  });

  it("shows switch to recovery key link", () => {
    renderScreen({ initialized: true });
    expect(screen.getByText(/use recovery key/i)).toBeTruthy();
  });

  it("switches to recovery mode when link clicked", async () => {
    renderScreen({ initialized: true });
    fireEvent.click(screen.getByText(/use recovery key/i));

    await waitFor(() => {
      expect(screen.getByText(/recovery unlock/i)).toBeTruthy();
      expect(screen.getByLabelText(/recovery key/i)).toBeTruthy();
    });
  });
});

describe("UnlockScreen — recovery mode", () => {
  it("calls api.unlockRecovery with trimmed input", async () => {
    const onUnlocked = vi.fn();
    renderScreen({ initialized: true, onUnlocked });

    // Switch to recovery mode
    fireEvent.click(screen.getByText(/use recovery key/i));
    await waitFor(() => expect(screen.getByLabelText(/recovery key/i)).toBeTruthy());

    const recInput = screen.getByLabelText(/recovery key/i);
    await userEvent.type(recInput, "  MY-RECOVERY-KEY-123  ");
    fireEvent.click(screen.getByRole("button", { name: /unlock with recovery key/i }));

    await waitFor(() => {
      expect(mockApi.unlockRecovery).toHaveBeenCalledWith("MY-RECOVERY-KEY-123");
      expect(onUnlocked).toHaveBeenCalled();
    });
  });

  it("shows switch back to password link", async () => {
    renderScreen({ initialized: true });
    fireEvent.click(screen.getByText(/use recovery key/i));

    await waitFor(() => {
      expect(screen.getByText(/use master password/i)).toBeTruthy();
    });
  });
});

describe("UnlockScreen — show-recovery mode (post-setup)", () => {
  it("displays the recovery key when pendingRecoveryKey is set", () => {
    renderScreen({ pendingRecoveryKey: "HELLO-KEY" });
    expect(screen.getByText(/save your recovery key/i)).toBeTruthy();
    expect(screen.getByText("HELLO-KEY")).toBeTruthy();
  });

  it("copies recovery key to clipboard", async () => {
    renderScreen({ pendingRecoveryKey: "COPY-ME-KEY" });
    fireEvent.click(screen.getByText(/copy recovery key/i));

    await waitFor(() => {
      expect(mockCopySecret).toHaveBeenCalledWith("COPY-ME-KEY");
      expect(screen.getByText(/copied/i)).toBeTruthy();
    });
  });

  it("blocks continuation until checkbox is checked", () => {
    renderScreen({ pendingRecoveryKey: "SOME-KEY" });

    const continueBtn = screen.getByText(/continue to vault/i).closest("button");
    expect(continueBtn?.disabled).toBe(true);
  });

  it("enables button and fires callback after checkbox", async () => {
    const onRecoveryConfirmed = vi.fn();
    renderScreen({
      pendingRecoveryKey: "CONTINUE-KEY",
      onRecoveryConfirmed,
    });

    const checkbox = screen.getByRole("checkbox");
    fireEvent.click(checkbox);

    await waitFor(() => {
      const continueBtn = screen.getByText(/continue to vault/i).closest("button");
      expect(continueBtn?.disabled).toBe(false);
    });

    fireEvent.click(screen.getByText(/continue to vault/i));
    await waitFor(() => {
      expect(onRecoveryConfirmed).toHaveBeenCalled();
    });
  });
});