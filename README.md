# Fortnite Chest Counter

Cuenta automáticamente los cofres que abres en Fortnite y te los muestra en una esquina de la pantalla mientras juegas.

Funciona tanto con **GeForce NOW** como con el **launcher directo de Fortnite**.

*English: a small Windows app that counts the chests you open in Fortnite and shows the count in a corner of the screen. It works with GeForce NOW and with the Fortnite launcher. The interface is available in English; the game must be set to Spanish (Latin America).*

## Qué hace

- Cuenta los **cofres abiertos**: de tesoro, raros y de espíritus. Los cofres destruidos no se cuentan.
- Lleva la cuenta de la **partida**, del **día** y del **total**, con el promedio de cofres por partida.
- Detecta sola cuándo empieza y termina cada partida.
- Muestra un **contador pequeño, solo texto y sin fondo**, en la esquina que elijas o donde lo dejes arrastrándolo. Tú decides qué enseña, con o sin colores, su tamaño y su opacidad.
- Al terminar cada partida, el contador enseña unos segundos un **resumen**: cofres y duración.
- El contador solo aparece con el juego delante, se adapta a la ventana y la resolución del juego, y se puede mostrar u ocultar con un **atajo de teclado**.
- **Historial** de las partidas de las últimas 24 horas y **récords**: más cofres en una partida, partida más larga y promedio del día.
- **Pausa** el conteo cuando quieras, desde el panel, el icono de la bandeja o con un atajo de teclado.
- **Deshaz** el último cofre si se contó uno de más.
- Puede **iniciarse con Windows**; al cerrar el panel, el programa sigue en la bandeja, junto al reloj. Al pasar el ratón por su icono ves los cofres de la partida y del día.
- Guarda una **copia de seguridad** de tus estadísticas cada día y puedes **restaurarla** desde Configuración.
- Ponte un **objetivo de cofres del día** y ve cuánto te falta, también sobre el juego.
- Tema **claro u oscuro**, y un panel que se adapta a su tamaño y se maneja con el teclado.
- Si no consigue ver el juego, te lo **avisa en el panel** con una pista de qué hacer.
- Te avisa cuando hay una versión nueva y se actualiza solo al pulsar **Actualizar**.

## Requisitos

- Windows 10 o Windows 11.
- Fortnite en **español de Latinoamérica**.
- El juego en **pantalla completa en ventana** o en ventana.

## Instalación

1. Descarga `FortniteChestCounter.exe` desde [Releases](https://github.com/JaimeCardonagz/ContadorFn/releases).
2. Ábrelo. No necesita instalación.
3. Windows mostrará "Windows protegió su PC" porque el ejecutable no está firmado: pulsa **Más información** → **Ejecutar de todas formas**.

## Primer uso

La primera vez, el programa pregunta cuántos cofres de cada tipo y cuántas partidas llevas. Esos números son tu punto de partida; desde ahí suma solo. Si empiezas de cero, déjalo todo en 0. Antes de empezar te enseña un resumen para confirmarlo.

Después abre el juego: el contador aparecerá en la esquina y el panel se irá a la bandeja. Un clic en el icono de la bandeja lo vuelve a abrir; el clic derecho ofrece **Pausar conteo** y **Salir**.

## El panel

| Pestaña | Para qué sirve |
|---|---|
| Contador | Cofres de la partida, del día (con el objetivo del día) y del total; pausar el conteo y deshacer el último cofre |
| Estadísticas | Total de cada tipo, partidas jugadas, promedio por partida, últimos 7 días y récords |
| Historial | Partidas de las últimas 24 horas: hora, duración y cofres |
| Configuración | Idioma, objetivo del día, tema, inicio con Windows, contador sobre el juego, atajos de teclado, botón con el que abres los cofres, copias de seguridad, opciones avanzadas y reinicio de estadísticas |

## Privacidad

Todo se queda en tu equipo. El programa no modifica el juego y no tiene cuentas ni telemetría. Su única conexión a internet es para comprobar si hay una versión nueva, y se puede desactivar en Configuración → Avanzado.

## Desarrollo

Rust + Tauri 2 + Svelte 5.

```bash
npm install
npm run tauri dev
```

## Licencia

[MIT](LICENSE)
