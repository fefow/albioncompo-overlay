//! Cómo se arma la tira a partir de una build: qué casilleros, en qué orden,
//! tiers, colores y la build de prueba para `--demo`.

use slint::Color;

use crate::api;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    /// Clave del casillero en `variants` de la web.
    fn variant_key(self) -> &'static str {
        match self {
            SlotKind::Mount => "mount",
            SlotKind::Weapon => "weapon",
            SlotKind::Offhand => "secondhand",
            SlotKind::Head => "helm",
            SlotKind::Armor => "armor",
            SlotKind::Shoes => "boot",
            SlotKind::Cape => "cape",
            SlotKind::Potion => "potion",
            SlotKind::Food => "food",
        }
    }

    /// Ícono de reemplazo (comandos SVG, viewBox 24x24) mientras carga el real.
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

/// Un casillero de la build: ítem principal y alternativa (las "variants" de la web).
#[derive(Clone, Debug, PartialEq)]
pub struct SlotItem {
    pub kind: SlotKind,
    /// Id normalizado (mayúsculas), vacío si el casillero no lleva nada.
    pub id: String,
    pub variant: Option<String>,
}

fn norm(id: Option<&String>) -> String {
    id.map(|s| s.trim().to_uppercase()).unwrap_or_default()
}

/// Igual que resolveSlotDisplay() de la web: en composiciones viejas el
/// encantamiento quedó invertido entre el ítem y su variante.
fn resolve_slot(item: String, variant: Option<String>) -> (String, Option<String>) {
    let (Some(v), false) = (&variant, item.is_empty()) else {
        return (item, variant);
    };
    let split = |s: &str| -> (String, bool) {
        match s.split_once('@') {
            Some((b, _)) => (b.to_owned(), true),
            None => (s.to_owned(), false),
        }
    };
    let (ib, ie) = split(&item);
    let (vb, ve) = split(v);
    if ib == vb && ie && !ve {
        let v = v.clone();
        return (v, Some(item));
    }
    (item, variant)
}

/// Casilleros en el orden de la tira. Con rol de montura de batalla la
/// montura va primero (es el ítem principal), si no no se muestra.
pub fn slot_items(build: &api::Build, role: &str) -> Vec<SlotItem> {
    let mut kinds = Vec::with_capacity(9);
    if is_mount_role(role) {
        kinds.push(SlotKind::Mount);
    }
    kinds.extend([
        SlotKind::Weapon,
        SlotKind::Offhand,
        SlotKind::Head,
        SlotKind::Armor,
        SlotKind::Shoes,
        SlotKind::Cape,
        SlotKind::Potion,
        SlotKind::Food,
    ]);
    kinds
        .into_iter()
        .map(|kind| {
            let raw = match kind {
                SlotKind::Mount => &build.mount,
                SlotKind::Weapon => &build.weapon,
                SlotKind::Offhand => &build.secondhand,
                SlotKind::Head => &build.helm,
                SlotKind::Armor => &build.armor,
                SlotKind::Shoes => &build.boot,
                SlotKind::Cape => &build.cape,
                SlotKind::Potion => &build.potion,
                SlotKind::Food => &build.food,
            };
            let variant =
                Some(norm(build.variants.get(kind.variant_key()))).filter(|v| !v.is_empty());
            let (id, variant) = resolve_slot(norm(raw.as_ref()), variant);
            SlotItem { kind, id, variant }
        })
        .collect()
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
        _ => Color::from_rgb_u8(0xd6, 0xad, 0x5e),
    }
}

/// Mismo criterio que normalizeManualCompositionRole() de la web.
pub fn role_color(role: &str) -> Color {
    let r = role.to_uppercase();
    if r.contains("RDPS") || r.contains("RANGE") {
        Color::from_rgb_u8(0xff, 0x9a, 0x62)
    } else if r.contains("MDPS") || r.contains("MELEE") {
        Color::from_rgb_u8(0xff, 0x7a, 0x6b)
    } else if r.contains("TANK") {
        Color::from_rgb_u8(0x6a, 0xa8, 0xff)
    } else if r.contains("SUPPORT") {
        Color::from_rgb_u8(0xf0, 0xa8, 0x30)
    } else if r.contains("HEAL") {
        Color::from_rgb_u8(0x45, 0xd6, 0xa0)
    } else if r.contains("DPS") {
        Color::from_rgb_u8(0xff, 0x9a, 0x62)
    } else {
        Color::from_rgb_u8(0xc9, 0xb4, 0x8f)
    }
}

