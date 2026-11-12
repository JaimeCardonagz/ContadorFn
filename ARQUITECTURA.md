# Fortnite Chest Counter — Arquitectura

> Versión del documento: 1.2 · Fecha: 2026-11-11 · Estado: diseño aprobado, sin implementar
>
> **Prioridad (v1.1): primero Windows.** Linux se aborda en una fase posterior, cuando la versión de Windows esté estable. Todo lo marcado como Linux en este documento es referencia para esa fase y **no forma parte de la v1**.

## 1. Visión

Aplicación de escritorio **ligera, portable y de código abierto** que cuenta automáticamente los cofres abiertos en Fortnite, clasificándolos por tipo y consolidando estadísticas **por partida**, **por sesión (= día)** y **acumulado histórico**.

**Principio de seguridad:** la app **no lee ni modifica** la memoria ni los archivos del juego, **no inyecta** nada en su proceso y **no envía entradas** al juego. Solo analiza la pantalla y el audio del sistema (como OBS o Discord) y *escucha* pasivamente el teclado/mando. No existe garantía oficial de Epic, por lo que en la descripción pública se dirá: *"No lee ni modifica el juego; solo analiza la pantalla."*

## 2. Requisitos cerrados

| Área | Decisión |
|---|---|
| Plataformas del SO | **v1: Windows 10 (1903+) y Windows 11.** Fase posterior: Linux (X11 y Wayland) |
| Fuentes de juego | v1: Fortnite nativo, GeForce Now y Xbox Cloud Gaming (navegador), todo en Windows. Fase posterior: GeForce Now en Linux |
| Modos de juego | Battle Royale y Cero Construcción |
| Resoluciones | 16:9 (**1366×768**, 1080p, 1440p, 4K) y 16:10. 1366×768 es la del equipo de pruebas y la primera que debe funcionar |
| Variaciones de HUD | Escala de HUD personalizada, idioma del juego distinto al español |
| Categorías | **Cofre de tesoro**, **cofre raro**, **cofre de espíritus** y **caja de munición**. La munición se cuenta aparte y no suma al total de cofres. Cofres de jefe y cualquier otro objeto: **ignorados** |
| Precisión | Objetivo: **exactitud total** (sin falsos positivos); verificación multiseñal |
| Entrada | En Configuración el usuario indica con qué abre los cofres: una **tecla**, un **botón del ratón** o un **botón del mando** (por defecto la tecla `E`). Abrir es **un toque**, no mantener |
| Fin de partida | Al **morir** o **ganar**, y además al **detectar el lobby** (salida anticipada) |
| Sesión | **Sesión = Día**. Inicio automático (al detectar partida) + botones manuales |
| Atajos globales | Forzar nueva partida · Iniciar/Detener sesión · Mostrar/Ocultar ventana |
| Interfaz | **Solo ventana flotante** (overlay), con gráficos de historial |
| Streaming | Ocultar ventana en OBS/capturas: **configurable** |
| Persistencia | **SQLite** local, sin servidor |
| Exportación / nube | No en v1 |
| Idioma de la app | Español + Inglés (implementado; se elige en Configuración y por defecto sigue al sistema) |
| Idioma del juego | Español de Latinoamérica (implementado). Inglés: pendiente de capturas del juego en inglés para crear sus plantillas |
| Distribución | **Portable**, GitHub Releases, licencia **MIT**, aviso de nueva versión, plantillas/calibraciones actualizables |
| Audiencia | Amigos / comunidad pequeña (sin firma de código en v1) |

## 3. Stack tecnológico

**Rust + Tauri 2 + Svelte**

| Motivo | Detalle |
|---|---|
| Ejecutable ligero | ~8–15 MB portable (vs. 100 MB+ con Python/OpenCV) |
| Multiplataforma | Un mismo código para Windows y Linux |
| UI amigable | HTML/CSS/Svelte: transparencia, animaciones y gráficos ligeros |
| Bajo consumo | Motor nativo en Rust, sin runtime pesado ni OpenCV |

### Dependencias previstas (orientativas)

En la v1 solo se implementa la columna **Windows**. La columna Linux se conserva para la fase posterior; los *traits* `FrameSource`, `AudioSource` e `InputSource` (§4) son los que permiten añadirla después sin tocar el detector.

| Función | Windows | Linux |
|---|---|---|
| Captura de pantalla | DXGI Desktop Duplication (Win10, sin borde amarillo) / Windows Graphics Capture (Win11) | xdg-desktop-portal ScreenCast + PipeWire |
| Captura de audio | WASAPI loopback | Fuente *monitor* de PipeWire |
| OCR | `Windows.Media.Ocr` (integrado en el SO, 0 MB extra) | `ocrs` (Rust puro, modelo ~10 MB) |
| Procesado de imagen | `image` / `imageproc` (sin OpenCV) | ídem |
| Audio (huella espectral) | `rustfft` | ídem |
| Mando | `gilrs` (XInput) | `gilrs` (evdev) |
| Teclado (escucha pasiva) | Raw Input | evdev (requiere permisos) o portal; ver §11 |
| Atajos globales | `tauri-plugin-global-shortcut` | ídem / portal GlobalShortcuts en Wayland |
| Base de datos | `rusqlite` (SQLite embebido, modo WAL) | ídem |
| UI web | WebView2 (preinstalado en Win11 y Win10 actualizado) | WebKitGTK |
| Gráficos | `uPlot` o `Chart.js` (ligero) | ídem |

