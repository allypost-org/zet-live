use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
    time::Duration,
};

use arc_swap::ArcSwap;
use tracing::warn;
use url::Url;

use crate::{cli::ServerConfig, database::Database};

#[derive(Debug)]
struct ProviderDefinition {
    id: &'static str,
    name: &'static str,
    auth_url: &'static str,
    token_url: &'static str,
    userinfo_url: &'static str,
    scopes: &'static [&'static str],
    /// How to read the subject/email/name/picture out of the userinfo JSON.
    mapping: UserinfoMapping,
}

#[derive(Debug, Clone, Copy)]
pub struct UserinfoMapping {
    pub subject: &'static str,
    pub email: &'static str,
    pub name: &'static str,
    pub picture: &'static str,
}

const PRESETS: &[ProviderDefinition] = &[
    ProviderDefinition {
        id: "google",
        name: "Google",
        auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
        token_url: "https://oauth2.googleapis.com/token",
        userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo",
        scopes: &["openid", "email", "profile"],
        mapping: UserinfoMapping {
            subject: "sub",
            email: "email",
            name: "name",
            picture: "{picture}",
        },
    },
    ProviderDefinition {
        id: "microsoft",
        name: "Microsoft",
        auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        userinfo_url: "https://graph.microsoft.com/oidc/userinfo",
        scopes: &["openid", "email", "profile"],
        // Microsoft's OIDC userinfo `picture` is an auth-required Graph URL —
        // useless to an `<img>`, so no avatar is extracted.
        mapping: UserinfoMapping {
            subject: "sub",
            email: "email",
            name: "name",
            picture: "",
        },
    },
    ProviderDefinition {
        id: "facebook",
        name: "Facebook",
        auth_url: "https://www.facebook.com/v19.0/dialog/oauth",
        token_url: "https://graph.facebook.com/v19.0/oauth/access_token",
        userinfo_url: "https://graph.facebook.com/v19.0/me?fields=id,name,email,picture.type(large)",
        scopes: &["public_profile", "email"],
        mapping: UserinfoMapping {
            subject: "id",
            email: "email",
            name: "name",
            picture: "{picture.data.url}",
        },
    },
    ProviderDefinition {
        id: "linkedin",
        name: "LinkedIn",
        auth_url: "https://www.linkedin.com/oauth/v2/authorization",
        token_url: "https://www.linkedin.com/oauth/v2/accessToken",
        userinfo_url: "https://api.linkedin.com/v2/userinfo",
        scopes: &["openid", "profile", "email"],
        mapping: UserinfoMapping {
            subject: "sub",
            email: "email",
            name: "name",
            picture: "{picture}",
        },
    },
    ProviderDefinition {
        id: "discord",
        name: "Discord",
        auth_url: "https://discord.com/oauth2/authorize",
        token_url: "https://discord.com/api/oauth2/token",
        userinfo_url: "https://discord.com/api/users/@me",
        scopes: &["identify", "email"],
        // `avatar` is a hash; build the CDN URL from the user id + hash.
        mapping: UserinfoMapping {
            subject: "id",
            email: "email",
            name: "global_name",
            picture: "https://cdn.discordapp.com/avatars/{id}/{avatar}.png?size=128",
        },
    },
];

fn preset(id: &str) -> Option<&'static ProviderDefinition> {
    PRESETS.iter().find(|p| p.id == id)
}

