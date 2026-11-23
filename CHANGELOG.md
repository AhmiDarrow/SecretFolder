# Changelog

All notable changes to SecretFolder are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] — 2026-11-23

SecretFolder 1.0 has a new interface built on RemedyUI, and runs on Linux as well as Windows. Your vault, master password, recovery key and language are the same.

Hi I'm Ahmi, hope this helps!

### Changed

- **New interface on RemedyUI.** The Tauri and React app is gone. SecretFolder is now one native Rust program: no web view, no JavaScript. The vault, crypto and on-disk format are the same code as 0.2.0, so there is no migration.
- All eleven languages from 0.2.0, the language you picked, the explorer, settings, tray and signed updates carry over.
- Copies out of the text editor (Ctrl+C, Ctrl+X) are kept out of clipboard history and cleared after 30 seconds, like the Copy button.

### Added

- **Linux (x86-64).** Runs on distributions with glibc 2.28 or newer (Debian 10, RHEL 8 and its rebuilds, Ubuntu 20.04 and later). Install with `tar -xzf SecretFolder-1.0.0-linux-x86_64.tar.gz --strip-components=1 -C ~/.local` (the program, a menu entry and the icon). The vault is kept in `~/.local/share/com.ahmi.secretfolder`. Later updates install from **About → Check for updates**.

### Install and update

- Windows: `SecretFolder_1.0.0_x64-setup.exe`. It takes over an earlier SecretFolder install (0.1.x and 0.2.0, the setup or the MSI) and leaves the vault in `%APPDATA%\com.ahmi.secretfolder` alone.
- Already installed on Windows: **About → Check for updates → Install & restart**.

## [0.2.0] — 2026-11-15

Now available in Spanish, French, German, Portuguese (Brazil), Italian, Russian, Polish, Japanese, Korean and Chinese (Simplified). Apps use your system language by default; change it any time in Settings → Language.

Hi I'm Ahmi, hope this helps!

### Added

- **Ten languages.** English remains the source catalog. SecretFolder also ships Spanish, French, German, Portuguese (Brazil), Italian, Russian, Polish, Japanese, Korean, and Chinese (Simplified). On a new profile the app follows the Windows language when that language is one of these, and otherwise uses English. Change it any time in Settings → Language. The choice applies immediately and is kept beside the vault, not inside it.
- The tray menu, setup, explorer, and About use the chosen language. A missing translation falls back to English.

### Install and update

- Windows setup: `SecretFolder_0.2.0_x64-setup.exe` (Start Menu, desktop shortcut, and taskbar icon). `SecretFolder_0.2.0_x64_en-US.msi` is attached as well.
- Already installed: **About → Check for updates → Install & restart**. The update replaces the application only.
- The vault stays in `%APPDATA%\com.ahmi.secretfolder` and unlocks with the same master password and recovery key. Language and other settings stay with it.
- Local only. There is no cloud sync and no account. Network use is limited to signed updates from GitHub Releases.

Same text: [docs/RELEASE-0.2.0.md](docs/RELEASE-0.2.0.md).

## [0.1.7] — 2026-11-14

Idle auto-lock works again, and a save or delete that fails can no longer leave the vault half-updated.

Hi I'm Ahmi, hope this helps!

### Fixes

