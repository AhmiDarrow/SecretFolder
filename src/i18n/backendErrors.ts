import { formatNumber, translate, type LocaleId } from "./index";

const EXACT: Record<string, string> = {
  "Something went wrong.": "err.generic",
  "vault is not initialized": "err.notInitialized",
  "vault is already initialized": "err.alreadyInitialized",
  "vault is already unlocked": "err.alreadyUnlocked",
  "vault is locked": "err.locked",
  "incorrect password or recovery key": "err.badPassword",
  "item not found": "err.notFound",
  "invalid name": "err.invalidName",
  "vault lock poisoned": "err.vaultLockPoisoned",
  "throttle lock poisoned": "err.throttleLockPoisoned",
  "path does not exist or cannot be resolved": "err.pathMissing",
  "import path must be under your home directory (Desktop, Documents, or Downloads)":
    "err.importPath",
  "invalid export path": "err.invalidExportPath",
  "export path cannot be resolved": "err.exportUnresolved",
  "cannot export into the SecretFolder data folder — choose another location":
    "err.exportIntoVault",
  "export path must be under your home directory (Desktop, Documents, or Downloads)":
    "err.exportPath",
  "path is not a file": "err.notAFile",
  "url not allowed": "err.urlNotAllowed",
  "open external url is only supported on Windows in v1": "err.openUrlUnsupported",
  "no recovery key configured": "err.noRecovery",
  "parent is not a folder": "err.parentNotFolder",
  "folder path too deep": "err.folderTooDeep",
  "path entry is not a folder": "err.pathNotFolder",
  "cannot move a folder into itself": "err.moveIntoSelf",
  "cannot move a folder into its own subfolder": "err.moveIntoChild",
  "use create_folder for folders": "err.useCreateFolder",
  "item is not a text file": "err.notText",
  "cannot export a folder as bytes": "err.cannotExportFolder",
  "folder hierarchy too deep": "err.hierarchyTooDeep",
  "no data dir": "err.noDataDir",
  "no parent": "err.noParent",
  "failed to read file": "err.readFailed",
  "read failed": "err.readFailedShort",
  "unsupported locale": "err.unsupportedLocale",
  "copy failed": "unlock.copyFailed",
};

/**
 * Map a known English backend string to the active locale.
 * Unknown text (including technical tails already extracted) is returned as-is.
 */
export function translateBackendError(id: LocaleId, raw: string): string {
  const exact = EXACT[raw];
  if (exact) return translate(id, exact);

  let match = /^crypto error: ([\s\S]*)$/.exec(raw);
  if (match) return translate(id, "err.crypto", { detail: match[1] });

  match = /^io error: ([\s\S]*)$/.exec(raw);
  if (match) return translate(id, "err.io", { detail: match[1] });

  match = /^file too large \(max (\d+) bytes\)$/.exec(raw);
  if (match) return translate(id, "err.tooLarge", { n: formatNumber(id, Number(match[1])) });

  match = /^too many unlock attempts [—-] try again in (\d+)s$/.exec(raw);
  if (match) return translate(id, "err.throttle", { n: formatNumber(id, Number(match[1])) });

  match = /^Folder is not empty \((\d+) item\)\. Empty it first, or confirm deleting everything inside\.$/.exec(
    raw,
  );
  if (match) {
    return translate(id, "err.folderNotEmptyOne", { n: formatNumber(id, Number(match[1])) });
  }

  match = /^Folder is not empty \((\d+) items\)\. Empty it first, or confirm deleting everything inside\.$/.exec(
    raw,
  );
  if (match) {
    return translate(id, "err.folderNotEmptyMany", { n: formatNumber(id, Number(match[1])) });
  }

  match = /^invalid base64: ([\s\S]*)$/.exec(raw);
  if (match) return translate(id, "err.badBase64", { detail: match[1] });

  match = /^open url: ([\s\S]*)$/.exec(raw);
  if (match) return translate(id, "err.openUrl", { detail: match[1] });

  match = /^vault format version (\d+) is invalid$/.exec(raw);
  if (match) return translate(id, "err.versionInvalid", { v: match[1] });

  match =
    /^vault format version (\d+) is newer than this app supports \((\d+)\) — upgrade SecretFolder; refusing to open so saved files are not rewritten$/.exec(
      raw,
    );
  if (match) {
    return translate(id, "err.versionNewer", { v: match[1], supported: match[2] });
  }

  match = /^atomic vault replace failed \(saved files left untouched\): ([\s\S]*)$/.exec(raw);
  if (match) return translate(id, "err.atomicReplace", { detail: match[1] });

  match = /^vault\.json parse: ([\s\S]*)$/.exec(raw);
  if (match) return translate(id, "err.vaultParse", { detail: match[1] });

  return raw;
}
