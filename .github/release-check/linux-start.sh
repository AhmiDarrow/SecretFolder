#!/usr/bin/env bash
# Starts a release's Linux package on this distribution (run inside its container by
# release-check.yml): unpack it as a user would, start it on a virtual screen against a copy of
# the 0.2.0 test vault, and check that the window appears, the program keeps running, and the
# vault is left byte for byte.
#
#   VERSION=1.0.0 .github/release-check/linux-start.sh    (from the repository root, with dist/)
set -euo pipefail
: "${VERSION:?VERSION is not set}"
# /etc/os-release has its own VERSION, so read it in a subshell.
PRETTY_NAME=$(. /etc/os-release && echo "$PRETTY_NAME")
ID=$(. /etc/os-release && echo "$ID")
echo "== $PRETTY_NAME, $(ldd --version | head -1)"

# What a desktop has and a bare container does not: an X server for the test, and the
# window-system libraries the program loads.
case "$ID" in
  debian)
    # Debian 10 is on archive.debian.org now.
    sed -i -e 's|deb.debian.org|archive.debian.org|g' -e 's|security.debian.org|archive.debian.org|g' \
      -e '/buster-updates/d' /etc/apt/sources.list
    apt-get -o Acquire::Check-Valid-Until=false update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq xvfb x11-utils libx11-xcb1 libxcursor1 \
      libxrandr2 libxi6 libxkbcommon0 libxkbcommon-x11-0 >/dev/null
    ;;
  ubuntu)
    apt-get update -qq || {
      sed -i 's|http://[a-z.]*ubuntu.com|http://old-releases.ubuntu.com|g' /etc/apt/sources.list
      apt-get update -qq
    }
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq xvfb x11-utils libx11-xcb1 libxcursor1 \
      libxrandr2 libxi6 libxkbcommon0 libxkbcommon-x11-0 >/dev/null
    ;;
  rocky | rhel | almalinux | centos)
    dnf install -y -q xorg-x11-server-Xvfb xorg-x11-utils libX11-xcb libXcursor libXrandr libXi \
      libxkbcommon libxkbcommon-x11 tar gzip findutils >/dev/null
    ;;
  *)
    echo "::error::no package list for $ID"
    exit 1
    ;;
esac

echo "== install as the README says"
mkdir -p "$HOME/.local"
tar -xzf "dist/SecretFolder-$VERSION-linux-x86_64.tar.gz" --strip-components=1 -C "$HOME/.local"
prog="$HOME/.local/bin/SecretFolder"
ls -l "$prog" "$HOME/.local/share/applications/com.ahmi.secretfolder.desktop"
if ldd "$prog" | grep 'not found'; then
  echo "::error::a library the program needs is missing"
  exit 1
fi

vault="$(mktemp -d)/vault"
cp -R remedyui/tests/fixtures/vault-v0.2.0 "$vault"
hashes() { (cd "$vault" && find . -type f -exec sha256sum {} + | sort -k2); }
before="$(hashes)"

Xvfb :99 -screen 0 1280x800x24 >/tmp/xvfb.log 2>&1 &
export DISPLAY=:99
sleep 2

echo "== start"
SECRETFOLDER_VAULT="$vault" "$prog" >/tmp/app.log 2>&1 &
app=$!
fail() {
  echo "::error::$1"
  echo "--- program output"
  cat /tmp/app.log || true
  echo "--- windows"
  xwininfo -root -tree | head -40 || true
  exit 1
}
seen=
for _ in $(seq 1 45); do
  sleep 1
  kill -0 "$app" 2>/dev/null || fail "SecretFolder exited at start"
  if xwininfo -root -tree | grep -q '"SecretFolder'; then
    seen=1
    break
  fi
done
[ -n "$seen" ] || fail "no SecretFolder window after 45 s"
xwininfo -root -tree | grep '"SecretFolder'
sleep 5
kill -0 "$app" 2>/dev/null || fail "SecretFolder exited after its window appeared"
kill "$app"
wait "$app" 2>/dev/null || true

[ "$before" = "$(hashes)" ] || fail "the vault changed"
echo "== program output"
cat /tmp/app.log
echo "PASS  $PRETTY_NAME: installed, window shown, kept running, vault byte for byte"
