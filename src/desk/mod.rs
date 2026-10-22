//! App de escritorio: una ventana con secciones (Inicio, Compos, Ajustes) que
//! convive con el overlay. La ventana se crea al abrirla y se destruye al
//! mandarla a la bandeja, así no ocupa memoria mientras se juega; lo que la
//! persona tenía abierto queda en `DeskState`.
//!
//! Cada sección tiene su módulo acá y su vista en `ui/desk/`. Para sumar una:
//! agregarla a `Section` y `SECTIONS`, su texto en `texts.rs`, su vista y su
//! global en `ui/desk/`, y llamar a su `wire`/`render` desde este módulo.

pub mod compos;
mod home;
mod settings;
pub mod texts;

use std::rc::Rc;
use std::time::Duration;

use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::window::ResizeDirection;
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

use super::{App, AppWindow, DeskShell, DeskTr, NavItem, WindowKind};
use crate::comps::CompsState;

/// Cada cuánto se mira si Albion está abierto (una lista de procesos: barato).
const GAME_POLL: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Home,
    Compos,
    Settings,
}

/// Orden de la barra lateral (tiene que coincidir con ui/desk/app.slint).
const SECTIONS: [Section; 3] = [Section::Home, Section::Compos, Section::Settings];

impl Section {
    fn icon(self) -> &'static str {
        match self {
            Section::Home => "M4 11l8-6.5 8 6.5M6 9.5V20h4.5v-5.5h3V20H18V9.5",
            Section::Compos => "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
            Section::Settings => {
                "M4 7h9M17 7h3M15 5v4M4 12h3M11 12h9M9 10v4M4 17h11M19 17h1M17 15v4"
            }
        }
    }

    fn index(self) -> i32 {
        SECTIONS.iter().position(|s| *s == self).unwrap_or(0) as i32
    }
}

pub struct DeskState {
    pub window: Option<AppWindow>,
    section: Section,
    game_running: bool,
    /// `--juego`: simula que Albion está abierto (para probar sin el juego).
    fake_game: bool,
    game_timer: Timer,
    pub compos: CompsState,
    /// La sección Compos ya pidió la primera página.
    compos_loaded: bool,
}

impl DeskState {
    pub fn new(fake_game: bool) -> DeskState {
        DeskState {
            window: None,
            section: Section::Home,
            game_running: fake_game,
            fake_game,
            game_timer: Timer::default(),
            compos: CompsState::default(),
            compos_loaded: false,
        }
    }
}

/// Arranca el control del juego y abre la ventana, salvo que la persona haya
/// elegido arrancar en la bandeja.
pub fn start(app: &App, in_tray: bool) {
    check_game(app);
    let weak = Rc::downgrade(app);
    app.st
        .borrow()
        .desk
        .game_timer
        .start(TimerMode::Repeated, GAME_POLL, move || {
            if let Some(app) = weak.upgrade() {
                check_game(&app);
            }
        });
    let start_in_tray = in_tray || app.st.borrow().config.start_in_tray;
    if !start_in_tray {
        show(app);
    }
}

fn albion_running(app: &App) -> bool {
    if app.st.borrow().desk.fake_game {
        return true;
    }
    #[cfg(windows)]
    return crate::win::albion_running();
    #[cfg(not(windows))]
    false
}

fn check_game(app: &App) {
    let running = albion_running(app);
    let changed = {
        let mut st = app.st.borrow_mut();
        let changed = st.desk.game_running != running;
        st.desk.game_running = running;
        changed
    };
    if !changed {
        return;
    }
    let (window, keep) = {
        let st = app.st.borrow();
        (
            st.desk.window.as_ref().map(ComponentHandle::clone_strong),
            st.config.keep_on_game,
        )
    };
    if let Some(window) = window {
        window.global::<DeskShell>().set_game_running(running);
        // Arrancó el juego: la ventana se va a la bandeja y queda el overlay.
        if running && !keep {
            hide(app);
        }
    }
}