- **Idle auto-lock locks the vault.** Only keyboard, pointer, and scroll input resets the idle timer. A background ping every 30 seconds had been keeping the vault unlocked for as long as the window was open.
- **Failed unlocks wait longer each time.** After repeated failures the pause steps through 10 seconds, 30 seconds, 60 seconds, and 5 minutes. A successful unlock clears that count. Each lockout had been clearing the counter that was supposed to raise the next pause, so the wait stayed at 10 seconds.
- **Unlock works in the first minute after Windows starts.** The rate limiter could crash when its 60-second attempt window was still younger than the system timer, and then every later unlock failed until SecretFolder was restarted.
- **A rejected edit leaves the file alone.** Saving text checks that the item is a text file, and that any new name is allowed, before anything is written. An image or other file keeps its contents when that save is refused.
- **A failed delete keeps the item.** The index is saved before file contents are removed. If that save fails, the item and its contents stay.
- **A failed save matches what is on disk.** The open session adopts the new index only after the write succeeds. A password change that fails to save still unlocks with the previous password. If the first save during setup fails, setup can be tried again.
- **Import continues after one bad file.** Files that import are kept and shown, and SecretFolder reports the ones that failed.
- **Idle-lock minutes are checked before they are saved.** A blank value, or a value under 1 minute, is rejected. A value over 240 minutes is saved as 240 minutes.
- **Exports stay out of the vault folder.** A decrypted copy cannot be saved into SecretFolder's data folder (`%APPDATA%\com.ahmi.secretfolder`), where it would sit beside the vault or overwrite `vault.json`.

### Build/packaging

- Development tools only: Vite 7.3.8, Vitest 4.1.12, and patched nanoid and source-map-js. No runtime dependency of the installed app changed.

### Install and update

- Windows setup: `SecretFolder_0.1.7_x64-setup.exe` (Start Menu, desktop shortcut, and taskbar icon). `SecretFolder_0.1.7_x64_en-US.msi` is attached as well.
- Already installed: **About → Check for updates → Install & restart**. The update replaces the application only.
- The vault stays in `%APPDATA%\com.ahmi.secretfolder` and unlocks with the same master password and recovery key.
- Local only. There is no cloud sync and no account. Network use is limited to signed updates from GitHub Releases.

Same text: [docs/RELEASE-0.1.7.md](docs/RELEASE-0.1.7.md).

## [0.1.6] — 2026-08-11

### Changed
- **Settings & About panels** are compact: tighter padding/type, no inner scroll region, denser About header + link/update row so content fits without scrolling
- Settings idle-lock copy states the **15-minute default** clearly

### Notes
- Idle auto-lock default remains **15 minutes** (`DEFAULT_IDLE_LOCK_SECS = 900`)

## [0.1.5] — 2026-08-11

### Security
- **Vault durability (Windows):** replace `vault.json` with `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)` — never delete the live file before the new bytes are at the destination (closes a crash/failure window that could wipe the only copy of secrets)
- Snapshot `vault.json.bak` before each successful index replace; boot restores from `.bak` if the live file is missing or unreadable (never treat that as a fresh install)
- Refuse to open vault format versions newer than this build supports (no write-back that could strand secrets)
- Hard product invariant documented: updates replace the app binary only; vault under `%APPDATA%\com.ahmi.secretfolder` must remain unlockable after every update

### Fixed
- Windows index save path no longer uses delete-then-rename (same class of bug fixed in SecretSticky 0.1.6)

## [0.1.4] — 2026-07-30

### Fixed
- Published **v0.1.3** was left as a GitHub **draft**, so `/releases/latest/download/latest.json` still pointed at 0.1.2 and auto-update could not see 0.1.3; release workflow now publishes non-draft by default
- Re-uploaded broken `latest.json` asset on the 0.1.3 release (CDN 404 despite API listing)

### Security
- **Install & restart** locks the vault (drops master key + plaintext session) **before** the signed installer runs / process relaunches
- Documented hard guarantee: updater only replaces the app under the install directory; vault blobs stay under `%APPDATA%\com.ahmi.secretfolder` and are never a target of the update path

### Changed
- Release workflow `releaseDraft: false` so tagged builds become Latest immediately (drafts break the updater channel)

## [0.1.3] — 2026-07-30

### Fixed
- **About → Check for updates** now offers **Install & restart** (download + signed install + relaunch). Previously it only reported that an update existed and never installed it
- About page links (GitHub profile / repo / releases / Patreon) surface errors instead of failing silently
- External URL open on Windows uses `rundll32 url.dll,FileProtocolHandler` (with `cmd start` / `explorer` fallbacks) so links open reliably under `CREATE_NO_WINDOW`

