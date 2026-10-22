//! Configuración local del jugador y direcciones de los servicios.
//!
//! Se guarda en %APPDATA%\albioncompo-overlay\config.txt como líneas
//! `clave=valor`. Solo el código de la asignación (que vence a las 4 h),
//! el nombre, el rol y el idioma: nada de contraseñas ni claves.

use std::path::PathBuf;

const APP_DIR: &str = "albioncompo-overlay";

/// Proyecto de Supabase y su clave pública (anon). La anon key es la misma que
/// publica la web en su JavaScript: no es un secreto. Se inyectan al compilar
/// desde las variables del repositorio en GitHub Actions y se pueden pisar con
/// variables de entorno para probar contra otro servidor.
pub fn supabase_url() -> Option<String> {
    env_or(
        "ALBIONCOMPO_SUPABASE_URL",
        option_env!("ALBIONCOMPO_SUPABASE_URL"),
    )
}

pub fn supabase_anon_key() -> Option<String> {
    env_or(
        "ALBIONCOMPO_SUPABASE_ANON_KEY",
        option_env!("ALBIONCOMPO_SUPABASE_ANON_KEY"),
    )
}

/// Web de AlbionCompo: de ahí salen los nombres de ítems por idioma (archivos
/// estáticos en Vercel, no consumen Supabase).
pub fn site_url() -> String {
    env_or("ALBIONCOMPO_SITE_URL", option_env!("ALBIONCOMPO_SITE_URL"))
        .unwrap_or_else(|| "https://albioncompo.com".into())
}

/// Servidor oficial de íconos de Albion.
pub fn render_url() -> String {
    env_or("ALBIONCOMPO_RENDER_URL", None)
        .unwrap_or_else(|| "https://render.albiononline.com".into())
}

fn env_or(name: &str, built_in: Option<&'static str>) -> Option<String> {
    std::env::var(name)
        .ok()
        .or_else(|| built_in.map(str::to_owned))
        .map(|v| v.trim().trim_end_matches('/').to_owned())
        .filter(|v| !v.is_empty())
}

/// Carpeta de datos: %APPDATA% en Windows, $XDG_CONFIG_HOME o ~/.config en
/// el resto (solo para desarrollo).
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join(APP_DIR)
}

/// Caché de íconos y nombres: %LOCALAPPDATA% en Windows (no viaja con el perfil).
pub fn cache_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join(APP_DIR))
        .unwrap_or_else(|| data_dir().join("cache"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Es,
    En,
    Pt,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Es => "es",
            Lang::En => "en",
            Lang::Pt => "pt",
        }
    }

    pub fn from_code(code: &str) -> Lang {
        match code {
            "en" => Lang::En,
            "pt" => Lang::Pt,
            _ => Lang::Es,
        }
    }

    pub fn index(self) -> i32 {
        match self {
            Lang::Es => 0,
            Lang::En => 1,
            Lang::Pt => 2,
        }
    }

    pub fn from_index(i: i32) -> Lang {
        match i {
            1 => Lang::En,
            2 => Lang::Pt,
            _ => Lang::Es,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    pub code: String,
    pub name: String,
    pub role: String,
    pub lang: Lang,
    /// Ya eligió idioma alguna vez (si no, el ingreso arranca por ahí).
    pub lang_chosen: bool,
    /// La tira con la build queda siempre visible (si no, se despliega desde el botón).
    pub pin_strip: bool,
    /// El panel de la party queda siempre visible.
    pub pin_party: bool,
    // App de escritorio. Van en negativo para que el valor por defecto
    // (`false`) sea el comportamiento normal.
    /// Arranca solo en la bandeja, sin abrir la ventana.
    pub start_in_tray: bool,
    /// No esconde la ventana cuando se abre Albion.
    pub keep_on_game: bool,
    /// Cerrar la ventana cierra todo (si no, sigue en la bandeja).
    pub quit_on_close: bool,
}

impl Config {
    pub fn path() -> PathBuf {
        data_dir().join("config.txt")
    }

    /// Sin archivo (o sin idioma guardado) usa el idioma de Windows.
    pub fn load() -> Config {
        let text = std::fs::read_to_string(Self::path()).unwrap_or_default();
        let mut config = Self::parse(&text);
        if config.lang_chosen {
            return config;
        }
        config.lang = crate::i18n::system_lang();
        config
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.serialize())
    }

    pub fn has_assignment(&self) -> bool {
        !self.code.is_empty() && !self.name.is_empty()
    }

    fn parse(text: &str) -> Config {
        let mut config = Config::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().to_owned();
            match key.trim() {
                "code" => config.code = value,
                "name" => config.name = value,
                "role" => config.role = value,
                "lang" => {
                    config.lang = Lang::from_code(&value);
                    config.lang_chosen = true;
                }
                "pin_strip" => config.pin_strip = value == "1",
                "pin_party" => config.pin_party = value == "1",
                "start_in_tray" => config.start_in_tray = value == "1",
                "keep_on_game" => config.keep_on_game = value == "1",
                "quit_on_close" => config.quit_on_close = value == "1",
                _ => {}
            }
        }
        config
    }

    fn serialize(&self) -> String {
        // Los saltos de línea romperían el formato; no deberían aparecer nunca.
        let clean = |s: &str| s.replace(['\r', '\n'], " ");
        format!(
            "code={}\nname={}\nrole={}\nlang={}\npin_strip={}\npin_party={}\n\
             start_in_tray={}\nkeep_on_game={}\nquit_on_close={}\n",
            clean(&self.code),
            clean(&self.name),
            clean(&self.role),
            self.lang.code(),
            u8::from(self.pin_strip),
            u8::from(self.pin_party),
            u8::from(self.start_in_tray),
            u8::from(self.keep_on_game),
            u8::from(self.quit_on_close),
        )
    }
}

/// Mismo alfabeto que generateAccessCode() en la web (sin I, O, 0 ni 1).
pub fn normalize_code(input: &str) -> Option<String> {
    let code = input.trim().to_uppercase();
    let valid = code.len() == 6
        && code
            .chars()
            .all(|c| matches!(c, 'A'..='H' | 'J'..='N' | 'P'..='Z' | '2'..='9'));
    valid.then_some(code)
}

/// Igual que normalizeName() de la web: recorta, colapsa espacios, minúsculas.
pub fn normalize_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let config = Config {
            code: "D75TGV".into(),
            name: "Dusius".into(),
            role: "HEALER".into(),
            lang: Lang::Pt,
            lang_chosen: true,
            pin_strip: true,
            pin_party: false,
            start_in_tray: false,
            keep_on_game: true,
            quit_on_close: true,
        };
        assert_eq!(Config::parse(&config.serialize()), config);
    }

    #[test]
    fn codes() {
        assert_eq!(normalize_code(" d75tgv "), Some("D75TGV".into()));
        assert_eq!(normalize_code("D75TG0"), None);
        assert_eq!(normalize_code("D75TGVX"), None);
        assert_eq!(normalize_code("IIIIII"), None);
    }

    #[test]
    fn names() {
        assert_eq!(normalize_name("  Dusius   The  Best "), "dusius the best");
    }
}
