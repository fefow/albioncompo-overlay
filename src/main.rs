// Sin consola en Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod api;
mod assets;
mod config;
mod data;
#[cfg(windows)]
mod win;
mod worker;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::sync::mpsc::Sender;
use std::time::Duration;

use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::window::{WindowAttributes, WindowLevel};
use slint::{
    CloseRequestResponse, Color, ComponentHandle, Image, LogicalSize, ModelRc, PhysicalPosition,
    Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode, VecModel,
};

use api::{ApiError, Overlay};
use config::{Config, Lang, normalize_code, normalize_name};
use worker::{Command, Event};

slint::include_modules!();

/// El tooltip aparece si el mouse se queda este tiempo sobre un casillero.
const TOOLTIP_DELAY: Duration = Duration::from_millis(300);
/// Cada cuánto se mira si Albion sigue en primer plano (una consulta barata a Windows).
#[cfg(windows)]
const FOREGROUND_POLL: Duration = Duration::from_millis(300);
/// Consultas a Supabase. Cada una deja una línea de log en el proyecto, así
/// que se espacian según lo que se espera que cambie, y no se consulta
/// mientras el overlay está oculto.
const POLL_WAITING: Duration = Duration::from_secs(15);
const POLL_NOT_JOINED: Duration = Duration::from_secs(60);
const POLL_ASSIGNED: Duration = Duration::from_secs(180);
const POLL_OFFLINE: Duration = Duration::from_secs(30);
const POLL_RATE_LIMITED: Duration = Duration::from_secs(120);
/// Separación entre la tira y las ventanas de abajo, y entre la tira y el borde del juego.
const GAP: f32 = 6.0;
const TOP_MARGIN: f32 = 8.0;
const SLOT_STEP: f32 = 42.0;
const STRIP_PAD_LEFT: f32 = 9.0;
/// Celda del rol a la izquierda de los ítems (ancho + separación).
const ROLE_CELL: f32 = 88.0;
const PARTY_WIDTH: f32 = 280.0;
/// Casilleros que ocupa la zona de mensaje cuando todavía no hay build.
const MESSAGE_SLOTS: usize = 8;

struct Args {
    demo: bool,
    montura: bool,
    pildora: bool,
    siempre: bool,
    alpha: u8,
}

/// `--demo` usa una asignación de prueba sin servidor (`--montura` le pone el
/// rol de montura de batalla). `--pildora` arranca en modo píldora.
/// `--opacidad N` de 30 a 100 (por defecto 92). `--siempre` lo muestra aunque
/// Albion no esté en primer plano (para probar sin el juego).
fn parse_args() -> Args {
    let mut args = Args {
        demo: false,
        montura: false,
        pildora: false,
        siempre: false,
        alpha: pct_to_alpha(92),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--demo" => args.demo = true,
            "--montura" => {
                args.demo = true;
                args.montura = true;
            }
            "--pildora" => args.pildora = true,
            "--siempre" => args.siempre = true,
            "--opacidad" => {
                if let Some(pct) = it.next().and_then(|v| v.parse::<u32>().ok()) {
                    args.alpha = pct_to_alpha(pct.clamp(30, 100));
                }
            }
            _ => {}
        }
    }
    args
}

fn pct_to_alpha(pct: u32) -> u8 {
    (pct * 255 / 100) as u8
}

thread_local! {
    /// La próxima ventana que se cree es una ventana común (la de ingreso):
    /// con foco y en la barra de tareas, para poder escribir el código.
    static NEXT_WINDOW_NORMAL: Cell<bool> = const { Cell::new(false) };
}

/// Atributos de las ventanas del overlay: sin marco, siempre encima y, sobre
/// todo, sin robarle el foco al juego al aparecer.
fn window_attributes(attrs: WindowAttributes) -> WindowAttributes {
    if NEXT_WINDOW_NORMAL.get() {
        return attrs
            .with_active(true)
            .with_window_level(WindowLevel::AlwaysOnTop);
    }
    let attrs = attrs
        .with_active(false)
        .with_decorations(false)
        .with_resizable(false)
        .with_window_level(WindowLevel::AlwaysOnTop);
    #[cfg(windows)]
    {
        use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
        attrs.with_skip_taskbar(true)
    }
    #[cfg(not(windows))]
    attrs
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    /// Todavía no se cargó un código.
    NoCode,
    Loading,
    /// Anotado, pero el líder todavía no le asignó build.
    Waiting,
    /// El código anda pero el nombre no está en la asignación.
    NotJoined,
    Assigned,
    Expired,
    Invalid,
    Offline,
    RateLimited,
    NotConfigured,
}

struct TipInfo {
    name: String,
    label: String,
    tier: String,
    color: Color,
    alt: String,
}

