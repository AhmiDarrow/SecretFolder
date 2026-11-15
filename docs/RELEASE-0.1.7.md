# SecretFolder 0.1.7

Idle auto-lock works again, and a save or delete that fails can no longer leave the vault half-updated.

Hi I'm Ahmi, hope this helps!

## Fixes

- **Idle auto-lock locks the vault.** Only keyboard, pointer, and scroll input resets the idle timer. A background ping every 30 seconds had been keeping the vault unlocked for as long as the window was open.
- **Failed unlocks wait longer each time.** After repeated failures the pause steps through 10 seconds, 30 seconds, 60 seconds, and 5 minutes. A successful unlock clears that count. Each lockout had been clearing the counter that was supposed to raise the next pause, so the wait stayed at 10 seconds.
- **Unlock works in the first minute after Windows starts.** The rate limiter could crash when its 60-second attempt window was still younger than the system timer, and then every later unlock failed until SecretFolder was restarted.
- **A rejected edit leaves the file alone.** Saving text checks that the item is a text file, and that any new name is allowed, before anything is written. An image or other file keeps its contents when that save is refused.
- **A failed delete keeps the item.** The index is saved before file contents are removed. If that save fails, the item and its contents stay.
- **A failed save matches what is on disk.** The open session adopts the new index only after the write succeeds. A password change that fails to save still unlocks with the previous password. If the first save during setup fails, setup can be tried again.
- **Import continues after one bad file.** Files that import are kept and shown, and SecretFolder reports the ones that failed.
- **Idle-lock minutes are checked before they are saved.** A blank value, or a value under 1 minute, is rejected. A value over 240 minutes is saved as 240 minutes.
- **Exports stay out of the vault folder.** A decrypted copy cannot be saved into SecretFolder's data folder (`%APPDATA%\com.ahmi.secretfolder`), where it would sit beside the vault or overwrite `vault.json`.

## Build/packaging

- Development tools only: Vite 7.3.8, Vitest 4.1.12, and patched nanoid and source-map-js. No runtime dependency of the installed app changed.

## Install and update

- Windows setup: `SecretFolder_0.1.7_x64-setup.exe` (Start Menu, desktop shortcut, and taskbar icon). `SecretFolder_0.1.7_x64_en-US.msi` is attached as well.
- Already installed: **About → Check for updates → Install & restart**. The update replaces the application only.
- The vault stays in `%APPDATA%\com.ahmi.secretfolder` and unlocks with the same master password and recovery key.
- Local only. There is no cloud sync and no account. Network use is limited to signed updates from GitHub Releases.
