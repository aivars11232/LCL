#!/usr/bin/env python3
"""Derive every LCL brand image from the one supplied master.

Run from the repository root:

    python3 assets/brand/derive_brand_assets.py

This is an authoring tool, not a build step. Its outputs are committed, so
building, installing and running LCL need neither Python nor Pillow. It exists
so that the derivatives can be regenerated and checked rather than being
unexplained binaries in a tree.

What it is allowed to do
------------------------

Only non-generative preparation: trim fully transparent outer margins, fit or
downsample, and pad transparently to a square. Nothing here draws, recolours,
traces, upscales or composites a background. The visible design, its
anti-aliased edges and its proportions come through unchanged, and every output
keeps its alpha channel.

The master is never rewritten. Its SHA-256 is checked before anything is read
from it, and a mismatch stops the run rather than producing art from an unknown
source.

Why a raster set rather than one scalable file
----------------------------------------------

The supplied master is a raster image. There is no vector original, and tracing
one would be inventing detail the source does not contain, so each size is
produced by downsampling and no output is larger than the master's visible
extent.
"""

import hashlib
import pathlib
import sys

try:
    from PIL import Image, ImageOps
except ImportError:  # pragma: no cover - the message is the whole behaviour
    sys.exit("Pillow is required to regenerate brand assets: pip install Pillow")

ROOT = pathlib.Path(__file__).resolve().parents[2]
MASTER = ROOT / "assets/brand/lcl-logo-master.png"
MANIFEST = ROOT / "assets/brand/BRAND_ASSETS.sha256"

# The supplied file, as delivered. Recorded here so the tool refuses to run
# against a substitute.
MASTER_SHA256 = "6c930f61a0db7c7e2e1d7e5926b18a4d74dee1f921c0937f8c8e8092d3a96b3d"

# The header image's height in the source it is served at. The page displays it
# far smaller and lets the browser keep the ratio, so this is a resolution
# budget for high-density screens rather than a layout number.
MARK_HEIGHT = 96

# Sizes a Linux icon theme is asked for. None exceeds the master's visible
# extent of 341 by 338, so every one is a downsample.
ICON_SIZES = (16, 24, 32, 48, 64, 128, 256)

# The browser tab icon. One size, declared explicitly by the page.
FAVICON_SIZE = 32


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_master():
    if not MASTER.is_file():
        sys.exit(f"the master is missing: {MASTER}")
    actual = digest(MASTER)
    if actual != MASTER_SHA256:
        sys.exit(
            f"the master at {MASTER} hashes {actual},\n"
            f"but this tool only derives from {MASTER_SHA256}.\n"
            "Restore the supplied file rather than deriving from an unknown one."
        )
    image = Image.open(MASTER)
    if image.mode != "RGBA":
        sys.exit(f"the master must carry an alpha channel, found mode {image.mode}")
    return image


def trimmed(image):
    """The master without its fully transparent outer margin.

    `getbbox` on an RGBA image is the alpha bounding box, so this removes only
    pixels that are invisible. Nothing visible is cropped.
    """
    box = image.getbbox()
    if box is None:
        sys.exit("the master is entirely transparent")
    return image.crop(box)


def by_height(image, height):
    """Downsample to an exact height, keeping the aspect ratio."""
    width = max(1, round(image.width * height / image.height))
    return image.resize((width, height), Image.LANCZOS)


def square(image, size):
    """Fit into a transparent square. Never stretched to fill it."""
    fitted = ImageOps.contain(image, (size, size), Image.LANCZOS)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    canvas.paste(
        fitted,
        ((size - fitted.width) // 2, (size - fitted.height) // 2),
    )
    return canvas


def write(image, relative, written):
    path = ROOT / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    # No metadata: a timestamp or a tool name in the file would make two runs
    # of this script produce different bytes for the same picture.
    image.save(path, "PNG", optimize=True)
    written.append(relative)
    print(f"  {relative}  {image.width}x{image.height}  {image.mode}")


def main():
    master = load_master()
    visible = trimmed(master)
    print(f"master  {master.width}x{master.height}, visible {visible.width}x{visible.height}")
    written = []

    # The workspace header, served by the binary it is compiled into.
    write(
        by_height(visible, MARK_HEIGHT),
        "impl/crates/lcl-workspace/assets/brand/lcl-mark.png",
        written,
    )
    write(
        square(visible, FAVICON_SIZE),
        f"impl/crates/lcl-workspace/assets/brand/lcl-icon-{FAVICON_SIZE}.png",
        written,
    )

    # The installed application icon, and the document icon for the media type
    # this product registers. Both come from the same source, so a launcher and
    # the files it opens are recognisably one thing.
    for size in ICON_SIZES:
        icon = square(visible, size)
        write(
            icon,
            f"packaging/icons/hicolor/{size}x{size}/apps/lcl-workspace.png",
            written,
        )
        write(
            icon,
            f"packaging/icons/hicolor/{size}x{size}/mimetypes/text-x-lcl.png",
            written,
        )

    lines = [f"{MASTER_SHA256}  assets/brand/lcl-logo-master.png"]
    lines += [f"{digest(ROOT / relative)}  {relative}" for relative in sorted(written)]
    MANIFEST.write_text("\n".join(lines) + "\n")
    print(f"\nwrote {MANIFEST.relative_to(ROOT)} covering {len(lines)} files")
    print("verify with:  sha256sum -c assets/brand/BRAND_ASSETS.sha256")


if __name__ == "__main__":
    main()
