import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateCheckResult =
  | { kind: "up-to-date" }
  | { kind: "available"; version: string; body?: string | null }
  | { kind: "error"; message: string };

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
 * Returns false if nothing to install.
 */
export async function downloadAndInstallUpdate(
  onProgress?: (pct: number | null) => void,
): Promise<boolean> {
  const update = await check();
  if (!update) {
    return false;
  }

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
          onProgress?.(Math.min(100, Math.round((downloaded / contentLength) * 100)));
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
