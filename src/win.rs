//! Llamadas chicas a Win32 que Slint no expone: ventana sin foco, fuera de la
//! barra de tareas, opacidad, esquinas redondeadas, click-through, atajos y
//! saber si Albion está en primer plano.
//! Nada de hooks globales, inyección ni lectura de memoria del juego.

use std::cell::RefCell;

use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::winit::window::Window as WinitWindow;
use windows::Win32::Foundation::{
    COLORREF, CloseHandle, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{ClientToScreen, CreateRoundRectRgn, SetWindowRgn};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GWL_STYLE, GetClientRect, GetForegroundWindow, GetWindowLongPtrW,
    GetWindowThreadProcessId, LWA_ALPHA, SET_WINDOW_POS_FLAGS, STYLESTRUCT, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_HOTKEY,
    WM_NCACTIVATE, WM_NCDESTROY, WM_NCPAINT, WM_STYLECHANGING, WS_CAPTION, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_SYSMENU,
};

/// Nombre del ejecutable del juego.
const GAME_EXE: &str = "Albion-Online.exe";

#[derive(Clone, Copy, PartialEq, Eq)]
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

/// Estilos de barra de título que winit deja puestos aunque la ventana no
/// tenga marco. Con la región de esquinas redondeadas, Windows los pinta como
/// una barra blanca con el título, así que se sacan.
const CAPTION_STYLES: u32 = WS_CAPTION.0 | WS_SYSMENU.0;

/// Sin foco al hacer clic (el juego no pierde el teclado), sin barra de título,
/// sin botón en la barra de tareas ni en Alt+Tab, y con opacidad por ventana.
/// `mouse_through`: el mouse la atraviesa siempre (para el tooltip).
pub fn make_overlay(hwnd: Hwnd, alpha: u8, mouse_through: bool) {
    unsafe {
        let _ = SetWindowSubclass(hwnd.0, Some(subclass_proc), SUBCLASS_ID, 0);
        let style = GetWindowLongPtrW(hwnd.0, GWL_STYLE) & !(CAPTION_STYLES as isize);
        SetWindowLongPtrW(hwnd.0, GWL_STYLE, style);
    }
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
        // Para que Windows recalcule el marco con los estilos nuevos.
        let flags: SET_WINDOW_POS_FLAGS =
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
        let _ = SetWindowPos(hwnd.0, None, 0, 0, 0, 0, flags);
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

/// Oculta o muestra sin activar (no le saca el foco al juego).
pub fn set_visible(hwnd: Hwnd, visible: bool) {
    unsafe {
        let _ = ShowWindow(hwnd.0, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
    }
}

/// Área de dibujo de la ventana del juego, en píxeles de pantalla.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GameRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Recuerda la última ventana en primer plano para no preguntarle a Windows
/// el nombre del proceso cada vez.
#[derive(Default)]
pub struct ForegroundWatch {
    last: Option<(HWND, bool)>,
}

impl ForegroundWatch {
    /// Si la ventana en primer plano es Albion, devuelve su área de dibujo.
    /// Solo usa el nombre del ejecutable (como el Administrador de tareas):
    /// no lee nada de la memoria del juego.
    pub fn albion_rect(&mut self) -> Option<GameRect> {
        let fg = unsafe { GetForegroundWindow() };
        if fg.is_invalid() {
            return None;
        }
        let is_albion = match self.last {
            Some((hwnd, is_albion)) if hwnd == fg => is_albion,
            _ => {
                let is_albion =
                    process_exe_name(fg).is_some_and(|n| n.eq_ignore_ascii_case(GAME_EXE));
                self.last = Some((fg, is_albion));
                is_albion
            }
        };
        if !is_albion {
            return None;
        }
        let mut rect = RECT::default();
        let mut origin = POINT::default();
        unsafe {
            GetClientRect(fg, &mut rect).ok()?;
            if !ClientToScreen(fg, &mut origin).as_bool() {
                return None;
            }
        }
        Some(GameRect {
            x: origin.x,
            y: origin.y,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        })
    }
}

fn process_exe_name(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .is_ok();
    unsafe {
        let _ = CloseHandle(process);
    }
    if !ok {
        return None;
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit(['\\', '/']).next().map(str::to_owned)
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
/// Llamar después de `make_overlay`, que instala el subclass que las recibe.
pub fn register_hotkeys(hwnd: Hwnd, handler: impl Fn(Hotkey) + 'static) -> Vec<Hotkey> {
    HOTKEY_HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(handler)));
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
    match msg {
        WM_HOTKEY => {
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
        // Sin barra de título: nada que pintar fuera del área de dibujo.
        WM_NCPAINT => return LRESULT(0),
        WM_NCACTIVATE => return LRESULT(1),
        // winit vuelve a poner WS_CAPTION cuando cambia otros estilos; se filtra acá.
        WM_STYLECHANGING if wparam.0 as i32 == GWL_STYLE.0 => {
            let styles = lparam.0 as *mut STYLESTRUCT;
            if !styles.is_null() {
                unsafe { (*styles).styleNew &= !CAPTION_STYLES };
            }
        }
        WM_NCDESTROY => unsafe {
            for (_, id, _) in HOTKEYS {
                let _ = UnregisterHotKey(Some(hwnd), id);
            }
            let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
        },
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
