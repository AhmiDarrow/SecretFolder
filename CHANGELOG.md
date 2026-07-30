# Changelog

All notable changes to SecretFolder are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

### Notes

- Sibling product to SecretSticky — separate app data dir, AAD namespace, identifier, and release channel
- v1 item size cap ~25 MiB; no OS folder mount / FUSE
