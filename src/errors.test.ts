import { describe, expect, it } from "vitest";
import { formatError } from "./errors";

describe("formatError", () => {
  it("passes through strings and Errors", () => {
    expect(formatError("vault is locked")).toBe("vault is locked");
    expect(formatError(new Error("incorrect password"))).toBe(
      "incorrect password",
    );
  });

  it("unwraps common IPC object shapes", () => {
    expect(formatError({ message: "item not found" })).toBe("item not found");
    expect(formatError({ error: "file too large" })).toBe("file too large");
    expect(formatError({ payload: { message: "invalid name" } })).toBe(
      "invalid name",
    );
    expect(formatError({ data: "io error: disk full" })).toBe(
      "io error: disk full",
    );
  });

  it("does not emit [object Object]", () => {
    expect(formatError({})).not.toBe("[object Object]");
    expect(formatError(null)).toBe("Something went wrong.");
  });
});