struct State {
    config: Config,
    /// Datos del último ingreso pedido, hasta que el servidor responde.
    pending: Option<Config>,
    phase: Phase,
    overlay: Option<Overlay>,
    /// id original -> nombre en el idioma elegido.
    names: HashMap<String, String>,
    icons: HashMap<String, Image>,
    /// Ids ya pedidos al hilo de red (para no repetir).
    requested: HashSet<String>,
    tips: Vec<TipInfo>,
    tooltip: Option<TooltipWindow>,
    hover_timer: Timer,
    party: Option<PartyWindow>,
    setup: Option<SetupWindow>,
    poll_timer: Timer,
    poll_inflight: bool,
    /// Tocaba consultar mientras el overlay estaba oculto: se hace al volver.
    poll_due: bool,
    retry_after: Option<u64>,
    /// Tamaño y posición aplicados (para no tocar la ventana si no cambiaron).
    last_geometry: Option<(i32, i32, i32, i32)>,
    visible: bool,
    #[cfg_attr(not(windows), allow(dead_code))]
    alpha: u8,
    #[cfg(windows)]
    hwnd: Option<win::Hwnd>,
    /// Área del juego cuando está en primer plano; la tira se centra arriba de ella.
    #[cfg(windows)]
    game: Option<win::GameRect>,
    #[cfg(windows)]
    watch: win::ForegroundWatch,
    #[cfg(windows)]
    follow_timer: Timer,
}

struct Ctx {
    ui: OverlayWindow,
    st: RefCell<State>,
    tx: Sender<Command>,
    demo: bool,
}

type App = Rc<Ctx>;

fn main() -> Result<(), slint::PlatformError> {
    let args = parse_args();

    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(window_attributes)
        .select()?;

    // El hilo de red responde en el hilo de la UI; la app todavía no existe
    // cuando se crea, así que se engancha después.
    let slot: Rc<RefCell<Option<Weak<Ctx>>>> = Rc::new(RefCell::new(None));
    let tx = worker::spawn({
        let slot = slot.clone();
        move |event| {
            let app = slot.borrow().as_ref().and_then(Weak::upgrade);
            if let Some(app) = app {
                on_event(&app, event);
            }
        }
    });

    let ui = OverlayWindow::new()?;
    ui.set_pill_mode(args.pildora);

    let mut config = Config::load();
    let mut overlay = None;
    let mut phase = if config.has_assignment() {
        Phase::Loading
    } else {
        Phase::NoCode
    };
    if args.demo {
        config.code = "DEMO00".into();
        config.name = "Dusius".into();
        overlay = Some(data::demo_overlay(args.montura));
        phase = Phase::Assigned;
    }

    let app: App = Rc::new(Ctx {
        ui,
        st: RefCell::new(State {
            config,
            pending: None,
            phase,
            overlay,
            names: HashMap::new(),
            icons: HashMap::new(),
            requested: HashSet::new(),
            tips: Vec::new(),
            tooltip: None,
            hover_timer: Timer::default(),
            party: None,
            setup: None,
            poll_timer: Timer::default(),
            poll_inflight: false,
            poll_due: false,
            retry_after: None,
            last_geometry: None,
            visible: true,
            alpha: args.alpha,
            #[cfg(windows)]
            hwnd: None,
            #[cfg(windows)]
            game: None,
            #[cfg(windows)]
            watch: win::ForegroundWatch::default(),
            #[cfg(windows)]
            follow_timer: Timer::default(),
        }),
        tx,
        demo: args.demo,
    });
    *slot.borrow_mut() = Some(Rc::downgrade(&app));

    wire_callbacks(&app);
    render(&app);

    // Lo que necesita la ventana real de Windows se hace cuando winit la creó.
    let weak = Rc::downgrade(&app);
    let siempre = args.siempre;
    slint::spawn_local(async move {
        let Some(app) = weak.upgrade() else { return };
        let Ok(winit_window) = app.ui.window().winit_window().await else {
            return;
        };
        #[cfg(windows)]
        if let Some(hwnd) = win::hwnd_of(&winit_window) {
            let alpha = app.st.borrow().alpha;
            win::make_overlay(hwnd, alpha, false);
            app.st.borrow_mut().hwnd = Some(hwnd);
            let weak = Rc::downgrade(&app);
            let failed = win::register_hotkeys(hwnd, move |key| {
                if let Some(app) = weak.upgrade() {
                    on_hotkey(&app, key);
                }
            });
            if !failed.is_empty() {
                eprintln!("atajos ocupados por otra app: {failed:?}");
            }
        }
        drop(winit_window);
        place_main(&app, true);

        // Solo visible con Albion-Online.exe en primer plano, salvo con --siempre.
        #[cfg(windows)]
        if !siempre {
            follow_game(&app);
            let weak = Rc::downgrade(&app);
            app.st
                .borrow()
                .follow_timer
                .start(TimerMode::Repeated, FOREGROUND_POLL, move || {
                    if let Some(app) = weak.upgrade() {
                        follow_game(&app);
                    }
                });
        }
        #[cfg(not(windows))]
        let _ = siempre;

        let phase = app.st.borrow().phase;
        match phase {
            Phase::Loading => poll_now(&app),
            Phase::NoCode => open_setup(&app),
            _ => {}
        }
    })
    .expect("el bucle de eventos de Slint no está disponible");

    app.ui.run()
}

