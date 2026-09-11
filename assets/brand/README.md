# The LCL mark

One supplied image, and everything derived from it.

## The master

`lcl-logo-master.png` is the file the owner supplied, byte for byte.

| | |
| --- | --- |
| SHA-256 | `6c930f61a0db7c7e2e1d7e5926b18a4d74dee1f921c0937f8c8e8092d3a96b3d` |
| Format | PNG, RGBA, 577 by 432 |
| Visible extent | 341 by 338, at (117, 46) to (458, 384) |
| Transparency | real, alpha spans 0 to 255 |

It is never rewritten, never regenerated, and never replaced. The artwork is
red and orange `LCL` lettering inside a green luminous border inside a dark
metallic frame, on transparency.

## The derivatives

`derive_brand_assets.py` produces every other image in the repository from that
one file, and refuses to run if the master's hash does not match. It performs
only non-generative preparation: trimming the fully transparent outer margin,
downsampling, and padding to a square with transparency. Nothing draws,
recolours, traces, upscales or adds a background, and no output is larger than
the master's visible extent.

```sh
python3 assets/brand/derive_brand_assets.py
sha256sum -c assets/brand/BRAND_ASSETS.sha256
```

Two runs produce identical bytes. `BRAND_ASSETS.sha256` records the master and
all sixteen derivatives, and is the file to check when auditing what shipped.

| Where it goes | What it is |
| --- | --- |
| `impl/crates/lcl-workspace/assets/brand/lcl-mark.png` | the header mark, 97 by 96, compiled into the binary |
| `impl/crates/lcl-workspace/assets/brand/lcl-icon-32.png` | the browser tab icon, 32 by 32 |
| `packaging/icons/hicolor/<size>/apps/lcl-workspace.png` | the installed application icon, seven sizes |
| `packaging/icons/hicolor/<size>/mimetypes/text-x-lcl.png` | the document icon for `text/x-lcl`, same source |

The derivatives are committed, so building, installing and running LCL need
neither Python nor Pillow. The script is an authoring tool.

## What a raster master means

There is no vector original. Tracing one would invent detail the source does
not contain, so every size is a downsample and none is an upscale. This is not
a scalable mark and nothing here claims it is.

## Where it appears, and where it does not

The workspace header and the browser tab come from the two workspace files
above, served over the same token-gated loopback routes as the stylesheet and
the script. The desktop menu entry and the `.lcl` document icon come from the
installed theme files.

The running browser's own window and taskbar identity is **not** changed by any
of this. That belongs to the browser, and setting a favicon does not alter it.
Changing it would need a native wrapper, which is a separate decision nobody
has taken.
