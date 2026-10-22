# albioncompo-overlay

Overlay para Windows que muestra tu build asignada de [AlbionCompo](https://albioncompo.com) arriba del juego, sin tapar nada y gastando lo mínimo de RAM y CPU.

> **Estado: prototipo 3.** Conectado al planificador de asignaciones de la web: entrás con el código, te anotás con tu nombre y rol, y cuando el líder te asigna una build la ves arriba del juego con íconos, nombres y la party completa.

## App de escritorio

Al abrirlo aparece la ventana de **AlbionCompo**, sin marco y con barra lateral (Inicio, Compos, Ajustes):

- **Inicio**: si Albion está abierto, tu asignación del Comp Planner (con el botón para ingresar o cambiar el código), accesos rápidos y los atajos del juego.
- **Compos**: las compos públicas en tarjetas, con búsqueda por nombre, y una pestaña para abrir una privada con su código o link. Al abrir una ves todas sus builds con los 8 o 9 casilleros; pasando el mouse ves el nombre del ítem, y con **Ver en el overlay** la build aparece en la tira mientras jugás.
- **Ajustes**: idioma, si la ventana se abre al iniciar, si se va a la bandeja cuando arranca Albion y si cerrarla la deja en la bandeja (por defecto sí, así el overlay sigue andando). También fijar la build o la party.

Cada 4 s mira si `Albion-Online.exe` está en la lista de procesos, igual que el Administrador de tareas. La ventana se crea al abrirla y se destruye al mandarla a la bandeja, así no ocupa memoria mientras jugás. El clic en el ícono de la bandeja la vuelve a abrir. `--bandeja` arranca sin abrirla; `--juego` simula el juego abierto para probar.

Cada sección vive en su módulo (`src/desk/<sección>.rs`) y su vista (`ui/desk/<sección>.slint`), con su propio global de Slint, así sumar una nueva no toca las otras.

## Seguro frente al juego

Es una ventana aparte, siempre encima y sin foco. No inyecta nada en el juego, no lee su memoria, no instala hooks de teclado ni mira el tráfico de red. Para que la veas arriba del juego, Albion tiene que estar en modo **ventana sin bordes**.

## Cómo probarlo

1. Bajá `albioncompo-overlay.exe` de la última ejecución en [Actions](../../actions) (artefacto `albioncompo-overlay-windows`) o de [Releases](../../releases).
2. Comprobá el SHA-256 con el de `SHA256SUMS.txt`: `Get-FileHash albioncompo-overlay.exe`.
3. Ejecutalo. Como todavía no está firmado, Windows SmartScreen va a avisar: **Más información → Ejecutar de todas formas**.

### Primer uso

El idioma se elige en Ajustes (la primera vez toma el de Windows). Para entrar a una asignación, **Ingresar código** en el Inicio abre la ventana de ingreso, en dos pasos:

1. **Código**: el de 6 caracteres que genera quien arma la asignación en la web. Si no existe o venció, lo avisa ahí mismo.
2. **Registro**: muestra la asignación encontrada y pide **tu nombre en el juego** y los **roles que podés jugar** (podés elegir varios). Si ya te anotaron desde la web con ese nombre, solo te reconoce.

Arriba a la derecha elegís el idioma (ES, EN o PT): cambia la interfaz y los nombres de los ítems. La primera vez toma el idioma de Windows.

El código, el nombre, los roles y el idioma se guardan en `%APPDATA%\albioncompo-overlay\config.txt` para la próxima vez. No se guarda ninguna contraseña ni clave.

### La tira

Aparece solo cuando **Albion-Online.exe** está en primer plano, arriba al centro del área del juego (también con resoluciones o ventanas modificadas). Si cambiás a otra ventana se oculta. Para saberlo mira 3 veces por segundo qué programa está en primer plano, igual que el Administrador de tareas: no toca el juego.

- **Sin build todavía**: muestra tu rol y el estado (esperando al líder, código vencido, sin conexión...). Un clic en el mensaje abre el ingreso o reintenta.
- **Con build asignada**: a la izquierda el rol y el nombre de la build, después los 8 casilleros (9 con rol de montura de batalla, con la montura primero). Pasá el mouse sobre un casillero y a los 300 ms aparece el nombre del ítem, el tier y la alternativa si la build tiene una.
- **Botón de la party** (a la derecha): despliega quiénes están anotados, con su rol y el arma de su build, ordenados por rol. Tu fila va resaltada.
- **Varias parties y caller**: si el líder armó la asignación con varias parties, cuando te asigna build la celda de la build dice en cuál estás ("P2 · …") y el panel muestra solo tu party con su nombre. Si cargó quién callea, aparece arriba en el panel. Necesita `supabase/assignment-parties.sql` de la web corrido antes que `sql/overlay.sql`.
- **Buscar compos**: el botón del menú (o Ctrl+Shift+B) abre un panel chico, oculto por defecto, con las compos públicas (con buscador por nombre) y una pestaña "Con código" para abrir una privada con su código o link. Al elegir una compo ves sus builds y podés mostrar una en la tira. Solo consulta cuando buscás o abrís algo, sin polling, y queda fijo hasta que lo cerrás. Necesita [`sql/compos.sql`](sql/compos.sql), corrido después de `sql/overlay.sql`.
- **Clic derecho** en cualquier parte de la tira: abre la ventana de ingreso (cambiar código, nombre, idioma o salir de la asignación).
- **Por defecto todo queda minimizado en el botón flotante** con el logo de AlbionCompo, en el borde derecho del juego. Sin asignación, el botón abre el ingreso; con asignación, despliega la build arriba y la party al lado, y otro clic los vuelve a minimizar. Cuando el líder te asigna una build nueva, el borde del botón late hasta que lo tocás. Desde el menú podés **fijar** la build o la party para que queden siempre visibles (se recuerda para la próxima). El puntito muestra el estado (dorado esperando build, color del rol con build, ámbar si hay un problema). Clic derecho: el mismo menú que la bandeja.
- **Ícono en la bandeja** de Windows (al lado del reloj): clic abre la app; clic derecho muestra un menú para ver la party, ocultar el overlay, hacer que los clics atraviesen la tira, cambiar a píldora o salir.

Si necesitás clickear en la zona donde está el overlay, tenés dos opciones: **ocultarlo** (Ctrl+Shift+H o desde el menú) o dejarlo visible pero que **los clics lo atraviesen** (Ctrl+Shift+L).

Los íconos salen de `render.albiononline.com` y los nombres de los mismos archivos que usa la web; los dos quedan en caché en `%LOCALAPPDATA%\albioncompo-overlay`, así que se bajan una sola vez.

Consulta la asignación cada 15 s mientras esperás build, cada 3 min cuando ya la tenés y nunca mientras el juego no está en primer plano. Si no hubo cambios el servidor responde "sin cambios" sin mandar todo de nuevo.

| Atajo | Qué hace |
|---|---|
| Ctrl+Shift+L | Click-through: el mouse atraviesa el overlay y el juego recibe todos los clics (el borde se pone ámbar) |
| Ctrl+Shift+M | Alterna entre la tira y la píldora |
| Ctrl+Shift+P | Abre o cierra el panel de la party |
| Ctrl+Shift+B | Abre o cierra el buscador de compos |
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
