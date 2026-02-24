#!/usr/bin/env python3
"""Generate Focusmute tray icons from the extracted 'ff' glyph mask.

Uses ff-extracted.png (the actual Focusrite 'ff' glyph) as an alpha mask
to produce pixel-perfect icons at all sizes.

Produces:
  icon-live.ico   (16–256 multi-size)
  icon-muted.ico  (16–256 multi-size)
  icon-live.png   (256×256)
  icon-muted.png  (256×256)

Requires: Pillow (pip install Pillow)
"""

from __future__ import annotations

from pathlib import Path
from PIL import Image, ImageDraw

# ── Colour palette ────────────────────────────────────────────────────
GOLD = (221, 182, 105)
BG = (30, 30, 30)
RED = (200, 40, 40, 255)
RED_OUTLINE = (60, 15, 15, 255)

# ── Layout constants (fractions of icon size) ─────────────────────────
CORNER_RADIUS_FRAC = 0.09  # 9 % of icon side
LOGO_W_FRAC = 0.78  # logo width  / icon side
LOGO_H_FRAC = 0.64  # logo height / icon side

# Crossbar geometry measured from ff-extracted.png (503 × 417).
# The crossbar is the thin horizontal stroke through both f's.
CROSSBAR_Y_FRAC = 0.477        # vertical centre in glyph space
CROSSBAR_X0_FRAC = 0.062       # left extent
CROSSBAR_X1_FRAC = 0.942       # right extent
CROSSBAR_SRC_THICKNESS = 13    # source px thickness
CROSSBAR_SRC_HEIGHT = 417      # source glyph height

# ── Output sizes ──────────────────────────────────────────────────────
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]
PNG_SIZE = 256

# ── Glyph mask ────────────────────────────────────────────────────────
ASSETS_DIR = Path(__file__).parent
_MASK_CACHE: Image.Image | None = None


def _load_mask() -> Image.Image:
    """Load ff-extracted.png as a grayscale alpha mask (white = opaque)."""
    global _MASK_CACHE
    if _MASK_CACHE is None:
        _MASK_CACHE = Image.open(ASSETS_DIR / "ff-extracted.png").convert("L")
    return _MASK_CACHE


# ═══════════════════════════════════════════════════════════════════════
#  Rendering
# ═══════════════════════════════════════════════════════════════════════

def _render_logo(size: int) -> Image.Image:
    """Render the 'ff' logo at *size* × *size*."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Rounded-rect background
    cr = round(CORNER_RADIUS_FRAC * size)
    draw.rounded_rectangle([(0, 0), (size - 1, size - 1)], radius=cr, fill=(*BG, 255))

    # Scale the glyph mask to fit the logo area
    mask_src = _load_mask()
    target_w = round(LOGO_W_FRAC * size)
    target_h = round(LOGO_H_FRAC * size)

    # Fit preserving aspect ratio
    src_w, src_h = mask_src.size
    scale = min(target_w / src_w, target_h / src_h)
    glyph_w = round(src_w * scale)
    glyph_h = round(src_h * scale)
    mask_scaled = mask_src.resize((glyph_w, glyph_h), Image.LANCZOS)

    # Centre the glyph on the icon
    ox = (size - glyph_w) // 2
    oy = (size - glyph_h) // 2

    # Create a gold-coloured layer and composite using the mask
    gold_layer = Image.new("RGBA", (glyph_w, glyph_h), (*GOLD, 255))
    img.paste(gold_layer, (ox, oy), mask=mask_scaled)

    # At small sizes the crossbar (≈13 src px) scales below 1 px and
    # vanishes.  Draw an explicit gold line to guarantee visibility.
    # Use 2 px width so the line survives Windows DPI scaling.
    natural_thickness = glyph_h * CROSSBAR_SRC_THICKNESS / CROSSBAR_SRC_HEIGHT
    if natural_thickness < 1.5:
        cy = oy + round(glyph_h * CROSSBAR_Y_FRAC)
        x0 = ox + round(glyph_w * CROSSBAR_X0_FRAC)
        x1 = ox + round(glyph_w * CROSSBAR_X1_FRAC)
        draw.line([(x0, cy), (x1, cy)], fill=(*GOLD, 255), width=1)

    return img


def _add_strikethrough(img: Image.Image) -> Image.Image:
    """Add a red diagonal strikethrough with dark outline."""
    size = img.width
    result = img.copy()
    draw = ImageDraw.Draw(result)

    # Diagonal from top-left to bottom-right, inset from corners
    margin = size * 0.12
    x0, y0 = margin, margin          # top-left
    x1, y1 = size - margin, size - margin  # bottom-right

    # Outline (dark) — thicker
    outline_w = max(3, round(size * 0.055))
    draw.line([(x0, y0), (x1, y1)], fill=RED_OUTLINE, width=outline_w)

    # Red line — thinner
    line_w = max(2, round(size * 0.035))
    draw.line([(x0, y0), (x1, y1)], fill=RED, width=line_w)

    return result


# ═══════════════════════════════════════════════════════════════════════
#  File output
# ═══════════════════════════════════════════════════════════════════════

def _save_ico(images: list[Image.Image], path: Path):
    """Save a multi-resolution ICO file."""
    images_sorted = sorted(images, key=lambda im: im.width, reverse=True)
    images_sorted[0].save(
        str(path),
        format="ICO",
        sizes=[(im.width, im.height) for im in images_sorted],
        append_images=images_sorted[1:],
    )


def main():
    out_dir = ASSETS_DIR

    # Render live icons at all sizes
    print("Rendering live icons...")
    live_images = []
    for sz in ICO_SIZES:
        img = _render_logo(sz)
        live_images.append(img)
        if sz == PNG_SIZE:
            img.save(str(out_dir / "icon-live.png"))
            print(f"  icon-live.png ({sz}×{sz})")

    _save_ico(live_images, out_dir / "icon-live.ico")
    print(f"  icon-live.ico (sizes: {ICO_SIZES})")

    # Render muted icons
    print("Rendering muted icons...")
    muted_images = []
    for sz in ICO_SIZES:
        base = _render_logo(sz)
        muted = _add_strikethrough(base)
        muted_images.append(muted)
        if sz == PNG_SIZE:
            muted.save(str(out_dir / "icon-muted.png"))
            print(f"  icon-muted.png ({sz}×{sz})")

    _save_ico(muted_images, out_dir / "icon-muted.ico")
    print(f"  icon-muted.ico (sizes: {ICO_SIZES})")

    print("Done.")


if __name__ == "__main__":
    main()
