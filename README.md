# albioncompo-overlay

Overlay para Windows que muestra tu build asignada de [AlbionCompo](https://albioncompo.com) arriba del juego, sin tapar nada y gastando lo mínimo de RAM y CPU.

> **Estado: prototipo 2.** Muestra datos de prueba fijos y no se conecta a internet. Sirve para medir cuánto consume la interfaz antes de sumar la red, los íconos y los nombres por idioma.

## Seguro frente al juego

Es una ventana aparte, siempre encima y sin foco. No inyecta nada en el juego, no lee su memoria, no instala hooks de teclado ni mira el tráfico de red. Para que la veas arriba del juego, Albion tiene que estar en modo **ventana sin bordes**.

## Cómo probarlo

1. Bajá `albioncompo-overlay.exe` de la última ejecución en [Actions](../../actions) (artefacto `albioncompo-overlay-windows`) o de [Releases](../../releases).
2. Comprobá el SHA-256 con el de `SHA256SUMS.txt`: `Get-FileHash albioncompo-overlay.exe`.
3. Ejecutalo. Como todavía no está firmado, Windows SmartScreen va a avisar: **Más información → Ejecutar de todas formas**.

La tira aparece arriba al centro de la pantalla. Pasá el mouse sobre un casillero y a los 300 ms aparece el nombre del ítem.

| Atajo | Qué hace |
|---|---|
| Ctrl+Shift+L | Click-through: el mouse atraviesa el overlay y el juego recibe todos los clics (el borde se pone ámbar) |
| Ctrl+Shift+M | Alterna entre la tira y la píldora |
| Ctrl+Shift+Q | Cierra el overlay |

Opciones al ejecutar desde una terminal:

| Opción | Qué hace |
|---|---|
| `--montura` | Simula el rol de montura de batalla (9 casilleros) |
| `--pildora` | Arranca en modo píldora |
| `--opacidad 80` | Opacidad de 30 a 100 (por defecto 92) |

## Compilar

Lo compila GitHub Actions en `windows-latest` en cada push ([`build.yml`](.github/workflows/build.yml)). Para compilarlo a mano hace falta Rust estable:

```
cargo build --release
```

Usa [Slint](https://slint.dev) con el renderer por software (sin GPU ni WebView) y el crate `windows` para lo que Slint no expone: ventana sin foco, fuera de la barra de tareas, opacidad, esquinas redondeadas, click-through y atajos con `RegisterHotKey`.

## Licencia

El código de este repositorio es [MIT](LICENSE).

Slint se usa bajo la [Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md), que es gratis para aplicaciones de escritorio a cambio de mostrar este distintivo en la página de descarga:

[![Made with Slint](https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png)](https://slint.dev)
