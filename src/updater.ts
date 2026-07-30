import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { api } from "./api";

export type UpdateCheckResult =
  | { kind: "up-to-date" }
  | { kind: "available"; version: string; body?: string | null }
  | { kind: "error"; message: string };

/**
 * Lock the vault before the installer runs / process exits.
 * Best-effort: never blocks install if already locked or IPC fails.
 * Updater only replaces the app binary under Local\Programs — vault
 * lives under %APPDATA%\com.ahmi.secretfolder and is never a target.
 */
export async function lockVaultBeforeUpdate(): Promise<void> {
  try {
    await api.lock();
  } catch {
    // already locked / not initialized / racing idle lock — safe to continue
  }
}

/** Probe GitHub Releases for a newer signed build (no download yet). */
export async function checkForAppUpdate(): Promise<UpdateCheckResult> {
  try {
    const update = await check();
    if (!update) {
      return { kind: "up-to-date" };
    }
    return {
      kind: "available",
      version: update.version,
      body: update.body,
    };
  } catch (e) {
    return { kind: "error", message: String(e) };
  }
}

/**
 * Download + install the available update, then relaunch.
 * Locks the vault first so no session keys / open writes race the installer.
 * Returns false if nothing to install.
 *
 * Safety: Tauri updater installs into the app install dir only. Vault blobs
 * stay under %APPDATA%\com.ahmi.secretfolder and are never read/written by
 * the updater path.
 */
export async function downloadAndInstallUpdate(
  onProgress?: (pct: number | null) => void,
): Promise<boolean> {
  const update = await check();
  if (!update) {
    return false;
  }

  // Drop master key + plaintext session before binary replace / relaunch.
  await lockVaultBeforeUpdate();

  let downloaded = 0;
  let contentLength: number | undefined;

  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        contentLength = event.data.contentLength;
        onProgress?.(0);
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        if (contentLength && contentLength > 0) {
          onProgress?.(
            Math.min(100, Math.round((downloaded / contentLength) * 100)),
          );
        } else {
          onProgress?.(null);
        }
        break;
      case "Finished":
        onProgress?.(100);
        break;
    }
  });

  await relaunch();
  return true;
}