pub fn show(app: &App) {
    if let Some(window) = app.st.borrow().desk.window.as_ref() {
        let _ = window.show();
        bring_to_front(window);
        return;
    }
    super::NEXT_WINDOW.set(WindowKind::Desk);
    let window = AppWindow::new();
    super::NEXT_WINDOW.set(WindowKind::Overlay);
    let Ok(window) = window else { return };

    // Con la app abierta, el idioma se elige en Ajustes (no hace falta el
    // paso de idioma de la ventana de ingreso).
    {
        let mut st = app.st.borrow_mut();
        if !st.config.lang_chosen {
            st.config.lang_chosen = true;
            if !app.demo {
                let _ = st.config.save();
            }
        }
    }

    wire(app, &window);
    app.st.borrow_mut().desk.window = Some(window.clone_strong());
    apply_texts(app);
    render(app);
    if window.show().is_err() {
        app.st.borrow_mut().desk.window = None;
        return;
    }
    let weak = window.as_weak();
    let _ = slint::spawn_local(async move {
        let Some(window) = weak.upgrade() else { return };
        if window.window().winit_window().await.is_err() {
            return;
        }
        #[cfg(windows)]
        let _ = window.window().with_winit_window(|w| {
            if let Some(hwnd) = crate::win::hwnd_of(w) {
                crate::win::style_desk(hwnd);
            }
        });
        bring_to_front(&window);
    });
    let section = app.st.borrow().desk.section;
    if section == Section::Compos {
        compos::ensure_loaded(app);
    }
}

fn bring_to_front(window: &AppWindow) {
    #[cfg(windows)]
    let _ = window.window().with_winit_window(|w| {
        if let Some(hwnd) = crate::win::hwnd_of(w) {
            crate::win::bring_to_front(hwnd);
        }
    });
    #[cfg(not(windows))]
    let _ = window.window().with_winit_window(|w| w.focus_window());
}

/// Manda la ventana a la bandeja. Se destruye en la vuelta siguiente del
/// bucle de eventos porque esto puede venir de uno de sus callbacks.
pub fn hide(app: &App) {
    let window = app.st.borrow_mut().desk.window.take();
    if let Some(window) = window {
        let _ = window.hide();
        Timer::single_shot(Duration::ZERO, move || drop(window));
    }
}

fn close_requested(app: &App) {
    if app.st.borrow().config.quit_on_close {
        super::quit(app);
    } else {
        hide(app);
    }
}

pub fn navigate(app: &App, section: Section) {
    app.st.borrow_mut().desk.section = section;
    if let Some(window) = app.st.borrow().desk.window.as_ref() {
        window.global::<DeskShell>().set_page(section.index());
    }
    if section == Section::Compos {
        compos::ensure_loaded(app);
    }
}

fn wire(app: &App, window: &AppWindow) {
    let shell = window.global::<DeskShell>();
    let weak = Rc::downgrade(app);
    shell.on_navigate(move |index| {
        if let (Some(app), Some(section)) = (weak.upgrade(), SECTIONS.get(index as usize)) {
            navigate(&app, *section);
        }
    });
    let handle = window.as_weak();
    shell.on_minimize(move || {
        if let Some(w) = handle.upgrade() {
            let _ = w.window().with_winit_window(|w| w.set_minimized(true));
        }
    });
    let handle = window.as_weak();
    shell.on_toggle_maximize(move || {
        if let Some(w) = handle.upgrade() {
            let maximized = w
                .window()
                .with_winit_window(|win| {
                    let next = !win.is_maximized();
                    win.set_maximized(next);
                    next
                })
                .unwrap_or(false);
            w.global::<DeskShell>().set_maximized(maximized);
        }
    });
    let handle = window.as_weak();
    shell.on_drag(move || {
        if let Some(w) = handle.upgrade() {
            let _ = w.window().with_winit_window(|w| w.drag_window());
        }
    });
    let handle = window.as_weak();
    shell.on_resize(move |direction| {
        let direction = match direction {
            0 => ResizeDirection::North,
            1 => ResizeDirection::South,
            2 => ResizeDirection::East,
            3 => ResizeDirection::West,
            4 => ResizeDirection::NorthEast,
            5 => ResizeDirection::NorthWest,
            6 => ResizeDirection::SouthEast,
            _ => ResizeDirection::SouthWest,
        };
        if let Some(w) = handle.upgrade() {
            let _ = w
                .window()
                .with_winit_window(|w| w.drag_resize_window(direction));
        }
    });
    let weak = Rc::downgrade(app);
    shell.on_close(move || {
        if let Some(app) = weak.upgrade() {
            close_requested(&app);
        }
    });
    let weak = Rc::downgrade(app);
    window.window().on_close_requested(move || {
        if let Some(app) = weak.upgrade() {
            close_requested(&app);
        }
        CloseRequestResponse::KeepWindowShown
    });

    home::wire(app, window);
    compos::wire(app, window);
    settings::wire(app, window);
}

