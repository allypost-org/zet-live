use rust_embed::RustEmbed;

use crate::frontend_assets;

#[derive(RustEmbed)]
#[folder = "../frontend/dist"]
pub(super) struct SpaAssets;

#[derive(serde::Deserialize)]
struct FrontendVersion {
    id: String,
}

impl SpaAssets {
    pub(super) fn version_id() -> Option<String> {
        let asset = Self::get("frontend-version.json")?;
        let version = serde_json::from_slice::<FrontendVersion>(&asset.data).ok()?;
        (version.id.len() == 64
            && version
                .id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        .then_some(version.id)
    }
}

#[inline]
pub async fn serve_fallback(uri: axum::http::Uri) -> axum::response::Response {
    frontend_assets::serve_fallback(&uri, &SpaAssets::get)
}