Herramientas de desarrollo (no se distribuyen): scripts en Python para analizar capturas de depuración.

## 4. Arquitectura general

```
┌──────────────────────────── Proceso Rust (backend Tauri) ────────────────────────────┐
│                                                                                     │
│  ┌──────────────┐   frames ROI   ┌──────────────────┐                               │
│  │ FrameSource  │───────────────▶│                  │                               │
│  │  (trait)     │                │                  │   eventos    ┌─────────────┐  │
│  ├ DxgiCapture  │                │                  │─────────────▶│ EventStore  │  │
│  ├ WgcCapture   │  muestras PCM  │    DETECTOR      │              │  (SQLite)   │  │
│  ├ PipeWire     │───────────────▶│ (máquina de      │              └──────┬──────┘  │
│  └ ImageFolder  │                │  estados §6)     │                     │         │
│    (tests)      │  pulsaciones   │                  │              ┌──────▼──────┐  │
│  ┌──────────────┐───────────────▶│                  │─────────────▶│  Stats      │  │
│  │ InputSource  │                └────────▲─────────┘   estado     │ (agregados  │  │
│  │ teclado/mando│                         │                        │  en memoria)│  │
│  └──────────────┘                ┌────────┴─────────┐              └──────┬──────┘  │
│                                  │  Calibration     │                     │         │
│  ┌──────────────┐                │  Profile (JSON)  │                     │         │
│  │ AudioSource  │                └──────────────────┘                     │         │
│  └──────────────┘                                                          │         │
└────────────────────────────────────────────────────────────────────────────┼─────────┘
                                                              eventos Tauri │
                                                                     ┌──────▼──────┐
                                                                     │  UI Svelte  │
                                                                     │  (overlay)  │
                                                                     └─────────────┘
```

**Principio clave:** el `Detector` no sabe de dónde vienen los frames. Las fuentes son intercambiables (`FrameSource`, `AudioSource`, `InputSource`), lo que permite probar el detector con carpetas de capturas guardadas en modo depuración, sin necesidad del juego.

## 5. Señales de detección

La detección **no usa machine learning ni clips de entrenamiento**. Se basa en elementos del HUD.

> **Verificado en el juego (vídeo del 2026-11-11, 1080p, español de Latinoamérica).** Así se abre un cofre:
>
> 1. Al apuntar a un contenedor aparece junto a la mira, a su derecha, un recuadro oscuro con el icono de la tecla y el texto **"BUSCAR"**.
> 2. **Un solo toque** de la tecla arranca un **anillo de progreso alrededor del icono de la tecla**. Se completa solo en unos **0,7 s** y **no se puede cancelar**.
> 3. El recuadro sigue visible ~0,25 s más y desaparece: el cofre queda abierto.
> 4. Unos 0,35 s después aparecen dos avisos en posiciones fijas del HUD:
>    - **Aviso de experiencia** (a la derecha, sobre el centro): una cantidad y, debajo, **qué se abrió**. Visto: **"+600 PE / COFRE"** (cofre de tesoro), **"+900 PE / COFRE RARO"**, **"+900 PE / COFRE DE ESPÍRITUS"** y **"+300 PE / CAJA DE MUNICIÓN"**. Puede llevar líneas intermedias ("NIVEL DE ESPÍRITU") y otra cantidad (+750), pero la última línea es siempre el tipo. El usuario confirma que sale con todos los cofres.
>    - **"+75 CONTENEDOR ABIERTO"** (banda a la derecha). Salió con cualquier contenedor, y otro aviso puede ocupar su sitio ("¡SUBISTE DE NIVEL!").

| # | Señal | Qué demuestra | Cómo se obtiene |
|---|---|---|---|
| S1 | Recuadro **"BUSCAR"** junto a la mira | Hay un contenedor delante | Plantilla del recuadro en una zona fija a la derecha de la mira |
| S2 | **Tipo de contenedor** | Si es cofre de tesoro, raro, de espíritus o caja de munición | **Texto de la última línea del aviso de experiencia** ("COFRE", "COFRE RARO", "COFRE DE ESPÍRITUS", "CAJA DE MUNICIÓN"). No hace falta mirar el color del cofre |
| S3 | **Toque de la tecla** | El jugador interactuó | Escucha pasiva de teclado y mando; basta una pulsación |
| S4 | **Anillo de progreso** | La apertura arrancó y terminó | Arco alrededor del icono de la tecla, dentro del recuadro |
| S5 | **Aviso de apertura** | El contenedor realmente se abrió | El mismo aviso de experiencia que da el tipo, en su zona fija. "CONTENEDOR ABIERTO" no se usa: no distingue tipos y otros avisos lo tapan |

> Se exigen tres tipos de prueba independientes: **lo que se ve antes** (S1, S4), **lo que hace el jugador** (S3) y **lo que ocurre después** (S2 + S5, que llegan juntas en el aviso).

> **Sonido descartado** (decisión del usuario, 2026-11-11): el sonido de apertura no se usará. El aviso visual confirma la apertura y da el tipo, así que no hace falta.

### El botón de abrir (S3) en la versión actual

