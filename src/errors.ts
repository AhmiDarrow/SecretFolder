/**
 * Normalize thrown values from Tauri invoke / DOM into a short user-facing string.
 * Avoids "[object Object]" when the runtime throws a plain payload.
 */
export function formatError(err: unknown): string {
  if (err == null) return "Something went wrong.";
  if (typeof err === "string") {
    const t = err.trim();
    return t || "Something went wrong.";
  }
  if (err instanceof Error) {
    const t = err.message?.trim();
    return t || err.name || "Something went wrong.";
  }
  if (typeof err === "object") {
    const o = err as Record<string, unknown>;
    for (const key of ["message", "error", "msg"] as const) {
      const v = o[key];
      if (typeof v === "string" && v.trim()) return v.trim();
    }
    // Nested IPC shapes: { data: "…" } / { payload: { message } }
    for (const key of ["data", "payload"] as const) {
      const inner = o[key];
      if (typeof inner === "string" && inner.trim()) return inner.trim();
      if (inner && typeof inner === "object") {
        const nested = formatError(inner);
        if (nested !== "Something went wrong.") return nested;
      }
    }
    try {
      const json = JSON.stringify(err);
      if (json && json !== "{}" && json !== "null") return json;
    } catch {
      /* ignore */
    }
  }
  const s = String(err);
  return s === "[object Object]" ? "Something went wrong." : s;
}
