//! Cliente de las RPC del overlay en Supabase (ver sql/overlay.sql).
//! Solo dos llamadas: `get_overlay` (lectura con `rev` para responder
//! "sin cambios") y `join_overlay` (anotarse con nombre y rol).

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::config;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Entry {
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub build_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Build {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub weapon: Option<String>,
    #[serde(default)]
    pub secondhand: Option<String>,
    #[serde(default)]
    pub helm: Option<String>,
    #[serde(default)]
    pub armor: Option<String>,
    #[serde(default)]
    pub boot: Option<String>,
    #[serde(default)]
    pub cape: Option<String>,
    #[serde(default)]
    pub potion: Option<String>,
    #[serde(default)]
    pub food: Option<String>,
    #[serde(default)]
    pub mount: Option<String>,
    #[serde(default)]
    pub variants: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Named {
    #[serde(default)]
    pub name: Option<String>,
}

/// Respuesta de `get_overlay`. Según `status` vienen unos campos u otros.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Overlay {
    pub status: String,
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub retry_after: Option<u64>,
    #[serde(default)]
    pub assignment: Named,
    #[serde(default)]
    pub composition: Named,
    #[serde(default)]
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub builds: Vec<Build>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Join {
    pub status: String,
}

#[derive(Debug)]
pub enum ApiError {
    /// Falta la URL o la clave pública de Supabase en esta compilación.
    NotConfigured,
    /// Sin conexión, timeout o respuesta inesperada.
    Network(String),
}

pub struct Client {
    agent: ureq::Agent,
    base: Option<String>,
    key: Option<String>,
}

impl Client {
    pub fn new() -> Client {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(12)))
            .user_agent(concat!("albioncompo-overlay/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Client {
            agent,
            base: config::supabase_url(),
            key: config::supabase_anon_key(),
        }
    }

    pub fn agent(&self) -> &ureq::Agent {
        &self.agent
    }

    pub fn get_overlay(&self, code: &str, rev: Option<&str>) -> Result<Overlay, ApiError> {
        self.rpc("get_overlay", json!({ "p_code": code, "p_rev": rev }))
    }

    pub fn join_overlay(&self, code: &str, name: &str, role: &str) -> Result<Join, ApiError> {
        let role = (!role.is_empty()).then_some(role);
        self.rpc(
            "join_overlay",
            json!({ "p_code": code, "p_name": name, "p_role": role }),
        )
    }

    fn rpc<T: serde::de::DeserializeOwned>(
        &self,
        name: &str,
        body: serde_json::Value,
    ) -> Result<T, ApiError> {
        let (Some(base), Some(key)) = (&self.base, &self.key) else {
            return Err(ApiError::NotConfigured);
        };
        let mut request = self
            .agent
            .post(format!("{base}/rest/v1/rpc/{name}"))
            .header("apikey", key);
        // La anon key vieja es un JWT y también va como Bearer; la nueva
        // "publishable" (sb_publishable_...) va solo en `apikey`.
        if key.starts_with("eyJ") {
            request = request.header("Authorization", format!("Bearer {key}"));
        }
        request
            .send_json(body)
            .and_then(|r| r.into_body().read_json::<T>())
            .map_err(|e| ApiError::Network(e.to_string()))
    }
}