- La app consulta unas 30 veces por segundo si el botón configurado está pulsado (teclado y ratón con `GetAsyncKeyState`, mando con XInput) y recuerda la última pulsación.
- Cada detección anota si hubo una pulsación en los **4 s anteriores** al aviso. Eso sube su confianza y queda en el registro.
- La opción **"Contar solo si pulsé ese botón justo antes"** lo vuelve obligatorio. Viene desactivada porque falta comprobar que las pulsaciones se ven con GeForce Now en primer plano; si no se vieran, activarla dejaría el contador a cero.
- El botón por sí solo **no distingue** dos aperturas seguidas del mismo tipo: tras abrir un cofre se pulsa el mismo botón para recoger el botín. Eso lo resolverán el recuadro "BUSCAR" y el anillo.

### Idioma del juego

Las plantillas del aviso son texto, así que cada idioma del juego necesita las suyas. Hoy solo existen las de español de Latinoamérica. Para el **inglés** hacen falta cuatro capturas con el juego en inglés (un aviso de cada tipo); también sirven los recortes `…-desconocido.png` que la app guarda sola al jugar en inglés.

### Otras detecciones (contexto de partida)

| Evento | Señal vista en el vídeo | Frecuencia de análisis |
|---|---|---|
| Lobby | Panel "BATTLE ROYALE" con "Buscando servidor" / botón "CANCELAR" | 1 fps |
| Inicio de partida | HUD de partida presente (minimapa arriba a la derecha, barra de vida abajo a la izquierda) | 1 fps |
| Eliminación | "QUEDASTE N.º" arriba y "TE ELIMINÓ …" abajo | 2 fps |
| Fin de partida | Pantalla con "JUGAR DE NUEVO" / "REGRESAR A LA SALA" | 1 fps |
| Victoria | Sin verificar todavía | 2 fps |

## 6. Máquina de estados de apertura

```
[REPOSO]
   │  S1 recuadro "BUSCAR" visible
   ▼
[A_LA_VISTA]
   │  S3 toque de tecla (si hay escucha)  Y  S4 el anillo arranca
   ▼
[ABRIENDO]  ← el anillo avanza solo; se anota si ya había un aviso en pantalla (es del contenedor anterior)
   │
   ├─ el anillo no termina ni desaparece en 3 s   → no cuenta (se guarda en depuración)
   │
   │  S4 anillo completo   O   el recuadro deja de verse (el jugador giró la cámara;
   │                           la apertura no se cancela)
   ▼
[VERIFICANDO]  (ventana de 1,5 s)
   │
   ├─ sin aviso reconocido                        → no cuenta (se guarda en depuración)
   │
   │  el recuadro ha desaparecido  Y  S5 aviso con un tipo reconocido (S2)
   │  (si el aviso es el mismo que ya había, se espera 0,8 s por si cambia)
   ▼
[CONTAR +1 (tipo del aviso)] → pausa de 0,5 s → REPOSO
```

### Resultado de la primera prueba real (2026-11-12)

Probado en el equipo de pruebas con GeForce Now (pantalla **1360×768**, no 1366):

- **La captura funciona**: GDI ve la imagen de GeForce Now (29 ms la pantalla completa), así que no hace falta DXGI.
- **El botón de abrir se ve**: las 10 detecciones tenían una pulsación de `E` justo antes. La opción de exigirla ya se puede activar en ese equipo.
- **Contó 10 de 11**: 8 cofres de tesoro, 1 caja de munición y 1 cofre de espíritus, todos correctos y ninguno de más. **Se le escapó un cofre de tesoro.**
- **Causa del fallo**: las plantillas venían de un vídeo de OBS reescalado y el texto real solo coincidía píxel a píxel un 57–70 %, justo en el umbral. Además el aviso entra deslizándose ~0,5 s antes de quedarse quieto ~1,4 s, y en movimiento se dibuja distinto.
- **Arreglo**: la comparación pasó de coincidencia píxel a píxel a **correlación entre máscaras difuminadas**, con varias plantillas por etiqueta (vídeo y capturas reales). Con las capturas reales, el texto correcto da ≥ 0,92 y otro texto de la misma anchura ≤ 0,81; el umbral está en 0,86. El cofre que se perdió ahora se lee, y es uno de los casos de prueba.
- La zona de lectura se amplió hacia arriba y a la izquierda: en un aviso de cuatro líneas la cantidad quedaba cortada.
- Otros avisos vistos y correctamente ignorados: "ESCUDOS LLENOS", "ESCUDO ROTO X2", "ELIMINACIÓN", "NUEVA FASE DE LA TORMENTA".

Pendiente tras esta prueba: una captura real de **"COFRE RARO"** (su plantilla sigue siendo solo la del vídeo).

### Interfaz en dos ventanas (implementado, 2026-11-12)

