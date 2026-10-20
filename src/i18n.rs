//! Textos de la interfaz en español, inglés y portugués (los mismos idiomas
//! que la web). Los de las ventanas de Slint van al global `Tr`; el resto
//! (estados, errores, tooltips) se arma desde Rust con `text()`.

use crate::config::Lang;

/// Roles tal como se guardan en la asignación (en inglés, como los entiende la web).
pub const ROLES: [&str; 6] = [
    "TANK",
    "HEALER",
    "SUPPORT",
    "RANGE DPS",
    "MELEE DPS",
    "BATTLE MOUNT",
];

#[cfg_attr(not(windows), allow(dead_code))]
pub struct Text {
    // Ventana de ingreso.
    pub code_title: &'static str,
    pub code_help: &'static str,
    pub code_label: &'static str,
    pub continue_: &'static str,
    pub close: &'static str,
    pub back: &'static str,
    pub register_title: &'static str,
    pub register_help: &'static str,
    pub name_label: &'static str,
    pub name_placeholder: &'static str,
    pub roles_label: &'static str,
    pub join: &'static str,
    pub leave: &'static str,
    pub lang_label: &'static str,
    pub demo: &'static str,
    /// Mismo orden que `ROLES`; son los nombres que usa la web en cada idioma.
    pub roles: [&'static str; 6],
    pub mount_short: &'static str,
    pub no_role: &'static str,
    // Validación y errores.
    pub bad_code: &'static str,
    pub bad_name: &'static str,
    pub err_invalid: &'static str,
    pub err_expired: &'static str,
    pub err_rate_limited: &'static str,
    pub err_name_taken: &'static str,
    pub err_full: &'static str,
    pub err_network: &'static str,
    pub err_not_configured: &'static str,
    pub err_unexpected: &'static str,
    // Tira.
    pub st_no_code: &'static str,
    pub st_loading: &'static str,
    pub st_waiting: &'static str,
    pub st_not_joined: &'static str,
    pub st_expired: &'static str,
    pub st_invalid: &'static str,
    pub st_offline: &'static str,
    pub st_rate_limited: &'static str,
    pub st_not_configured: &'static str,
    // Tooltip y party.
    /// Mismo orden que `data::SlotKind`.
    pub slots: [&'static str; 9],
    pub no_item: &'static str,
    pub alternative: &'static str,
    pub assignment: &'static str,
    /// "{n} anotados · {m} con build"
    pub party_summary: &'static str,
    /// "{n} anotados"
    pub members: &'static str,
    // Menú de la bandeja y del botón flotante.
    pub menu_open: &'static str,
    pub menu_party: &'static str,
    pub menu_hide: &'static str,
    pub menu_click_through: &'static str,
    pub menu_pill: &'static str,
    pub menu_quit: &'static str,
}

const ES: Text = Text {
    code_title: "Entrá a tu party",
    code_help: "Pedile el código de 6 caracteres a quien armó la asignación en albioncompo.com.",
    code_label: "CÓDIGO DE LA ASIGNACIÓN",
    continue_: "CONTINUAR",
    close: "CERRAR",
    back: "← Cambiar código",
    register_title: "Anotate en la asignación",
    register_help: "Cuando el líder te asigne una build la vas a ver arriba del juego.",
    name_label: "TU NOMBRE EN EL JUEGO",
    name_placeholder: "Ej.: Dusius",
    roles_label: "ROLES QUE PODÉS JUGAR · PODÉS ELEGIR VARIOS",
    join: "ENTRAR",
    leave: "SALIR",
    lang_label: "IDIOMA",
    demo: "Modo demo: no se conecta con AlbionCompo.",
    roles: [
        "Tank",
        "Healer",
        "Support",
        "DPS rango",
        "DPS melee",
        "Montura de batalla",
    ],
    mount_short: "MONTURA",
    no_role: "SIN ROL",
    bad_code: "El código tiene 6 letras y números, por ejemplo D75TGV.",
    bad_name: "Poné tu nombre del juego (entre 2 y 32 caracteres).",
    err_invalid: "Ese código no existe. Revisalo con quien lidera la party.",
    err_expired: "Esa asignación ya venció. Pedile a quien lidera que cree una nueva.",
    err_rate_limited: "Demasiados intentos. Probá de nuevo en unos minutos.",
    err_name_taken: "Ese nombre ya está anotado en la asignación.",
    err_full: "La asignación está llena.",
    err_network: "No hay conexión con AlbionCompo. Probá de nuevo.",
    err_not_configured: "Esta versión del overlay no tiene configurado el servidor de AlbionCompo.",
    err_unexpected: "Respuesta inesperada del servidor.",
    st_no_code: "Clic acá para ingresar el código de la party",
    st_loading: "Cargando la asignación",
    st_waiting: "Esperando que el líder te asigne una build",
    st_not_joined: "No figurás en la asignación · clic para entrar",
    st_expired: "La asignación venció · clic para cambiar el código",
    st_invalid: "Código inválido · clic para cambiarlo",
    st_offline: "Sin conexión · reintentando (clic para reintentar ya)",
    st_rate_limited: "Demasiados intentos · esperá unos minutos",
    st_not_configured: "Esta versión no tiene configurado el servidor",
    slots: [
        "Montura",
        "Arma",
        "Mano secundaria",
        "Casco",
        "Armadura",
        "Botas",
        "Capa",
        "Poción",
        "Comida",
    ],
    no_item: "Sin ítem",
    alternative: "Alternativa: ",
    assignment: "Asignación",
    party_summary: "{n} anotados · {m} con build",
    members: "{n} anotados",
    menu_open: "Abrir AlbionCompo (código y registro)\tCtrl+Shift+K",
    menu_party: "Ver la party\tCtrl+Shift+P",
    menu_hide: "Ocultar overlay\tCtrl+Shift+H",
    menu_click_through: "Que los clics atraviesen la tira\tCtrl+Shift+L",
    menu_pill: "Modo píldora\tCtrl+Shift+M",
    menu_quit: "Salir\tCtrl+Shift+Q",
};

const EN: Text = Text {
    code_title: "Join your party",
    code_help: "Ask whoever created the assignment on albioncompo.com for the 6-character code.",
    code_label: "ASSIGNMENT CODE",
    continue_: "CONTINUE",
    close: "CLOSE",
    back: "← Change code",
    register_title: "Sign up for the assignment",
    register_help: "Once the leader assigns you a build, you'll see it on top of the game.",
    name_label: "YOUR IN-GAME NAME",
    name_placeholder: "E.g.: Dusius",
    roles_label: "ROLES YOU CAN PLAY · PICK AS MANY AS YOU LIKE",
    join: "JOIN",
    leave: "LEAVE",
    lang_label: "LANGUAGE",
    demo: "Demo mode: not connected to AlbionCompo.",
    roles: [
        "Tank",
        "Healer",
        "Support",
        "Range DPS",
        "Melee DPS",
        "Battle Mount",
    ],
    mount_short: "MOUNT",
    no_role: "NO ROLE",
    bad_code: "The code has 6 letters and numbers, for example D75TGV.",
    bad_name: "Enter your in-game name (2 to 32 characters).",
    err_invalid: "That code doesn't exist. Check it with your party leader.",
    err_expired: "That assignment has expired. Ask the leader to create a new one.",
    err_rate_limited: "Too many attempts. Try again in a few minutes.",
    err_name_taken: "That name is already signed up for this assignment.",
    err_full: "The assignment is full.",
    err_network: "Can't reach AlbionCompo. Try again.",
    err_not_configured: "This build of the overlay has no AlbionCompo server configured.",
    err_unexpected: "Unexpected server response.",
    st_no_code: "Click here to enter your party code",
    st_loading: "Loading the assignment",
    st_waiting: "Waiting for the leader to assign you a build",
    st_not_joined: "You're not in the assignment · click to join",
    st_expired: "The assignment expired · click to change the code",
    st_invalid: "Invalid code · click to change it",
    st_offline: "Offline · retrying (click to retry now)",
    st_rate_limited: "Too many attempts · wait a few minutes",
    st_not_configured: "This build has no server configured",
    slots: [
        "Mount", "Weapon", "Off-hand", "Head", "Armor", "Shoes", "Cape", "Potion", "Food",
    ],
    no_item: "No item",
    alternative: "Alternative: ",
    assignment: "Assignment",
    party_summary: "{n} signed up · {m} with a build",
    members: "{n} signed up",
    menu_open: "Open AlbionCompo (code and sign-up)\tCtrl+Shift+K",
    menu_party: "Show party\tCtrl+Shift+P",
    menu_hide: "Hide overlay\tCtrl+Shift+H",
    menu_click_through: "Let clicks pass through the bar\tCtrl+Shift+L",
    menu_pill: "Pill mode\tCtrl+Shift+M",
    menu_quit: "Quit\tCtrl+Shift+Q",
};

const PT: Text = Text {
    code_title: "Entre na sua party",
    code_help: "Peça o código de 6 caracteres para quem criou a atribuição no albioncompo.com.",
    code_label: "CÓDIGO DA ATRIBUIÇÃO",
    continue_: "CONTINUAR",
    close: "FECHAR",
    back: "← Trocar código",
    register_title: "Inscreva-se na atribuição",
    register_help: "Quando o líder atribuir uma build, você vai vê-la em cima do jogo.",
    name_label: "SEU NOME NO JOGO",
    name_placeholder: "Ex.: Dusius",
    roles_label: "FUNÇÕES QUE VOCÊ JOGA · ESCOLHA QUANTAS QUISER",
    join: "ENTRAR",
    leave: "SAIR",
    lang_label: "IDIOMA",
    demo: "Modo demo: não se conecta ao AlbionCompo.",
    roles: [
        "Tank",
        "Healer",
        "Support",
        "DPS ranged",
        "DPS melee",
        "Montaria de batalha",
    ],
    mount_short: "MONTARIA",
    no_role: "SEM FUNÇÃO",
    bad_code: "O código tem 6 letras e números, por exemplo D75TGV.",
    bad_name: "Digite seu nome no jogo (de 2 a 32 caracteres).",
    err_invalid: "Esse código não existe. Confira com o líder da party.",
    err_expired: "Essa atribuição expirou. Peça ao líder para criar outra.",
    err_rate_limited: "Muitas tentativas. Tente de novo em alguns minutos.",
    err_name_taken: "Esse nome já está inscrito na atribuição.",
    err_full: "A atribuição está cheia.",
    err_network: "Sem conexão com o AlbionCompo. Tente de novo.",
    err_not_configured: "Esta versão do overlay não tem o servidor do AlbionCompo configurado.",
    err_unexpected: "Resposta inesperada do servidor.",
    st_no_code: "Clique aqui para digitar o código da party",
    st_loading: "Carregando a atribuição",
    st_waiting: "Esperando o líder atribuir uma build",
    st_not_joined: "Você não está na atribuição · clique para entrar",
    st_expired: "A atribuição expirou · clique para trocar o código",
    st_invalid: "Código inválido · clique para trocar",
    st_offline: "Sem conexão · tentando de novo (clique para tentar já)",
    st_rate_limited: "Muitas tentativas · espere alguns minutos",
    st_not_configured: "Esta versão não tem o servidor configurado",
    slots: [
        "Montaria",
        "Arma",
        "Mão secundária",
        "Elmo",
        "Armadura",
        "Botas",
        "Capa",
        "Poção",
        "Comida",
    ],
    no_item: "Sem item",
    alternative: "Alternativa: ",
    assignment: "Atribuição",
    party_summary: "{n} inscritos · {m} com build",
    members: "{n} inscritos",
    menu_open: "Abrir AlbionCompo (código e inscrição)\tCtrl+Shift+K",
    menu_party: "Ver a party\tCtrl+Shift+P",
    menu_hide: "Ocultar overlay\tCtrl+Shift+H",
    menu_click_through: "Cliques atravessam a barra\tCtrl+Shift+L",
    menu_pill: "Modo pílula\tCtrl+Shift+M",
    menu_quit: "Sair\tCtrl+Shift+Q",
};

pub fn text(lang: Lang) -> &'static Text {
    match lang {
        Lang::Es => &ES,
        Lang::En => &EN,
        Lang::Pt => &PT,
    }
}

/// Mensaje para un `status` de las RPC (o "network" / "not_configured").
pub fn error(lang: Lang, status: &str) -> &'static str {
    let t = text(lang);
    match status {
        "invalid" => t.err_invalid,
        "expired" => t.err_expired,
        "rate_limited" => t.err_rate_limited,
        "name_taken" => t.err_name_taken,
        "full" => t.err_full,
        "bad_name" => t.bad_name,
        "network" => t.err_network,
        "not_configured" => t.err_not_configured,
        _ => t.err_unexpected,
    }
}

