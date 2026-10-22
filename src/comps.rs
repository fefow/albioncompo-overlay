//! Buscador de compos: lista pública con búsqueda por nombre, o una compo por
//! su código (privadas). Es un panel chico, oculto hasta que se abre, que queda
//! fijo hasta que se cierra. No consulta nada por su cuenta: solo cuando se
//! busca, se abre una compo o se pide "ver más".

use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, PhysicalPosition, VecModel};

use super::{
    App, BuildRow, CompRow, CompsWindow, GAP, i18n, member_icon_id, request_assets, role_color,
    style_after_show,
};
use crate::api::{self, CompDetail, CompItem, CompList};
use crate::config::Lang;
use crate::worker::{Command, CompTarget};

/// Ancho lógico del panel (tiene que coincidir con `CompsWindow` en overlay.slint).
pub const COMPS_WIDTH: f32 = 300.0;

/// Lo que muestra un buscador de compos (el panel del overlay o la sección de
/// la app): la lista pública cargada y la compo abierta.
#[derive(Default)]
pub struct CompsState {
    /// Búsqueda que se mandó (para descartar respuestas de una anterior).
    pub query: String,
    pub items: Vec<CompItem>,
    pub more: bool,
    pub loaded: bool,
    pub loading: bool,
    /// Mensaje de estado y si es un error.
    pub status: Option<(String, bool)>,
    pub detail: Option<Detail>,
    pub selected: Option<usize>,
}

pub struct Detail {
    /// Lo que se pidió: el id de la compo si es pública, o su código.
    pub reference: String,
    pub name: String,
    pub private: bool,
    pub builds: Vec<api::Build>,
}

impl CompsState {
    pub fn start_list(&mut self, query: &str, offset: u32) {
        if offset == 0 {
            self.items.clear();
            self.more = false;
        }
        self.query = query.to_owned();
        self.loading = true;
        self.status = None;
    }

    pub fn start_detail(&mut self) {
        self.loading = true;
        self.status = None;
    }

    /// Suma una página a la lista. Devuelve `false` si la respuesta es de una
    /// búsqueda anterior y se descartó.
    pub fn apply_list(
        &mut self,
        lang: Lang,
        query: &str,
        offset: u32,
        result: Result<CompList, String>,
    ) -> bool {
        if self.query != query || self.items.len() as u32 != offset {
            return false;
        }
        self.loading = false;
        match result {
            Ok(list) => {
                self.loaded = true;
                self.more = list.more;
                self.items.extend(list.items);
                self.status = self
                    .items
                    .is_empty()
                    .then(|| (i18n::text(lang).comps_empty.to_owned(), false));
            }
            Err(status) => {
                self.status = Some((i18n::error(lang, &status).to_owned(), true));
            }
        }
        true
    }

    pub fn apply_detail(
        &mut self,
        lang: Lang,
        reference: String,
        result: Result<CompDetail, String>,
    ) {
        self.loading = false;
        match result {
            Ok(detail) => {
                self.detail = Some(Detail {
                    reference,
                    name: detail.composition.name.unwrap_or_default(),
                    private: detail.private,
                    builds: detail.builds,
                });
                self.selected = None;
                self.status = None;
            }
            Err(status) => {
                let text = if status == "invalid" {
                    i18n::text(lang).comps_not_found
                } else {
                    i18n::error(lang, &status)
                };
                self.status = Some((text.to_owned(), true));
            }
        }
    }
}

/// Saca el código o el id de lo que pegó la persona: un link
/// (`.../comp/<id>?code=ABC123`), un id o el código solo. Si el link trae
/// `code=`, ese código es lo que abre una compo privada.
pub fn parse_reference(input: &str) -> String {
    let trimmed = input.trim();
    if let Some(code) = query_param(trimmed, "code") {
        return code;
    }
    let tail = trimmed
        .split_once("/comp/")
        .map_or(trimmed, |(_, rest)| rest);
    tail.split(['?', '#', '/', ' '])
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn query_param(input: &str, key: &str) -> Option<String> {
    let query = input.split_once('?')?.1;
    let query = query.split('#').next().unwrap_or(query);
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key && !v.is_empty()).then(|| v.to_owned())
    })
}

pub fn toggle(app: &App) {
    if app.st.borrow().comps.is_some() {
        close(app);
    } else {
        open(app);
    }
}

fn open(app: &App) {
    let Ok(window) = CompsWindow::new() else {
        return;
    };
    set_texts(app, &window);
    wire(app, &window);
    app.st.borrow_mut().comps = Some(window.clone_strong());
    let first = !app.st.borrow().comps_ui.loaded;
    if first {
        request_list(app, String::new(), 0);
    }
    render(app);
    reposition(app);
    if window.show().is_ok() {
        app.st.borrow_mut().comps_shown = true;
        style_after_show(app, window.as_weak(), 7.0);
    }
    app.launcher.set_active(true);
}

