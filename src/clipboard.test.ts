import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clearClipboard, copySecret } from "./clipboard";

describe("copySecret", () => {
  let written: string[] = [];
  let current = "";

  beforeEach(() => {
    written = [];
    current = "";
    vi.useFakeTimers();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn(async (t: string) => {
          written.push(t);
          current = t;
        }),
        readText: vi.fn(async () => current),
      },
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("writes text and schedules clear", async () => {
    const res = await copySecret("sk-test-secret", 5_000);
    expect(res.ok).toBe(true);
    expect(res.clearedInMs).toBe(5_000);
    expect(written).toEqual(["sk-test-secret"]);

    await vi.advanceTimersByTimeAsync(4_999);
    expect(written).toEqual(["sk-test-secret"]);

    await vi.advanceTimersByTimeAsync(1);
    expect(written[written.length - 1]).toBe("");
  });

  it("does not clear if clipboard changed", async () => {
    const res = await copySecret("secret-a", 2_000);
    expect(res.ok).toBe(true);
    current = "user-copied-something-else";
    await vi.advanceTimersByTimeAsync(2_000);
    // clearClipboardIfMatches should not overwrite foreign clipboard
    expect(written.filter((w) => w === "").length).toBe(0);
  });

  it("returns error when clipboard write fails", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn(async () => {
          throw new Error("denied");
        }),
        readText: vi.fn(async () => ""),
      },
    });
    const res = await copySecret("x");
    expect(res.ok).toBe(false);
    expect(res.error).toMatch(/denied/);
  });

  it("clearClipboard bumps generation and writes empty", async () => {
    await copySecret("pending", 60_000);
    await clearClipboard();
    expect(written[written.length - 1]).toBe("");
    // original timer should not fire a second clear after generation bump
    const before = written.length;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(written.length).toBe(before);
  });
});