/// Mismo criterio que normalizeManualCompositionRole() de la web: índice en `ROLES`.
pub fn role_index(role: &str) -> Option<usize> {
    let upper = role.trim().to_uppercase();
    let upper = upper.strip_prefix("ROLE.").unwrap_or(&upper);
    let n: String = upper
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if n.is_empty() {
        None
    } else if n == "RDPS" || n == "R DPS" || n.contains("RANGED DPS") || n.contains("RANGE DPS") {
        Some(3)
    } else if n == "MDPS" || n.contains("MELEE DPS") || n.contains("DPS MELEE") {
        Some(4)
    } else if n.contains("TANK") {
        Some(0)
    } else if n.contains("SUPPORT") {
        Some(2)
    } else if n.contains("HEAL") {
        Some(1)
    } else if n.contains("RDPS") || n.contains("RANGED") || n.contains("RANGE") {
        Some(3)
    } else if n.contains("MOUNT") {
        Some(5)
    } else if n.contains("DPS") || n.contains("MELEE") {
        Some(4)
    } else {
        None
    }
}

/// Partes de un rol guardado: "TANK, HEALER" -> ["TANK", "HEALER"].
pub fn split_roles(role: &str) -> Vec<&str> {
    role.split([',', '/'])
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .collect()
}

/// Rol para mostrar, traducido como en la web y en mayúsculas.
/// Con varios roles los une con " · ".
pub fn role_label(lang: Lang, role: &str, short_mount: bool) -> String {
    let t = text(lang);
    split_roles(role)
        .into_iter()
        .map(|part| match role_index(part) {
            Some(5) if short_mount => t.mount_short.to_owned(),
            Some(i) => t.roles[i].to_uppercase(),
            None => part.to_uppercase(),
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Idioma de Windows (o de $LANG en desarrollo) para el primer arranque.
pub fn system_lang() -> Lang {
    #[cfg(windows)]
    {
        // SAFETY: sin parámetros, solo lee la configuración del usuario.
        let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        match id & 0x3ff {
            0x09 => Lang::En,
            0x16 => Lang::Pt,
            _ => Lang::Es,
        }
    }
    #[cfg(not(windows))]
    {
        let lang = std::env::var("LANG").unwrap_or_default();
        if lang.starts_with("en") {
            Lang::En
        } else if lang.starts_with("pt") {
            Lang::Pt
        } else {
            Lang::Es
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_like_the_web() {
        assert_eq!(role_index("RANGE DPS"), Some(3));
        assert_eq!(role_index("role.rdps"), Some(3));
        assert_eq!(role_index("Melee DPS"), Some(4));
        assert_eq!(role_index("BATTLE MOUNT"), Some(5));
        assert_eq!(role_index("healer"), Some(1));
        assert_eq!(role_index("Caller"), None);
    }

    #[test]
    fn several_roles() {
        assert_eq!(
            role_label(Lang::Es, "TANK, RANGE DPS", false),
            "TANK · DPS RANGO"
        );
        assert_eq!(role_label(Lang::En, "BATTLE MOUNT", true), "MOUNT");
        assert_eq!(role_label(Lang::Pt, "Caller", false), "CALLER");
    }
}
