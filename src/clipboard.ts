/** Copy secrets with automatic clipboard clear (default 30s). */

const DEFAULT_CLEAR_MS = 30_000;

let clearTimer: number | null = null;
let clearGeneration = 0;

export type CopySecretResult = {
  ok: boolean;
  clearedInMs: number;
  error?: string;
};

export async function copySecret(
  text: string,
  clearAfterMs: number = DEFAULT_CLEAR_MS,
): Promise<CopySecretResult> {
  try {
    await navigator.clipboard.writeText(text);
  } catch (e) {
    return { ok: false, clearedInMs: 0, error: String(e) };
  }

  if (clearTimer !== null) {
    window.clearTimeout(clearTimer);
    clearTimer = null;
  }

  const gen = ++clearGeneration;
  const ms = Math.max(1_000, clearAfterMs);

  clearTimer = window.setTimeout(() => {
    if (gen !== clearGeneration) return;
    void clearClipboardIfMatches(text);
    clearTimer = null;
  }, ms);

  return { ok: true, clearedInMs: ms };
}

/** Best-effort clear (used on lock / manual clear). */
export async function clearClipboard(): Promise<void> {
  clearGeneration += 1;
  if (clearTimer !== null) {
    window.clearTimeout(clearTimer);
    clearTimer = null;
  }
  try {
    await navigator.clipboard.writeText("");
  } catch {
    /* webview may deny empty write — ignore */
  }
}

async function clearClipboardIfMatches(expected: string): Promise<void> {
  try {
    const current = await navigator.clipboard.readText();
    // Only wipe if still our secret (user may have copied something else).
    if (current === expected) {
      await navigator.clipboard.writeText("");
    }
  } catch {
    try {
      await navigator.clipboard.writeText("");
    } catch {
      /* ignore */
    }
  }
}
