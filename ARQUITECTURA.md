# Fortnite Chest Counter — Arquitectura

> Versión del documento: 1.1 · Fecha: 2026-11-10 · Estado: diseño aprobado, sin implementar
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
| Resoluciones | 16:9 (1080p, 1440p, 4K) y 16:10 |
| Variaciones de HUD | Escala de HUD personalizada, idioma del juego distinto al español |
| Categorías | Normal, Raro (azul), Espíritu (morado/rosado), Jefe/Bóveda. Cajas de munición y demás: **ignoradas** |
| Precisión | Objetivo: **exactitud total** (sin falsos positivos); verificación multiseñal |
| Entrada | Teclado (tecla de interactuar configurable, por defecto `E`) y mando |
| Fin de partida | Al **morir** o **ganar**, y además al **detectar el lobby** (salida anticipada) |
| Sesión | **Sesión = Día**. Inicio automático (al detectar partida) + botones manuales |
| Atajos globales | Forzar nueva partida · Iniciar/Detener sesión · Mostrar/Ocultar ventana |
| Interfaz | **Solo ventana flotante** (overlay), con gráficos de historial |
| Streaming | Ocultar ventana en OBS/capturas: **configurable** |
| Persistencia | **SQLite** local, sin servidor |
| Exportación / nube | No en v1 |
| Idioma de la app | Español + Inglés |
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

La detección **no usa machine learning ni clips de entrenamiento**. Se basa en señales del HUD aprendidas por **calibración guiada** en el equipo del propio usuario.

| # | Señal | Qué demuestra | Cómo se obtiene |
|---|---|---|---|
| S1 | Prompt **"Abrir"** visible | Hay algo interactuable delante | Plantilla del prompt aprendida en calibración; OCR como respaldo y para detectar idioma |
| S2 | **Color en la mira** | Qué objeto es (cofre y tipo, o caja de munición/puerta) | Histograma de tono (HSV) en una ROI centrada en la mira, comparado con perfiles calibrados |
| S3 | **Tecla/botón mantenido** | El jugador está interactuando | Escucha pasiva de teclado y mando |
| S4 | **Indicador de progreso al 100%** | La apertura se completó | Medición del llenado del círculo/barra del prompt |
| S5 | **Confirmación posterior** | El cofre realmente se abrió | Sonido de apertura (huella espectral) y/o cambio visual en la mira (cofre abierto, botín) |

> Se exigen tres tipos de prueba independientes: **lo que se ve** (S1, S2, S4), **lo que hace el jugador** (S3) y **lo que ocurre después** (S5).

### Otras detecciones (contexto de partida)

| Evento | Señal | Frecuencia de análisis |
|---|---|---|
| Inicio de partida | HUD de partida presente (minimapa/barra de vida) tras el autobús | 1 fps |
| Eliminación | Pantalla/texto de eliminación (OCR) | 2 fps |
| Victoria | Texto "Victoria Magistral" / equivalente por idioma (OCR) | 2 fps |
| Lobby | Pantalla de lobby (botón "Jugar"/"Play" y ausencia de HUD de partida) | 1 fps |

## 6. Máquina de estados de apertura

```
[REPOSO]
   │  S1 prompt "Abrir" visible  Y  S2 color de cofre en la mira
   ▼
[COFRE_A_LA_VISTA]  ← se fija el tipo (normal / raro / espíritu / jefe)
   │  S3 tecla/botón pulsado
   ▼
[ABRIENDO]  ← se sigue el llenado del indicador (S4)
   │
   ├─ se suelta la tecla antes del 100%          → REPOSO   (no cuenta)
   ├─ el prompt desaparece antes del 100%        → REPOSO   (no cuenta)
   ├─ el color en la mira deja de ser de cofre   → REPOSO   (no cuenta)
   │
   │  S4 indicador = 100%  Y  prompt desaparece
   ▼
[VERIFICANDO]  (ventana ≈ 1 s)
   │
   ├─ sin S5                                     → REPOSO   (no cuenta; se guarda en depuración)
   │
   │  S5 sonido de apertura  Y/O  cambio visual en la mira
   ▼
[CONTAR +1 (tipo)] → cooldown → REPOSO
```

### Casos que nunca suben el contador

| Situación | Motivo |
|---|---|
| Mantener `E` sin apuntar a nada | No hay S1 |
| Mantener `E` frente a una caja de munición | S2 no es de cofre |
| Abrir una puerta (también dice "Abrir") | No hay indicador que llegue al 100% y S2 no es de cofre |
| Empezar a abrir y soltar | S4 no llega al 100% |
| Alejarse a mitad de la apertura | El prompt desaparece antes del 100% |
| Mantener `E` pegado mucho tiempo | Una aparición del prompt equivale como máximo a una apertura |
| El mismo cofre "parpadea" en pantalla | Cooldown y un cofre abierto ya no muestra el prompt |

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
  chest_type  TEXT NOT NULL,             -- 'normal','rare','spirit','boss'; sin CHECK (los tipos cambian por temporada)
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
- **Ajuste manual**: el histórico se puede corregir a mano (cofres abiertos antes de instalar la app, o correcciones), por tipo o sin clasificar. Se guarda como ajuste aparte, no como eventos falsos, y ningún tipo puede quedar por debajo de cero. No afecta a la partida ni a la sesión.
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
> **Overlay compacto (decidido el 2026-11-10, pendiente de implementar):** durante el juego solo se verá texto pequeño sobre fondo transparente, al estilo del overlay de Discord, en la **esquina que elija el usuario** y sin tapar nada. Mostrará únicamente tres datos: **número de partida**, **cofres abiertos** y **cofres de la partida actual**. El panel de arriba queda como vista de ajustes/edición.

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
- **Equipo de pruebas**: PC aparte con **Windows 10 22H2**. Solo recibe el `.exe` portable; no tiene entorno de desarrollo.
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

- Aspecto exacto del prompt "Abrir" y de su indicador de progreso.
- Si el sonido de apertura difiere entre tipos de cofre y respecto a la caja de munición.
- Colores reales de cofres de espíritu y de jefe/bóveda en la temporada actual.
- Textos/pantallas de eliminación, victoria y lobby en cada idioma soportado.
- Texto real del prompt del cofre (posiblemente "Buscar" y no "Abrir") y si coincide con el de las puertas.

### Decisiones de diseño abiertas (cerrar en la fase V)

- **Criterio medible de precisión**: sustituir "exactitud total" por una meta comprobable (p. ej. 0 falsos positivos en N partidas grabadas y un máximo de cofres perdidos).
- **Formato de grabación para tests**: `ImageFolderSource` solo reproduce frames; S3 y S5 necesitan audio y pulsaciones con marcas de tiempo.
- **Agrupación por día en los gráficos** cuando hay varias sesiones en una misma fecha.

### Decisiones ya implementadas (revisables)

- **Combinación mínima de señales**: para contar se exigen S1 + S2 + S4 completo + desaparición del prompt + al menos una confirmación S5 (sonido **o** cambio visual) dentro de 1 s. S3 se exige solo si hay escucha de entrada disponible. Una confirmación anterior a que el indicador se complete no vale.
- **"HOY"** muestra la sesión activa (una sesión que cruza la medianoche sigue siendo una); sin sesión activa muestra cero.
- **"PARTIDA"** muestra la partida en curso o, al terminar, la última de la sesión hasta que empiece otra.
- **Cofre sin partida detectada**: si se cuenta un cofre sin haber detectado el inicio de partida, se abre partida (y sesión) en ese momento.
- **Tipos de cofre** y **cierre inesperado**: ver §8.
