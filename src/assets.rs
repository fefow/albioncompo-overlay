//! Nombres de ítems por idioma e íconos, con caché en disco.
//!
//! - Nombres: los mismos `items_<idioma>.txt` que usa la web (estáticos en
//!   Vercel). Se bajan una vez por semana y de cada archivo solo se guardan en
//!   memoria los ítems que se muestran.
//! - Íconos: render.albiononline.com, en 64 px, guardados en disco para no
//!   volver a pedirlos.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::config::{self, Lang};

/// La web cambia este número cuando regenera los archivos de nombres.
const ITEMS_VERSION: &str = "v3";
const NAMES_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);
const ICON_SIZE: u32 = 64;
const TIER_PREFERENCE: [&str; 7] = ["T8_", "T7_", "T6_", "T5_", "T4_", "T3_", "T2_"];

/// Ícono decodificado en RGBA8.
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn names_path(lang: Lang) -> PathBuf {
    config::cache_dir()
        .join("names")
        .join(format!("items_{}_{ITEMS_VERSION}.txt", lang.code()))
}

fn names_url(lang: Lang) -> String {
    let file = match lang {
        Lang::En => "items.txt".to_owned(),
        other => format!("items_{}.txt", other.code()),
    };
    format!("{}/{file}?v={ITEMS_VERSION}", config::site_url())
}

fn is_fresh(path: &Path, max_age: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < max_age)
}

fn download(agent: &ureq::Agent, url: &str, limit: u64) -> Option<Vec<u8>> {
    agent
        .get(url)
        .call()
        .ok()?
        .into_body()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .ok()
}

fn write_atomic(path: &Path, bytes: &[u8]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Texto del archivo de nombres: de la caché si es reciente, si no de la web
/// (y si la web falla, la caché vieja).
fn names_text(agent: &ureq::Agent, lang: Lang) -> Option<String> {
    let path = names_path(lang);
    if !is_fresh(&path, NAMES_MAX_AGE)
        && let Some(bytes) = download(agent, &names_url(lang), 8 * 1024 * 1024)
    {
        write_atomic(&path, &bytes);
    }
    std::fs::read_to_string(&path).ok()
}

/// Comida y pociones a veces se guardan sin tier ("MEAL_OMELETTE"): la web
/// elige el primer tier que existe, de T8 para abajo. Acá igual.
fn candidates(id: &str) -> Vec<String> {
    let has_tier = id.len() > 3 && id.starts_with('T') && id.as_bytes()[2] == b'_';
    if has_tier || !(id.contains("MEAL_") || id.contains("POTION_")) {
        let mut list = vec![id.to_owned()];
        if let Some((base, _)) = id.split_once('@') {
            list.push(base.to_owned());
        }
        return list;
    }
    let base = id.split('@').next().unwrap_or(id);
    TIER_PREFERENCE
        .iter()
        .map(|t| format!("{t}{base}"))
        .collect()
}

/// Busca los nombres de `ids` en el idioma pedido (y en inglés si falta).
/// Devuelve id original -> (id resuelto con tier, nombre).
pub fn resolve_names(
    agent: &ureq::Agent,
    lang: Lang,
    ids: &HashSet<String>,
) -> HashMap<String, (String, String)> {
    let wanted: HashSet<String> = ids.iter().flat_map(|id| candidates(id)).collect();
    let mut found = HashMap::new();
    let langs: &[Lang] = if lang == Lang::En {
        &[Lang::En]
    } else {
        &[lang, Lang::En]
    };
    for &l in langs {
        if let Some(text) = names_text(agent, l) {
            parse_names(&text, &wanted, &mut found);
        }
    }
    ids.iter()
        .filter_map(|id| {
            candidates(id)
                .into_iter()
                .find_map(|c| found.get(&c).map(|name| (c.clone(), name.clone())))
                .map(|(resolved, name)| {
                    // Si el nombre salió del id sin encantamiento, el ícono
                    // igual se pide con el encantamiento original.
                    let resolved =
                        if id.contains('@') && !resolved.contains('@') && id.starts_with(&resolved)
                        {
                            id.clone()
                        } else {
                            resolved
                        };
                    (id.clone(), (resolved, name))
                })
        })
        .collect()
}

/// Líneas con el formato "  470: T4_POTION_HEAL   : Poción de Curación".
/// Solo guarda los ids pedidos; no pisa los que ya estaban (idioma preferido primero).
fn parse_names(text: &str, wanted: &HashSet<String>, out: &mut HashMap<String, String>) {
    for line in text.lines() {
        let mut parts = line.splitn(3, ':');
        let (Some(num), Some(id), Some(name)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if num.trim().parse::<u32>().is_err() {
            continue;
        }
        let id = id.trim().to_uppercase();
        if wanted.contains(&id) && !out.contains_key(&id) {
            let name = name.trim();
            if !name.is_empty() {
                out.insert(id, name.to_owned());
            }
        }
    }
}

fn icon_path(id: &str) -> PathBuf {
    let safe: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '@' {
                c
            } else {
                '_'
            }
        })
        .collect();
    config::cache_dir()
        .join("icons")
        .join(format!("{safe}_{ICON_SIZE}.png"))
}

/// Ícono de un ítem: de la caché o de render.albiononline.com.
pub fn load_icon(agent: &ureq::Agent, id: &str) -> Option<Pixels> {
    let path = icon_path(id);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            let url = format!(
                "{}/v1/item/{}.png?size={ICON_SIZE}",
                config::render_url(),
                id.replace('@', "%40")
            );
            let bytes = download(agent, &url, 512 * 1024)?;
            // Solo se guarda si de verdad es un PNG que se puede leer.
            let pixels = decode_png(&bytes)?;
            write_atomic(&path, &bytes);
            return Some(pixels);
        }
    };
    decode_png(&bytes)
}

fn decode_png(bytes: &[u8]) -> Option<Pixels> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Pixels {
        width: info.width,
        height: info.height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names() {
        let text = "  470: T4_POTION_HEAL                     : Poción de Curación\n\
                    471: T4_POTION_HEAL@1                     : Poción de Curación\n\
                    5: T8_MAIN_HOLYSTAFF : Bastón sagrado del anciano\n\
                    garbage line\n";
        let wanted: HashSet<String> = ["T4_POTION_HEAL@1", "T8_MAIN_HOLYSTAFF"]
            .into_iter()
            .map(String::from)
            .collect();
        let mut out = HashMap::new();
        parse_names(text, &wanted, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out["T8_MAIN_HOLYSTAFF"], "Bastón sagrado del anciano");
    }

    #[test]
    fn untiered_food_tries_tiers() {
        let c = candidates("MEAL_OMELETTE@1");
        assert_eq!(c[0], "T8_MEAL_OMELETTE");
        assert_eq!(c.len(), 7);
        assert_eq!(candidates("T8_2H_BOW@2"), vec!["T8_2H_BOW@2", "T8_2H_BOW"]);
    }

    #[test]
    fn decodes_png() {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[255, 0, 0, 0, 255, 0]).unwrap();
        }
        let px = decode_png(&bytes).unwrap();
        assert_eq!((px.width, px.height), (2, 1));
        assert_eq!(px.rgba, vec![255, 0, 0, 255, 0, 255, 0, 255]);
    }
}
