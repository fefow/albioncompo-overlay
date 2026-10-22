//! Sección Compos: las públicas en tarjetas con búsqueda, una privada por su
//! código y el detalle con todos los casilleros de cada build. Usa las mismas
//! RPC que el panel del overlay, con su propia búsqueda. Solo consulta cuando
//! la persona entra a la sección, busca o abre algo.

use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, VecModel};

use super::super::{
    App, AppWindow, CompsPage, DeskBuild, DeskComp, Slot, i18n, request_assets, role_color,
};
use super::home::open_url;
use crate::api::{self, CompDetail, CompList};
use crate::comps::parse_reference;
use crate::config;
use crate::data;
use crate::worker::{Command, CompTarget};

/// La primera vez que se entra a la sección se pide la lista.
pub fn ensure_loaded(app: &App) {
    let first = {
        let mut st = app.st.borrow_mut();
        let first = !st.desk.compos_loaded;
        st.desk.compos_loaded = true;
        first
    };
    if first {
        request_list(app, String::new(), 0);
    }
}

pub fn wire(app: &App, window: &AppWindow) {
    let page = window.global::<CompsPage>();
    let weak = Rc::downgrade(app);
    let handle = window.as_weak();
    page.on_search(move || {
        let Some(app) = weak.upgrade() else { return };
        let query = handle
            .upgrade()
            .map(|w| w.global::<CompsPage>().get_query().trim().to_owned())
            .unwrap_or_default();
        request_list(&app, query, 0);
    });
    let weak = Rc::downgrade(app);
    page.on_more(move || {
        let Some(app) = weak.upgrade() else { return };
        let (query, offset) = {
            let st = app.st.borrow();
            (
                st.desk.compos.query.clone(),
                st.desk.compos.items.len() as u32,
            )
        };
        request_list(&app, query, offset);
    });
    let weak = Rc::downgrade(app);
    page.on_open_item(move |index| {
        let Some(app) = weak.upgrade() else { return };
        let reference = app
            .st
            .borrow()
            .desk
            .compos
            .items
            .get(index as usize)
            .map(|item| item.share_id.clone());
        if let Some(reference) = reference {
            request_detail(&app, reference);
        }
    });
    let weak = Rc::downgrade(app);
    let handle = window.as_weak();
    page.on_open_code(move || {
        let Some(app) = weak.upgrade() else { return };
        let input = handle
            .upgrade()
            .map(|w| w.global::<CompsPage>().get_code().to_string())
            .unwrap_or_default();
        let reference = parse_reference(&input);
        if reference.is_empty() {
            let lang = app.st.borrow().config.lang;
            app.st.borrow_mut().desk.compos.status =
                Some((i18n::text(lang).comps_not_found.to_owned(), true));
            render(&app);
            return;
        }
        request_detail(&app, reference);
    });
    let weak = Rc::downgrade(app);
    page.on_back(move || {
        let Some(app) = weak.upgrade() else { return };
        {
            let mut st = app.st.borrow_mut();
            st.desk.compos.detail = None;
            st.desk.compos.status = None;
        }
        render(&app);
    });
    let weak = Rc::downgrade(app);
    page.on_toggle_overlay(move |index| {
        let Some(app) = weak.upgrade() else { return };
        toggle_overlay(&app, index as usize);
    });
    let weak = Rc::downgrade(app);
    page.on_open_web(move || {
        let Some(app) = weak.upgrade() else { return };
        let reference = {
            let st = app.st.borrow();
            st.desk
                .compos
                .detail
                .as_ref()
                .filter(|d| !d.private)
                .map(|d| d.reference.clone())
        };
        if let Some(reference) = reference {
            open_url(&format!("{}/comp/{reference}", config::site_url()));
        }
    });
}

fn request_list(app: &App, query: String, offset: u32) {
    app.st.borrow_mut().desk.compos.start_list(&query, offset);
    let _ = app.tx.send(Command::CompList {
        target: CompTarget::Desk,
        query,
        offset,
    });
    render(app);
}

fn request_detail(app: &App, reference: String) {
    app.st.borrow_mut().desk.compos.start_detail();
    let _ = app.tx.send(Command::CompGet {
        target: CompTarget::Desk,
        reference,
    });
    render(app);
}

pub fn on_list(app: &App, query: String, offset: u32, result: Result<CompList, String>) {
    let applied = {
        let mut st = app.st.borrow_mut();
        let lang = st.config.lang;
        st.desk.compos.apply_list(lang, &query, offset, result)
    };
    if applied {
        render(app);
    }
}

