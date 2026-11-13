# Fortnite Chest Counter

Cuenta automáticamente los cofres que abres en Fortnite y te los muestra en una esquina de la pantalla mientras juegas.

**No lee ni modifica el juego; solo analiza la pantalla**, como lo haría un programa de captura. No hay garantía oficial de Epic Games.

*English: a small Windows app that counts the chests you open in Fortnite by reading the on-screen XP notice. The interface is available in English; chest detection currently needs the game set to Spanish (Latin America).*

## Qué hace

- Cuenta **cofres de tesoro**, **cofres raros** y **cofres de espíritus**, y los **cañones dorados** como cofres de tesoro.
- Lleva la cuenta de la **partida**, del **día** y del **total**, y el promedio de cofres por partida.
- Detecta solo cuándo empieza y termina cada partida.
- Muestra un **contador pequeño, solo texto y sin fondo**, en la esquina que elijas. Tú decides qué enseña: número de partida, cofres totales y cofres de la partida (sumados o por tipo).
- El contador solo aparece con GeForce NOW o Fortnite delante; el resto del tiempo el programa queda en la bandeja del sistema, junto al reloj.

## Requisitos

- Windows 10 (versión 2004 o posterior) o Windows 11.
- Fortnite en **español de Latinoamérica**, con el HUD a su tamaño por defecto.
- El juego ocupando **todo el monitor principal** (GeForce NOW a pantalla completa, o Fortnite en "ventana en pantalla completa").
- Probado con GeForce NOW a 1360×768. Otras resoluciones deberían funcionar, pero no están probadas.

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
| Configuración | Idioma, qué muestra el contador y en qué esquina, botón con el que abres los cofres, reiniciar estadísticas |
| Diagnóstico | Estado de la detección y herramientas para investigar fallos |

## Cómo cuenta

Al abrir un cofre, el juego muestra un aviso de experiencia ("+600 PE / COFRE", "COFRE RARO", "COFRE DE ESPÍRITUS"). El programa mira esa zona de la pantalla unas diez veces por segundo y suma un cofre cada vez que aparece un aviso nuevo, o cada vez que su multiplicador sube ("X2", "X3"…) al abrir varios iguales seguidos.

El cañón dorado no muestra ese aviso: se reconoce por su recuadro "ENCENDER MECHA" y su anillo de progreso.

## Limitaciones

- Solo reconoce los textos del juego en español de Latinoamérica.
- Un cofre **destruido** en vez de abierto no se cuenta: el juego no muestra ningún aviso.
- Si el aviso queda muy tapado por otros carteles durante todo el tiempo que está en pantalla, ese cofre puede no contarse.
- Con muy poca vida (menos de un 15 %) durante más de 25 segundos seguidos, el programa puede dar la partida por terminada antes de tiempo.

## Privacidad

Todo se queda en tu equipo. El programa no envía nada a internet y no tiene cuentas ni telemetría. Las estadísticas se guardan en `%APPDATA%\io.github.jaimecardonagz.fortnitechestcounter`.

## Desarrollo

Rust + Tauri 2 + Svelte 5. El diseño está en [ARQUITECTURA.md](ARQUITECTURA.md).

```bash
npm install
npm run tauri dev
```

Cada push a `main` compila el ejecutable de Windows con GitHub Actions (workflow *Build Windows*).

## Licencia

[MIT](LICENSE)