pub fn close(app: &App) {
    let window = {
        let mut st = app.st.borrow_mut();
        st.comps_shown = false;
        st.comps.take()
    };
    if let Some(window) = window {
        let _ = window.hide();
    }
    let had_preview = app.st.borrow_mut().preview.take().is_some();
    if had_preview {
        super::render(app);
        super::sync_views(app);
    }
}

/// Muestra u oculta el panel según si el overlay se ve (mismo criterio que
/// el resto de las ventanas), sin perder lo que la persona tenía abierto.
pub fn sync_visibility(app: &App, show: bool) {
    let (window, shown) = {
        let st = app.st.borrow();
        (
            st.comps.as_ref().map(ComponentHandle::clone_strong),
            st.comps_shown,
        )
    };
    let Some(window) = window else { return };
    if show && !shown {
        reposition(app);
        if window.show().is_ok() {
            app.st.borrow_mut().comps_shown = true;
            style_after_show(app, window.as_weak(), 7.0);
        }
    } else if !show && shown {
        app.st.borrow_mut().comps_shown = false;
        let _ = window.hide();
    }
}

/// Debajo del botón flotante, alineado a su borde derecho.
pub fn reposition(app: &App) {
    let Some(window) = app
        .st
        .borrow()
        .comps
        .as_ref()
        .map(ComponentHandle::clone_strong)
    else {
        return;
    };
    let launcher = app.launcher.window();
    let scale = launcher.scale_factor();
    let pos = launcher.position();
    let size = launcher.size();
    let width = (COMPS_WIDTH * scale).round() as i32;
    let gap = (GAP * scale).round() as i32;
    window.window().set_position(PhysicalPosition::new(
        pos.x + size.width as i32 - width,
        pos.y + size.height as i32 + gap,
    ));
}

fn set_texts(app: &App, window: &CompsWindow) {
    let t = i18n::text(app.st.borrow().config.lang);
    window.set_heading(t.comps_title.into());
    window.set_tab_public(t.comps_tab_public.into());
    window.set_tab_code(t.comps_tab_code.into());
    window.set_search_placeholder(t.comps_search.into());
    window.set_code_placeholder(t.comps_code.into());
    window.set_open_text(t.comps_open.into());
    window.set_more_text(t.comps_more.into());
    window.set_view_text(t.comps_view.into());
    window.set_stop_text(t.comps_stop_view.into());
    window.set_private_text(t.comps_private.into());
}

fn wire(app: &App, window: &CompsWindow) {
    let weak = Rc::downgrade(app);
    window.on_search({
        let weak = weak.clone();
        let window = window.as_weak();
        move || {
            let (Some(app), Some(window)) = (weak.upgrade(), window.upgrade()) else {
                return;
            };
            let query = window.get_query().trim().to_owned();
            request_list(&app, query, 0);
            render(&app);
        }
    });
    window.on_more({
        let weak = weak.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let (query, offset) = {
                let st = app.st.borrow();
                (st.comps_ui.query.clone(), st.comps_ui.items.len() as u32)
            };
            request_list(&app, query, offset);
            render(&app);
        }
    });
    window.on_open_item({
        let weak = weak.clone();
        move |index| {
            let Some(app) = weak.upgrade() else { return };
            let reference = app
                .st
                .borrow()
                .comps_ui
                .items
                .get(index as usize)
                .map(|item| item.share_id.clone());
            if let Some(reference) = reference {
                request_detail(&app, reference);
            }
        }
    });
    window.on_open_code({
        let weak = weak.clone();
        let window = window.as_weak();
        move || {
            let (Some(app), Some(window)) = (weak.upgrade(), window.upgrade()) else {
                return;
            };
            let reference = parse_reference(&window.get_code());
            if !reference.is_empty() {
                request_detail(&app, reference);
            }
        }
    });
    window.on_back({
        let weak = weak.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            {
                let mut st = app.st.borrow_mut();
                st.comps_ui.detail = None;
                st.comps_ui.selected = None;
                st.comps_ui.status = None;
            }
            render(&app);
        }
    });
    window.on_pick_build({
        let weak = weak.clone();
        move |index| {
            let Some(app) = weak.upgrade() else { return };
            app.st.borrow_mut().comps_ui.selected = Some(index as usize);
            render(&app);
        }
    });
    window.on_view_in_strip({
        let weak = weak.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let build = {
                let st = app.st.borrow();
                st.comps_ui
                    .detail
                    .as_ref()
                    .zip(st.comps_ui.selected)
                    .and_then(|(d, i)| d.builds.get(i).cloned())
            };
            if let Some(build) = build {
                {
                    // Sin selección el botón pasa a "Dejar de ver en la tira".
                    let mut st = app.st.borrow_mut();
                    st.preview = Some(build);
                    st.comps_ui.selected = None;
                }
                super::render(&app);
                super::sync_views(&app);
                render(&app);
            }
        }
    });
    window.on_stop_view({
        let weak = weak.clone();
        move || {
            let Some(app) = weak.upgrade() else { return };
            app.st.borrow_mut().preview = None;
            super::render(&app);
            super::sync_views(&app);
            render(&app);
        }
    });
    window.on_close_clicked(move || {
        if let Some(app) = weak.upgrade() {
            close(&app);
            super::sync_views(&app);
        }
    });
}

