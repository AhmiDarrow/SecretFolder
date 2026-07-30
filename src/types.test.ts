import { describe, expect, it } from "vitest";
import type { ItemKind, ItemPreview, VaultStatus } from "./types";

describe("vault types", () => {
  it("accepts item kinds used by the explorer", () => {
    const kinds: ItemKind[] = ["text", "image", "binary", "folder"];
    expect(kinds).toHaveLength(4);
  });

  it("shapes a list preview like the API", () => {
    const item: ItemPreview = {
      id: "abc",
      name: "notes.txt",
      kind: "text",
      mime: "text/plain",
      size: 12,
      parentId: null,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    };
    expect(item.kind).toBe("text");
    expect(item.size).toBeGreaterThan(0);
  });

  it("shapes vault status fields", () => {
    const status: VaultStatus = {
      initialized: true,
      unlocked: true,
      itemCount: 0,
      hasRecoveryKey: true,
      idleLockSecs: 900,
    };
    expect(status.idleLockSecs).toBe(900);
    expect(status.unlocked).toBe(true);
  });
});
