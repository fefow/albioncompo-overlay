//! Inicio: estado de la asignación del Comp Planner, accesos y atajos.

use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, VecModel};

use super::super::{App, AppWindow, HomePage, HotkeyRow, Phase, i18n, open_setup, status_for};
use super::texts;
use crate::config;

/// Teclas de los atajos que se muestran, en el orden de `DeskText::hotkeys`.
const HOTKEY_KEYS: [&str; 5] = [
    "Ctrl+Shift+P",
    "Ctrl+Shift+B",
    "Ctrl+Shift+H",
    "Ctrl+Shift+L",
    "Ctrl+Shift+M",
];

pub fn wire(app: &App, window: &AppWindow) {
    let page = window.global::<HomePage>();
    let weak = Rc::downgrade(app);
    page.on_open_setup(move || {
        if let Some(app) = weak.upgrade() {
            open_setup(&app);
        }
    });
    page.on_open_site(|| open_url(&config::site_url()));
}

pub fn render(app: &App, window: &AppWindow) {
    let st = app.st.borrow();
    let lang = st.config.lang;
    let t = i18n::text(lang);
    let page = window.global::<HomePage>();

    let has = st.phase != Phase::NoCode && !st.config.code.is_empty();
    page.set_has_assignment(has);
    if has {
        let overlay = st.overlay.as_ref();
        let assignment = overlay.and_then(|o| o.assignment.name.clone());
        let composition = overlay.and_then(|o| o.composition.name.clone());
        let name = assignment
            .clone()
            .or_else(|| composition.clone())
            .unwrap_or_else(|| format!("{} {}", t.assignment, st.config.code));
        let mut detail = Vec::new();
        if let Some(c) = composition.filter(|_| assignment.is_some()) {
            detail.push(c);
        }
        if let Some(o) = overlay {
            detail.push(t.members.replace("{n}", &o.entries.len().to_string()));
        }
        if !st.config.name.is_empty() {
            detail.push(st.config.name.clone());
        }
        let (status, color) = match st.phase {
            Phase::Assigned => (
                texts::text(lang).assign_ready,
                slint::Color::from_rgb_u8(0x6f, 0xcf, 0x7f),
            ),
            phase => status_for(lang, phase),
        };
        page.set_assign_name(name.into());
        page.set_assign_detail(detail.join(" · ").into());
        page.set_assign_status(status.into());
        page.set_assign_status_color(color);
    }

    let names = texts::text(lang).hotkeys;
    let rows: Vec<HotkeyRow> = HOTKEY_KEYS
        .iter()
        .zip(names)
        .map(|(keys, label)| HotkeyRow {
            keys: (*keys).into(),
            label: label.into(),
        })
        .collect();
    page.set_hotkeys(ModelRc::new(VecModel::from(rows)));
}

pub fn open_url(url: &str) {
    #[cfg(windows)]
    crate::win::open_url(url);
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}
