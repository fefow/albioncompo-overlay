// Sin consola en Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod api;
mod assets;
mod config;
mod data;
mod i18n;
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
/// Botón flotante de AlbionCompo, en el borde derecho del juego.
const LAUNCHER_SIZE: f32 = 48.0;
const LAUNCHER_MARGIN: f32 = 10.0;
/// Altura del botón, como fracción del alto del juego (arriba del minimapa).
const LAUNCHER_HEIGHT: f32 = 0.32;
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
    /// Código validado en el primer paso del ingreso y su asignación.
    checked: Option<(String, Overlay)>,
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
    /// Albion en primer plano (o `--siempre`).
    #[cfg_attr(not(windows), allow(dead_code))]
    game_active: bool,
    /// El jugador lo ocultó desde la bandeja o con Ctrl+Shift+H.
    #[cfg_attr(not(windows), allow(dead_code))]
    user_hidden: bool,
    /// El panel de la party se abrió desde el botón flotante (va al lado de él).
    party_by_launcher: bool,
    #[cfg_attr(not(windows), allow(dead_code))]
    alpha: u8,
    #[cfg(windows)]
    launcher_hwnd: Option<win::Hwnd>,
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
    launcher: LauncherWindow,
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
    let launcher = LauncherWindow::new()?;

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
        launcher,
        st: RefCell::new(State {
            config,
            pending: None,
            checked: None,
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
            game_active: args.siempre,
            user_hidden: false,
            party_by_launcher: false,
            alpha: args.alpha,
            #[cfg(windows)]
            launcher_hwnd: None,
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

    apply_tr(app.ui.global::<Tr>(), app.st.borrow().config.lang);
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
            let weak = Rc::downgrade(&app);
            win::add_tray(hwnd, "AlbionCompo overlay", move |event| {
                let Some(app) = weak.upgrade() else { return };
                match event {
                    win::TrayEvent::Open => open_setup(&app),
                    win::TrayEvent::Menu => show_menu(&app),
                }
            });
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

    // Botón flotante: sin foco como la tira, pero siempre se puede clickear.
    app.launcher.show()?;
    let weak = Rc::downgrade(&app);
    slint::spawn_local(async move {
        let Some(app) = weak.upgrade() else { return };
        let Ok(winit_window) = app.launcher.window().winit_window().await else {
            return;
        };
        #[cfg(windows)]
        if let Some(hwnd) = win::hwnd_of(&winit_window) {
            let alpha = app.st.borrow().alpha;
            win::make_overlay(hwnd, alpha, false);
            app.st.borrow_mut().launcher_hwnd = Some(hwnd);
            let visible = app.st.borrow().visible;
            win::set_visible(hwnd, visible);
        }
        drop(winit_window);
        place_launcher(&app);
    })
    .expect("el bucle de eventos de Slint no está disponible");

    let result = app.ui.run();
    #[cfg(windows)]
    if let Some(hwnd) = app.st.borrow().hwnd {
        win::remove_tray(hwnd);
    }
    result
}

fn wire_callbacks(app: &App) {
    let weak = Rc::downgrade(app);
    app.launcher.on_clicked(move || {
        let Some(app) = weak.upgrade() else { return };
        let phase = app.st.borrow().phase;
        match phase {
            Phase::NoCode | Phase::Invalid | Phase::Expired | Phase::NotConfigured => {
                open_setup(&app)
            }
            _ => toggle_party(&app, true),
        }
    });
    let weak = Rc::downgrade(app);
    app.launcher.on_right_clicked(move || {
        if let Some(app) = weak.upgrade() {
            show_menu(&app);
        }
    });

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
            toggle_party(&app, false);
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

fn status_for(lang: Lang, phase: Phase) -> (&'static str, Color) {
    let t = i18n::text(lang);
    let muted = Color::from_rgb_u8(0xa8, 0x96, 0x79);
    let gold = Color::from_rgb_u8(0xd6, 0xad, 0x5e);
    let warn = Color::from_rgb_u8(0xff, 0xb4, 0x54);
    let error = Color::from_rgb_u8(0xff, 0x7a, 0x6b);
    match phase {
        Phase::NoCode => (t.st_no_code, gold),
        Phase::Loading => (t.st_loading, muted),
        Phase::Waiting => (t.st_waiting, muted),
        Phase::NotJoined => (t.st_not_joined, warn),
        Phase::Assigned => ("", muted),
        Phase::Expired => (t.st_expired, warn),
        Phase::Invalid => (t.st_invalid, error),
        Phase::Offline => (t.st_offline, warn),
        Phase::RateLimited => (t.st_rate_limited, warn),
        Phase::NotConfigured => (t.st_not_configured, error),
    }
}

/// Color del rol (con varios roles, el del primero).
fn role_color(role: &str) -> Color {
    data::role_color(i18n::split_roles(role).first().copied().unwrap_or(""))
}

/// Pasa los textos del idioma elegido a una ventana.
fn apply_tr(tr: Tr<'_>, lang: Lang) {
    let t = i18n::text(lang);
    tr.set_code_title(t.code_title.into());
    tr.set_code_help(t.code_help.into());
    tr.set_code_label(t.code_label.into());
    tr.set_continue(t.continue_.into());
    tr.set_close(t.close.into());
    tr.set_back(t.back.into());
    tr.set_register_title(t.register_title.into());
    tr.set_register_help(t.register_help.into());
    tr.set_name_label(t.name_label.into());
    tr.set_name_placeholder(t.name_placeholder.into());
    tr.set_roles_label(t.roles_label.into());
    tr.set_join(t.join.into());
    tr.set_leave(t.leave.into());
    tr.set_lang_label(t.lang_label.into());
    let roles: Vec<slint::SharedString> = t.roles.iter().map(|r| (*r).into()).collect();
    tr.set_roles(ModelRc::new(VecModel::from(roles)));
    tr.set_no_role(t.no_role.into());
    tr.set_alternative(t.alternative.into());
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
    let (slots, tips, role, build_name, status, count, lang) = {
        let st = app.st.borrow();
        let lang = st.config.lang;
        let t = i18n::text(lang);
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
                        t.no_item.into()
                    } else {
                        name_of(&item.id)
                    },
                    label: t.slots[item.kind as usize].into(),
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
            status_for(lang, Phase::Waiting)
        } else {
            status_for(lang, st.phase)
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
        (slots, tips, role, build_name, status, count, lang)
    };

    let ui = &app.ui;
    // En la píldora va el primer casillero con ítem: la montura o el arma.
    let no_item = i18n::text(lang).no_item;
    if let Some(main) = tips.iter().find(|t| t.name != no_item) {
        ui.set_main_name(main.name.clone().into());
        ui.set_main_tier(main.tier.clone().into());
        ui.set_main_tier_color(main.color);
    }
    ui.set_role_color(role_color(&role));
    // "BATTLE MOUNT" no entra en la celda del rol: va la versión corta.
    ui.set_role(i18n::role_label(lang, &role, true).into());
    ui.set_loading(app.st.borrow().phase == Phase::Loading);
    ui.set_build_name(build_name.into());
    ui.set_status_text(status.0.into());
    ui.set_status_color(status.1);
    ui.set_party_count(count.into());
    let badge = {
        let st = app.st.borrow();
        match st.phase {
            Phase::NoCode => None,
            Phase::Assigned => Some(role_color(&role)),
            Phase::Loading | Phase::Waiting => Some(Color::from_rgb_u8(0xd6, 0xad, 0x5e)),
            _ => Some(Color::from_rgb_u8(0xff, 0xb4, 0x54)),
        }
    };
    app.launcher.set_show_badge(badge.is_some());
    if let Some(color) = badge {
        app.launcher.set_badge_color(color);
    }
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
        Event::Checked(code, result) => on_checked(app, code, result),
        Event::Joined(Err(status)) => {
            app.st.borrow_mut().pending = None;
            let lang = app.st.borrow().config.lang;
            if let Some(setup) = current_setup(app) {
                setup.set_busy(false);
                setup.set_error_text(i18n::error(lang, &status).into());
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
    apply_tr(tip.global::<Tr>(), app.st.borrow().config.lang);
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

/// Orden del panel: tanques, healers, supports, DPS, monturas y el resto.
fn role_rank(role: &str) -> usize {
    i18n::split_roles(role)
        .first()
        .and_then(|r| i18n::role_index(r))
        .unwrap_or(6)
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
        let lang = st.config.lang;
        let t = i18n::text(lang);
        let me = normalize_name(&st.config.name);
        let mut rows: Vec<(usize, String, Member)> = overlay
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
                        t.no_role.into()
                    } else {
                        i18n::role_label(lang, &role, true).into()
                    },
                    role_color: role_color(&role),
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
            .unwrap_or_else(|| t.assignment.into())
            .to_uppercase();
        let subheading = t
            .party_summary
            .replace("{n}", &rows.len().to_string())
            .replace("{m}", &assigned.to_string());
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
    let width = (PARTY_WIDTH * scale).round() as i32;
    let gap = (GAP * scale).round() as i32;
    if app.st.borrow().party_by_launcher {
        // A la izquierda del botón flotante.
        let pos = app.launcher.window().position();
        party
            .window()
            .set_position(PhysicalPosition::new(pos.x - width - gap, pos.y));
        return;
    }
    let pos = ui.window().position();
    let size = ui.window().size();
    party.window().set_position(PhysicalPosition::new(
        pos.x + size.width as i32 - width,
        pos.y + size.height as i32 + gap,
    ));
}

fn toggle_party(app: &App, from_launcher: bool) {
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
    {
        let mut st = app.st.borrow_mut();
        st.party = Some(party.clone_strong());
        st.party_by_launcher = from_launcher;
    }
    app.ui.set_party_open(true);
    app.launcher.set_active(true);
    render_party(app);
    if party.show().is_ok() {
        style_after_show(app, party.as_weak(), 7.0);
    }
}

fn close_party(app: &App) {
    app.ui.set_party_open(false);
    app.launcher.set_active(false);
    let party = app.st.borrow_mut().party.take();
    if let Some(party) = party {
        let _ = party.hide();
    }
}

fn current_setup(app: &App) -> Option<SetupWindow> {
    app.st
        .borrow()
        .setup
        .as_ref()
        .map(ComponentHandle::clone_strong)
}

/// Roles guardados ("TANK, HEALER") como bits de la ventana de ingreso.
fn roles_to_mask(role: &str) -> i32 {
    i18n::split_roles(role)
        .into_iter()
        .filter_map(i18n::role_index)
        .fold(0, |mask, i| mask | (1 << i))
}

fn mask_to_roles(mask: i32) -> String {
    i18n::ROLES
        .iter()
        .enumerate()
        .filter(|(i, _)| mask & (1 << i) != 0)
        .map(|(_, r)| *r)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Tarjeta del segundo paso: nombre de la asignación, composición y anotados.
fn fill_assignment_card(setup: &SetupWindow, overlay: &Overlay, lang: Lang) {
    let t = i18n::text(lang);
    let assignment = overlay.assignment.name.clone();
    let composition = overlay.composition.name.clone();
    let title = assignment
        .clone()
        .or_else(|| composition.clone())
        .unwrap_or_else(|| t.assignment.into());
    let members = t.members.replace("{n}", &overlay.entries.len().to_string());
    let detail = match composition.filter(|_| assignment.is_some()) {
        Some(c) => format!("{c} · {members}"),
        None => members,
    };
    setup.set_assignment_name(title.into());
    setup.set_assignment_detail(detail.into());
}

fn open_setup(app: &App) {
    if let Some(setup) = current_setup(app) {
        let _ = setup.show();
        return;
    }
    NEXT_WINDOW_NORMAL.set(true);
    let setup = SetupWindow::new();
    NEXT_WINDOW_NORMAL.set(false);
    let Ok(setup) = setup else { return };

    {
        let mut st = app.st.borrow_mut();
        let c = st.config.clone();
        apply_tr(setup.global::<Tr>(), c.lang);
        setup.set_code(c.code.clone().into());
        setup.set_name(c.name.clone().into());
        setup.set_role_mask(roles_to_mask(&c.role));
        setup.set_lang_index(c.lang.index());
        if app.demo {
            setup.set_info_text(i18n::text(c.lang).demo.into());
        }
        // Con la asignación ya cargada se abre directo en el registro.
        let loaded = st.overlay.clone().filter(|_| !c.code.is_empty());
        if let Some(overlay) = loaded {
            setup.set_joined(my_entry(&overlay, &c.name).is_some());
            fill_assignment_card(&setup, &overlay, c.lang);
            setup.set_step(1);
            st.checked = Some((c.code.clone(), overlay));
        }
    }

    let weak = Rc::downgrade(app);
    setup.on_check_code(move || {
        if let Some(app) = weak.upgrade() {
            check_code(&app);
        }
    });
    let weak = Rc::downgrade(app);
    setup.on_submit(move || {
        if let Some(app) = weak.upgrade() {
            submit_setup(&app);
        }
    });
    let weak = Rc::downgrade(app);
    setup.on_back(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(setup) = current_setup(&app) {
            setup.set_error_text("".into());
            setup.set_step(0);
            setup.invoke_focus_code();
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
            st.checked = None;
            st.phase = Phase::NoCode;
            st.poll_timer.stop();
        }
        close_party(&app);
        close_setup(&app);
        render(&app);
    });
    let weak = Rc::downgrade(app);
    setup.on_lang_changed(move |index| {
        if let Some(app) = weak.upgrade() {
            set_lang(&app, Lang::from_index(index));
        }
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
        if setup.get_step() == 0 {
            setup.invoke_focus_code();
        } else {
            setup.invoke_focus_name();
        }
        app.st.borrow_mut().setup = Some(setup);
    }
}

/// Primer paso: valida el formato y pregunta al servidor si el código existe.
fn check_code(app: &App) {
    let Some(setup) = current_setup(app) else {
        return;
    };
    let lang = app.st.borrow().config.lang;
    setup.set_error_text("".into());
    let Some(code) = normalize_code(&setup.get_code()) else {
        setup.set_error_text(i18n::text(lang).bad_code.into());
        return;
    };
    setup.set_code(code.clone().into());
    setup.set_busy(true);
    if app.demo {
        let overlay = app
            .st
            .borrow()
            .overlay
            .clone()
            .unwrap_or_else(|| data::demo_overlay(false));
        on_event(app, Event::Checked(code, Ok(overlay)));
        return;
    }
    let _ = app.tx.send(Command::Check { code });
}

fn on_checked(app: &App, code: String, result: Result<Overlay, String>) {
    let Some(setup) = current_setup(app) else {
        return;
    };
    setup.set_busy(false);
    let config = app.st.borrow().config.clone();
    match result {
        Ok(overlay) => {
            let name = setup.get_name();
            setup.set_joined(config.code == code && my_entry(&overlay, &name).is_some());
            fill_assignment_card(&setup, &overlay, config.lang);
            app.st.borrow_mut().checked = Some((code, overlay));
            setup.set_error_text("".into());
            setup.set_step(1);
            setup.invoke_focus_name();
        }
        Err(status) => setup.set_error_text(i18n::error(config.lang, &status).into()),
    }
}

/// Segundo paso: se anota con nombre y roles.
fn submit_setup(app: &App) {
    let Some(setup) = current_setup(app) else {
        return;
    };
    let (lang, checked) = {
        let st = app.st.borrow();
        (st.config.lang, st.checked.clone())
    };
    setup.set_error_text("".into());
    let Some((code, checked)) = checked else {
        setup.set_step(0);
        return;
    };
    let name = setup
        .get_name()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let len = name.chars().count();
    if !(2..=32).contains(&len) {
        setup.set_error_text(i18n::text(lang).bad_name.into());
        return;
    }
    let role = mask_to_roles(setup.get_role_mask());
    let config = Config {
        code: code.clone(),
        name: name.clone(),
        role: role.clone(),
        lang,
    };
    setup.set_name(name.clone().into());
    setup.set_busy(true);
    app.st.borrow_mut().pending = Some(config);

    if app.demo {
        // Sin servidor: se anota en la asignación de prueba.
        let mut overlay = checked;
        if my_entry(&overlay, &name).is_none() {
            overlay.entries.push(api::Entry {
                name,
                role: (!role.is_empty()).then_some(role),
                build_id: None,
            });
        }
        on_event(app, Event::Joined(Ok(overlay)));
        return;
    }
    let _ = app.tx.send(Command::Join { code, name, role });
}

/// Cambia el idioma de la interfaz y de los nombres de ítems, al momento.
fn set_lang(app: &App, lang: Lang) {
    {
        let mut st = app.st.borrow_mut();
        if st.config.lang == lang {
            return;
        }
        st.config.lang = lang;
        st.names.clear();
        st.requested.clear();
        if let Some(pending) = st.pending.as_mut() {
            pending.lang = lang;
        }
        if !app.demo {
            let _ = st.config.save();
        }
    }
    apply_tr(app.ui.global::<Tr>(), lang);
    let st = app.st.borrow();
    if let Some(tip) = &st.tooltip {
        apply_tr(tip.global::<Tr>(), lang);
    }
    if let Some(setup) = &st.setup {
        apply_tr(setup.global::<Tr>(), lang);
        setup.set_error_text("".into());
        if app.demo {
            setup.set_info_text(i18n::text(lang).demo.into());
        }
        if let Some((_, overlay)) = &st.checked {
            fill_assignment_card(setup, overlay, lang);
        }
    }
    drop(st);
    render(app);
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

/// Sigue al juego: la tira y el botón se ven solo con Albion en primer plano
/// y se mantienen dentro de su área (sirve con resoluciones o ventanas modificadas).
#[cfg(windows)]
fn follow_game(app: &App) {
    let game = app.st.borrow_mut().watch.albion_rect();
    if let Some(rect) = game {
        let moved = app.st.borrow().game != Some(rect);
        if moved {
            app.st.borrow_mut().game = Some(rect);
            place_main(app, false);
            place_launcher(app);
        }
    }
    app.st.borrow_mut().game_active = game.is_some();
    apply_visibility(app);
}

/// Muestra u oculta la tira y el botón según el juego y lo que eligió el jugador.
#[cfg_attr(not(windows), allow(dead_code))]
fn apply_visibility(app: &App) {
    let (show, was) = {
        let st = app.st.borrow();
        (st.game_active && !st.user_hidden, st.visible)
    };
    if show == was {
        return;
    }
    if !show {
        app.ui.set_hovered_slot(-1);
        app.st.borrow().hover_timer.stop();
        hide_tooltip(app);
        close_party(app);
    }
    #[cfg(windows)]
    {
        let st = app.st.borrow();
        for hwnd in [st.hwnd, st.launcher_hwnd].into_iter().flatten() {
            win::set_visible(hwnd, show);
        }
    }
    app.st.borrow_mut().visible = show;
    if show && app.st.borrow().poll_due {
        poll_now(app);
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn toggle_hidden(app: &App) {
    let hidden = !app.st.borrow().user_hidden;
    app.st.borrow_mut().user_hidden = hidden;
    apply_visibility(app);
}

#[cfg_attr(not(windows), allow(dead_code))]
fn toggle_click_through(app: &App) {
    let ui = &app.ui;
    let on = !ui.get_click_through();
    ui.set_click_through(on);
    if on {
        ui.set_hovered_slot(-1);
        app.st.borrow().hover_timer.stop();
        hide_tooltip(app);
    }
    #[cfg(windows)]
    if let Some(hwnd) = app.st.borrow().hwnd {
        win::set_click_through(hwnd, on);
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn toggle_pill(app: &App) {
    let ui = &app.ui;
    ui.set_pill_mode(!ui.get_pill_mode());
    ui.set_hovered_slot(-1);
    app.st.borrow().hover_timer.stop();
    hide_tooltip(app);
    place_main(app, true);
}

#[cfg_attr(not(windows), allow(dead_code))]
fn quit(app: &App) {
    #[cfg(windows)]
    if let Some(hwnd) = app.st.borrow().hwnd {
        win::remove_tray(hwnd);
    }
    #[cfg(not(windows))]
    let _ = app;
    let _ = slint::quit_event_loop();
}

/// Botón flotante en el borde derecho del juego (o del monitor).
fn place_launcher(app: &App) {
    let window = app.launcher.window();
    let scale = window.scale_factor();
    let size = (LAUNCHER_SIZE * scale).round() as i32;
    let margin = (LAUNCHER_MARGIN * scale).round() as i32;
    let mut area = None;
    #[cfg(windows)]
    if let Some(g) = app.st.borrow().game {
        area = Some((g.x, g.y, g.width, g.height));
    }
    if area.is_none() {
        area = window
            .with_winit_window(|w| w.current_monitor().or_else(|| w.primary_monitor()))
            .flatten()
            .map(|m| {
                let (p, s) = (m.position(), m.size());
                (p.x, p.y, s.width as i32, s.height as i32)
            });
    }
    let Some((x, y, w, h)) = area else { return };
    window.set_size(LogicalSize::new(LAUNCHER_SIZE, LAUNCHER_SIZE));
    window.set_position(PhysicalPosition::new(
        x + w - size - margin,
        y + (h as f32 * LAUNCHER_HEIGHT) as i32,
    ));
    #[cfg(windows)]
    if let Some(hwnd) = app.st.borrow().launcher_hwnd {
        win::round_corners(hwnd, size as u32, size as u32, (size / 2) as u32);
    }
    reposition_party(app);
}

/// Menú de la bandeja y del clic derecho en el botón flotante.
fn show_menu(app: &App) {
    #[cfg(windows)]
    {
        const OPEN: u32 = 1;
        const PARTY: u32 = 2;
        const HIDE: u32 = 3;
        const CLICK_THROUGH: u32 = 4;
        const PILL: u32 = 5;
        const QUIT: u32 = 6;
        let Some(hwnd) = app.st.borrow().hwnd else {
            return;
        };
        let (t, has_overlay, hidden) = {
            let st = app.st.borrow();
            (
                i18n::text(st.config.lang),
                st.overlay.is_some(),
                st.user_hidden,
            )
        };
        let item = |id: u32, text: &str, checked: bool, enabled: bool| win::MenuItem {
            id,
            text: text.to_owned(),
            checked,
            enabled,
        };
        let items = [
            item(OPEN, t.menu_open, false, true),
            item(PARTY, t.menu_party, false, has_overlay),
            item(0, "", false, true),
            item(HIDE, t.menu_hide, hidden, true),
            item(
                CLICK_THROUGH,
                t.menu_click_through,
                app.ui.get_click_through(),
                true,
            ),
            item(PILL, t.menu_pill, app.ui.get_pill_mode(), true),
            item(0, "", false, true),
            item(QUIT, t.menu_quit, false, true),
        ];
        match win::popup_menu(hwnd, &items) {
            Some(OPEN) => open_setup(app),
            Some(PARTY) => {
                if app.st.borrow().user_hidden {
                    toggle_hidden(app);
                }
                toggle_party(app, true);
            }
            Some(HIDE) => toggle_hidden(app),
            Some(CLICK_THROUGH) => toggle_click_through(app),
            Some(PILL) => toggle_pill(app),
            Some(QUIT) => quit(app),
            _ => {}
        }
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
fn on_hotkey(app: &App, key: win::Hotkey) {
    match key {
        win::Hotkey::ClickThrough => toggle_click_through(app),
        win::Hotkey::PillMode => toggle_pill(app),
        win::Hotkey::Party => toggle_party(app, false),
        win::Hotkey::Setup => open_setup(app),
        win::Hotkey::Hide => toggle_hidden(app),
        win::Hotkey::Quit => quit(app),
    }
}
