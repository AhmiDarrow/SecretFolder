# Changelog

All notable changes to SecretFolder are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
