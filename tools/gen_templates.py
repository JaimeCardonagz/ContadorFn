"""Genera src-tauri/src/notice_templates.rs a partir de imágenes de ejemplo.

Cada plantilla es la máscara de texto de la última línea del aviso. Se usan
varias por etiqueta: el aviso se desliza al aparecer y el texto no se dibuja
igual en movimiento que parado, ni igual en un vídeo reescalado que en la
captura real.

Uso: python3 tools/gen_templates.py   (desde la raíz; necesita Debug/ e Imagenes/)
"""
import sys
sys.path.insert(0, 'tools')
import notice_proto as P
from eval_ncc import text_of
from PIL import Image

REAL = 'Imagenes/FortniteChestCounter prueba 1/avisos/'
SOURCES = [
    # (tipo, descripción, imagen, ¿pantalla completa?)
    ('Normal', 'COFRE, vídeo de OBS', 'Debug/frames/normal_a_48.8.png', True),
    ('Normal', 'COFRE, captura real deslizándose', REAL + '1794520067173-normal.png', False),
    ('Normal', 'COFRE, captura real ya parado', REAL + '1794520067573-desconocido.png', False),
    ('Rare', 'COFRE RARO, vídeo de OBS (falta una captura real)', 'Debug/frames/rare_14.2.png', True),
    ('Spirit', 'COFRE DE ESPÍRITUS, vídeo de OBS', 'Debug/frames/spirit_11.2.png', True),
    ('Spirit', 'COFRE DE ESPÍRITUS, captura real', REAL + '1794520028132-spirit.png', False),
]

out = ['//! Plantillas de las etiquetas del aviso de experiencia (español de',
       '//! Latinoamérica, 768 px de alto). Generado por tools/gen_templates.py:',
       '//! no editar a mano.', '', 'use crate::store::ChestType;', '',
       'pub const TEMPLATES: &[(ChestType, &[&str])] = &[']
for kind, desc, path, full in SOURCES:
    im = Image.open(path).convert('RGB')
    text = P.masks(im)[0] if full else text_of(im)
    mask = P.line_mask(text, P.lines(text)[-1])
    out.append(f'    // {desc}')
    out.append(f'    (ChestType::{kind}, &[')
    out += [f'        "{"".join("#" if v else "." for v in row)}",' for row in mask]
    out.append('    ]),')
    print(f'{kind:7s} {len(mask[0])}x{len(mask)}  {desc}')
out.append('];')
path = 'src-tauri/src/notice_templates.rs'
previous = open(path).read()
fuse = previous[previous.index('/// Texto "ENCENDER MECHA"'):]  # plantilla del cañón: se conserva
open(path, 'w').write('\n'.join(out) + '\n\n' + fuse)
