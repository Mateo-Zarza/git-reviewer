"""Generate the app icon files from assets/app-icon/icon.svg.

    python scripts/make-icons.py

Writes, next to the SVG:
  - icon.ico      Windows: built into the .exe by build.rs.
  - AppIcon.icns  macOS: copied into the .app by .github/workflows/build.yml. Drawn with
                  padding around it, the size Apple's icon grid gives an app icon
                  (824 of 1024 pixels).

Run it after changing icon.svg, and commit the files it writes. It needs the resvg
command-line tool on PATH (`cargo install resvg`), or its path in the RESVG variable.
No Python packages are needed.
"""

import os
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ICON_DIR = Path(__file__).resolve().parent.parent / "assets" / "app-icon"
SVG = ICON_DIR / "icon.svg"

ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]

# The icns entry types that hold a PNG, and each one's size in pixels.
ICNS_TYPES = [
    (b"icp4", 16),
    (b"icp5", 32),
    (b"ic11", 32),  # 16@2x
    (b"ic12", 64),  # 32@2x
    (b"ic07", 128),
    (b"ic13", 256),  # 128@2x
    (b"ic08", 256),
    (b"ic14", 512),  # 256@2x
    (b"ic09", 512),
    (b"ic10", 1024),  # 512@2x
]

# Apple's grid: the icon's body is 824 pixels on a 1024 canvas, centered.
MACOS_BODY = 824


def resvg() -> str:
    tool = os.environ.get("RESVG") or shutil.which("resvg")
    if not tool:
        sys.exit("resvg not found: install it with `cargo install resvg`, or set RESVG")
    return tool


def render(svg: Path, size: int, out_dir: Path) -> bytes:
    png = out_dir / f"{svg.stem}-{size}.png"
    subprocess.run(
        [resvg(), "--width", str(size), "--height", str(size), str(svg), str(png)],
        check=True,
    )
    return png.read_bytes()


def padded_svg(out_dir: Path) -> Path:
    """icon.svg shrunk to MACOS_BODY and centered on the same 1024 canvas."""
    offset = (1024 - MACOS_BODY) / 2
    svg = out_dir / "icon-macos.svg"
    svg.write_text(
        '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" '
        'width="1024" height="1024" viewBox="0 0 1024 1024">'
        f'<image x="{offset}" y="{offset}" width="{MACOS_BODY}" height="{MACOS_BODY}" '
        f'xlink:href="{SVG.as_posix()}"/></svg>',
        encoding="utf-8",
    )
    return svg


def write_ico(pngs: list[tuple[int, bytes]], path: Path) -> None:
    # ICONDIR, then one 16-byte ICONDIRENTRY per image, then the PNGs. A width or
    # height of 0 means 256.
    header = struct.pack("<HHH", 0, 1, len(pngs))
    entries = b""
    data = b""
    offset = len(header) + 16 * len(pngs)
    for size, png in pngs:
        side = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(png), offset + len(data))
        data += png
    path.write_bytes(header + entries + data)


def write_icns(entries: list[tuple[bytes, bytes]], path: Path) -> None:
    # "icns" and the file's length, then each entry as type, length (with its own
    # 8-byte header) and data. Big-endian.
    body = b"".join(kind + struct.pack(">I", 8 + len(png)) + png for kind, png in entries)
    path.write_bytes(b"icns" + struct.pack(">I", 8 + len(body)) + body)


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)

        write_ico([(size, render(SVG, size, tmp)) for size in ICO_SIZES], ICON_DIR / "icon.ico")

        macos = padded_svg(tmp)
        rendered = {size: render(macos, size, tmp) for size in {size for _, size in ICNS_TYPES}}
        write_icns([(kind, rendered[size]) for kind, size in ICNS_TYPES], ICON_DIR / "AppIcon.icns")

    print(f"Wrote {ICON_DIR / 'icon.ico'} and {ICON_DIR / 'AppIcon.icns'}")


if __name__ == "__main__":
    main()