/// Textos fijos y la barra lateral, en el idioma elegido.
pub fn apply_texts(app: &App) {
    let Some(window) = app
        .st
        .borrow()
        .desk
        .window
        .as_ref()
        .map(|w| w.clone_strong())
    else {
        return;
    };
    let lang = app.st.borrow().config.lang;
    let t = texts::text(lang);
    let nav: Vec<NavItem> = SECTIONS
        .iter()
        .map(|s| NavItem {
            label: t.nav[s.index() as usize].into(),
            icon: s.icon().into(),
        })
        .collect();
    let shell = window.global::<DeskShell>();
    shell.set_nav(ModelRc::new(VecModel::from(nav)));
    shell.set_version(concat!("v", env!("CARGO_PKG_VERSION")).into());

    let tr = window.global::<DeskTr>();
    tr.set_home_title(t.home_title.into());
    tr.set_home_subtitle(t.home_subtitle.into());
    tr.set_game_title(t.game_title.into());
    tr.set_game_running(t.game_running.into());
    tr.set_game_closed(t.game_closed.into());
    tr.set_game_help_running(t.game_help_running.into());
    tr.set_game_help_closed(t.game_help_closed.into());
    tr.set_assign_title(t.assign_title.into());
    tr.set_assign_none(t.assign_none.into());
    tr.set_assign_enter(t.assign_enter.into());
    tr.set_assign_change(t.assign_change.into());
    tr.set_quick_title(t.quick_title.into());
    tr.set_quick_compos(t.quick_compos.into());
    tr.set_quick_site(t.quick_site.into());
    tr.set_hotkeys_title(t.hotkeys_title.into());
    tr.set_tab_public(t.tab_public.into());
    tr.set_tab_code(t.tab_code.into());
    tr.set_search_placeholder(t.search_placeholder.into());
    tr.set_search(t.search.into());
    tr.set_code_placeholder(t.code_placeholder.into());
    tr.set_open(t.open.into());
    tr.set_more(t.more.into());
    tr.set_back(t.back.into());
    tr.set_private(t.private.into());
    tr.set_view_overlay(t.view_overlay.into());
    tr.set_in_overlay(t.in_overlay.into());
    tr.set_open_web(t.open_web.into());
    tr.set_overlay_hint(t.overlay_hint.into());
    tr.set_settings_lang(t.settings_lang.into());
    tr.set_settings_app(t.settings_app.into());
    tr.set_opt_start_window(t.opt_start_window.into());
    tr.set_opt_start_window_help(t.opt_start_window_help.into());
    tr.set_opt_hide_on_game(t.opt_hide_on_game.into());
    tr.set_opt_hide_on_game_help(t.opt_hide_on_game_help.into());
    tr.set_opt_close_tray(t.opt_close_tray.into());
    tr.set_opt_close_tray_help(t.opt_close_tray_help.into());
    tr.set_settings_overlay(t.settings_overlay.into());
    tr.set_opt_pin_strip(t.opt_pin_strip.into());
    tr.set_opt_pin_party(t.opt_pin_party.into());
    tr.set_settings_about(t.settings_about.into());
    tr.set_about_text(t.about_text.into());
}

/// Pasa todo el estado a la ventana (si está abierta).
pub fn render(app: &App) {
    let Some(window) = app
        .st
        .borrow()
        .desk
        .window
        .as_ref()
        .map(|w| w.clone_strong())
    else {
        return;
    };
    {
        let st = app.st.borrow();
        let shell = window.global::<DeskShell>();
        shell.set_page(st.desk.section.index());
        shell.set_game_running(st.desk.game_running);
    }
    home::render(app, &window);
    compos::render(app);
    settings::render(app, &window);
}

/// Cambió el idioma: textos fijos y todo lo armado desde Rust.
pub fn on_lang(app: &App) {
    apply_texts(app);
    render(app);
}

/// Cambió la asignación o su estado: solo el Inicio depende de eso.
pub fn on_overlay(app: &App) {
    let Some(window) = app
        .st
        .borrow()
        .desk
        .window
        .as_ref()
        .map(|w| w.clone_strong())
    else {
        return;
    };
    home::render(app, &window);
}