pub fn is_mount_role(role: &str) -> bool {
    role.to_uppercase().contains("MOUNT")
}

/// Asignación de prueba para `--demo`: misma forma que la respuesta de la RPC.
pub fn demo_overlay(con_montura: bool) -> api::Overlay {
    let build = |id: &str, role: &str, weapon: &str, extra: &[(&str, &str)]| {
        let mut b = api::Build {
            id: Some(id.into()),
            role: Some(role.into()),
            weapon: Some(weapon.into()),
            ..Default::default()
        };
        for (k, v) in extra {
            let v = Some(v.to_string());
            match *k {
                "secondhand" => b.secondhand = v,
                "helm" => b.helm = v,
                "armor" => b.armor = v,
                "boot" => b.boot = v,
                "cape" => b.cape = v,
                "potion" => b.potion = v,
                "food" => b.food = v,
                "mount" => b.mount = v,
                _ => {}
            }
        }
        b
    };
    let mut healer = build(
        "b1",
        if con_montura {
            "BATTLE MOUNT"
        } else {
            "HEALER"
        },
        "T8_MAIN_HOLYSTAFF@1",
        &[
            ("secondhand", "T8_OFF_BOOK"),
            ("helm", "T8_HEAD_CLOTH_SET2"),
            ("armor", "T8_ARMOR_CLOTH_SET2"),
            ("boot", "T8_SHOES_CLOTH_SET2"),
            ("cape", "T8_CAPE@2"),
            ("potion", "T7_POTION_HEAL"),
            ("food", "T8_MEAL_OMELETTE"),
            ("mount", "T8_MOUNT_MAMMOTH_BATTLE@1"),
        ],
    );
    healer.name = Some("Sagrado ZvZ".into());
    healer
        .variants
        .insert("weapon".into(), "T8_2H_HOLYSTAFF@1".into());
    let entry = |name: &str, role: &str, build: Option<&str>| api::Entry {
        name: name.into(),
        role: Some(role.into()),
        build_id: build.map(str::to_owned),
    };
    api::Overlay {
        status: "ok".into(),
        rev: Some("demo".into()),
        retry_after: None,
        assignment: api::Named {
            name: Some("Portal ZvZ".into()),
        },
        composition: api::Named {
            name: Some("Compo de prueba".into()),
        },
        entries: vec![
            entry("Dusius", "HEALER", Some("b1")),
            entry("RKFigue", "TANK", Some("b2")),
            entry("MegaPikachu", "SUPPORT", Some("b3")),
            entry("SaidST", "RANGE DPS", Some("b4")),
            entry("sitzz", "MELEE DPS", None),
        ],
        builds: vec![
            healer,
            build("b2", "TANK", "T8_MAIN_MACE@1", &[]),
            build("b3", "SUPPORT", "T8_2H_ARCANESTAFF", &[]),
            build("b4", "RANGE DPS", "T8_2H_LONGBOW@2", &[]),
        ],
    }
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

    #[test]
    fn slots_order_and_mount() {
        let demo = demo_overlay(false);
        let b = &demo.builds[0];
        let slots = slot_items(b, "HEALER");
        assert_eq!(slots.len(), 8);
        assert_eq!(slots[0].kind, SlotKind::Weapon);
        assert_eq!(slots[0].variant.as_deref(), Some("T8_2H_HOLYSTAFF@1"));
        let slots = slot_items(b, "BATTLE MOUNT");
        assert_eq!(slots.len(), 9);
        assert_eq!(slots[0].kind, SlotKind::Mount);
    }

    #[test]
    fn legacy_inverted_enchant() {
        let (id, v) = resolve_slot("T8_2H_BOW@1".into(), Some("T8_2H_BOW".into()));
        assert_eq!(id, "T8_2H_BOW");
        assert_eq!(v.as_deref(), Some("T8_2H_BOW@1"));
    }
}
