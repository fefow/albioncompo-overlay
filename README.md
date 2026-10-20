# albioncompo-overlay

Overlay para Windows que muestra tu build asignada de [AlbionCompo](https://albioncompo.com) arriba del juego, sin tapar nada y gastando lo mínimo de RAM y CPU.

> **Estado: prototipo 3.** Conectado al planificador de asignaciones de la web: entrás con el código, te anotás con tu nombre y rol, y cuando el líder te asigna una build la ves arriba del juego con íconos, nombres y la party completa.

## Seguro frente al juego

Es una ventana aparte, siempre encima y sin foco. No inyecta nada en el juego, no lee su memoria, no instala hooks de teclado ni mira el tráfico de red. Para que la veas arriba del juego, Albion tiene que estar en modo **ventana sin bordes**.

## Cómo probarlo

1. Bajá `albioncompo-overlay.exe` de la última ejecución en [Actions](../../actions) (artefacto `albioncompo-overlay-windows`) o de [Releases](../../releases).
2. Comprobá el SHA-256 con el de `SHA256SUMS.txt`: `Get-FileHash albioncompo-overlay.exe`.
3. Ejecutalo. Como todavía no está firmado, Windows SmartScreen va a avisar: **Más información → Ejecutar de todas formas**.

### Primer uso

Al abrirlo por primera vez aparece la ventana de ingreso, en dos pasos:

1. **Código**: el de 6 caracteres que genera quien arma la asignación en la web. Si no existe o venció, lo avisa ahí mismo.
2. **Registro**: muestra la asignación encontrada y pide **tu nombre en el juego** y los **roles que podés jugar** (podés elegir varios). Si ya te anotaron desde la web con ese nombre, solo te reconoce.

Arriba a la derecha elegís el idioma (ES, EN o PT): cambia la interfaz y los nombres de los ítems. La primera vez toma el idioma de Windows.

El código, el nombre, los roles y el idioma se guardan en `%APPDATA%\albioncompo-overlay\config.txt` para la próxima vez. No se guarda ninguna contraseña ni clave.

### La tira

Aparece solo cuando **Albion-Online.exe** está en primer plano, arriba al centro del área del juego (también con resoluciones o ventanas modificadas). Si cambiás a otra ventana se oculta. Para saberlo mira 3 veces por segundo qué programa está en primer plano, igual que el Administrador de tareas: no toca el juego.

- **Sin build todavía**: muestra tu rol y el estado (esperando al líder, código vencido, sin conexión...). Un clic en el mensaje abre el ingreso o reintenta.
- **Con build asignada**: a la izquierda el rol y el nombre de la build, después los 8 casilleros (9 con rol de montura de batalla, con la montura primero). Pasá el mouse sobre un casillero y a los 300 ms aparece el nombre del ítem, el tier y la alternativa si la build tiene una.
- **Botón de la party** (a la derecha): despliega quiénes están anotados, con su rol y el arma de su build, ordenados por rol. Tu fila va resaltada.
- **Clic derecho** en cualquier parte de la tira: abre la ventana de ingreso (cambiar código, nombre, idioma o salir de la asignación).
- **Botón flotante** con el logo de AlbionCompo, en el borde derecho del juego: sin asignación abre el ingreso; con asignación despliega la party al lado. El puntito muestra el estado (dorado esperando build, color del rol con build, ámbar si hay un problema). Clic derecho: el mismo menú que la bandeja.
- **Ícono en la bandeja** de Windows (al lado del reloj): clic abre el ingreso; clic derecho muestra un menú para ver la party, ocultar el overlay, hacer que los clics atraviesen la tira, cambiar a píldora o salir.

Si necesitás clickear en la zona donde está el overlay, tenés dos opciones: **ocultarlo** (Ctrl+Shift+H o desde el menú) o dejarlo visible pero que **los clics lo atraviesen** (Ctrl+Shift+L).

Los íconos salen de `render.albiononline.com` y los nombres de los mismos archivos que usa la web; los dos quedan en caché en `%LOCALAPPDATA%\albioncompo-overlay`, así que se bajan una sola vez.

Consulta la asignación cada 15 s mientras esperás build, cada 3 min cuando ya la tenés y nunca mientras el juego no está en primer plano. Si no hubo cambios el servidor responde "sin cambios" sin mandar todo de nuevo.

| Atajo | Qué hace |
|---|---|
| Ctrl+Shift+L | Click-through: el mouse atraviesa el overlay y el juego recibe todos los clics (el borde se pone ámbar) |
| Ctrl+Shift+M | Alterna entre la tira y la píldora |
| Ctrl+Shift+P | Abre o cierra el panel de la party |
| Ctrl+Shift+K | Abre la ventana de ingreso (código, nombre, idioma) |
| Ctrl+Shift+H | Oculta o vuelve a mostrar todo el overlay (tira, botón y party) |
| Ctrl+Shift+Q | Cierra el overlay |

Opciones al ejecutar desde una terminal:

| Opción | Qué hace |
|---|---|
| `--demo` | Usa una asignación de prueba, sin código ni servidor |
| `--montura` | Como `--demo`, con rol de montura de batalla (9 casilleros) |
| `--pildora` | Arranca en modo píldora |
| `--siempre` | Se muestra aunque Albion no esté en primer plano (para probar sin el juego) |
| `--opacidad 80` | Opacidad de 30 a 100 (por defecto 92) |

## Compilar

Lo compila GitHub Actions en `windows-latest` en cada push ([`build.yml`](.github/workflows/build.yml)). Para compilarlo a mano hace falta Rust estable:

```
cargo build --release
```

Para conectarse, el .exe necesita la dirección del proyecto de Supabase y su clave pública (anon), la misma que la web ya publica en su JavaScript. En GitHub van como **Variables** del repositorio (Settings → Secrets and variables → Actions → Variables): `ALBIONCOMPO_SUPABASE_URL` y `ALBIONCOMPO_SUPABASE_ANON_KEY`. A mano, se definen como variables de entorno antes de compilar. La base necesita las funciones de [`sql/overlay.sql`](sql/overlay.sql).

Usa [Slint](https://slint.dev) con el renderer por software (sin GPU ni WebView) y el crate `windows` para lo que Slint no expone: ventana sin foco, fuera de la barra de tareas, opacidad, esquinas redondeadas, click-through y atajos con `RegisterHotKey`.

## Licencia

El código de este repositorio es [MIT](LICENSE).

Slint se usa bajo la [Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md), que es gratis para aplicaciones de escritorio a cambio de mostrar este distintivo en la página de descarga:

[![Made with Slint](https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png)](https://slint.dev)
