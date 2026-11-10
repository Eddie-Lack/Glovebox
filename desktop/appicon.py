"""The app's icon: a garib (a gold four-pointed star) on a dark round tile. Drawn with Pillow, so tools/build_app.py can
write it into the .exe as an .ico and the window can use the same picture."""
import io, math
from PIL import Image, ImageDraw

SIZES = (16, 24, 32, 48, 64, 128, 256)


def picture(size=256):
    s = size * 4   # drawn large, then shrunk: smooth edges
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    pad = s * 0.04
    d.ellipse((pad, pad, s - pad, s - pad), fill=(29, 31, 38, 255), outline=(242, 193, 78, 255), width=max(1, int(s * 0.035)))
    c, outer, inner = s / 2, s * 0.36, s * 0.11
    pts = []
    for i in range(8):
        r = outer if i % 2 == 0 else inner
        a = math.pi / 2 + i * math.pi / 4
        pts.append((c + r * math.cos(a), c - r * math.sin(a)))
    d.polygon(pts, fill=(242, 193, 78, 255))
    return img.resize((size, size), Image.LANCZOS)


def write_ico(path):
    picture(256).save(path, format="ICO", sizes=[(n, n) for n in SIZES])


def png_bytes(size=256):
    out = io.BytesIO()
    picture(size).save(out, format="PNG")
    return out.getvalue()


def qicon():
    from PySide6.QtGui import QIcon, QPixmap
    icon = QIcon()
    for n in SIZES:
        pm = QPixmap()
        pm.loadFromData(png_bytes(n), "PNG")
        icon.addPixmap(pm)
    return icon
