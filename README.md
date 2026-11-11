# Fortnite Chest Counter

Contador de cofres para Fortnite. **No lee ni modifica el juego; solo analiza la pantalla.**

> Estado: primera versión funcional en pruebas. Cuenta cofres de tesoro, cofres raros y cajas de munición leyendo el aviso de experiencia del juego (español de Latinoamérica, 1366×768).
> El diseño completo está en [ARQUITECTURA.md](ARQUITECTURA.md).

La primera versión apunta a **Windows 10/11**. Linux llegará en una fase posterior.

## Desarrollo

Requisitos: [Rust](https://rustup.rs), Node.js 24 y los [requisitos de Tauri 2](https://tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev
```

## Ejecutable de Windows

Cada push a `main` compila el `.exe` portable con GitHub Actions (workflow *Build Windows*). Se descarga desde la pestaña **Actions** del repositorio, como artefacto `FortniteChestCounter-windows`.

Necesita WebView2, que ya viene con Windows 11 y con Windows 10 actualizado. Como el ejecutable no está firmado, SmartScreen mostrará un aviso: *Más información* → *Ejecutar de todas formas*.

## Licencia

[MIT](LICENSE)
