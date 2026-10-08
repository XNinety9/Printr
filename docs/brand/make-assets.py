"""Déclinaisons du logo (site, README, interface web), à partir des deux originaux :
printr-logo.png (fond sombre) et printr-logo-light.png (fond clair).

    uv run --with pillow python docs/brand/make-assets.py
"""

from collections import deque
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
DARK = Image.open(ROOT / "docs/brand/printr-logo.png").convert("RGBA")
LIGHT = Image.open(ROOT / "docs/brand/printr-logo-light.png").convert("RGBA")
BLACK = (17, 16, 15)
# Bas du pictogramme : le mot « Printr » commence vers y = 880.
MARK_BOX = (236, 70, 1018, 852)


def transparent(img: Image.Image, tolerance: int = 30) -> Image.Image:
    """Retire le fond crème de la version claire, par remplissage depuis les bords : le ticket
    et l'imprimante, cernés de noir, ne sont pas atteints. Bords adoucis."""
    rgb = img.convert("RGB")
    w, h = rgb.size
    px = rgb.load()
    ref = px[8, 8]
    near = lambda c: sum(abs(a - b) for a, b in zip(c, ref)) <= tolerance
    seen = bytearray(w * h)
    queue = deque([(x, y) for x in range(w) for y in (0, h - 1)] + [(x, y) for y in range(h) for x in (0, w - 1)])
    while queue:
        x, y = queue.popleft()
        i = y * w + x
        if seen[i] or not near(px[x, y]):
            continue
        seen[i] = 1
        queue.extend(p for p in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)) if 0 <= p[0] < w and 0 <= p[1] < h)
    mask = Image.frombytes("L", (w, h), bytes(0 if s else 255 for s in seen))
    mask = ImageChops.multiply(mask, mask.filter(ImageFilter.GaussianBlur(1.0)).point(lambda v: min(255, v * 2)))
    out = rgb.convert("RGBA")
    out.putalpha(mask.filter(ImageFilter.GaussianBlur(0.6)))
    return out


def on_square(img: Image.Image, size: int, scale: float, color, radius: int = 0) -> Image.Image:
    """Image centrée sur un carré de couleur (icônes d'appli : iOS refuse la transparence)."""
    canvas = Image.new("RGBA", (size, size), color + (255,))
    inner = int(size * scale)
    canvas.alpha_composite(img.resize((inner, inner), Image.LANCZOS), ((size - inner) // 2, (size - inner) // 2))
    if radius:
        mask = Image.new("L", (size, size), 0)
        ImageDraw.Draw(mask).rounded_rectangle([0, 0, size - 1, size - 1], radius, fill=255)
        canvas.putalpha(mask)
    return canvas


def save(img: Image.Image, path: str, size: int | None = None):
    if size:
        img = img.resize((size, size), Image.LANCZOS)
    target = ROOT / path
    target.parent.mkdir(parents=True, exist_ok=True)
    img.save(target, optimize=True)
    print(f"{path:34} {img.size[0]}×{img.size[1]}")


clear = transparent(LIGHT)
mark = clear.crop(MARK_BOX)

save(DARK, "docs/logo-dark.png", 512)
save(clear, "docs/logo-light.png", 512)
save(mark, "docs/mark.png", 256)
save(mark, "web/favicon.png", 96)
save(mark, "web/mark.png", 256)
save(on_square(mark, 512, 0.86, (246, 241, 226)), "web/icon-512.png")
save(on_square(mark, 192, 0.86, (246, 241, 226)), "web/icon-192.png")
save(on_square(mark, 180, 0.86, (246, 241, 226)), "web/apple-touch-icon.png")
