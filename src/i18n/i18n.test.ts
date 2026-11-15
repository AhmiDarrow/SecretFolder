import { afterEach, describe, expect, it } from "vitest";
import { translateBackendError } from "./backendErrors";
import {
  detectLocale,
  formatBytes,
  formatNumber,
  resetI18nForTests,
  setLocale,
  tPlural,
  translate,
} from "./index";

afterEach(() => {
  resetI18nForTests();
});

describe("translate", () => {
  it("falls back to English when a key is missing from the locale", () => {
    expect(translate("de", "unlock.titleSetup")).toBe("Eigenen Tresor anlegen");
    expect(translate("de", "no.such.key")).toBe("no.such.key");
    expect(translate("en", "unlock.titleUnlock")).toBe("Unlock SecretFolder");
  });

  it("keeps placeholders and never returns a blank string", () => {
    expect(translate("en", "about.latest", { version: "0.1.7" })).toBe(
      "You're on the latest version (0.1.7).",
    );
    expect(translate("en", "missing.key")).toBe("missing.key");
    expect(translate("ja", "").length).toBeGreaterThan(0);
  });

  it("keeps the Ahmi line identical", () => {
    for (const id of ["en", "es", "de", "ja", "zh-CN"] as const) {
      expect(translate(id, "about.hello")).toBe("Hi I'm Ahmi, hope this helps!");
    }
  });
});

describe("detectLocale", () => {
  it("maps supported tags and skips Traditional Chinese", () => {
    expect(detectLocale(["de-DE"])).toBe("de");
    expect(detectLocale(["pt-PT"])).toBe("pt-BR");
    expect(detectLocale(["zh-Hans-CN"])).toBe("zh-CN");
    expect(detectLocale(["zh-TW", "fr-FR"])).toBe("fr");
    expect(detectLocale(["zh-HK"])).toBe("en");
    expect(detectLocale(["en-US"])).toBe("en");
    expect(detectLocale(undefined)).toBe("en");
  });
});

describe("plurals and numbers", () => {
  it("uses locale plural categories and byte units", () => {
    expect(tPlural("en", "items", 1)).toBe("1 item");
    expect(tPlural("en", "items", 2)).toBe("2 items");
    expect(tPlural("fr", "items", 0)).toBe("0 élément");
    expect(tPlural("fr", "items", 2)).toBe("2 éléments");
    expect(tPlural("ru", "items", 1)).toBe("1 элемент");
    expect(tPlural("ru", "items", 2)).toBe("2 элемента");
    expect(tPlural("ru", "items", 5)).toBe("5 элементов");
    expect(tPlural("pl", "items", 1)).toBe("1 element");
    expect(tPlural("pl", "items", 2)).toBe("2 elementy");
    expect(tPlural("pl", "items", 5)).toBe("5 elementów");
    expect(formatBytes("en", 12)).toBe("12 B");
    expect(formatBytes("en", 1024)).toBe("1.0 KB");
    expect(formatBytes("en", 25 * 1024 * 1024)).toBe("25.0 MB");
    expect(formatNumber("en", 1000)).toMatch(/1[.,\s\u00a0\u202f]?000/);
  });
});

describe("translateBackendError", () => {
  it("translates known English errors and passes unknown text through", () => {
    setLocale("de", { persist: false });
    expect(translateBackendError("de", "vault is locked")).toBe("der Tresor ist gesperrt");
    expect(translateBackendError("de", "Wrong password")).toBe("Wrong password");
    expect(translateBackendError("en", "Something went wrong.")).toBe("Something went wrong.");
    expect(translateBackendError("de", "Folder is not empty (1 item). Empty it first, or confirm deleting everything inside.")).toContain("1");
    expect(translateBackendError("en", "too many unlock attempts — try again in 10s")).toBe(
      "too many unlock attempts — try again in 10s",
    );
  });
});
