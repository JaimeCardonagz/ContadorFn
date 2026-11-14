#!/bin/sh
# Extrae de los vídeos de Debug/ la zona del aviso de experiencia a 1366x768
# y 10 fps, para la prueba `watch::tests::videos_completos`.
# Uso: sh tools/extraer_avisos.sh   (desde la raíz del proyecto; necesita ffmpeg)
set -e
extraer() {
    out="Debug/avisos-10fps/$2"
    rm -rf "$out" && mkdir -p "$out"
    # La zona es notice::roi_rect(1366, 768) = x 819, y 180, 273x165.
    ffmpeg -v error -i "$1" -vf "fps=10,scale=1366:768:flags=bicubic,crop=273:165:819:180" "$out/%05d.png"
    echo "$2: $(ls "$out" | wc -l) recortes"
    # Zona del HUD, 1 fps: scene::zone_rect(768) = x 22, y 698, 80x26.
    bar="Debug/hud-1fps/$2"
    rm -rf "$bar" && mkdir -p "$bar"
    ffmpeg -v error -i "$1" -vf "fps=1,scale=1366:768:flags=bicubic,crop=80:26:22:698" "$bar/%05d.png"
}
extraer "Debug/Cofres Prueba.mp4" cofres-prueba
extraer "Debug/Cofre Raro + Caja de Municion.mp4" raro-municion
extraer "Debug/Cofre Espiritu.mp4" espiritu
