# Security Policy

## Non-negotiable: updates must NEVER put vault data at risk

**Hard product invariant.** App updates (NSIS/MSI install, in-app auto-update, or replacing the binary) must **never**:

1. Delete, truncate, wipe, or overwrite `%APPDATA%\com.ahmi.secretfolder\` (including `vault.json` / `blobs/`) as part of install/update
2. Rewrite ciphertext so existing items cannot decrypt with the same master password / recovery key
3. Ship a vault-format change that cannot **read** vaults written by every prior supported 0.1.x build
4. Replace a load/decrypt failure with an empty vault “to fix” it

**Allowed / required:**

- Installers and the updater replace **application binaries only** under the install dir — never the user vault under AppData
- On-disk format evolution is **additive and backward-compatible** only (new optional fields with defaults; read old → write new only after a successful unlock)
- Atomic vault index saves (`vault.json.tmp` then OS replace-in-place — never delete `vault.json` before the new file is durable)
- Last-known-good sibling `vault.json.bak` before each successful replace; boot restores from `.bak` if the live file is missing or unreadable
- UI or ACL bugs may block *display*, but must not destroy *ciphertext on disk*

If a change could violate this, it **does not ship**. Fix data safety first.

## Threat model (honest)

SecretFolder is an **at-rest** encrypted file vault for a single trusted Windows user.

### What SecretFolder protects

- Vault data under the app data directory (`%APPDATA%\com.ahmi.secretfolder\`)
- Item **names** and **contents** (authenticated encryption)
- Casual offline access (stolen disk image, unattended backup copy)
- Continuity of saved files across app updates (see invariant above)

Crypto: **Argon2id** password KDF + **XChaCha20-Poly1305** AEAD, with a stable content key so password changes do not rewrite every blob. AAD strings use the `secretfolder-*` namespace (not SecretSticky).

### What SecretFolder does **not** protect

- Malware, keyloggers, or a hostile process while the vault is unlocked
- Screen capture / shoulder surfing of open editors and image previews
- Cold-boot or DMA attacks against a live unlocked session
- Compelled disclosure of your password or recovery key
- Integrity of the **update channel** if GitHub or your machine is already fully compromised (updates are signed; vault still never syncs)

### Network

| Traffic | Purpose |
|---------|---------|
| **None** for vault data | No cloud sync, accounts, analytics, or telemetry |
| **GitHub Releases only** | Signed auto-update (`latest.json` + installer artifacts) |

If you need air-gapped operation, do not use Check for updates / Install & restart, and/or block the endpoints in your firewall.

### Product defaults

- Idle auto-lock (default 15 minutes)
- Clipboard auto-clear after copy (default 30 seconds)
- Strict CSP in the WebView
- Least-privilege Tauri capabilities
- Separate app identity from SecretSticky (data dir, identifier, AAD, icons, repo)
- Master password minimum length: **12** characters
- Unlock rate limit (sliding window + escalating cooldown); all auth/crypto failures count
- Import/export paths restricted to user home / Desktop / Documents / Downloads / temp / vault root (component-boundary checks)
- Session plaintext item cache cleared on lock; content-key material zeroized on drop

### Limits (v1)

- Items are encrypted in memory (practical size cap ~25 MiB per item)
- Not a FUSE/driver mount — import/export and in-app preview only
- Recovery key is shown once at setup; store it offline


## Updates / signing

Release artifacts are signed with a Tauri updater keypair (rsign). The public key is embedded in `src-tauri/tauri.conf.json`.

- Private key lives only in the maintainer key store and GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY` (rsign encrypted format, empty password).
- Key rotation (e.g. 0.1.2) breaks automatic update verification from builds signed with the previous key — users must install the new build once, then updates chain again.
- Never commit private keys. Prefer `TAURI_SIGNING_PRIVATE_KEY` over path-based secrets in CI.
- Releases must be **published** (not draft). Draft releases leave `/releases/latest` on the previous tag and break auto-update.

### Updater must never touch vault data

Hard separation (by design, not best-effort):

| Path | Role |
|------|------|
| Install dir (e.g. `%LOCALAPPDATA%\Programs\SecretFolder`) | App binary + resources — **only** thing the updater replaces |
| `%APPDATA%\com.ahmi.secretfolder\` | Encrypted vault root (`default_vault_root`) — **never** an updater target |

Additional guards:

1. Before download/install, the UI path calls `lock_vault` so the master key and plaintext session map are dropped (best-effort if already locked).
2. NSIS is `currentUser` install mode; vault stays under Roaming AppData, not next to the exe.
3. Updater network allowlist is GitHub Releases only; no vault I/O on that code path.
4. Uninstall/update of the app package must not delete AppData vault files (standard Tauri/NSIS layout — data dir is outside the install prefix).
5. Index saves use temp + `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)` on Windows (never delete-then-rename) and keep `vault.json.bak` as last-known-good.

## Reporting a vulnerability

1. **Do not** open a public GitHub issue for exploitable bugs.
2. Email or message the maintainer privately (GitHub: [AhmiDarrow](https://github.com/AhmiDarrow)).
3. Include: SecretFolder version, OS build, steps to reproduce, impact.
4. Allow reasonable time for a fix before public disclosure.

## Supported versions

| Version | Supported |
|---------|-----------|
| Latest release on GitHub | Yes |
| Older releases | Best-effort only — please upgrade |

## Hardening tips for users

- Use a long, unique master password
- Store the recovery key offline (paper / password manager not synced to the same PC alone)
- Lock from the tray when leaving the desk
- Keep Windows and WebView2 updated
- Prefer official GitHub Release installers (signed updater path)

