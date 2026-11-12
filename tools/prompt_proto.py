"""Prototipo: texto blanco sobre recuadro oscuro junto a la mira (solo desarrollo)."""
import sys
sys.path.insert(0, 'tools')
import notice_proto as P
ROI = (0.50, 0.40, 0.72, 0.58)

def white_text(im):
    W, H = im.size; px = im.load()
    luma = [[(299*px[x,y][0] + 587*px[x,y][1] + 114*px[x,y][2]) // 1000 for x in range(W)] for y in range(H)]
    white = [[min(px[x,y]) >= 200 and max(px[x,y]) - min(px[x,y]) <= 40 for x in range(W)] for y in range(H)]
    out = [[False]*W for _ in range(H)]
    for y in range(H):
        for x in range(W):
            if white[y][x]:
                dark = sum(1 for yy in range(max(0,y-3), min(H,y+4)) for xx in range(max(0,x-3), min(W,x+4)) if luma[yy][xx] < 120)
                out[y][x] = dark >= 8
    return out

def crop(im):
    w, h = im.size
    return im.crop((int(ROI[0]*w), int(ROI[1]*h), int(ROI[2]*w), int(ROI[3]*h)))
