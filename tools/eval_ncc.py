"""Evalúa la comparación por correlación (NCC sobre máscaras difuminadas) con
los recortes reales del equipo de pruebas. Solo desarrollo."""
import sys, glob, math
sys.path.insert(0, 'tools')
import notice_proto as P
from PIL import Image

def text_of(im):
    W, H = im.size; px = im.load()
    fill = [[P.is_fill(px[x, y]) for x in range(W)] for y in range(H)]
    out = [[P.is_outline(px[x, y]) for x in range(W)] for y in range(H)]
    return [[fill[y][x] and any(out[yy][xx] for yy in range(max(0,y-2),min(H,y+3)) for xx in range(max(0,x-2),min(W,x+3))) for x in range(W)] for y in range(H)]

def last_label(im):
    t = text_of(im); ls = P.lines(t)
    return P.line_mask(t, ls[-1]) if len(ls) >= 2 else None

PAD = 4
def blur(m):
    H, W = len(m) + 2*PAD, len(m[0]) + 2*PAD
    g = [[0.0]*W for _ in range(H)]
    for y in range(len(m)):
        for x in range(len(m[0])):
            if m[y][x]:
                for yy in (-1,0,1):
                    for xx in (-1,0,1):
                        g[y+PAD+yy][x+PAD+xx] += 1/9
    return g

def ncc(a, b, dx, dy):
    H = max(len(a), len(b)); W = max(len(a[0]), len(b[0]))
    sa = sb = saa = sbb = sab = 0.0; n = H*W
    for y in range(H):
        for x in range(W):
            va = a[y][x] if y < len(a) and x < len(a[0]) else 0.0
            yb, xb = y - dy, x - dx
            vb = b[yb][xb] if 0 <= yb < len(b) and 0 <= xb < len(b[0]) else 0.0
            sa += va; sb += vb; saa += va*va; sbb += vb*vb; sab += va*vb
    den = math.sqrt(max(saa - sa*sa/n, 1e-9) * max(sbb - sb*sb/n, 1e-9))
    return (sab - sa*sb/n) / den

def score(m, t):
    if abs(len(m[0]) - len(t[0])) > 4 or abs(len(m) - len(t)) > 2: return 0.0
    a, b = blur(m), blur(t)
    return max(ncc(a, b, dx, dy) for dx in range(-3, 4) for dy in range(-1, 2))

if __name__ == '__main__':
    D = 'Imagenes/FortniteChestCounter/'
    obs = P.dump_templates()
    t, _ = P.masks(Image.open('Debug/frames/spirit_11.2.png').convert('RGB')); obs['spirit'] = P.line_mask(t, P.lines(t)[-1])
    real = {}
    for f in sorted(glob.glob(D + 'avisos/*.png')):
        m = last_label(Image.open(f).convert('RGB'))
        if m: real[f.split('/')[-1][:-4]] = m
    for f in sorted(glob.glob(D + 'rafaga-*/*.jpg')):
        m = last_label(Image.open(f).convert('RGB').crop((843, 207, 1088, 314)))
        if m: real['raf' + f[-8:-4]] = m
    kind_of = lambda m: {38: 'normal', 37: 'normal', 111: 'ammo', 118: 'spirit'}.get(len(m[0]))
    texts = {k: m for k, m in real.items()}
    def negatives(width, exclude_kind):
        out = []
        for k, m in texts.items():
            if kind_of(m) == exclude_kind: continue
            for x0 in range(0, max(1, len(m[0]) - width + 1), 2):
                win = [row[x0:x0+width] for row in m]
                if len(win[0]) < width - 4 or sum(map(sum, win)) < width * 1.5: continue
                if exclude_kind == 'normal' and kind_of(m) == 'spirit' and x0 < 6: continue  # es la palabra COFRE
                out.append(win)
        for k, m in obs.items():
            if k == exclude_kind: continue
            for x0 in range(0, max(1, len(m[0]) - width + 1), 2):
                win = [row[x0:x0+width] for row in m]
                if len(win[0]) < width - 4 or sum(map(sum, win)) < width * 1.5: continue
                if exclude_kind == 'normal' and k in ('spirit', 'rare') and x0 < 6: continue
                if exclude_kind == 'rare' and k == 'spirit': continue
                out.append(win)
        return out
    sets = {
        'normal': {'solo OBS': [obs['normal']], 'OBS+reales': [obs['normal'], real['1794520067173-normal'], real['1794520067573-desconocido']]},
        'ammo': {'solo OBS': [obs['ammo']], 'OBS+reales': [obs['ammo'], real['1794519915962-ammo'], real['1794519915761-desconocido']]},
        'spirit': {'solo OBS': [obs['spirit']], 'OBS+reales': [obs['spirit'], real['1794520028132-spirit']]},
        'rare': {'solo OBS': [obs['rare']]},
    }
    for kind, variants in sets.items():
        pos = [m for m in real.values() if kind_of(m) == kind]
        neg = negatives(len(obs[kind][0]), kind)
        for name, temps in variants.items():
            ps = sorted(max(score(m, t) for t in temps) for m in pos)
            ns = sorted(max(score(w, t) for t in temps) for w in neg)
            p = f"positivos n={len(ps)} min={ps[0]:.2f} mediana={ps[len(ps)//2]:.2f}" if ps else "positivos: sin muestras reales"
            print(f"{kind:7s} {name:10s}: {p} | negativos n={len(ns)} max={ns[-1]:.2f} p95={ns[int(len(ns)*0.95)]:.2f}")
