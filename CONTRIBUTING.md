# Contributing to SecretFolder

Thanks for helping harden a local secrets app. Keep changes small, tested, and honest about the threat model.

## Prerequisites

- Windows 10/11 (primary target)
- Node.js 20+
- Rust stable (edition 2021)
- WebView2 (ships with modern Windows)

## Setup

```bash
git clone https://github.com/AhmiDarrow/SecretFolder.git
cd SecretFolder
npm install
npm run tauri:dev
```

## Tests

```bash
npm test            # tsc --noEmit + Vitest
npm run test:rust   # cargo test in src-tauri
npm run test:all
```

Before a PR: green `npm run test:all` and a quick manual smoke of unlock → import → edit → lock → tray.

## Project map

| Path | Role |
|------|------|
| `src/screens/` | Unlock + Explorer UI |
| `src-tauri/src/crypto.rs` | KDF / AEAD / recovery wrap |
| `src-tauri/src/vault.rs` | Disk format + session |
| `src-tauri/src/commands.rs` | IPC surface |
| `SECURITY.md` | Threat model — update when behavior changes |

## Identity rules (do not break)

- Identifier: `com.ahmi.secretfolder`
- App data: **not** SecretSticky’s folder
- AAD / domain strings: `secretfolder-*` only
- Icons, productName, GitHub repo: SecretFolder-specific
- Never read/write SecretSticky vault files or keys

## Vault durability rules (do not break)

1. **AppData is sacred** — updates replace the app binary only. Never delete/rewrite `%APPDATA%\com.ahmi.secretfolder\` from install/update paths.
2. **Backward-compatible vault reads** — any new code must still unlock and list items from vaults written by prior 0.1.x builds. Prefer optional fields + defaults; no destructive migrations. Persist via temp + OS replace-in-place (never delete `vault.json` before the new bytes are durable); keep `vault.json.bak` as last-known-good.
3. **No silent data wipe** — failed decrypt, locked vault, or ACL deny must error clearly; never replace the vault with an empty one “to fix” load errors.

## Scope discipline

In scope: explorer, text editor, image preview, import/export, tray, idle lock, clipboard clear, signed updater.

Out of scope for v1: cloud sync, accounts, FUSE/driver mounts, full video vault, multi-user sharing.

## Release / updater signing

Release workflow builds NSIS/MSI with `createUpdaterArtifacts` and publishes `latest.json`.

```bash
# Generate a minisign keypair (once per product or shared org key — document which)
npx tauri signer generate -w ~/.tauri/secretfolder.key
# Public key → tauri.conf.json plugins.updater.pubkey
# Private key → gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/secretfolder.key
```

Bump versions together: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and `CHANGELOG.md`.

```bash
git tag v0.1.0
git push origin v0.1.0
```

## Code style

- TypeScript strict; prefer small pure helpers with Vitest coverage
- Rust: `cargo fmt`, meaningful `AppError` variants, zeroize secrets
- UI: match existing dark chrome (`#1a1b1e` / `#24262b`, amber `#fbbf24`, Inter)

## License

By contributing you agree your changes are MIT-licensed with the project.
