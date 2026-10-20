// Sin consola en Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod data;
#[cfg(windows)]
mod win;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::window::{WindowAttributes, WindowLevel};
use slint::{
    ComponentHandle, LogicalSize, Model, ModelRc, PhysicalPosition, Timer, TimerMode, VecModel,
};

slint::include_modules!();

/// El tooltip aparece si el mouse se queda este tiempo sobre un casillero.
const TOOLTIP_DELAY: Duration = Duration::from_millis(300);
/// Separación entre la tira y el tooltip, y entre la tira y el borde de la pantalla.
const GAP: f32 = 6.0;
const TOP_MARGIN: f32 = 8.0;
const SLOT_STEP: f32 = 42.0;
const STRIP_PAD_LEFT: f32 = 9.0;

struct Args {
    montura: bool,
    pildora: bool,
    alpha: u8,
}

/// `--montura` simula el rol de montura de batalla (9 casilleros).
/// `--pildora` arranca en modo píldora. `--opacidad N` de 30 a 100 (por defecto 92).
fn parse_args() -> Args {
    let mut args = Args {
        montura: false,
        pildora: false,
        alpha: pct_to_alpha(92),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--montura" => args.montura = true,
            "--pildora" => args.pildora = true,
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

/// Atributos de todas las ventanas: sin marco, siempre encima y, sobre todo,
/// sin robarle el foco al juego al aparecer.
fn window_attributes(attrs: WindowAttributes) -> WindowAttributes {
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

struct TipInfo {
    name: String,
    label: String,
    tier: String,
    color: slint::Color,
}

struct State {
    #[cfg_attr(not(windows), allow(dead_code))]
    alpha: u8,
    tips: Vec<TipInfo>,
    tooltip: Option<TooltipWindow>,
    timer: Timer,
    #[cfg(windows)]
    hwnd: Option<win::Hwnd>,
}

fn main() -> Result<(), slint::PlatformError> {
    let args = parse_args();

    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(window_attributes)
        .select()?;

    let build = data::sample_build(args.montura);
    let ui = OverlayWindow::new()?;

    let mut slots = Vec::with_capacity(build.items.len());
    let mut tips = Vec::with_capacity(build.items.len());
    for item in &build.items {
        let (tier, enchant) = data::tier_of(item.id);
        let color = data::enchant_color(enchant);
        slots.push(Slot {
            icon: item.kind.icon().into(),
            tier: tier.clone().into(),
            tier_color: color,
            enchanted: enchant > 0,
            empty: item.id.is_empty(),
        });
        tips.push(TipInfo {
            name: item.name.into(),
            label: item.kind.label().into(),
            tier,
            color,
        });
    }

    // En la píldora va el primer casillero: la montura si es rol de montura, si no el arma.
    let main = &tips[0];
    ui.set_role(build.role.into());
    ui.set_role_color(data::role_color(build.role));
    ui.set_main_name(main.name.clone().into());
    ui.set_main_tier(main.tier.clone().into());
    ui.set_main_tier_color(main.color);
    ui.set_slots(ModelRc::new(VecModel::from(slots)));
    ui.set_pill_mode(args.pildora);

    let state = Rc::new(RefCell::new(State {
        alpha: args.alpha,
        tips,
        tooltip: None,
        timer: Timer::default(),
        #[cfg(windows)]
        hwnd: None,
    }));

    ui.on_slot_hover({
        let weak = ui.as_weak();
        let state = state.clone();
        move |index, entered| {
            let Some(ui) = weak.upgrade() else { return };
            if !entered {
                state.borrow().timer.stop();
                hide_tooltip(&state);
                return;
            }
            let weak = ui.as_weak();
            let state2 = state.clone();
            state
                .borrow()
                .timer
                .start(TimerMode::SingleShot, TOOLTIP_DELAY, move || {
                    if let Some(ui) = weak.upgrade()
                        && ui.get_hovered_slot() == index
                        && !ui.get_click_through()
                    {
                        show_tooltip(&ui, &state2, index as usize);
                    }
                });
        }
    });

    // Lo que necesita la ventana real de Windows se hace cuando winit la creó.
    let weak = ui.as_weak();
    let setup_state = state.clone();
    slint::spawn_local(async move {
        let Some(ui) = weak.upgrade() else { return };
        let Ok(winit_window) = ui.window().winit_window().await else {
            return;
        };
        #[cfg(windows)]
        {
            if let Some(hwnd) = win::hwnd_of(&winit_window) {
                win::make_overlay(hwnd, setup_state.borrow().alpha, false);
                setup_state.borrow_mut().hwnd = Some(hwnd);
                let weak = ui.as_weak();
                let state = setup_state.clone();
                let failed = win::register_hotkeys(hwnd, move |key| {
                    if let Some(ui) = weak.upgrade() {
                        on_hotkey(&ui, &state, key);
                    }
                });
                if !failed.is_empty() {
                    eprintln!("atajos ocupados por otra app: {failed:?}");
                }
            }
        }
        drop(winit_window);
        place_main(&ui, &setup_state);
    })
    .expect("el bucle de eventos de Slint no está disponible");

    ui.run()
}

/// Tamaño lógico de la ventana principal según el modo.
fn main_size(ui: &OverlayWindow) -> LogicalSize {
    if ui.get_pill_mode() {
        LogicalSize::new(300.0, 34.0)
    } else {
        let n = ui.get_slots().row_count() as f32;
        LogicalSize::new(n * SLOT_STEP - 4.0 + 2.0 * STRIP_PAD_LEFT, 54.0)
    }
}

/// Ajusta tamaño, esquinas y posición (arriba al centro del monitor).
fn place_main(ui: &OverlayWindow, state: &Rc<RefCell<State>>) {
    let scale = ui.window().scale_factor();
    let size = main_size(ui);
    ui.window().set_size(size);
    let width = (size.width * scale).round() as i32;
    let height = (size.height * scale).round() as i32;

    #[cfg(windows)]
    if let Some(hwnd) = state.borrow().hwnd {
        let radius = if ui.get_pill_mode() { 17.0 } else { 10.0 };
        win::round_corners(
            hwnd,
            width as u32,
            height as u32,
            (radius * scale).round() as u32,
        );
    }
    #[cfg(not(windows))]
    let _ = (state, height);

    let monitor = ui
        .window()
        .with_winit_window(|w| w.current_monitor().or_else(|| w.primary_monitor()))
        .flatten();
    if let Some(monitor) = monitor {
        let origin = monitor.position();
        let msize = monitor.size();
        let x = origin.x + (msize.width as i32 - width) / 2;
        let y = origin.y + (TOP_MARGIN * scale).round() as i32;
        ui.window().set_position(PhysicalPosition::new(x, y));
    }
}

fn show_tooltip(ui: &OverlayWindow, state: &Rc<RefCell<State>>, index: usize) {
    hide_tooltip(state);
    let Ok(tip) = TooltipWindow::new() else {
        return;
    };
    {
        let st = state.borrow();
        let Some(info) = st.tips.get(index) else {
            return;
        };
        tip.set_item_name(info.name.clone().into());
        tip.set_slot_label(info.label.clone().into());
        tip.set_tier(info.tier.clone().into());
        tip.set_tier_color(info.color);
    }

    // Debajo de la tira si arriba no hay lugar (la tira va pegada al borde superior).
    let scale = ui.window().scale_factor();
    let main_pos = ui.window().position();
    let main_h = ui.window().size().height as i32;
    let tip_h = (46.0 * scale).round() as i32;
    let gap = (GAP * scale).round() as i32;
    let x = main_pos.x + ((STRIP_PAD_LEFT + index as f32 * SLOT_STEP) * scale).round() as i32;
    let above = main_pos.y - tip_h - gap;
    let monitor_top = ui
        .window()
        .with_winit_window(|w| w.current_monitor().map(|m| m.position().y))
        .flatten()
        .unwrap_or(0);
    let y = if above >= monitor_top {
        above
    } else {
        main_pos.y + main_h + gap
    };
    tip.window().set_position(PhysicalPosition::new(x, y));

    if tip.show().is_err() {
        return;
    }

    #[cfg(windows)]
    {
        let weak = tip.as_weak();
        let alpha = state.borrow().alpha;
        let _ = slint::spawn_local(async move {
            let Some(tip) = weak.upgrade() else { return };
            let Ok(winit_window) = tip.window().winit_window().await else {
                return;
            };
            if let Some(hwnd) = win::hwnd_of(&winit_window) {
                // El mouse atraviesa el tooltip para no tapar el casillero.
                win::make_overlay(hwnd, alpha, true);
                let size = tip.window().size();
                win::round_corners(
                    hwnd,
                    size.width,
                    size.height,
                    (8.0 * tip.window().scale_factor()).round() as u32,
                );
            }
        });
    }

    state.borrow_mut().tooltip = Some(tip);
}

fn hide_tooltip(state: &Rc<RefCell<State>>) {
    let tip = state.borrow_mut().tooltip.take();
    if let Some(tip) = tip {
        let _ = tip.hide();
    }
}

#[cfg(windows)]
fn on_hotkey(ui: &OverlayWindow, state: &Rc<RefCell<State>>, key: win::Hotkey) {
    match key {
        win::Hotkey::ClickThrough => {
            let on = !ui.get_click_through();
            ui.set_click_through(on);
            if on {
                ui.set_hovered_slot(-1);
                state.borrow().timer.stop();
                hide_tooltip(state);
            }
            if let Some(hwnd) = state.borrow().hwnd {
                win::set_click_through(hwnd, on);
            }
        }
        win::Hotkey::PillMode => {
            ui.set_pill_mode(!ui.get_pill_mode());
            ui.set_hovered_slot(-1);
            state.borrow().timer.stop();
            hide_tooltip(state);
            place_main(ui, state);
        }
        win::Hotkey::Quit => {
            let _ = slint::quit_event_loop();
        }
    }
}
