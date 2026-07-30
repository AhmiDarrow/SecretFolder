"""Prepare master app icon and public favicons from the approved ComfyUI concept."""
from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SRC_CANDIDATES = [
    Path(r"C:\Users\Administrator\Desktop\SecretFolder-icon-preview.png"),
    Path(r"C:\Users\Administrator\.remedy\comfy_out\remedy_comfy_00014_.png"),
    Path(
        r"C:\Users\Administrator\.remedy\attachments"
        r"\24a7b1c0-854c-4d94-9530-16ca23645561\remedy_comfy_00014_.png"
    ),
    Path(r"C:\Users\Administrator\Documents\comfy\ComfyUI\output\remedy_comfy_00014_.png"),
]


def main() -> int:
    src = next((p for p in SRC_CANDIDATES if p.is_file()), None)
    if src is None:
        print("source icon not found", file=sys.stderr)
        return 1

    public = ROOT / "public"
    public.mkdir(parents=True, exist_ok=True)
    master = ROOT / "app-icon.png"
    assets = ROOT / "src" / "assets"
    assets.mkdir(parents=True, exist_ok=True)

    im = Image.open(src).convert("RGBA")
    # Solid tile for Windows tray / installer — no accidental transparency.
    px = im.load()
    w, h = im.size
    for y in range(h):
        for x in range(w):
            r, g, b, _a = px[x, y]
            px[x, y] = (r, g, b, 255)

    im.save(master, "PNG", optimize=True)
    im.resize((32, 32), Image.Resampling.LANCZOS).save(public / "favicon-32.png", "PNG")
    im.resize((192, 192), Image.Resampling.LANCZOS).save(public / "icon-192.png", "PNG")
    im.save(public / "app-icon.png", "PNG", optimize=True)
    # UI mark used on unlock / about screens
    im.resize((128, 128), Image.Resampling.LANCZOS).save(assets / "app-mark.png", "PNG")
    print(f"wrote {master} {im.size} {master.stat().st_size} bytes from {src}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
