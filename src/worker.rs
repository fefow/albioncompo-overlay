//! Hilo de red: las llamadas HTTP y la lectura de disco no bloquean la UI.
//! La UI le manda pedidos por un canal y recibe las respuestas en su propio
//! bucle de eventos con `slint::invoke_from_event_loop`.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::api::{self, ApiError, Client, Overlay};
use crate::assets::{self, Pixels};
use crate::config::{Lang, normalize_name};

pub enum Command {
    Poll {
        code: String,
        rev: Option<String>,
    },
    Join {
        code: String,
        name: String,
        role: String,
    },
    /// Nombres en el idioma pedido y los íconos de esos ítems.
    Assets {
        lang: Lang,
        ids: Vec<String>,
    },
}

pub enum Event {
    Overlay(Result<Overlay, ApiError>),
    /// `Ok` con la asignación ya leída, o el mensaje para mostrar.
    Joined(Result<Overlay, String>),
    /// id original -> (id con tier, nombre).
    Names(Lang, Vec<(String, String, String)>),
    Icon(String, Pixels),
}

type Handler = Box<dyn Fn(Event)>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
}

/// Arranca el hilo. `handler` corre en el hilo de la UI con cada respuesta.
pub fn spawn(handler: impl Fn(Event) + 'static) -> Sender<Command> {
    HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(handler)));
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("albioncompo-red".into())
        // Stack chico: el hilo solo hace HTTP y decodifica PNG de 64 px.
        .stack_size(512 * 1024)
        .spawn(move || run(rx))
        .expect("no se pudo crear el hilo de red");
    tx
}

fn send(event: Event) {
    let _ = slint::invoke_from_event_loop(move || {
        HANDLER.with(|h| {
            if let Some(handler) = h.borrow().as_ref() {
                handler(event);
            }
        });
    });
}

fn run(rx: Receiver<Command>) {
    let client = Client::new();
    while let Ok(command) = rx.recv() {
        match command {
            Command::Poll { code, rev } => {
                send(Event::Overlay(client.get_overlay(&code, rev.as_deref())));
            }
            Command::Join { code, name, role } => {
                send(Event::Joined(join(&client, &code, &name, &role)));
            }
            Command::Assets { lang, ids } => {
                let wanted: HashSet<String> = ids.into_iter().collect();
                let names = assets::resolve_names(client.agent(), lang, &wanted);
                let mut resolved = Vec::with_capacity(names.len());
                let mut list = Vec::with_capacity(names.len());
                for (id, (full, name)) in names {
                    resolved.push((id.clone(), full.clone()));
                    list.push((id, full, name));
                }
                send(Event::Names(lang, list));
                // Ids sin nombre conocido igual se intentan con el id original.
                let mut icons: Vec<(String, String)> = resolved;
                for id in &wanted {
                    if !icons.iter().any(|(orig, _)| orig == id) {
                        icons.push((id.clone(), id.clone()));
                    }
                }
                for (orig, full) in icons {
                    if let Some(pixels) = assets::load_icon(client.agent(), &full) {
                        send(Event::Icon(orig, pixels));
                    }
                }
            }
        }
    }
}

fn status_message(status: &str) -> String {
    match status {
        "invalid" => "Ese código no existe. Revisalo con quien lidera la party.".into(),
        "expired" => "Esa asignación ya venció. Pedile a quien lidera que cree una nueva.".into(),
        "rate_limited" => "Demasiados intentos. Probá de nuevo en unos minutos.".into(),
        "name_taken" => "Ese nombre ya está anotado en la asignación.".into(),
        "full" => "La asignación está llena.".into(),
        "bad_name" => "El nombre tiene que tener entre 2 y 32 caracteres.".into(),
        other => format!("Respuesta inesperada del servidor ({other})."),
    }
}

fn api_message(error: ApiError) -> String {
    match error {
        ApiError::NotConfigured => {
            "Esta versión del overlay no tiene configurado el servidor de AlbionCompo.".into()
        }
        ApiError::Network(_) => "No hay conexión con AlbionCompo. Probá de nuevo.".into(),
    }
}

/// Entra a la asignación: si el nombre ya está anotado (por ejemplo, desde la
/// web o una sesión anterior) solo lo recuerda; si no, lo anota.
fn join(client: &Client, code: &str, name: &str, role: &str) -> Result<Overlay, String> {
    let overlay = client.get_overlay(code, None).map_err(api_message)?;
    if overlay.status != "ok" {
        return Err(status_message(&overlay.status));
    }
    let me = normalize_name(name);
    if overlay
        .entries
        .iter()
        .any(|e| normalize_name(&e.name) == me)
    {
        return Ok(overlay);
    }
    let joined: api::Join = client.join_overlay(code, name, role).map_err(api_message)?;
    if joined.status != "ok" {
        return Err(status_message(&joined.status));
    }
    client
        .get_overlay(code, None)
        .map_err(api_message)
        .and_then(|o| {
            if o.status == "ok" {
                Ok(o)
            } else {
                Err(status_message(&o.status))
            }
        })
}