- **Contador sobre el juego**: ventana sin bordes, transparente, siempre encima y atravesable por los clics, en la esquina que elija el usuario. Solo texto: número de partida (histórico), cofres abiertos (histórico) y cofres de esta partida.
- **Panel**: ventana normal (ya no queda encima del juego) con pestañas: Contador, Estadísticas (total de cada tipo, partidas jugadas y promedio de cada tipo por partida), Configuración, Histórico (ajuste manual) y Diagnóstico. Cerrar el panel cierra la app.
- En Configuración se elige si el contador **se muestra sobre el juego** o la app queda **en segundo plano** (sigue contando sin dibujar nada), y en qué esquina.
- La esquina inferior izquierda no se ofrece para el contador: ahí está la barra de vida.
- El contador va **pegado al borde** de la esquina elegida y solo aparece **con el juego delante** (ventana activa con "Fortnite" o "GeForce NOW" en el título, o HUD de partida a la vista); nunca al abrir el programa. Cuando aparece, el panel se retira solo a la **bandeja del sistema** (icono pequeño junto al reloj); un clic en ese icono lo vuelve a abrir.
- Configuración permite **no contar las cajas de munición**.
- La pestaña Histórico se eliminó: el punto de partida se fija solo en la pantalla inicial.
- **Aspecto**: paleta "Gaming" de la skill `ui-ux-pro-max` (violeta sobre fondo muy oscuro) como variables CSS, tarjetas por bloque y un color por tipo de contenedor (dorado tesoro, azul raro, rosa espíritus, verde munición). Fuentes del sistema: no se descargan fuentes de internet. Para ver la interfaz sin Tauri, `npm run dev` la abre en el navegador con datos de ejemplo (`src/lib/tauri.ts`).
- **Primera vez**: antes de mostrar nada, el panel pregunta cuántos cofres de tesoro, raros y de espíritus lleva abiertos el jugador (la munición no se pregunta) y cuántas partidas lleva. Eso queda como punto de partida del histórico y **borra todo lo anterior**: lo contado por el programa (cofres, munición, partidas, sesiones) y los ajustes manuales; el promedio por partida es (lo indicado + lo contado) / (partidas indicadas + partidas contadas) y se actualiza solo.

### Segunda prueba real y cañón dorado (2026-11-12)

- Segunda prueba en el equipo real: 27 contenedores contados (20 de tesoro, 4 raros, 2 de munición, 1 de espíritus); el usuario confirma que los cuatro tipos contaron bien. "COFRE RARO" coincidió 0,94 con su plantilla. El inicio y el fin de partida se detectaron solos.
- **Cañón dorado**: suelta botín como un cofre pero no muestra aviso de experiencia. Su recuadro dice "ENCENDER MECHA". Se cuenta **como cofre de tesoro** (decisión del usuario) cuando, con ese recuadro a la vista, avanza el anillo de progreso y después el recuadro desaparece (`fuse.rs`). Probado con la ráfaga real del cañón (cuenta uno) y con la del cofre (no cuenta ninguno).

### Inicio y fin de partida automáticos (implementado, 2026-11-12)

La señal es la **barra de vida** (abajo a la izquierda), que se mira una vez por segundo:

- **Empieza** la partida cuando la barra se ve tres veces seguidas. Ocurre ya en la isla inicial, antes del autobús, no al abrir el primer cofre.
- **Termina** cuando la barra lleva **25 s** sin verse (eliminación, victoria o vuelta al lobby). La hora de fin que se guarda es el momento en que desapareció. Abrir el mapa o el inventario la oculta solo unos segundos y no cuenta.
- Durante los **3 primeros minutos** el margen es de **60 s**, porque la barra se oculta mientras se va en el autobús.
- En la pantalla de eliminación hay otra barra verde (la del rival), pero en otra posición.

Comprobado con el vídeo que va del lobby a la eliminación: detecta el inicio en la isla, no se confunde con el autobús y cierra la partida en el segundo en que eliminan al jugador.

Limitaciones: con menos del ~15 % de vida la parte de la barra que se mira queda vacía; si eso dura más de 25 s seguidos, la partida se cerraría antes de tiempo. Estar derribado en dúos o escuadrones no está probado.

**La ventana de la app ya no estorba.** En la primera prueba real la ventana tapaba justo la barra de vida. Ahora, por defecto, la ventana se **excluye de las capturas** (`WDA_EXCLUDEFROMCAPTURE`, Windows 10 2004+): la app ve el juego aunque su ventana esté encima, en cualquier parte de la pantalla. A cambio no aparece en OBS ni en capturas; se puede desactivar en Configuración, y entonces no debe tapar ni la barra de vida ni la zona del aviso.

### Primera versión funcional (implementada, 2026-11-11)

La app ya cuenta sola, de momento **usando solo el aviso de experiencia** (S2 + S5):