fn request_list(app: &App, query: String, offset: u32) {
    app.st.borrow_mut().comps_ui.start_list(&query, offset);
    let _ = app.tx.send(Command::CompList {
        target: CompTarget::Panel,
        query,
        offset,
    });
}

fn request_detail(app: &App, reference: String) {
    app.st.borrow_mut().comps_ui.start_detail();
    let _ = app.tx.send(Command::CompGet {
        target: CompTarget::Panel,
        reference,
    });
    render(app);
}

pub fn on_list(app: &App, query: String, offset: u32, result: Result<CompList, String>) {
    let applied = {
        let mut st = app.st.borrow_mut();
        let lang = st.config.lang;
        st.comps_ui.apply_list(lang, &query, offset, result)
    };
    if applied {
        render(app);
    }
}

pub fn on_detail(app: &App, reference: String, result: Result<CompDetail, String>) {
    let icons = {
        let mut st = app.st.borrow_mut();
        let lang = st.config.lang;
        st.comps_ui.apply_detail(lang, reference, result);
        st.comps_ui
            .detail
            .as_ref()
            .map(|d| d.builds.iter().filter_map(member_icon_id).collect())
            .unwrap_or_default()
    };
    request_assets(app, icons);
    render(app);
}

/// Pasa el estado a la ventana (si está abierta).
pub fn render(app: &App) {
    let (window, rows) = {
        let st = app.st.borrow();
        let Some(window) = st.comps.as_ref().map(ComponentHandle::clone_strong) else {
            return;
        };
        let ui = &st.comps_ui;
        let t = i18n::text(st.config.lang);
        let items: Vec<CompRow> = ui
            .items
            .iter()
            .map(|item| CompRow {
                name: item
                    .name
                    .clone()
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| item.share_id.clone())
                    .into(),
                meta: {
                    let builds = t.comps_builds.replace("{n}", &item.builds.to_string());
                    match item
                        .activity
                        .as_deref()
                        .map(str::trim)
                        .filter(|a| !a.is_empty())
                    {
                        Some(activity) => format!("{} · {builds}", activity.to_uppercase()),
                        None => builds,
                    }
                }
                .into(),
            })
            .collect();
        let builds: Vec<BuildRow> = ui
            .detail
            .as_ref()
            .map(|detail| {
                detail
                    .builds
                    .iter()
                    .enumerate()
                    .map(|(i, build)| {
                        let role = build
                            .role
                            .as_deref()
                            .unwrap_or_default()
                            .trim()
                            .to_uppercase();
                        let image = member_icon_id(build).and_then(|id| st.icons.get(&id).cloned());
                        BuildRow {
                            name: build
                                .name
                                .clone()
                                .filter(|n| !n.trim().is_empty())
                                .or_else(|| {
                                    member_icon_id(build).and_then(|id| st.names.get(&id).cloned())
                                })
                                .unwrap_or_else(|| t.no_item.to_owned())
                                .into(),
                            role: if role.is_empty() {
                                "".into()
                            } else {
                                i18n::role_label(st.config.lang, &role, true).into()
                            },
                            role_color: role_color(&role),
                            has_image: image.is_some(),
                            image: image.unwrap_or_default(),
                            selected: ui.selected == Some(i),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        window.set_mode(i32::from(ui.detail.is_some()));
        window.set_loading(ui.loading);
        window.set_has_more(ui.more);
        window.set_detail_name(
            ui.detail
                .as_ref()
                .map(|d| d.name.clone())
                .unwrap_or_default()
                .into(),
        );
        window.set_detail_private(ui.detail.as_ref().is_some_and(|d| d.private));
        window.set_can_view(ui.selected.is_some());
        window.set_previewing(st.preview.is_some());
        let (status, is_error) = match (&ui.status, ui.loading) {
            (Some((text, error)), _) => (text.clone(), *error),
            (None, true) => (t.comps_loading.to_owned(), false),
            (None, false) => (String::new(), false),
        };
        window.set_status_text(status.into());
        window.set_status_error(is_error);
        (window, (items, builds))
    };
    window.set_items(ModelRc::new(VecModel::from(rows.0)));
    window.set_builds(ModelRc::new(VecModel::from(rows.1)));
}

#[cfg(test)]
mod tests {
    use super::parse_reference;

    #[test]
    fn referencias_de_compo() {
        assert_eq!(parse_reference("  K7M2QP "), "K7M2QP");
        assert_eq!(parse_reference("AbC123xyz"), "AbC123xyz");
        assert_eq!(
            parse_reference("https://albioncompo.com/comp/AbC123xyz"),
            "AbC123xyz"
        );
        // Un link privado trae el código, que es lo que abre la compo.
        assert_eq!(
            parse_reference("https://albioncompo.com/comp/AbC123xyz?code=K7M2QP"),
            "K7M2QP"
        );
        assert_eq!(parse_reference("/comp/AbC123xyz#top"), "AbC123xyz");
        assert_eq!(parse_reference(""), "");
    }
}