fn wire_callbacks(app: &App) {
    let ui = &app.ui;
    let weak = Rc::downgrade(app);
    ui.on_slot_hover(move |index, entered| {
        let Some(app) = weak.upgrade() else { return };
        if !entered {
            app.st.borrow().hover_timer.stop();
            hide_tooltip(&app);
            return;
        }
        let weak = Rc::downgrade(&app);
        app.st
            .borrow()
            .hover_timer
            .start(TimerMode::SingleShot, TOOLTIP_DELAY, move || {
                if let Some(app) = weak.upgrade()
                    && app.ui.get_hovered_slot() == index
                    && !app.ui.get_click_through()
                {
                    show_tooltip(&app, index as usize);
                }
            });
    });

    let weak = Rc::downgrade(app);
    ui.on_party_clicked(move || {
        if let Some(app) = weak.upgrade() {
            toggle_party(&app);
        }
    });

    let weak = Rc::downgrade(app);
    ui.on_open_setup(move || {
        if let Some(app) = weak.upgrade() {
            open_setup(&app);
        }
    });

    let weak = Rc::downgrade(app);
    ui.on_status_clicked(move || {
        let Some(app) = weak.upgrade() else { return };
        let phase = app.st.borrow().phase;
        match phase {
            Phase::Offline | Phase::RateLimited => poll_now(&app),
            Phase::Loading | Phase::Waiting | Phase::Assigned => {}
            _ => open_setup(&app),
        }
    });
}

// ---------------------------------------------------------------------------
// Datos

fn my_entry<'a>(overlay: &'a Overlay, name: &str) -> Option<&'a api::Entry> {
    let me = normalize_name(name);
    if me.is_empty() {
        return None;
    }
    overlay
        .entries
        .iter()
        .find(|e| normalize_name(&e.name) == me)
}

fn build_of<'a>(overlay: &'a Overlay, entry: &api::Entry) -> Option<&'a api::Build> {
    let id = entry.build_id.as_deref()?;
    overlay.builds.iter().find(|b| b.id.as_deref() == Some(id))
}

fn phase_for(overlay: &Overlay, name: &str) -> Phase {
    match my_entry(overlay, name) {
        None => Phase::NotJoined,
        Some(entry) if build_of(overlay, entry).is_some() => Phase::Assigned,
        Some(_) => Phase::Waiting,
    }
}

/// Rol que se muestra: el de la build asignada o, si no hay, el que eligió al anotarse.
fn display_role(build: Option<&api::Build>, entry: Option<&api::Entry>) -> String {
    build
        .and_then(|b| b.role.as_deref())
        .or_else(|| entry.and_then(|e| e.role.as_deref()))
        .map(|r| r.trim().to_uppercase())
        .unwrap_or_default()
}

fn status_for(phase: Phase) -> (&'static str, Color) {
    let muted = Color::from_rgb_u8(0xa8, 0x96, 0x79);
    let gold = Color::from_rgb_u8(0xd6, 0xad, 0x5e);
    let warn = Color::from_rgb_u8(0xff, 0xb4, 0x54);
    let error = Color::from_rgb_u8(0xff, 0x7a, 0x6b);
    match phase {
        Phase::NoCode => ("Clic acá para ingresar el código de la party", gold),
        Phase::Loading => ("Cargando la asignación…", muted),
        Phase::Waiting => ("Esperando que el líder te asigne una build", muted),
        Phase::NotJoined => ("No figurás en la asignación · clic para entrar", warn),
        Phase::Assigned => ("", muted),
        Phase::Expired => ("La asignación venció · clic para cambiar el código", warn),
        Phase::Invalid => ("Código inválido · clic para cambiarlo", error),
        Phase::Offline => (
            "Sin conexión · reintentando (clic para reintentar ya)",
            warn,
        ),
        Phase::RateLimited => ("Demasiados intentos · esperá unos minutos", warn),
        Phase::NotConfigured => ("Esta versión no tiene configurado el servidor", error),
    }
}

fn pixels_to_image(pixels: &assets::Pixels) -> Image {
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &pixels.rgba,
        pixels.width,
        pixels.height,
    );
    Image::from_rgba8(buffer)
}

