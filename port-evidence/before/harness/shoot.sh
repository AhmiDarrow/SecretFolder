#!/usr/bin/env bash
# Renders every screen of the shipped React UI (with mock-tauri.js sample data) to
# port-evidence/before/*.png. Needs `npx vite --port 1420` running in the repo root and
# Microsoft Edge. Pages are laid out at the window size and cropped to it.
set -u
cd "$(dirname "$0")/.."
EDGE="/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"
BASE="http://localhost:1420/port-evidence/before/harness/index.html"
shot() {
  local name=$1 w=$2 h=$3 qs=$4
  local ww=$(( w < 600 ? 600 : w ))
  timeout 60 "$EDGE" --headless=old --disable-gpu --hide-scrollbars --user-data-dir="$TEMP/folder-edge" \
    --window-size=$ww,$h --virtual-time-budget=6000 --screenshot="$(cygpath -w "$PWD")/$name.png" \
    "$BASE?w=$w&h=$h&$qs" >/dev/null 2>&1
  powershell -NoProfile -Command "Add-Type -AssemblyName System.Drawing; \$s=[System.Drawing.Image]::FromFile('$(cygpath -w "$PWD")/$name.png'); \$b=New-Object System.Drawing.Bitmap $w,$h; \$g=[System.Drawing.Graphics]::FromImage(\$b); \$g.DrawImage(\$s,0,0,(New-Object System.Drawing.Rectangle 0,0,$w,$h),[System.Drawing.GraphicsUnit]::Pixel); \$s.Dispose(); \$b.Save('$(cygpath -w "$PWD")/$name.png'); \$b.Dispose()"
  echo "$name"
}
shot 01-setup 920 640 "state=setup"
shot 01-setup-short-password 920 640 "state=setup&fill=short&act=Create%20vault"
shot 02-recovery-key 920 640 "state=setup&fill=1&act=Create%20vault"
shot 03-unlock 920 640 "state=locked"
shot 03-unlock-wrong-password 920 640 "state=locked&fill=1&act=Unlock"
shot 03-unlock-recovery-key 920 640 "state=locked&act=Use%20recovery%20key"
shot 04-files 920 640 "state=unlocked"
shot 04-files-empty 920 640 "state=unlocked&empty=1"
shot 04-files-narrow 720 480 "state=unlocked"
shot 04-files-wide 1400 900 "state=unlocked"
shot 05-folder 920 640 "state=unlocked&act=dbl:Documents"
shot 06-text-open 920 640 "state=unlocked&act=Passwords.txt"
shot 07-image-open 920 640 "state=unlocked&act=dbl:Documents,photo.png"
shot 08-binary-open 920 640 "state=unlocked&act=dbl:Documents,dbl:Taxes%202026,backup.bin"
shot 09-new-folder-dialog 920 640 "state=unlocked&act=New%20folder"
shot 10-delete-folder-dialog 920 640 "state=unlocked&act=Remove%20folder%20Documents"
shot 10-delete-item-dialog 920 640 "state=unlocked&act=Remove%20Passwords.txt"
shot 11-settings 920 640 "state=unlocked&act=Settings"
shot 12-about 920 640 "state=unlocked&act=About"