### Changed
- About update UX matches SecretSticky: busy state, pending version, progress text while downloading

## [0.1.2] — 2026-07-30

### Fixed
- Updater signing: rotate to Tauri 2.11-compatible rsign private key format (empty password) so CI release signing succeeds
- GitHub Actions `TAURI_SIGNING_PRIVATE_KEY` secret re-seeded to match the new keypair
- Updater `pubkey` in `tauri.conf.json` was accidentally **double-base64-encoded** (build failed with `Missing encoded key in public key`); now set to the raw minisign public key string from the keypair

### Security
- Path boundary (`path_is_under`), Session zeroize on drop, unlock throttle on crypto failures, and password minimum length 12 remain in force (from 0.1.1 hardening)

### Changed
- Updater public key in `tauri.conf.json` (new keypair). Existing 0.1.0/0.1.1 installers cannot verify 0.1.2+ update signatures — fresh install or manual upgrade required for the updater chain

## [0.1.1] — 2026-03-22

### Security

- Minimum master password length raised to **12** characters (setup, change password, UI)
- Unlock throttle counts **all** auth/crypto failures (not only `BadPassword`)
- Import/export path allowlists use **component-boundary** checks (no string-prefix sibling escapes)
- Session plaintext item map cleared on lock; temporary content-key copies zeroized after password change
- CSP connect-src tightened for updater endpoints; recovery-key UI holds local copy until confirmed

### Added

- Broader frontend + Rust test coverage (throttle, path validation, unlock/setup UI, explorer, updater helpers)
- `npm run test:all` script (TypeScript/Vitest + `cargo test`)

### Fixed

- Recovery key remains visible after first-run setup until the user confirms save
- Settings password change enforces the same 12-character minimum as setup

## [0.1.0] — 2026-07-29

### Added

- Master-password encrypted file vault (text, images, binary) for Windows
- Argon2id + XChaCha20-Poly1305 crypto core with recovery key and password change
- On-disk format: `vault.json` index + encrypted blobs; encrypted filenames
- Vault session API: setup / unlock / lock / idle-lock / CRUD / import-export
- Nested **folders / groups** with breadcrumbs and parent (`..`) navigation
- Explorer UI: list, search, text editor, image preview, About
- **Drag-and-drop import** from the OS (onto list or onto a folder row)
- **In-vault drag-move** onto folders or `..` (parent)
- Multi-select (click / Ctrl / Shift / Ctrl+A) with bulk delete and bulk move
- Native Save/Open dialogs for export and import paths
- System tray: Show · Lock · Quit
- Clipboard auto-clear and idle lock defaults
- Self-hosted **Inter** font (`@fontsource/inter`)
- Signed auto-update from GitHub Releases (Tauri updater + process plugins)
- CI and Release GitHub Actions workflows
- Folder+lock app icon; dark chrome matching the SecretSticky family look
- SECURITY.md threat model (at-rest vault; updates are the only network path)
- About: “Hi I'm Ahmi, hope this helps!” + profile / repo / releases / Patreon

### Fixed

- Recovery key held on screen after first-run setup until confirmed
- Tauri/IPC errors no longer surface as `[object Object]`
- Leaving a dirty text editor prompts before discarding unsaved changes
- Delete confirmation uses in-app dialogs (not `window.confirm`)
- Export uses native save dialog (WebView blob download path removed)
- Folder/rename dialog no longer wipes input while typing
- Folder checkbox / selection no longer opens the folder (Explorer-style click vs double-click)
- App icon: single folder + black lock (no double-imposed lock); brand mark on explorer header and About; taskbar / tray / installer ICO set regenerated from the approved master

### Notes

- Sibling product to SecretSticky — separate app data dir, AAD namespace, identifier, and release channel
- v1 item size cap ~25 MiB; no OS folder mount / FUSE
