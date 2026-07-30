# Security Policy

## Threat model (honest)

SecretFolder is an **at-rest** encrypted file vault for a single trusted Windows user.

### What SecretFolder protects

- Vault data under the app data directory (`%APPDATA%\com.ahmi.secretfolder\`)
- Item **names** and **contents** (authenticated encryption)
- Casual offline access (stolen disk image, unattended backup copy)

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

If you need air-gapped operation, do not use Check for updates / block the endpoints in your firewall.

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

