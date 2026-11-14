# Fortnite Chest Counter

Cuenta automáticamente los cofres que abres en Fortnite y te los muestra en una esquina de la pantalla mientras juegas.

Funciona tanto con **GeForce NOW** como con el **launcher directo de Fortnite**.

*English: a small Windows app that counts the chests you open in Fortnite and shows the count in a corner of the screen. It works with GeForce NOW and with the Fortnite launcher. The interface is available in English; the game must be set to Spanish (Latin America).*

## Qué hace

- Cuenta los **cofres abiertos**: de tesoro, raros y de espíritus. Los cofres destruidos no se cuentan.
- Lleva la cuenta de la **partida**, del **día** y del **total**, con el promedio de cofres por partida.
- Detecta sola cuándo empieza y termina cada partida.
- Muestra un **contador pequeño, solo texto y sin fondo**, en la esquina que elijas. Tú decides qué enseña, con o sin colores, su tamaño y su opacidad.
- El contador solo aparece con el juego delante; el resto del tiempo el programa queda en la bandeja del sistema, junto al reloj.

## Requisitos

- Windows 10 o Windows 11.
- Fortnite en **español de Latinoamérica**.
- El juego a pantalla completa en el monitor principal.

## Instalación

1. Descarga `FortniteChestCounter.exe` desde [Releases](https://github.com/JaimeCardonagz/ContadorFn/releases).
2. Ábrelo. No necesita instalación.
3. Windows mostrará "Windows protegió su PC" porque el ejecutable no está firmado: pulsa **Más información** → **Ejecutar de todas formas**.

## Primer uso

La primera vez, el programa pregunta cuántos cofres de cada tipo y cuántas partidas llevas. Esos números son tu punto de partida; desde ahí suma solo. Si empiezas de cero, déjalo todo en 0.

Después abre el juego: el contador aparecerá en la esquina y el panel se irá a la bandeja. Un clic en el icono de la bandeja lo vuelve a abrir; el clic derecho ofrece **Salir**.

## El panel

| Pestaña | Para qué sirve |
|---|---|
| Contador | Cofres de la partida, del día y del total |
| Estadísticas | Total de cada tipo, partidas jugadas y promedio por partida |
| Configuración | Idioma, contador sobre el juego, botón con el que abres los cofres y reinicio de estadísticas |

## Privacidad

Todo se queda en tu equipo. El programa no modifica el juego, no envía nada a internet y no tiene cuentas ni telemetría.

## Desarrollo

Rust + Tauri 2 + Svelte 5.

```bash
npm install
npm run tauri dev
```

## Licencia

[MIT](LICENSE)
