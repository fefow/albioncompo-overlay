//! Llamadas chicas a Win32 que Slint no expone: ventana sin foco, fuera de la
//! barra de tareas, opacidad, esquinas redondeadas, click-through y atajos.
//! Nada de hooks globales, inyección ni lectura de memoria del juego.

use std::cell::RefCell;

use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::winit::window::Window as WinitWindow;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, LWA_ALPHA, SetLayeredWindowAttributes, SetWindowLongPtrW,
    WM_HOTKEY, WM_NCDESTROY, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

#[derive(Clone, Copy)]
pub struct Hwnd(HWND);

pub fn hwnd_of(window: &WinitWindow) -> Option<Hwnd> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(Hwnd(HWND(h.hwnd.get() as *mut _))),
        _ => None,
    }
}

fn ex_style(hwnd: Hwnd) -> isize {
    unsafe { GetWindowLongPtrW(hwnd.0, GWL_EXSTYLE) }
}

fn set_ex_style(hwnd: Hwnd, style: isize) {
    unsafe {
        SetWindowLongPtrW(hwnd.0, GWL_EXSTYLE, style);
    }
}

/// Sin foco al hacer clic (el juego no pierde el teclado), sin botón en la
/// barra de tareas ni en Alt+Tab, y con opacidad por ventana.
/// `mouse_through`: el mouse la atraviesa siempre (para el tooltip).
pub fn make_overlay(hwnd: Hwnd, alpha: u8, mouse_through: bool) {
    let mut style = ex_style(hwnd)
        | WS_EX_NOACTIVATE.0 as isize
        | WS_EX_TOOLWINDOW.0 as isize
        | WS_EX_LAYERED.0 as isize;
    if mouse_through {
        style |= WS_EX_TRANSPARENT.0 as isize;
    }
    set_ex_style(hwnd, style);
    unsafe {
        let _ = SetLayeredWindowAttributes(hwnd.0, COLORREF(0), alpha, LWA_ALPHA);
    }
}

/// Click-through: el juego recibe todos los clics. Requiere WS_EX_LAYERED.
pub fn set_click_through(hwnd: Hwnd, on: bool) {
    let style = ex_style(hwnd);
    let flag = WS_EX_TRANSPARENT.0 as isize;
    set_ex_style(hwnd, if on { style | flag } else { style & !flag });
}

/// Recorta la ventana a un rectángulo redondeado (tamaño en píxeles físicos).
pub fn round_corners(hwnd: Hwnd, width: u32, height: u32, radius: u32) {
    let d = (radius * 2) as i32;
    unsafe {
        // +1: CreateRoundRectRgn excluye el borde derecho e inferior.
        let rgn = CreateRoundRectRgn(0, 0, width as i32 + 1, height as i32 + 1, d, d);
        // Si sale bien, Windows pasa a ser dueño de la región: no se libera acá.
        SetWindowRgn(hwnd.0, Some(rgn), true);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hotkey {
    ClickThrough,
    PillMode,
    Quit,
}

const HOTKEYS: [(Hotkey, i32, u32); 3] = [
    (Hotkey::ClickThrough, 1, b'L' as u32),
    (Hotkey::PillMode, 2, b'M' as u32),
    (Hotkey::Quit, 3, b'Q' as u32),
];

type HotkeyHandler = Box<dyn Fn(Hotkey)>;

thread_local! {
    static HOTKEY_HANDLER: RefCell<Option<HotkeyHandler>> = const { RefCell::new(None) };
}

const SUBCLASS_ID: usize = 0xA1B1;

/// Registra Ctrl+Shift+L / M / Q. RegisterHotKey no es un hook global: Windows
/// solo avisa cuando se aprieta esa combinación exacta. Devuelve las que no
/// se pudieron registrar (por ejemplo, porque otra app ya las usa).
pub fn register_hotkeys(hwnd: Hwnd, handler: impl Fn(Hotkey) + 'static) -> Vec<Hotkey> {
    HOTKEY_HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(handler)));
    unsafe {
        let _ = SetWindowSubclass(hwnd.0, Some(subclass_proc), SUBCLASS_ID, 0);
    }
    HOTKEYS
        .iter()
        .filter(|(_, id, vk)| unsafe {
            RegisterHotKey(
                Some(hwnd.0),
                *id,
                MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
                *vk,
            )
            .is_err()
        })
        .map(|(key, _, _)| *key)
        .collect()
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_HOTKEY {
        if let Some((key, _, _)) = HOTKEYS.iter().find(|(_, id, _)| *id as usize == wparam.0) {
            let key = *key;
            // Se difiere al bucle de eventos de Slint para no tocar la UI
            // desde adentro del procedimiento de ventana de winit.
            let _ = slint::invoke_from_event_loop(move || {
                HOTKEY_HANDLER.with(|h| {
                    if let Some(handler) = h.borrow().as_ref() {
                        handler(key);
                    }
                });
            });
        }
        return LRESULT(0);
    }
    if msg == WM_NCDESTROY {
        unsafe {
            for (_, id, _) in HOTKEYS {
                let _ = UnregisterHotKey(Some(hwnd), id);
            }
            let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