/// Pasa el estado a la tira: casilleros, rol, mensaje y contador de la party.
fn render(app: &App) {
    let mut wanted = Vec::new();
    let (slots, tips, role, build_name, status, count) = {
        let st = app.st.borrow();
        let overlay = st.overlay.as_ref();
        let entry = overlay.and_then(|o| my_entry(o, &st.config.name));
        let build = overlay.zip(entry).and_then(|(o, e)| build_of(o, e));
        let show_build = matches!(
            st.phase,
            Phase::Assigned | Phase::Offline | Phase::RateLimited
        );
        let role = display_role(build, entry);

        let mut slots = Vec::new();
        let mut tips = Vec::new();
        if let (true, Some(build)) = (show_build, build) {
            for item in data::slot_items(build, &role) {
                let (tier, enchant) = data::tier_of(&item.id);
                let color = data::enchant_color(enchant);
                let image = st.icons.get(&item.id).cloned();
                let name_of = |id: &str| st.names.get(id).cloned().unwrap_or_else(|| id.into());
                slots.push(Slot {
                    icon: item.kind.icon().into(),
                    has_image: image.is_some(),
                    image: image.unwrap_or_default(),
                    tier: tier.clone().into(),
                    tier_color: color,
                    enchanted: enchant > 0,
                    empty: item.id.is_empty(),
                });
                tips.push(TipInfo {
                    name: if item.id.is_empty() {
                        "Sin ítem".into()
                    } else {
                        name_of(&item.id)
                    },
                    label: item.kind.label().into(),
                    tier,
                    color,
                    alt: item.variant.as_deref().map(name_of).unwrap_or_default(),
                });
                if !item.id.is_empty() {
                    wanted.push(item.id);
                }
                if let Some(v) = item.variant {
                    wanted.push(v);
                }
            }
        }
        if let Some(o) = overlay {
            wanted.extend(party_weapon_ids(o));
        }
        let status = if slots.is_empty() && st.phase == Phase::Assigned {
            status_for(Phase::Waiting)
        } else {
            status_for(st.phase)
        };
        let count = overlay
            .map(|o| o.entries.len())
            .filter(|n| *n > 0)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let build_name = build
            .filter(|_| show_build)
            .and_then(|b| b.name.clone())
            .unwrap_or_default();
        (slots, tips, role, build_name, status, count)
    };

    let ui = &app.ui;
    // En la píldora va el primer casillero con ítem: la montura o el arma.
    if let Some(main) = tips.iter().find(|t| t.name != "Sin ítem") {
        ui.set_main_name(main.name.clone().into());
        ui.set_main_tier(main.tier.clone().into());
        ui.set_main_tier_color(main.color);
    }
    ui.set_role_color(data::role_color(&role));
    // "BATTLE MOUNT" no entra en la celda del rol.
    let role = if data::is_mount_role(&role) {
        "MONTURA".to_owned()
    } else {
        role
    };
    ui.set_role(role.into());
    ui.set_build_name(build_name.into());
    ui.set_status_text(status.0.into());
    ui.set_status_color(status.1);
    ui.set_party_count(count.into());
    ui.set_message_width(MESSAGE_SLOTS as f32 * SLOT_STEP - 4.0);
    ui.set_slots(ModelRc::new(VecModel::from(slots)));
    app.st.borrow_mut().tips = tips;

    if app.st.borrow().party.is_some() {
        render_party(app);
    }
    request_assets(app, wanted);
    place_main(app, false);
}

// ---------------------------------------------------------------------------
// Red

fn request_assets(app: &App, ids: Vec<String>) {
    let (lang, new) = {
        let mut st = app.st.borrow_mut();
        let new: Vec<String> = ids
            .into_iter()
            .filter(|id| !id.is_empty())
            .collect::<HashSet<_>>()
            .into_iter()
            .filter(|id| st.requested.insert(id.clone()))
            .collect();
        (st.config.lang, new)
    };
    if !new.is_empty() {
        let _ = app.tx.send(Command::Assets { lang, ids: new });
    }
}

fn poll_now(app: &App) {
    if app.demo {
        return;
    }
    let (code, rev) = {
        let mut st = app.st.borrow_mut();
        if st.poll_inflight || st.config.code.is_empty() {
            return;
        }
        st.poll_inflight = true;
        st.poll_due = false;
        st.poll_timer.stop();
        let rev = st.overlay.as_ref().and_then(|o| o.rev.clone());
        (st.config.code.clone(), rev)
    };
    let _ = app.tx.send(Command::Poll { code, rev });
}

/// Programa la próxima consulta. Si toca mientras el overlay está oculto,
/// se deja anotada y se hace cuando vuelve Albion.
fn schedule_poll(app: &App) {
    let Some(delay) = next_poll(app) else {
        app.st.borrow().poll_timer.stop();
        return;
    };
    let weak = Rc::downgrade(app);
    app.st
        .borrow()
        .poll_timer
        .start(TimerMode::SingleShot, delay, move || {
            let Some(app) = weak.upgrade() else { return };
            let visible = app.st.borrow().visible;
            if visible {
                poll_now(&app);
            } else {
                app.st.borrow_mut().poll_due = true;
            }
        });
}

