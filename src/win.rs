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
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::Shell::{
    DefSubclassProc, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
    RemoveWindowSubclass, SetWindowSubclass, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetSystemMetrics, HICON,
    IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, LoadIconW, LoadImageW, MF_CHECKED, MF_GRAYED,
    MF_SEPARATOR, MF_STRING, PostMessageW, SM_CXSMICON, SM_CYSMICON, SetForegroundWindow,
    TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP,
    WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GWL_STYLE, GetClientRect, GetForegroundWindow, GetWindowLongPtrW,
    GetWindowThreadProcessId, LWA_ALPHA, SET_WINDOW_POS_FLAGS, STYLESTRUCT, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_HOTKEY,
    WM_NCACTIVATE, WM_NCDESTROY, WM_NCPAINT, WM_STYLECHANGING, WS_CAPTION, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_SYSMENU,
};
use windows::core::PCWSTR;

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
    Party,
    Setup,
    Hide,
    Comps,
    Quit,
}

const HOTKEYS: [(Hotkey, i32, u32); 7] = [
    (Hotkey::ClickThrough, 1, b'L' as u32),
    (Hotkey::PillMode, 2, b'M' as u32),
    (Hotkey::Quit, 3, b'Q' as u32),
    (Hotkey::Party, 4, b'P' as u32),
    (Hotkey::Setup, 5, b'K' as u32),
    (Hotkey::Hide, 6, b'H' as u32),
    (Hotkey::Comps, 7, b'B' as u32),
];

type HotkeyHandler = Box<dyn Fn(Hotkey)>;

type TrayHandler = Box<dyn Fn(TrayEvent)>;

thread_local! {
    static HOTKEY_HANDLER: RefCell<Option<HotkeyHandler>> = const { RefCell::new(None) };
    static TRAY_HANDLER: RefCell<Option<TrayHandler>> = const { RefCell::new(None) };
}

/// Mensaje que Windows manda a la ventana cuando se toca el ícono de la bandeja.
const TRAY_MSG: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrayEvent {
    /// Clic izquierdo.
    Open,
    /// Clic derecho: menú.
    Menu,
}

/// Ícono del .exe (recurso 1, lo agrega build.rs) en el tamaño chico del sistema.
fn app_icon() -> HICON {
    unsafe {
        let module = GetModuleHandleW(None).ok();
        let icon = module.and_then(|m| {
            LoadImageW(
                Some(m.into()),
                PCWSTR(1 as _),
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                LR_DEFAULTCOLOR,
            )
            .ok()
        });
        match icon {
            Some(h) => HICON(h.0),
            None => LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
        }
    }
}

/// Pone el ícono en la bandeja de Windows (al lado del reloj). Los clics
/// llegan a `handler` en el hilo de la UI.
pub fn add_tray(hwnd: Hwnd, tip: &str, handler: impl Fn(TrayEvent) + 'static) -> bool {
    TRAY_HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(handler)));
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd.0,
        uID: TRAY_ID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: TRAY_MSG,
        hIcon: app_icon(),
        ..Default::default()
    };
    for (dst, src) in data.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
        *dst = src;
    }
    unsafe { Shell_NotifyIconW(NIM_ADD, &data).as_bool() }
}

pub fn remove_tray(hwnd: Hwnd) {
    let data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd.0,
        uID: TRAY_ID,
        ..Default::default()
    };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
    }
}

/// Opción del menú; `id` 0 es un separador.
pub struct MenuItem {
    pub id: u32,
    pub text: String,
    pub checked: bool,
    pub enabled: bool,
}

/// Menú contextual en la posición del mouse. Devuelve la opción elegida.
pub fn popup_menu(hwnd: Hwnd, items: &[MenuItem]) -> Option<u32> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let mut texts: Vec<Vec<u16>> = Vec::with_capacity(items.len());
        for item in items {
            if item.id == 0 {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                continue;
            }
            let mut flags = MF_STRING;
            if item.checked {
                flags |= MF_CHECKED;
            }
            if !item.enabled {
                flags |= MF_GRAYED;
            }
            texts.push(item.text.encode_utf16().chain(Some(0)).collect());
            let text = texts.last().map_or(PCWSTR::null(), |t| PCWSTR(t.as_ptr()));
            let _ = AppendMenuW(menu, flags, item.id as usize, text);
        }
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Sin esto el menú no se cierra al hacer clic afuera.
        let _ = SetForegroundWindow(hwnd.0);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RIGHTALIGN,
            pt.x,
            pt.y,
            None,
            hwnd.0,
            None,
        );
        let _ = PostMessageW(Some(hwnd.0), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        (chosen.0 > 0).then_some(chosen.0 as u32)
    }
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
        TRAY_MSG => {
            let event = match (lparam.0 & 0xffff) as u32 {
                WM_LBUTTONUP => Some(TrayEvent::Open),
                WM_RBUTTONUP | WM_CONTEXTMENU => Some(TrayEvent::Menu),
                _ => None,
            };
            if let Some(event) = event {
                let _ = slint::invoke_from_event_loop(move || {
                    TRAY_HANDLER.with(|h| {
                        if let Some(handler) = h.borrow().as_ref() {
                            handler(event);
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

/// ¿Está abierto Albion? Recorre la lista de procesos (lo mismo que muestra
/// el Administrador de tareas) y compara el nombre del ejecutable.
pub fn albion_running() -> bool {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return false;
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut found = false;
    let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while ok {
        let len = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(GAME_EXE) {
            found = true;
            break;
        }
        ok = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    unsafe {
        let _ = CloseHandle(snapshot);
    }
    found
}

/// Abre un link en el navegador que la persona tenga por defecto.
pub fn open_url(url: &str) {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let url: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(url.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Ventana de la app: esquinas redondeadas y sombra de Windows 11 (en
/// Windows 10 no hace nada) y la trae al frente.
pub fn style_desk(hwnd: Hwnd) {
    use windows::Win32::Graphics::Dwm::{
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };
    let pref = DWMWCP_ROUND;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd.0,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const _,
            std::mem::size_of_val(&pref) as u32,
        );
    }
}

pub fn bring_to_front(hwnd: Hwnd) {
    use windows::Win32::UI::WindowsAndMessaging::{IsIconic, SW_RESTORE};
    unsafe {
        if IsIconic(hwnd.0).as_bool() {
            let _ = ShowWindow(hwnd.0, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd.0);
    }
}
