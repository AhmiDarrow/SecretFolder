# SecretFolder → RemedyUI port checklist

The shipped app is the Tauri + React 0.1.7 release (`src/`, `src-tauri/`). The RemedyUI build
is `remedyui/`. It runs the shipped vault, crypto and error code, plus the unlock throttle and
import/export path rules (`src-tauri/src/policy.rs`, which both front ends share). Each line
below was checked against the shipped source. The proof is either a headless test in
`remedyui/src/flow_tests.rs` (driven through the accessibility tree and the pointer) or the
screenshots in `port-evidence/` (`before/` is the shipped UI, `after/` is RemedyUI).

## Data (never lose saved files)

- [x] Same vault, same place: `%APPDATA%\com.ahmi.secretfolder\` (`vault.json` plus `blobs/`), opened by the shipped `Vault::open_default`.
- [x] Same format. The vault code is the shipped `src-tauri/src/vault.rs`, so there is no migration.
- [x] `SECRETFOLDER_VAULT` selects another vault folder. It gets its own single-instance lock.
- [x] Vaults written by 0.1.0, 0.1.1, 0.1.5, 0.1.7 and 0.2.0 (every release whose vault code changed) open, can be edited (text, rename, import, idle lock, password), and read back through the shipped code by the new password and by the old recovery key. Untouched files stay byte-identical (`shipped_vaults_open_edit_and_reopen_without_loss`; made-up test vaults from `compat/shipped.sh` and `compat/fixture.sh`, which run the release tags' own vault code).
- [x] The release tags' own code also reads the vaults after the RemedyUI build has edited them (cross-check with `SECRETFOLDER_COMPAT_OUT` and `compat/shipped.sh <tag> dump`).
- [x] `.bak` restore and corrupt-file handling come from the shipped `open_root`, unchanged.

## Gate

- [x] Setup: master password and confirm, at least 12 characters, mismatch error, "Working…" while Argon2 runs off the UI thread. The app mark sits at the top of every gate card.
- [x] Recovery key shown once: "Copy recovery key" (secret clipboard, then "Copied"), "I saved this recovery key somewhere safe", and "Continue to vault" (needs the confirmation).
- [x] Unlock with the master password or the recovery key (spaces trimmed). Throttled after repeated failures.
- [x] A vault that cannot be opened gets its own screen.

## Explorer (Files)

- [x] Header: mark, "SecretFolder", item count; Files / Settings / About tabs; Lock; Hide.
- [x] Error banner with Dismiss (Escape also dismisses it).
- [x] Search this folder. Buttons: New text, New folder (name dialog), Import… (native dialog, several files).
- [x] Selection bar: "N selected", Delete, Clear, and the hints.
- [x] Path bar: Vault / … / current folder, ↑ Up. Every part of the path accepts dropped items.
- [x] ".." parent row inside a folder: opens the parent and accepts dropped items and files ("Parent folder · <name> · drop here to move up").
- [x] Item list: icon, name, row actions (Open for folders, Remove for every item), size, kind, modified. Columns sort.
- [x] Click selects, Ctrl/Shift multi-select, Ctrl+A, double-click or Enter opens, Delete/Backspace deletes, F2 renames.
- [x] Drag items onto a folder, the ".." row or the path to move them. Files dropped from other apps onto the list, a folder row or the path are imported there, with the shipped size and path rules.
- [x] Empty-folder messages.
- [x] Delete confirmation, worded as shipped: single, multiple, empty folder, non-empty folder ("Delete folder & contents" / "Delete all & contents" cascade).
- [x] Text: name field, Save (Ctrl+S), Copy (secret clipboard, cleared after 30 s), Export, Delete. Leaving unsaved edits asks first ("Leave without saving" / Stay).
- [x] Image: preview decoded at the size shown, with Rename, Export, Delete, type and size.
- [x] Binary file: "Binary file — export to open it with another app.", with Rename, Export, Delete.
- [x] Export through the native save dialog. Exports into the vault folder are refused (shipped policy).

## Settings

- [x] Idle lock in minutes (1–240), Save, "Idle lock set to N min.", and the at-least-1-minute error.
- [x] Change master password: current, new and confirm; at least 12 characters; mismatch error; "Password changed."; the recovery key stays valid. Runs off the UI thread with "Working…", as the Tauri command did.
- [x] A note when the system cannot keep the window out of screen capture.

## About

- [x] Greeting, "Local-only encrypted folder vault · MIT · v…", the privacy line.
- [x] GitHub, Repo, Releases and Patreon links (the shipped allowlist).
- [x] Check for updates, then install and restart, verified with the same minisign key as `tauri.conf.json`.

## App-wide

- [x] Tray icon (the shipped 32 px icon) with Show, Lock and Quit. Clicking it shows the window.
- [x] Window icon: the shipped app icon (`src-tauri/icons/128x128@2x.png`) in the taskbar, title bar and Alt+Tab, as the Tauri window had (`WindowSpec::icon`).
- [x] Type: Inter, as shipped: the same Fontsource 5.3.0 WOFF2 files (weights 400 to 800, all seven script subsets) in `remedyui/assets/fonts/inter/`, loaded by the engine and first in the theme's families (`remedyui/src/fonts.rs`).
- [x] Copies out of the text editor (Ctrl+C, Ctrl+X) go to the secret clipboard like the Copy button: kept out of history, cleared after 30 s (`TextField::copy_as_secret`). The shipped editor made ordinary copies; this closes that.
- [x] Error lines, the error banner, the empty-folder hint and the ".." row exist only while they apply and take no room otherwise (the engine's `show`; no app-side hiding).
- [x] Window hidden from screen capture. Closing it hides it to the tray. Single instance.
- [x] Idle lock (real input only) plus the vault's own idle clock. Locking clears a copied secret.
- [x] Brand: the shipped dark palette with the amber accent as a theme, and the shipped mark.
- [x] Accessibility: every control is named, and the engine's audit is clean on the gate, the recovery key screen, the explorer with items and inside folders. Grid rows are read through their cells.

## Languages (0.2.0)

- [x] The eleven shipped languages (en, es, fr, de, pt-BR, it, ru, pl, ja, ko, zh-CN) from 0.2.0's own catalogs (`src/i18n/locales/*.json`, compiled in unchanged). Strings the port shows that 0.2.0 had no key for are in `remedyui/src/port-strings.json`, in all eleven (`every_catalog_parses_and_has_every_english_key`).
- [x] The choice lives in `ui.json` beside the vault, read and written by 0.2.0's own `prefs.rs` (other keys kept). `vault.json`, its `.bak` and the blobs are never written by a switch: all eleven switched in turn leave them byte for byte (`language_switches_everything_and_never_touches_the_vault`).
- [x] An upgrade keeps its language: the 0.2.0 fixture vault opens in its saved German, and its `ui.json` is untouched (`shipped_vaults_open_edit_and_reopen_without_loss`). With nothing saved, the system's languages decide (0.2.0's `detectLocale` rules); `SECRETFOLDER_LOCALE` as in 0.2.0.
- [x] A picker on every gate card (before any vault exists) and in Settings. Every screen, dialog, plural, number, size, vault error, the tray menu (0.2.0's `tray_labels`) and the update strip switch at once; a restart opens in the saved language.

## Not carried over (with reason)

- Kind icons come from the engine's icon set, not emoji. Emoji depend on the system's fonts. Folders, text, images and other files each have their own (`Icon::Folder`, `Icon::Note`, `Icon::Image`, `Icon::File`).

## Targets

- [x] Windows: `cargo build --release`, and the built app was run against a temporary vault.
- [x] Linux x86-64: `cargo remedy package --target linux` with `[linux] glibc = "2.28"` (cargo-zigbuild). The program needs at most GLIBC_2.28 and links only libc, libm, libpthread and libdl, so it runs on Debian 10, RHEL 8, Ubuntu 20.04 and later. All tests pass on Linux. Vault: `~/.local/share/com.ahmi.secretfolder`. In-app updates replace the program (`SecretFolder_<v>_linux-x86_64`, signed by the release job).
- [ ] Started on Debian 10, Rocky 8 and Ubuntu 20.04, and the Windows installer run over installed 0.1.5 and 0.2.0 (vault and `ui.json` byte for byte): on GitHub's machines, before the release.