fn next_poll(app: &App) -> Option<Duration> {
    if app.demo {
        return None;
    }
    let st = app.st.borrow();
    match st.phase {
        Phase::Assigned => Some(POLL_ASSIGNED),
        Phase::Waiting | Phase::Loading => Some(POLL_WAITING),
        Phase::NotJoined => Some(POLL_NOT_JOINED),
        Phase::Offline => Some(POLL_OFFLINE),
        Phase::RateLimited => Some(
            st.retry_after
                .map(|s| Duration::from_secs(s.clamp(10, 900)))
                .unwrap_or(POLL_RATE_LIMITED),
        ),
        Phase::NoCode | Phase::Expired | Phase::Invalid | Phase::NotConfigured => None,
    }
}

fn on_event(app: &App, event: Event) {
    match event {
        Event::Overlay(result) => {
            {
                let mut st = app.st.borrow_mut();
                st.poll_inflight = false;
                match result {
                    Ok(overlay) => match overlay.status.as_str() {
                        "ok" => {
                            st.phase = phase_for(&overlay, &st.config.name);
                            st.overlay = Some(overlay);
                        }
                        "unchanged" => {
                            st.phase = match &st.overlay {
                                Some(o) => phase_for(o, &st.config.name),
                                None => Phase::Loading,
                            };
                        }
                        "expired" => {
                            st.phase = Phase::Expired;
                            st.overlay = None;
                        }
                        "invalid" => {
                            st.phase = Phase::Invalid;
                            st.overlay = None;
                        }
                        "rate_limited" => {
                            st.phase = Phase::RateLimited;
                            st.retry_after = overlay.retry_after;
                        }
                        _ => st.phase = Phase::Offline,
                    },
                    Err(ApiError::NotConfigured) => st.phase = Phase::NotConfigured,
                    Err(ApiError::Network(error)) => {
                        eprintln!("sin conexión: {error}");
                        st.phase = Phase::Offline;
                    }
                }
            }
            render(app);
            schedule_poll(app);
        }
        Event::Joined(Ok(overlay)) => {
            {
                let mut st = app.st.borrow_mut();
                let Some(config) = st.pending.take() else {
                    return;
                };
                if config.lang != st.config.lang {
                    st.names.clear();
                    st.requested.clear();
                }
                if !app.demo {
                    let _ = config.save();
                }
                st.phase = phase_for(&overlay, &config.name);
                st.config = config;
                st.overlay = Some(overlay);
            }
            close_setup(app);
            render(app);
            schedule_poll(app);
        }
        Event::Joined(Err(message)) => {
            app.st.borrow_mut().pending = None;
            let st = app.st.borrow();
            if let Some(setup) = &st.setup {
                setup.set_busy(false);
                setup.set_error_text(message.into());
            }
        }
        Event::Names(lang, list) => {
            {
                let mut st = app.st.borrow_mut();
                if lang != st.config.lang {
                    return;
                }
                for (id, _full, name) in list {
                    st.names.insert(id, name);
                }
            }
            render(app);
        }
        Event::Icon(id, pixels) => {
            app.st
                .borrow_mut()
                .icons
                .insert(id, pixels_to_image(&pixels));
            render(app);
        }
    }
}

// ---------------------------------------------------------------------------
// Ventanas

/// Tamaño lógico de la tira según el modo y la cantidad de casilleros
/// (más el botón de la party).
fn main_size(pill: bool, slots: usize) -> LogicalSize {
    if pill {
        LogicalSize::new(300.0, 34.0)
    } else {
        let n = (slots.max(MESSAGE_SLOTS) + 1) as f32;
        let role = if slots > 0 { ROLE_CELL } else { 0.0 };
        LogicalSize::new(role + n * SLOT_STEP - 4.0 + 2.0 * STRIP_PAD_LEFT, 54.0)
    }
}

/// Ajusta tamaño, esquinas y posición: arriba al centro del área del juego,
/// o del monitor si todavía no se vio el juego.
fn place_main(app: &App, force: bool) {
    let ui = &app.ui;
    let scale = ui.window().scale_factor();
    let size = main_size(ui.get_pill_mode(), slint::Model::row_count(&ui.get_slots()));
    let width = (size.width * scale).round() as i32;
    let height = (size.height * scale).round() as i32;
    let margin = (TOP_MARGIN * scale).round() as i32;

    let mut origin = None;
    #[cfg(windows)]
    if let Some(game) = app.st.borrow().game {
        origin = Some((game.x + (game.width - width) / 2, game.y + margin));
    }
    if origin.is_none() {
        let monitor = ui
            .window()
            .with_winit_window(|w| w.current_monitor().or_else(|| w.primary_monitor()))
            .flatten();
        if let Some(monitor) = monitor {
            let pos = monitor.position();
            let msize = monitor.size();
            origin = Some((pos.x + (msize.width as i32 - width) / 2, pos.y + margin));
        }
    }
    let (x, y) = origin.unwrap_or((0, margin));

    let geometry = (x, y, width, height);
    if !force && app.st.borrow().last_geometry == Some(geometry) {
        return;
    }
    app.st.borrow_mut().last_geometry = Some(geometry);
    ui.window().set_size(size);
    #[cfg(windows)]
    if let Some(hwnd) = app.st.borrow().hwnd {
        let radius = if ui.get_pill_mode() { 17.0 } else { 7.0 };
        win::round_corners(
            hwnd,
            width as u32,
            height as u32,
            (radius * scale).round() as u32,
        );
    }
    ui.window().set_position(PhysicalPosition::new(x, y));
    reposition_party(app);
}