pub fn on_detail(app: &App, reference: String, result: Result<CompDetail, String>) {
    {
        let mut st = app.st.borrow_mut();
        let lang = st.config.lang;
        st.desk.compos.apply_detail(lang, reference, result);
    }
    render(app);
}

/// Muestra la build en la tira del overlay, o la saca si ya estaba.
fn toggle_overlay(app: &App, index: usize) {
    {
        let mut st = app.st.borrow_mut();
        let Some(build) = st
            .desk
            .compos
            .detail
            .as_ref()
            .and_then(|d| d.builds.get(index).cloned())
        else {
            return;
        };
        if st.preview.as_ref().is_some_and(|p| same_build(p, &build)) {
            st.preview = None;
        } else {
            st.preview = Some(build);
        }
    }
    super::super::render(app);
    super::super::sync_views(app);
    super::super::comps::render(app);
    render(app);
}

fn same_build(a: &api::Build, b: &api::Build) -> bool {
    a.id == b.id && a.name == b.name && a.weapon == b.weapon && a.role == b.role
}

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
    let mut wanted = Vec::new();
    {
        let st = app.st.borrow();
        let ui = &st.desk.compos;
        let lang = st.config.lang;
        let t = i18n::text(lang);
        let page = window.global::<CompsPage>();

        let items: Vec<DeskComp> = ui
            .items
            .iter()
            .map(|item| DeskComp {
                name: item
                    .name
                    .clone()
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| item.share_id.clone())
                    .into(),
                activity: item
                    .activity
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_uppercase()
                    .into(),
                builds: t
                    .comps_builds
                    .replace("{n}", &item.builds.to_string())
                    .into(),
            })
            .collect();

        let mut builds = Vec::new();
        if let Some(detail) = &ui.detail {
            for build in &detail.builds {
                let role = build
                    .role
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .to_uppercase();
                let mut slots = Vec::new();
                let mut names = Vec::new();
                for item in data::slot_items(build, &role) {
                    let (tier, enchant) = data::tier_of(&item.id);
                    let image = st.icons.get(&item.id).cloned();
                    names.push(
                        if item.id.is_empty() {
                            t.slots[item.kind as usize].to_owned()
                        } else {
                            st.names.get(&item.id).cloned().unwrap_or_default()
                        }
                        .into(),
                    );
                    slots.push(Slot {
                        icon: item.kind.icon().into(),
                        has_image: image.is_some(),
                        image: image.unwrap_or_default(),
                        tier: tier.into(),
                        tier_color: data::enchant_color(enchant),
                        enchanted: enchant > 0,
                        empty: item.id.is_empty(),
                    });
                    if !item.id.is_empty() {
                        wanted.push(item.id);
                    }
                }
                let name = build
                    .name
                    .clone()
                    .filter(|n| !n.trim().is_empty())
                    .or_else(|| {
                        build
                            .weapon
                            .as_deref()
                            .and_then(|w| st.names.get(&w.trim().to_uppercase()).cloned())
                    })
                    .unwrap_or_else(|| t.no_item.to_owned());
                builds.push(DeskBuild {
                    name: name.into(),
                    role: if role.is_empty() {
                        "".into()
                    } else {
                        i18n::role_label(lang, &role, false).into()
                    },
                    role_color: role_color(&role),
                    slots: ModelRc::new(VecModel::from(slots)),
                    names: ModelRc::new(VecModel::from(names)),
                    in_overlay: st.preview.as_ref().is_some_and(|p| same_build(p, build)),
                });
            }
        }

        page.set_mode(i32::from(ui.detail.is_some()));
        page.set_loading(ui.loading);
        page.set_has_more(ui.more);
        let (status, is_error) = match &ui.status {
            Some((text, error)) => (text.clone(), *error),
            None if ui.loading && ui.items.is_empty() => (t.comps_loading.to_owned(), false),
            None => (String::new(), false),
        };
        page.set_status_text(status.into());
        page.set_status_error(is_error);
        if let Some(detail) = &ui.detail {
            page.set_detail_name(detail.name.clone().into());
            page.set_detail_private(detail.private);
            page.set_can_open_web(!detail.private);
            page.set_detail_meta(
                t.comps_builds
                    .replace("{n}", &detail.builds.len().to_string())
                    .into(),
            );
        }
        page.set_items(ModelRc::new(VecModel::from(items)));
        page.set_builds(ModelRc::new(VecModel::from(builds)));
    }
    request_assets(app, wanted);
}
