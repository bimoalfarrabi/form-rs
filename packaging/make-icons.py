"""Build the app icon set from a rendered logo.

Run from the repository root:

    python3 packaging/make-icons.py

Inputs: `docs/images/material-metal.png` (2048x2048 render of the default logo).
Outputs: `packaging/icons/icon.png` (1024), `icon-<size>.png`, `icon.ico`,
`icon.icns`.
"""

from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "docs" / "images" / "material-metal.png"
OUT = ROOT / "packaging" / "icons"

SIZES = [16, 32, 48, 64, 128, 256, 512, 1024]


def rounded(img: Image.Image, radius_fraction: float = 0.22) -> Image.Image:
    """Mask the square render into a rounded square, like macOS/Windows icons."""
    size = img.size[0]
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle(
        (0, 0, size - 1, size - 1),
        radius=int(size * radius_fraction),
        fill=255,
    )
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(img.convert("RGBA"), (0, 0), mask)
    return out


def icns(entries: list[tuple[bytes, bytes]], dest: Path) -> None:
    """Write a PNG-based ICNS (supported since macOS 10.7)."""
    body = b"".join(t + len(d).to_bytes(4, "big") + d for t, d in entries)
    dest.write_bytes(b"icns" + (len(body) + 8).to_bytes(4, "big") + body)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    base = rounded(Image.open(SRC))
    base = base.resize((1024, 1024), Image.LANCZOS)
    base.save(OUT / "icon.png")

    for size in SIZES:
        base.resize((size, size), Image.LANCZOS).save(OUT / f"icon-{size}.png")

    base.resize((256, 256), Image.LANCZOS).save(
        OUT / "icon.ico",
        sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )

    def png(size: int) -> bytes:
        from io import BytesIO

        buf = BytesIO()
        base.resize((size, size), Image.LANCZOS).save(buf, format="PNG")
        return buf.getvalue()

    icns(
        [
            (b"ic07", png(128)),
            (b"ic08", png(256)),
            (b"ic09", png(512)),
            (b"ic10", png(1024)),
            (b"ic11", png(32)),
            (b"ic12", png(64)),
            (b"ic13", png(256)),
            (b"ic14", png(512)),
        ],
        OUT / "icon.icns",
    )
    print("wrote", OUT)


if __name__ == "__main__":
    main()