/// Estilo de overlay para el tooltip y el panel de la party: el mouse los
/// atraviesa y no le sacan el foco al juego.
#[cfg_attr(not(windows), allow(unused_variables))]
fn style_child(app: &App, window: &slint::Window, radius: f32) {
    #[cfg(windows)]
    {
        let alpha = app.st.borrow().alpha;
        let size = window.size();
        let scale = window.scale_factor();
        let _ = window.with_winit_window(|w| {
            if let Some(hwnd) = win::hwnd_of(w) {
                win::make_overlay(hwnd, alpha, true);
                win::round_corners(
                    hwnd,
                    size.width,
                    size.height,
                    (radius * scale).round() as u32,
                );
            }
        });
    }
}

/// Ancho aproximado del tooltip: el renderer por software no expone la
/// medida del texto antes de mostrar la ventana.
fn tooltip_width(info: &TipInfo) -> f32 {
    let main = info.name.chars().count() as f32 * 6.9;
    let alt = if info.alt.is_empty() {
        0.0
    } else {
        (info.alt.chars().count() + 13) as f32 * 6.2
    };
    (main.max(alt).max(120.0) + 26.0).min(460.0)
}

fn show_tooltip(app: &App, index: usize) {
    hide_tooltip(app);
    let Ok(tip) = TooltipWindow::new() else {
        return;
    };
    let size = {
        let st = app.st.borrow();
        let Some(info) = st.tips.get(index) else {
            return;
        };
        tip.set_item_name(info.name.clone().into());
        tip.set_slot_label(info.label.clone().into());
        tip.set_tier(info.tier.clone().into());
        tip.set_tier_color(info.color);
        tip.set_alt_name(info.alt.clone().into());
        let height = if info.alt.is_empty() { 46.0 } else { 62.0 };
        LogicalSize::new(tooltip_width(info), height)
    };
    tip.window().set_size(size);

    // Debajo de la tira, que siempre va arriba del juego.
    let ui = &app.ui;
    let scale = ui.window().scale_factor();
    let main_pos = ui.window().position();
    let main_h = ui.window().size().height as i32;
    let gap = (GAP * scale).round() as i32;
    let offset = STRIP_PAD_LEFT + ROLE_CELL + index as f32 * SLOT_STEP;
    let x = main_pos.x + (offset * scale).round() as i32;
    tip.window()
        .set_position(PhysicalPosition::new(x, main_pos.y + main_h + gap));
    if tip.show().is_err() {
        return;
    }
    style_after_show(app, tip.as_weak(), 6.0);
    app.st.borrow_mut().tooltip = Some(tip);
}

/// La ventana de Windows existe recién después de `show()`.
fn style_after_show<T: ComponentHandle + 'static>(app: &App, weak: slint::Weak<T>, radius: f32) {
    let app_weak = Rc::downgrade(app);
    let _ = slint::spawn_local(async move {
        let Some(component) = weak.upgrade() else {
            return;
        };
        if component.window().winit_window().await.is_err() {
            return;
        }
        if let Some(app) = app_weak.upgrade() {
            style_child(&app, component.window(), radius);
        }
    });
}

fn hide_tooltip(app: &App) {
    let tip = app.st.borrow_mut().tooltip.take();
    if let Some(tip) = tip {
        let _ = tip.hide();
    }
}

/// Orden del panel: tanques, healers, supports, DPS y el resto.
fn role_rank(role: &str) -> u8 {
    let r = role.to_uppercase();
    if r.contains("TANK") {
        0
    } else if r.contains("HEAL") {
        1
    } else if r.contains("SUPPORT") {
        2
    } else if r.contains("MOUNT") {
        5
    } else if r.contains("DPS") || r.contains("RANGE") || r.contains("MELEE") {
        3
    } else {
        6
    }
}

/// Arma (o montura) de cada build asignada: es el ícono de la fila en la party.
fn party_weapon_ids(overlay: &Overlay) -> Vec<String> {
    overlay
        .entries
        .iter()
        .filter_map(|e| build_of(overlay, e))
        .filter_map(member_icon_id)
        .collect()
}

fn member_icon_id(build: &api::Build) -> Option<String> {
    let role = build.role.as_deref().unwrap_or_default();
    data::slot_items(build, role)
        .into_iter()
        .map(|s| s.id)
        .find(|id| !id.is_empty())
}

