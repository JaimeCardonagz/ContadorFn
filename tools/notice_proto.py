"""Prototipo de análisis del aviso de experiencia (solo desarrollo).

Segmenta las líneas de texto del aviso en la zona fija del HUD e imprime sus
cajas, para elegir umbrales antes de escribir el lector en Rust.
Uso: python3 tools/notice_proto.py Debug/frames/*.png
"""
import sys
from PIL import Image

ROI = (0.60, 0.235, 0.80, 0.41)  # x0, y0, x1, y1 normalizados (igual que notice.rs)


def is_fill(p):
    r, g, b = p
    return r >= 205 and b >= 220 and g >= 165 and r - g >= 6 and b - g >= 6


def is_outline(p):
    r, g, b = p
    return b >= 95 and b - r >= 22 and r - g >= 8 and (r + g + b) < 480


def masks(im):
    w, h = im.size
    x0, y0, x1, y1 = int(ROI[0] * w), int(ROI[1] * h), int(ROI[2] * w), int(ROI[3] * h)
    px = im.load()
    W, H = x1 - x0, y1 - y0
    fill = [[is_fill(px[x0 + x, y0 + y]) for x in range(W)] for y in range(H)]
    out = [[is_outline(px[x0 + x, y0 + y]) for x in range(W)] for y in range(H)]
    # texto = relleno claro con contorno violeta a 2 px o menos
    text = [[False] * W for _ in range(H)]
    for y in range(H):
        for x in range(W):
            if fill[y][x]:
                near = any(
                    out[yy][xx]
                    for yy in range(max(0, y - 2), min(H, y + 3))
                    for xx in range(max(0, x - 2), min(W, x + 3))
                )
                text[y][x] = near
    return text, (x0, y0)


def lines(text, min_row=3, gap=2):
    H, W = len(text), len(text[0])
    rows = [sum(r) for r in text]
    runs, start, blank = [], None, 0
    for y in range(H + 1):
        on = y < H and rows[y] >= min_row
        if on:
            if start is None:
                start = y
            blank = 0
        elif start is not None:
            blank += 1
            if blank > gap or y == H:
                runs.append((start, y - blank + 1))
                start, blank = None, 0
    out = []
    for a, b in runs:
        cols = [x for x in range(W) if any(text[y][x] for y in range(a, b))]
        if cols:
            n = sum(sum(text[y]) for y in range(a, b))
            out.append((a, b, cols[0], cols[-1] + 1, n))
    return out


if __name__ == "__main__":
    for path in sys.argv[1:]:
        im = Image.open(path).convert("RGB")
        text, origin = masks(im)
        ls = lines(text)
        desc = "  ".join(f"y{a}-{b} x{c}-{d} h{b-a} w{d-c} n{n}" for a, b, c, d, n in ls)
        print(f"{path.split('/')[-1][:-4]:18s} {desc or '-'}")


def line_mask(text, line):
    a, b, c, d, _ = line
    return [row[c:d] for row in text[a:b]]


def iou(m, t, dx, dy):
    inter = union = 0
    H = max(len(m), len(t) + dy) + 1
    W = max(len(m[0]), len(t[0]) + dx) + 1
    for y in range(-1, H):
        for x in range(-2, W):
            a = 0 <= y < len(m) and 0 <= x < len(m[0]) and m[y][x]
            ty, tx = y - dy, x - dx
            b = 0 <= ty < len(t) and 0 <= tx < len(t[0]) and t[ty][tx]
            inter += a and b
            union += a or b
    return inter / union if union else 0.0


def classify(text, templates):
    ls = lines(text)
    if len(ls) < 2:
        return None, 0.0
    last = ls[-1]
    if not any(l[1] - l[0] >= 13 for l in ls[:-1]):  # falta la línea de la cantidad
        return None, 0.0
    m = line_mask(text, last)
    best = (None, 0.0)
    for name, t in templates.items():
        if abs(len(m[0]) - len(t[0])) > 4 or abs(len(m) - len(t)) > 2:
            continue
        s = max(iou(m, t, dx, dy) for dx in range(-2, 3) for dy in range(-1, 2))
        if s > best[1]:
            best = (name, s)
    return best


def dump_templates():
    src = {"normal": "Debug/frames/normal_a_48.8.png", "rare": "Debug/frames/rare_14.2.png", "ammo": "Debug/frames/ammo_6.6.png"}
    out = {}
    for name, path in src.items():
        text, _ = masks(Image.open(path).convert("RGB"))
        out[name] = line_mask(text, lines(text)[-1])
    return out
