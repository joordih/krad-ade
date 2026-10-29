"""Renders the Krad ADE icon in every format and size the packages need.

The logo is the transparent PNG next to this script. Its glyph is centered on
a white rounded tile and rasterised with Pillow for the PNG, ICO and ICNS
files and the README banner, so every output matches the logo.

Usage:
    python3 packaging/icon/generate_icons.py
"""

from __future__ import annotations

import logging
from pathlib import Path
from typing import Final

from PIL import Image, ImageDraw, ImageFilter, ImageFont

GRID: Final = 1024
SUPERSAMPLE: Final = 4
PNG_SIZES: Final = (16, 24, 32, 48, 64, 128, 256, 512, 1024)
ICO_SIZES: Final = (16, 24, 32, 48, 64, 128, 256)
ICNS_SIZES: Final = (16, 32, 64, 128, 256, 512, 1024)

ICON_DIR: Final = Path(__file__).resolve().parent
PACKAGING_DIR: Final = ICON_DIR.parent
OUTPUT_NAME: Final = "krad-ade"
LOGO_PATH: Final = ICON_DIR / f"{OUTPUT_NAME}-logo.png"
REPO_DIR: Final = PACKAGING_DIR.parent
FONT_DIR: Final = REPO_DIR / "assets" / "fonts"
BANNER_PATH: Final = REPO_DIR / "assets" / "banner.png"

TILE_INSET: Final = 100
TILE_RADIUS: Final = 185
TILE_COLOR: Final = "#ffffff"
GLYPH_INSET: Final = 230
GLYPH_COLOR: Final = "#000000"
GLYPH_ALPHA_THRESHOLD: Final = 128
SHADOW_ALPHA: Final = 0.28
SHADOW_BLUR: Final = 22
SHADOW_OFFSET: Final = 18

BANNER_SIZE: Final = (1600, 520)
BANNER_ICON: Final = 360
BANNER_MARGIN: Final = 110
BANNER_GAP: Final = 70
BANNER_BACKGROUND: Final = "#111114"
BANNER_TITLE: Final = "Krad ADE"
BANNER_TAGLINE: Final = "Many coding agents. One workspace."
BANNER_HARNESSES: Final = "Claude Code  only in Alpha. More harnesses coming soon."
BANNER_TITLE_FONT: Final = "Inter-Bold.ttf"
BANNER_TEXT_FONT: Final = "Inter-Regular.ttf"
BANNER_TITLE_SIZE: Final = 132
BANNER_TAGLINE_SIZE: Final = 48
BANNER_HARNESS_SIZE: Final = 30
BANNER_TITLE_COLOR: Final = "#f5f5f7"
BANNER_TAGLINE_COLOR: Final = "#c7c7cc"
BANNER_HARNESS_COLOR: Final = "#8e8e93"
BANNER_LINE_GAP: Final = 26

logger = logging.getLogger(__name__)