- Unas diez veces por segundo captura únicamente la zona del aviso (20 % × 17,5 % de la pantalla) y lee su última línea con plantillas ("COFRE", "COFRE RARO", "COFRE DE ESPÍRITUS", "CAJA DE MUNICIÓN").
- Cuenta un contenedor cada vez que **aparece** un aviso con un tipo conocido, no una vez por cada frame en que se ve.
- Sobre los tres vídeos completos del usuario (178 s, 1778 lecturas) cuenta exactamente lo que se abrió —4 cofres de tesoro, 1 caja de munición, 1 cofre raro, 1 cofre de espíritus— sin ninguna detección de más.
- Guarda en `Imágenes\FortniteChestCounter\avisos\` el recorte de cada detección y de los avisos que no reconoce, para comprobar aciertos y crear plantillas de tipos nuevos.

**Limitaciones conocidas de esta versión:**

- Dos contenedores **del mismo tipo** abiertos antes de que desaparezca el primer aviso (~2 s) cuentan como uno. Se resuelve al añadir el recuadro "BUSCAR" y el anillo, que distinguen cada apertura (la máquina de estados de arriba ya está escrita y probada, falta leer esas dos señales de la pantalla).
- El juego debe ocupar **todo el monitor principal**; si no, la zona del aviso no cae donde se espera.

### Casos que nunca suben el contador

| Situación | Motivo |
|---|---|
| Pulsar la tecla sin apuntar a nada | No hay S1 |
| Abrir una puerta o recoger un arma | No muestran "BUSCAR" ni producen aviso de contenedor |
| Abrir otro contenedor (nevera, barril) | El aviso no nombra un tipo de los que se cuentan |
| Pulsar frente a un cofre sin que arranque el anillo | No hay S4 |
| Anillo en pantalla sin haber visto el toque de tecla (habiendo escucha) | Falta S3 |
| Mirar un cofre con el aviso del anterior aún en pantalla | El aviso solo vale después de una apertura |
| Aviso que llega fuera de plazo | Ventana de verificación agotada |
| El mismo cofre "parpadea" en pantalla | Pausa tras contar y un cofre abierto ya no muestra el recuadro |

### Rendimiento

- Solo se analizan **ROIs pequeñas** (zona del prompt, zona de la mira, zonas de texto de fin de partida), nunca el frame completo.
- **Frecuencia adaptativa**: ~10 fps en REPOSO → ~30 fps en COFRE_A_LA_VISTA/ABRIENDO.
- Objetivo: **< 2% de CPU** y **< 80 MB de RAM** mientras se juega.

## 7. Calibración guiada

Se ejecuta en el **primer uso** y puede repetirse desde Ajustes. Sustituye a los clips de entrenamiento.

1. **Pantalla:** detecta monitor, resolución y relación de aspecto; el usuario confirma en qué pantalla está el juego.
2. **Prompt:** *"Apunta a cualquier cofre (sin abrirlo) y pulsa F8"* → aprende la posición, la escala (HUD personalizado) y la plantilla del prompt. El OCR identifica el idioma.
3. **Colores:** *"Apunta a un cofre normal y pulsa F8"*, luego a una **caja de munición**, y opcionalmente a un **raro**, **espíritu** y **jefe/bóveda** cuando se encuentren (calibración incremental).
4. **Indicador:** *"Abre el cofre"* → aprende la forma y el color del círculo/barra de progreso.
5. **Audio:** durante el paso anterior graba el sonido de apertura como huella de referencia.
6. **Entrada:** confirma la tecla de interactuar o el botón del mando.

El resultado se guarda como **perfil de calibración JSON**. Los perfiles se pueden compartir mediante el repositorio de GitHub ("plantillas actualizables").

## 8. Modelo de datos (SQLite)

Se guardan **eventos**, no contadores. Los totales se calculan por consulta y se cachean en memoria.

```sql
CREATE TABLE sessions (
  id          INTEGER PRIMARY KEY,
  started_at  INTEGER NOT NULL,          -- epoch ms
  ended_at    INTEGER,
  auto_start  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE matches (
  id          INTEGER PRIMARY KEY,
  session_id  INTEGER REFERENCES sessions(id),
  started_at  INTEGER NOT NULL,
  ended_at    INTEGER,
  end_reason  TEXT CHECK (end_reason IN ('eliminated','victory','lobby','manual','interrupted'))
);

CREATE TABLE chest_events (
  id          INTEGER PRIMARY KEY,
  ts          INTEGER NOT NULL,
  match_id    INTEGER REFERENCES matches(id),
  session_id  INTEGER REFERENCES sessions(id),
  chest_type  TEXT NOT NULL,             -- 'normal' (tesoro),'rare','spirit','ammo'; sin CHECK (los tipos cambian por temporada)
  confidence  REAL NOT NULL,
  signals     INTEGER NOT NULL           -- máscara de bits de S1..S5 presentes
);

CREATE INDEX idx_events_session ON chest_events(session_id);
CREATE INDEX idx_events_match   ON chest_events(match_id);
CREATE INDEX idx_events_ts      ON chest_events(ts);

CREATE TABLE adjustments (                -- ajustes manuales del histórico
  id          INTEGER PRIMARY KEY,
  ts          INTEGER NOT NULL,
  chest_type  TEXT,                      -- NULL = sin clasificar (solo suma al total)
  delta       INTEGER NOT NULL           -- puede ser negativo
);

CREATE TABLE match_adjustments (          -- ajustes manuales del total de partidas
  id          INTEGER PRIMARY KEY,
  ts          INTEGER NOT NULL,
  delta       INTEGER NOT NULL           -- puede ser negativo
);

CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);  -- versión de esquema, etc.
```

- Modo **WAL** y escrituras por evento (unas pocas por minuto): impacto en disco inapreciable.
- La base (`chests.db`) se guarda en la carpeta de datos que Tauri asigna al identificador de la app (`%APPDATA%\io.github.jaimecardonagz.fortnitechestcounter\` / `~/.local/share/io.github.jaimecardonagz.fortnitechestcounter/`), no junto al ejecutable.
- **Ajuste manual**: el histórico se puede corregir a mano (cofres abiertos antes de instalar la app, o correcciones), por tipo. Se guarda como ajuste aparte, no como eventos falsos, y ningún tipo puede quedar por debajo de cero. No afecta a la partida ni a la sesión.
- **Contador de partidas**: se muestran las partidas de la sesión activa y las de siempre. Cuenta toda partida registrada, también las forzadas a mano y las interrumpidas. El total histórico de partidas admite el mismo ajuste manual que los cofres.
- **Cierre inesperado**: al arrancar, las partidas que quedaron abiertas se cierran como `interrupted` con la hora de su último cofre. La sesión abierta se reanuda si su última actividad está dentro del periodo de inactividad; si no, se cierra en esa última actividad.
- `confidence` es la fracción de las señales S1..S5 presentes al contar (1.0 con las cinco; 0.8 sin S3).

## 9. Interfaz (ventana flotante)

```
┌──────────────────────────────────┐
│ ● EN PARTIDA            ⚙  ─  ✕ │
├──────────────────────────────────┤
│ PARTIDA                       65 │
│  Normal 58 · Raro 3 · Esp 4 · J 0│
├──────────────────────────────────┤
│ HOY  02:15:00                360 │
│  Normal 320 · Raro 18 · Esp 22   │
├──────────────────────────────────┤
│ HISTÓRICO                  5 274 │
├──────────────────────────────────┤
│  [▶ Iniciar]  [⏹ Detener]  [📊]  │
└──────────────────────────────────┘
```

> **Estado actual:** la ventana es un panel de trabajo sin pulir (contadores, botones y ajuste manual). Lo visual no es prioridad hasta que la detección funcione.
>
> **Overlay compacto (decidido el 2026-11-10, pendiente de implementar):** durante el juego solo se verá texto pequeño sobre fondo transparente, al estilo del overlay de Discord, en la **esquina que elija el usuario** y sin tapar nada. Mostrará únicamente tres datos: **número de partida** (histórico), **cofres abiertos** (histórico) y **cofres de la partida actual**. El panel de arriba queda como vista de ajustes/edición.

- **Siempre encima**, fondo semitransparente (opacidad configurable), tamaño compacto y redimensionable.
- **Click-through**: los clics atraviesan la ventana durante el juego. Un atajo alterna el **modo edición** (mover/redimensionar/pulsar botones).
- **Panel 📊**: gráficos de cofres por día, promedio por partida y distribución por tipo.
- **Ocultar en OBS/capturas** (opcional): `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` en Windows 10 2004+. No disponible en Linux.
- **Autoexclusión del análisis**: si la ventana está sobre la zona capturada y no está oculta a la captura, el detector **enmascara su rectángulo**.
- **Requisito de visibilidad**: el juego debe estar en **"Ventana en pantalla completa"** (borderless). En pantalla completa exclusiva ninguna ventana normal puede dibujarse encima. El asistente inicial lo explica y la app avisa si detecta que quedó tapada.
- Futuro (v2): widget de **Xbox Game Bar**, visible sobre pantalla completa exclusiva.

## 10. Lógica de partidas y sesiones

- **Partida**
  - Comienza al detectar el HUD de partida.
  - Termina por **eliminación**, **victoria**, **lobby** o **atajo manual**.
  - Consecuencia aceptada: si el jugador es reiniciado (furgoneta de reinicio) tras ser eliminado, cuenta como **partida nueva**.
- **Sesión (= Día)**
  - Inicio automático al detectar la primera partida si no hay sesión activa, o manual con **Iniciar**.
  - Termina con **Detener** o tras un periodo de inactividad configurable (por defecto 60 min sin partidas).
  - Una sesión de 23:00 a 02:00 es **una sola jornada**.
- **Histórico**: suma de todos los eventos.

## 11. Particularidades por plataforma

### Windows 10 / 11
- Mínimo **Windows 10 1903**.
- **WebView2**: si falta (algunos Win10 antiguos/LTSC), la app muestra un aviso con el enlace oficial de Microsoft. No se incluye dentro del ejecutable para mantenerlo ligero.
- Win10 usa **DXGI Desktop Duplication** (sin borde amarillo); Win11 puede usar **Windows Graphics Capture**.
- Sin firma de código: SmartScreen mostrará *"Windows protegió su PC"* → *Más información* → *Ejecutar de todas formas*. Se documenta en el README.

### Linux (incluido Wayland / GNOME) — fase posterior, no v1

Comprobado el 2026-11-10 en el equipo de desarrollo (Zorin OS 18.1, GNOME 46, Wayland): el portal GlobalShortcuts no está disponible (requiere GNOME 48+) y el usuario no pertenece al grupo `input`, así que ni el atajo de calibración (F8) ni S3 por teclado funcionarían sin configuración extra. Además, el AppImage de Tauri empaqueta WebKitGTK y suele pesar 70 MB o más. Todo esto se resuelve al abordar la fase de Linux.

- **Captura**: vía portal; GNOME pide permiso la primera vez (se puede recordar).
- **Siempre encima**: Wayland no permite que una app se fuerce encima. Se usará XWayland y, si no basta, la opción "Siempre encima" del menú de ventana de GNOME.
- **Atajos globales**: portal GlobalShortcuts (GNOME 48+); en versiones anteriores, configurables como atajos del sistema.
- **Escucha de teclado**: en Wayland no se pueden leer teclas globales sin permisos (grupo `input` para evdev). Si no hay permiso, S3 se desactiva y la verificación se apoya en S1+S2+S4+S5. El mando (`gilrs`) sí funciona normalmente.
- **Ocultar en OBS**: no disponible.
- **Formato**: AppImage portable.

## 12. Distribución y actualizaciones

- **Repositorio**: GitHub, licencia **MIT**.
- **Releases**: `FortniteChestCounter.exe` (portable) en la v1, generado con GitHub Actions. `FortniteChestCounter.AppImage` se añade en la fase de Linux.
- **Aviso de nueva versión**: al iniciar consulta la API de GitHub Releases (sin telemetría) y muestra un aviso; la actualización es manual.
- **Plantillas/calibraciones**: carpeta `profiles/` en el repositorio; la app puede descargar perfiles base por resolución e idioma cuando Epic cambie el HUD.
- **Privacidad**: sin cuentas, sin telemetría; la única conexión de red es la comprobación de versión y perfiles (desactivable).

## 13. Estructura del repositorio (propuesta)

```
ContadorFn/
├─ ARQUITECTURA.md
├─ README.md
├─ LICENSE
├─ src-tauri/                 # Backend Rust
│  ├─ src/
│  │  ├─ main.rs
│  │  ├─ capture/             # FrameSource: dxgi, wgc, pipewire, image_folder
│  │  ├─ audio/               # AudioSource: wasapi, pipewire, fingerprint
│  │  ├─ input/               # InputSource: teclado, mando
│  │  ├─ detect/              # señales S1..S5, máquina de estados, contexto de partida
│  │  ├─ ocr/                 # windows_ocr, ocrs
│  │  ├─ calibration/         # asistente y perfiles JSON
│  │  ├─ store/               # SQLite, migraciones, agregados
│  │  ├─ overlay/             # click-through, exclusión de captura, autoenmascarado
│  │  └─ i18n/
│  └─ tests/                  # tests con carpetas de capturas de depuración
├─ src/                       # Frontend Svelte (overlay, ajustes, gráficos, asistente)
├─ profiles/                  # perfiles de calibración compartidos
└─ tools/                     # scripts de análisis (Python, solo desarrollo)
```

## 14. Modo depuración y pruebas

### Diagnóstico (implementado)

Antes de escribir la detección hay que saber qué puede ver la app en el equipo real. La ventana incluye un panel de diagnóstico que comprueba tres cosas y deja los datos en `Imágenes\FortniteChestCounter\`:

- **Captura de pantalla.** `F8` guarda una captura sin pérdida (PNG) de lo que recibe la app y avisa si sale negra. De momento se usa **GDI (BitBlt)** sobre el monitor principal, por ser lo más simple; si con GeForce Now saliera negra, se pasa a DXGI Desktop Duplication (§3).
- **Ráfaga.** `F9` empieza o detiene una ráfaga de hasta 10 s a ~15 fps (JPEG) con un `frames.csv` que anota en cada frame el instante y si `E` estaba pulsada. Estas ráfagas son la materia prima para medir el prompt y el indicador, y luego sirven de casos de test.
- **Teclado y atajos.** Muestra en vivo si ve la tecla `E`, y cuenta por separado las pulsaciones de `F8`/`F9` que llegan leyendo el teclado y las que llegan como atajo global del sistema, para saber cuál de las dos vías funciona con el juego en primer plano.

Los botones equivalentes actúan con 5 s de retardo, para dar tiempo a volver al juego. Todo queda anotado en `diagnostico.log`.

### Modo depuración (previsto)

- Con el modo depuración activo, la app guarda localmente, por cada evento **contado o descartado en VERIFICANDO**:
  - las ROIs de los frames clave;
  - la máscara de señales;
  - el motivo de la decisión.
- Esas carpetas se convierten en **casos de test**: el detector se ejecuta sobre ellas con `ImageFolderSource` y se compara el resultado esperado.
- Las capturas de depuración son locales y **no se suben** al repositorio salvo decisión explícita del usuario.

## 15. Fases de desarrollo

| Fase | Objetivo | Entregable |
|---|---|---|
| V | **Validación previa**: capturas y grabación del juego real para cerrar el §17, y decisiones abiertas del §17 resueltas | §17 sin pendientes; documento actualizado |
| 0 | Esqueleto Tauri + captura de pantalla en Windows + prueba mínima de overlay (siempre encima, click-through) y atajo global sobre el juego | Ventana flotante que muestra la ROI capturada en vivo |
| 1 | Calibración guiada: prompt, colores e indicador | Perfil JSON generado desde el juego real |
| 2 | Detector S1+S2+S4 + máquina de estados + modo depuración | Conteo en vivo (sin audio ni entrada) |
| 3 | S3 (teclado/mando) + S5 (audio) | Verificación completa |
| 4 | Contexto de partida (inicio, eliminación, victoria, lobby) + sesiones + SQLite | Los tres contadores persistentes |
| 5 | Overlay final: click-through, atajos, gráficos, i18n, ocultar en OBS | UI completa |
| 6 | Empaquetado, GitHub Actions, aviso de versión, perfiles descargables | Release v1.0 (solo Windows) |
| 7 | Port a Linux: captura por portal/PipeWire, audio, entrada, overlay y atajos en Wayland, AppImage | Release con soporte Linux |

### Flujo de desarrollo y pruebas (v1)

- **Programación**: toda en el equipo Linux (Zorin OS). El código específico de Windows (captura DXGI, WASAPI, Raw Input, OCR, overlay) **no se puede ejecutar aquí**.
- **Equipo de pruebas**: PC aparte con **Windows 10 22H2**. Solo recibe el `.exe` portable; no tiene entorno de desarrollo. Configuración de referencia: **GeForce Now**, pantalla **1366×768 (16:9)**, juego en **español de Latinoamérica**, **teclado** con `E` para interactuar. Es el caso más exigente previsto: resolución baja más compresión de vídeo del streaming.
- **Compilación del `.exe`**: desde Linux por compilación cruzada o con GitHub Actions (runner de Windows). En la fase 0 se valida cuál de las dos resulta más práctica.
- **Ciclo de prueba**: cada cambio en código de Windows exige compilar, copiar el `.exe` y probar en el otro equipo. Para acortarlo:
  - el detector se desarrolla y se prueba en Linux con capturas grabadas en el PC de Windows (`ImageFolderSource`, §14);
  - el `.exe` escribe un registro de depuración en un archivo que se trae de vuelta junto con las capturas;
  - las capturas de la fase V se hacen con las herramientas del propio Windows (Recortes, Xbox Game Bar), sin necesidad de la app.

## 16. Riesgos y mitigaciones

| Riesgo | Impacto | Mitigación |
|---|---|---|
| Color de cofre ambiguo (noche, tormenta, cofre parcialmente tapado) | Cofre no contado o mal clasificado | Tono HSV, calibración en el equipo del usuario, confirmación por audio, modo depuración |
| Epic cambia el HUD en una nueva temporada | Detección rota | Recalibración desde Ajustes, perfiles actualizables por GitHub, tests con capturas |
| Compresión/lag de GeForce Now o Xbox Cloud | Señales más ruidosas | Calibración específica por fuente, umbrales por perfil |
| Ruido de audio (música, voz en Discord) | S5 falla | Huella espectral robusta; S5 acepta alternativa visual |
| El código de Windows no se puede ejecutar en el equipo de desarrollo | Ciclo de prueba lento; errores que solo aparecen en el PC de pruebas | Detector probado en Linux con capturas grabadas, registro de depuración en archivo, capa de plataforma lo más delgada posible |
| Restricciones de Wayland (fase posterior) | Overlay o atajos limitados | XWayland, portales, documentación clara |
| Anticheat con Fortnite nativo en Windows | Una ventana transparente siempre encima podría parecer sospechosa | Sin inyección ni lectura de memoria; mismas técnicas que OBS/Discord; sin garantía oficial de Epic (§1) |
| Juego en pantalla completa exclusiva | Overlay invisible | Asistente inicial y aviso automático |
| Falta WebView2 en Win10 antiguo | La app no abre | Detección y enlace de descarga oficial |
| Aviso de SmartScreen | Desconfianza del usuario | Código abierto, README con instrucciones; firma de código si crece la audiencia |

## 17. Pendiente de verificar en el juego

Resuelto con el vídeo del 2026-11-11: aspecto del prompt ("BUSCAR", no "Abrir") y de su anillo de progreso, tiempos de apertura, avisos de confirmación, y pantallas de lobby y eliminación (§5).

Confirmado por el usuario (2026-11-11):

- El aviso **"+PE / COFRE" sale con todos los cofres, siempre**. Pasa a ser la confirmación principal (S5).
- Los cofres dorados con calavera del vídeo son **cofres de tesoro**.
- Tercer vídeo (2026-11-11): el **cofre de espíritus** (oscuro, con una flor rosa encima) da "+900 PE / COFRE DE ESPÍRITUS".
- Segundo vídeo (2026-11-11): el **cofre raro** (plateado, brillo azul) da "+900 PE / COFRE RARO" y la **caja de munición** da "+300 PE / CAJA DE MUNICIÓN". El recuadro dice "BUSCAR" en ambos.
- La **munición se cuenta aparte** y no suma al total de cofres.
- El vídeo es del propio equipo de pruebas (pantalla 1366×768, GeForce Now); OBS lo guardó ampliado a 1080p. La **escala del HUD está por defecto**.
- En el overlay compacto, tanto "cofres abiertos" como "número de partida" son los **históricos**.

Sigue pendiente:

- **Diagnóstico en el equipo de pruebas**: todavía sin ejecutar. Dirá si la captura funciona con GeForce Now, si se ve la tecla `E` y si llegan `F8`/`F9`, y dará las posiciones reales a 1366×768.
- Sonido de apertura (el vídeo lo incluye; sin analizar todavía).
- Pantalla de victoria.

### Decisiones de diseño abiertas (cerrar en la fase V)

- **Criterio medible de precisión**: sustituir "exactitud total" por una meta comprobable (p. ej. 0 falsos positivos en N partidas grabadas y un máximo de cofres perdidos).
- **Formato de grabación para tests**: `ImageFolderSource` solo reproduce frames; S3 y S5 necesitan audio y pulsaciones con marcas de tiempo.
- **Agrupación por día en los gráficos** cuando hay varias sesiones en una misma fecha.

### Decisiones ya implementadas (revisables)

- **Combinación mínima de señales**: para contar se exigen S1 + anillo arrancado (S4) + desaparición del recuadro + aviso con un tipo reconocido (S2 + S5) dentro de 1,5 s. S3 se exige solo si hay escucha de entrada disponible. Un aviso anterior a que termine el anillo no vale.
- **"HOY"** muestra la sesión activa (una sesión que cruza la medianoche sigue siendo una); sin sesión activa muestra cero.
- **"PARTIDA"** muestra la partida en curso o, al terminar, la última de la sesión hasta que empiece otra.
- **Cofre sin partida detectada**: si se cuenta un cofre sin haber detectado el inicio de partida, se abre partida (y sesión) en ese momento.
- **Tipos de cofre** y **cierre inesperado**: ver §8.