fn render_party(app: &App) {
    let (heading, subheading, members) = {
        let st = app.st.borrow();
        let Some(overlay) = &st.overlay else {
            drop(st);
            close_party(app);
            return;
        };
        let me = normalize_name(&st.config.name);
        let mut rows: Vec<(u8, String, Member)> = overlay
            .entries
            .iter()
            .map(|e| {
                let build = build_of(overlay, e);
                let role = display_role(build, Some(e));
                let image = build
                    .and_then(member_icon_id)
                    .and_then(|id| st.icons.get(&id).cloned());
                let member = Member {
                    name: e.name.clone().into(),
                    role: if role.is_empty() {
                        "SIN ROL".into()
                    } else {
                        role.clone().into()
                    },
                    role_color: data::role_color(&role),
                    assigned: build.is_some(),
                    me: normalize_name(&e.name) == me,
                    has_image: image.is_some(),
                    image: image.unwrap_or_default(),
                };
                (role_rank(&role), e.name.to_lowercase(), member)
            })
            .collect();
        rows.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let assigned = rows.iter().filter(|r| r.2.assigned).count();
        let heading = overlay
            .assignment
            .name
            .clone()
            .or_else(|| overlay.composition.name.clone())
            .unwrap_or_else(|| "Asignación".into())
            .to_uppercase();
        let subheading = format!("{} anotados · {assigned} con build", rows.len());
        let members: Vec<Member> = rows.into_iter().map(|r| r.2).collect();
        (heading, subheading, members)
    };
    let Some(party) = app
        .st
        .borrow()
        .party
        .as_ref()
        .map(ComponentHandle::clone_strong)
    else {
        return;
    };
    let height = 66.0 + members.len() as f32 * 28.0;
    party.set_heading(heading.into());
    party.set_subheading(subheading.into());
    party.set_members(ModelRc::new(VecModel::from(members)));
    let size = LogicalSize::new(PARTY_WIDTH, height);
    if party.window().size() != size.to_physical(party.window().scale_factor()) {
        party.window().set_size(size);
        style_after_show(app, party.as_weak(), 7.0);
    }
    reposition_party(app);
}

/// El panel va debajo de la tira, alineado a la derecha (donde está el botón).
fn reposition_party(app: &App) {
    let Some(party) = app
        .st
        .borrow()
        .party
        .as_ref()
        .map(ComponentHandle::clone_strong)
    else {
        return;
    };
    let ui = &app.ui;
    let scale = ui.window().scale_factor();
    let pos = ui.window().position();
    let size = ui.window().size();
    let width = (PARTY_WIDTH * scale).round() as i32;
    let gap = (GAP * scale).round() as i32;
    party.window().set_position(PhysicalPosition::new(
        pos.x + size.width as i32 - width,
        pos.y + size.height as i32 + gap,
    ));
}

fn toggle_party(app: &App) {
    if app.st.borrow().party.is_some() {
        close_party(app);
        return;
    }
    if app.st.borrow().overlay.is_none() {
        // Sin asignación cargada no hay nada que mostrar: abre el ingreso.
        open_setup(app);
        return;
    }
    let Ok(party) = PartyWindow::new() else {
        return;
    };
    hide_tooltip(app);
    app.st.borrow_mut().party = Some(party.clone_strong());
    app.ui.set_party_open(true);
    render_party(app);
    if party.show().is_ok() {
        style_after_show(app, party.as_weak(), 7.0);
    }
}

fn close_party(app: &App) {
    app.ui.set_party_open(false);
    let party = app.st.borrow_mut().party.take();
    if let Some(party) = party {
        let _ = party.hide();
    }
}

fn open_setup(app: &App) {
    if let Some(setup) = app.st.borrow().setup.as_ref() {
        let _ = setup.show();
        return;
    }
    NEXT_WINDOW_NORMAL.set(true);
    let setup = SetupWindow::new();
    NEXT_WINDOW_NORMAL.set(false);
    let Ok(setup) = setup else { return };

    {
        let st = app.st.borrow();
        let c = &st.config;
        setup.set_code(if app.demo { "" } else { c.code.as_str() }.into());
        setup.set_name(c.name.clone().into());
        let roles = setup.get_roles();
        let index = (0..slint::Model::row_count(&roles))
            .find(|&i| slint::Model::row_data(&roles, i).is_some_and(|r| r == c.role.as_str()))
            .map_or(-1, |i| i as i32);
        setup.set_role_index(index);
        setup.set_lang_index(c.lang.index());
        setup.set_joined(c.has_assignment() && !app.demo);
        if app.demo {
            setup.set_info_text("Modo demo: no se conecta con AlbionCompo.".into());
        }
    }

    let weak = Rc::downgrade(app);
    setup.on_submit(move || {
        if let Some(app) = weak.upgrade() {
            submit_setup(&app);
        }
    });
    let weak = Rc::downgrade(app);
    setup.on_leave(move || {
        let Some(app) = weak.upgrade() else { return };
        {
            let mut st = app.st.borrow_mut();
            st.config.code.clear();
            if !app.demo {
                let _ = st.config.save();
            }
            st.overlay = None;
            st.phase = Phase::NoCode;
            st.poll_timer.stop();
        }
        close_party(&app);
        close_setup(&app);
        render(&app);
    });
    let weak = Rc::downgrade(app);
    setup.on_close_clicked(move || {
        if let Some(app) = weak.upgrade() {
            close_setup(&app);
        }
    });
    let weak = Rc::downgrade(app);
    setup.window().on_close_requested(move || {
        if let Some(app) = weak.upgrade() {
            close_setup(&app);
        }
        CloseRequestResponse::HideWindow
    });

    if setup.show().is_ok() {
        app.st.borrow_mut().setup = Some(setup);
    }
}