impl ProviderDefinition {
    fn build(&self, client_id: String, client_secret: String) -> Provider {
        Provider {
            id: self.id.to_string(),
            name: self.name.to_string(),
            client_id,
            client_secret,
            auth_url: Url::parse(self.auth_url).expect("static provider auth url"),
            token_url: Url::parse(self.token_url).expect("static provider token url"),
            userinfo_url: Url::parse(self.userinfo_url).expect("static provider userinfo url"),
            scopes: self.scopes.iter().map(|s| (*s).to_string()).collect(),
            mapping: self.mapping,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: Url,
    pub token_url: Url,
    pub userinfo_url: Url,
    pub scopes: Vec<String>,
    pub mapping: UserinfoMapping,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderPublic {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Providers {
    pub app_url: Option<Url>,
    pub session_max_age: Duration,
    pub allowed_origins: Vec<String>,
    pub map: HashMap<String, Provider>,
}

impl Providers {
    fn empty() -> Self {
        Self {
            app_url: None,
            allowed_origins: Vec::new(),
            session_max_age: Duration::from_hours(720),
            map: HashMap::new(),
        }
    }

    pub async fn load_from_db(
        app_url: Option<Url>,
        session_max_age: Duration,
        allowed_origins: Vec<String>,
    ) -> Self {
        let Some(app_url) = app_url else {
            return Self::empty();
        };
        let mut map = HashMap::new();

        let rows = sqlx::query!(
            "
            SELECT id,
                   client_id,
                   client_secret,
                   enabled
            FROM auth_providers
            "
        )
        .fetch_all(&Database::pool())
        .await;

        if let Ok(rows) = rows {
            for r in rows {
                let Some(def) = preset(&r.id) else {
                    warn!(id = %r.id, "auth_providers row has no matching preset; ignoring");
                    continue;
                };
                if r.enabled == 0 {
                    continue;
                }
                map.insert(def.id.to_string(), def.build(r.client_id, r.client_secret));
            }
        }

        Self {
            app_url: Some(app_url),
            session_max_age,
            allowed_origins,
            map,
        }
    }

    pub fn enabled(&self) -> bool {
        !self.map.is_empty()
    }

    pub const fn has_app_url(&self) -> bool {
        self.app_url.is_some()
    }

    pub fn is_allowed_origin(&self, origin: &str) -> bool {
        self.allowed_origins.iter().any(|o| o == origin)
    }

    pub fn get(&self, id: &str) -> Option<&Provider> {
        self.map.get(id)
    }

    pub fn public_list(&self) -> Vec<ProviderPublic> {
        self.map
            .values()
            .map(|p| ProviderPublic {
                id: p.id.clone(),
                name: p.name.clone(),
            })
            .collect()
    }

    pub fn redirect_uri(&self, provider_id: &str) -> Option<Url> {
        let app_url = self.app_url.as_ref()?;
        if !self.map.contains_key(provider_id) {
            return None;
        }
        let mut url = app_url.clone();
        url.path_segments_mut()
            .expect("Failed to get path segments")
            .extend(["api", "v1", "auth", provider_id, "callback"]);
        url.set_query(None);
        url.set_fragment(None);
        Some(url)
    }
}

pub fn preset_list() -> Vec<ProviderPublic> {
    PRESETS
        .iter()
        .map(|p| ProviderPublic {
            id: p.id.to_string(),
            name: p.name.to_string(),
        })
        .collect()
}

pub fn preset_exists(id: &str) -> bool {
    preset(id).is_some()
}

static PROVIDERS: LazyLock<ArcSwap<Providers>> =
    LazyLock::new(|| ArcSwap::from_pointee(Providers::empty()));

pub async fn init(server_config: &ServerConfig) {
    let app_url = server_config.app_url.clone();

    let session_max_age = server_config
        .session_max_age
        .to_duration(&jiff::Zoned::now())
        .expect("session_max_age should be convertible to Duration")
        .unsigned_abs();

    let allowed_origins =
        build_allowed_origins(app_url.as_ref(), &server_config.allowed_frontend_origins);

    let providers = Providers::load_from_db(app_url, session_max_age, allowed_origins).await;

    PROVIDERS.store(Arc::new(providers));
}

pub async fn reload() {
    let current = get();
    let providers = Providers::load_from_db(
        current.app_url.clone(),
        current.session_max_age,
        current.allowed_origins.clone(),
    )
    .await;

    PROVIDERS.store(Arc::new(providers));
}

fn build_allowed_origins(app_url: Option<&Url>, extra: &[String]) -> Vec<String> {
    let mut origins = Vec::new();
    if let Some(app_url) = app_url {
        origins.push(app_url.origin().ascii_serialization());
    }
    for raw in extra {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        match Url::parse(trimmed) {
            Ok(u) if u.scheme() == "http" || u.scheme() == "https" => {
                origins.push(u.origin().ascii_serialization());
            }
            _ => warn!(
                origin = trimmed,
                "Ignoring invalid ALLOWED_FRONTEND_ORIGINS entry"
            ),
        }
    }
    origins
}

pub fn get() -> Arc<Providers> {
    PROVIDERS.load_full()
}
