//! Datos de prueba fijos para el prototipo 2. Cuando conectemos la red, esto
//! sale de la RPC `get_overlay` y de los JSON de nombres por idioma.

use slint::Color;

#[derive(Clone, Copy)]
pub enum SlotKind {
    Mount,
    Weapon,
    Offhand,
    Head,
    Armor,
    Shoes,
    Cape,
    Potion,
    Food,
}

impl SlotKind {
    pub fn label(self) -> &'static str {
        match self {
            SlotKind::Mount => "Montura",
            SlotKind::Weapon => "Arma",
            SlotKind::Offhand => "Mano secundaria",
            SlotKind::Head => "Casco",
            SlotKind::Armor => "Armadura",
            SlotKind::Shoes => "Botas",
            SlotKind::Cape => "Capa",
            SlotKind::Potion => "Poción",
            SlotKind::Food => "Comida",
        }
    }

    /// Ícono de reemplazo (comandos SVG, viewBox 24x24) del diseño v2.
    pub fn icon(self) -> &'static str {
        match self {
            SlotKind::Mount => "M4 18l2-6 4-2 2-4 3 1 1 3 3 1v3l-3 1-2 3H8l-1 2z",
            SlotKind::Weapon => "M5 19L14 10M13 7.5a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0 -7 0M4 20l2-2",
            SlotKind::Offhand => "M12 3l7 3v5c0 5-3 8-7 10c-4-2-7-5-7-10V6z",
            SlotKind::Head => "M12 3C8 3 5 6 5 10v5l3 3h8l3-3v-5c0-4-3-7-7-7zM8.5 12h7",
            SlotKind::Armor => "M8 4L4 7l2 4 2-1v10h8V10l2 1 2-4-4-3-2 2h-4L8 4z",
            SlotKind::Shoes => "M8 4h6v7l5 3v4H6v-5l2-1V4z",
            SlotKind::Cape => "M6 4h12l2 16-8-3-8 3L6 4z",
            SlotKind::Potion => "M10 3h4M11 3v5l-4.5 8A3 3 0 0 0 9 20h6a3 3 0 0 0 2.5-4L13 8V3",
            SlotKind::Food => {
                "M3 13h18M5 13a7 7 0 0 0 14 0M9 6c0 1.5 1.5 1.5 1.5 3M14 6c0 1.5 1.5 1.5 1.5 3"
            }
        }
    }
}

pub struct Item {
    pub kind: SlotKind,
    /// Id de ao-bin-dumps, ej. "T8_HEAD_CLOTH_SET2@1". Vacío si no lleva.
    pub id: &'static str,
    /// Nombre localizado (en el prototipo, a mano).
    pub name: &'static str,
}

pub struct Build {
    pub role: &'static str,
    pub items: Vec<Item>,
}

/// Build de prueba. Con `con_montura` simula el rol de montura de batalla (9 casilleros).
pub fn sample_build(con_montura: bool) -> Build {
    let role = if con_montura {
        "BATTLE MOUNT"
    } else {
        "HEALER"
    };
    let mut items = Vec::with_capacity(9);
    // La montura solo es un casillero propio en el rol de montura de batalla.
    if is_mount_role(role) {
        items.push(Item {
            kind: SlotKind::Mount,
            id: "T8_MOUNT_MAMMOTH_BATTLE@1",
            name: "Mamut de batalla",
        });
    }
    items.extend([
        Item {
            kind: SlotKind::Weapon,
            id: "T8_MAIN_HOLYSTAFF@1",
            name: "Bastón sagrado del anciano",
        },
        Item {
            kind: SlotKind::Offhand,
            id: "T8_OFF_BOOK",
            name: "Tomo de hechizos del anciano",
        },
        Item {
            kind: SlotKind::Head,
            id: "T8_HEAD_CLOTH_SET2",
            name: "Capucha de clérigo del anciano",
        },
        Item {
            kind: SlotKind::Armor,
            id: "T8_ARMOR_CLOTH_SET2",
            name: "Túnica de clérigo del anciano",
        },
        Item {
            kind: SlotKind::Shoes,
            id: "T8_SHOES_CLOTH_SET2",
            name: "Sandalias de clérigo del anciano",
        },
        Item {
            kind: SlotKind::Cape,
            id: "T8_CAPE@2",
            name: "Capa del anciano",
        },
        Item {
            kind: SlotKind::Potion,
            id: "T7_POTION_HEAL",
            name: "Poción curativa mayor",
        },
        Item {
            kind: SlotKind::Food,
            id: "T8_MEAL_OMELETTE",
            name: "Tortilla de cerdo",
        },
    ]);
    Build { role, items }
}

/// "T8_MAIN_HOLYSTAFF@1" -> ("8.1", 1). Sin tier reconocible -> ("", 0).
pub fn tier_of(id: &str) -> (String, u8) {
    let (base, enchant) = match id.split_once('@') {
        Some((b, e)) => (b, e.parse::<u8>().unwrap_or(0)),
        None => (id, 0),
    };
    let tier = base
        .strip_prefix('T')
        .and_then(|rest| rest.split('_').next())
        .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()));
    match tier {
        Some(t) if enchant > 0 => (format!("{t}.{enchant}"), enchant),
        Some(t) => (t.to_string(), 0),
        None => (String::new(), 0),
    }
}

pub fn enchant_color(enchant: u8) -> Color {
    match enchant {
        1 => Color::from_rgb_u8(0x6b, 0xe0, 0x8f),
        2 => Color::from_rgb_u8(0x5a, 0xa9, 0xff),
        3 => Color::from_rgb_u8(0xb0, 0x7c, 0xff),
        4 => Color::from_rgb_u8(0xf0, 0xd0, 0x60),
        _ => Color::from_rgb_u8(0xf0, 0xa8, 0x30),
    }
}

/// Mismo criterio que normalizeManualCompositionRole() de la web.
pub fn role_color(role: &str) -> Color {
    let r = role.to_uppercase();
    if r.contains("TANK") {
        Color::from_rgb_u8(0x6a, 0xa8, 0xff)
    } else if r.contains("SUPPORT") {
        Color::from_rgb_u8(0xf0, 0xa8, 0x30)
    } else if r.contains("HEAL") {
        Color::from_rgb_u8(0x45, 0xd6, 0xa0)
    } else if r.contains("DPS") || r.contains("RANGE") {
        Color::from_rgb_u8(0xff, 0x9a, 0x62)
    } else {
        Color::from_rgb_u8(0xaa, 0xb6, 0xcc)
    }
}

pub fn is_mount_role(role: &str) -> bool {
    role.to_uppercase().contains("MOUNT")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers() {
        assert_eq!(tier_of("T8_MAIN_HOLYSTAFF@1"), ("8.1".into(), 1));
        assert_eq!(tier_of("T8_HEAD_CLOTH_SET2"), ("8".into(), 0));
        assert_eq!(tier_of("T4_2H_TOOL_PICK@3"), ("4.3".into(), 3));
        assert_eq!(tier_of(""), ("".into(), 0));
        assert_eq!(tier_of("UNIQUE_HIDEOUT"), ("".into(), 0));
    }

    #[test]
    fn mount_role() {
        assert!(is_mount_role("Battle Mount"));
        assert!(!is_mount_role("HEALER"));
    }
}