fn submit_setup(app: &App) {
    let Some(setup) = app
        .st
        .borrow()
        .setup
        .as_ref()
        .map(ComponentHandle::clone_strong)
    else {
        return;
    };
    setup.set_error_text("".into());
    let Some(code) = normalize_code(&setup.get_code()) else {
        setup.set_error_text("El código tiene 6 letras y números, por ejemplo D75TGV.".into());
        return;
    };
    let name = setup
        .get_name()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let len = name.chars().count();
    if !(2..=32).contains(&len) {
        setup.set_error_text("Poné tu nombre del juego (entre 2 y 32 caracteres).".into());
        return;
    }
    let index = setup.get_role_index();
    let role = (index >= 0)
        .then(|| slint::Model::row_data(&setup.get_roles(), index as usize))
        .flatten()
        .map(|r| r.to_string())
        .unwrap_or_default();
    let lang = Lang::from_index(setup.get_lang_index());
    let config = Config {
        code: code.clone(),
        name: name.clone(),
        role: role.clone(),
        lang,
    };
    setup.set_code(code.clone().into());
    setup.set_name(name.clone().into());
    setup.set_busy(true);

    if app.demo {
        // Sin servidor: se anota en la asignación de prueba.
        let mut overlay = app.st.borrow().overlay.clone().unwrap_or_default();
        if my_entry(&overlay, &name).is_none() {
            overlay.entries.push(api::Entry {
                name: name.clone(),
                role: (!role.is_empty()).then_some(role),
                build_id: None,
            });
        }
        app.st.borrow_mut().pending = Some(config);
        on_event(app, Event::Joined(Ok(overlay)));
        return;
    }
    app.st.borrow_mut().pending = Some(config);
    let _ = app.tx.send(Command::Join { code, name, role });
}

/// Cierra la ventana de ingreso. Se destruye en la vuelta siguiente del
/// bucle de eventos porque esto puede venir de uno de sus propios callbacks.
fn close_setup(app: &App) {
    let setup = app.st.borrow_mut().setup.take();
    if let Some(setup) = setup {
        let _ = setup.hide();
        Timer::single_shot(Duration::ZERO, move || drop(setup));
    }
}

/// Muestra la tira solo con Albion en primer plano y la mantiene centrada
/// arriba del área del juego (sirve con resoluciones o ventanas modificadas).
#[cfg(windows)]
fn follow_game(app: &App) {
    let game = app.st.borrow_mut().watch.albion_rect();
    let Some(hwnd) = app.st.borrow().hwnd else {
        return;
    };
    match game {
        Some(rect) => {
            let moved = app.st.borrow().game != Some(rect);
            if moved {
                app.st.borrow_mut().game = Some(rect);
                place_main(app, false);
            }
            if !app.st.borrow().visible {
                win::set_visible(hwnd, true);
                app.st.borrow_mut().visible = true;
                if app.st.borrow().poll_due {
                    poll_now(app);
                }
            }
        }
        None => {
            if app.st.borrow().visible {
                app.ui.set_hovered_slot(-1);
                app.st.borrow().hover_timer.stop();
                hide_tooltip(app);
                close_party(app);
                win::set_visible(hwnd, false);
                app.st.borrow_mut().visible = false;
            }
        }
    }
}

#[cfg(windows)]
fn on_hotkey(app: &App, key: win::Hotkey) {
    let ui = &app.ui;
    match key {
        win::Hotkey::ClickThrough => {
            let on = !ui.get_click_through();
            ui.set_click_through(on);
            if on {
                ui.set_hovered_slot(-1);
                app.st.borrow().hover_timer.stop();
                hide_tooltip(app);
            }
            if let Some(hwnd) = app.st.borrow().hwnd {
                win::set_click_through(hwnd, on);
            }
        }
        win::Hotkey::PillMode => {
            ui.set_pill_mode(!ui.get_pill_mode());
            ui.set_hovered_slot(-1);
            app.st.borrow().hover_timer.stop();
            hide_tooltip(app);
            place_main(app, true);
        }
        win::Hotkey::Party => toggle_party(app),
        win::Hotkey::Setup => open_setup(app),
        win::Hotkey::Quit => {
            let _ = slint::quit_event_loop();
        }
    }
}