def glyph_mask(side: int) -> Image.Image:
    """Crops the logo glyph and fits it, centered, into a square mask.

    Args:
        side: The side of the square mask in pixels.

    Returns:
        An L mask with the glyph's coverage.
    """
    alpha = Image.open(LOGO_PATH).getchannel("A")
    # The logo carries faint alpha noise far from the glyph; crop to solid ink.
    solid = alpha.point(lambda value: 255 if value > GLYPH_ALPHA_THRESHOLD else 0)
    glyph = alpha.crop(solid.getbbox())
    scale = side / max(glyph.size)
    fitted = glyph.resize(
        (round(glyph.width * scale), round(glyph.height * scale)),
        Image.Resampling.LANCZOS,
    )
    mask = Image.new("L", (side, side), 0)
    mask.paste(fitted, ((side - fitted.width) // 2, (side - fitted.height) // 2))
    return mask


def render(size: int) -> Image.Image:
    """Rasterises the icon at one size.

    Args:
        size: The side of the square icon in pixels.

    Returns:
        The RGBA icon.
    """
    work = size * SUPERSAMPLE
    scale = work / GRID
    canvas = Image.new("RGBA", (work, work), (0, 0, 0, 0))
    near, far = TILE_INSET * scale, work - TILE_INSET * scale
    box = (near, near, far, far)
    radius = TILE_RADIUS * scale
    shadow = Image.new("L", canvas.size, 0)
    offset = SHADOW_OFFSET * scale
    ImageDraw.Draw(shadow).rounded_rectangle(
        (box[0], box[1] + offset, box[2], box[3] + offset),
        radius,
        fill=round(255 * SHADOW_ALPHA),
    )
    shadow = shadow.filter(ImageFilter.GaussianBlur(SHADOW_BLUR * scale))
    canvas.paste(GLYPH_COLOR, mask=shadow)
    ImageDraw.Draw(canvas).rounded_rectangle(box, radius, fill=TILE_COLOR)
    inset = round(GLYPH_INSET * scale)
    canvas.paste(GLYPH_COLOR, (inset, inset), glyph_mask(work - 2 * inset))
    return canvas.resize((size, size), Image.Resampling.LANCZOS)


def render_banner() -> Image.Image:
    """Renders the README banner: the icon beside the name and the tagline.

    Returns:
        The RGBA banner.
    """
    width, height = BANNER_SIZE
    banner = Image.new("RGBA", BANNER_SIZE, BANNER_BACKGROUND)
    icon_top = (height - BANNER_ICON) // 2
    banner.alpha_composite(render(BANNER_ICON), (BANNER_MARGIN, icon_top))
    title_font = ImageFont.truetype(FONT_DIR / BANNER_TITLE_FONT, BANNER_TITLE_SIZE)
    tagline_font = ImageFont.truetype(FONT_DIR / BANNER_TEXT_FONT, BANNER_TAGLINE_SIZE)
    harness_font = ImageFont.truetype(FONT_DIR / BANNER_TEXT_FONT, BANNER_HARNESS_SIZE)
    lines = [
        (BANNER_TITLE, title_font, BANNER_TITLE_COLOR),
        (BANNER_TAGLINE, tagline_font, BANNER_TAGLINE_COLOR),
        (BANNER_HARNESSES, harness_font, BANNER_HARNESS_COLOR),
    ]
    draw = ImageDraw.Draw(banner)
    heights = [draw.textbbox((0, 0), text, font=font)[3] for text, font, _ in lines]
    total = sum(heights) + BANNER_LINE_GAP * (len(lines) - 1)
    left = BANNER_MARGIN + BANNER_ICON + BANNER_GAP
    top = (height - total) // 2
    for (text, font, color), line_height in zip(lines, heights, strict=True):
        draw.text((left, top), text, font=font, fill=color)
        top += line_height + BANNER_LINE_GAP
    if left + max(draw.textlength(text, font=font) for text, font, _ in lines) > width:
        logger.warning("Banner text runs past the right edge")
    return banner


def write_outputs() -> None:
    """Writes the PNG, ICO and ICNS files and the README banner."""
    png_dir = ICON_DIR / "png"
    png_dir.mkdir(parents=True, exist_ok=True)
    images = {size: render(size) for size in PNG_SIZES}
    for size, image in images.items():
        image.save(png_dir / f"{OUTPUT_NAME}-{size}.png", optimize=True)
    windows_dir = PACKAGING_DIR / "windows"
    windows_dir.mkdir(exist_ok=True)
    largest_ico = images[max(ICO_SIZES)]
    largest_ico.save(
        windows_dir / f"{OUTPUT_NAME}.ico",
        sizes=[(size, size) for size in ICO_SIZES],
        append_images=[images[size] for size in ICO_SIZES],
    )
    macos_dir = PACKAGING_DIR / "macos"
    macos_dir.mkdir(exist_ok=True)
    images[max(ICNS_SIZES)].save(
        macos_dir / f"{OUTPUT_NAME}.icns",
        append_images=[images[size] for size in ICNS_SIZES],
    )
    BANNER_PATH.parent.mkdir(parents=True, exist_ok=True)
    render_banner().save(BANNER_PATH, optimize=True)
    logger.info("Icons under %s, banner at %s", PACKAGING_DIR, BANNER_PATH)


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    write_outputs()
