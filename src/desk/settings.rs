//! Ajustes: idioma, comportamiento de la ventana con el juego y el overlay.

use std::rc::Rc;

use slint::ComponentHandle;

use super::super::{App, AppWindow, SettingsPage, set_lang, sync_views};
use crate::config::Lang;

pub fn wire(app: &App, window: &AppWindow) {
    let page = window.global::<SettingsPage>();
    let weak = Rc::downgrade(app);
    page.on_lang_picked(move |index| {
        if let Some(app) = weak.upgrade() {
            set_lang(&app, Lang::from_index(index));
        }
    });
    let weak = Rc::downgrade(app);
    page.on_toggled(move |option| {
        let Some(app) = weak.upgrade() else { return };
        {
            let mut st = app.st.borrow_mut();
            let c = &mut st.config;
            match option.as_str() {
                "start-window" => c.start_in_tray = !c.start_in_tray,
                "hide-on-game" => c.keep_on_game = !c.keep_on_game,
                "close-to-tray" => c.quit_on_close = !c.quit_on_close,
                "pin-strip" => c.pin_strip = !c.pin_strip,
                "pin-party" => c.pin_party = !c.pin_party,
                _ => return,
            }
            if !app.demo {
                let _ = st.config.save();
            }
        }
        sync_views(&app);
        if let Some(window) = app.st.borrow().desk.window.as_ref() {
            render(&app, window);
        }
    });
}

pub fn render(app: &App, window: &AppWindow) {
    let st = app.st.borrow();
    let c = &st.config;
    let page = window.global::<SettingsPage>();
    page.set_lang_index(c.lang.index());
    page.set_start_window(!c.start_in_tray);
    page.set_hide_on_game(!c.keep_on_game);
    page.set_close_to_tray(!c.quit_on_close);
    page.set_pin_strip(c.pin_strip);
    page.set_pin_party(c.pin_party);
}
