use rust_embed::RustEmbed;

use crate::frontend_assets;

#[derive(RustEmbed)]
#[folder = "../frontend/dist"]
struct SpaAssets;

#[inline]
pub async fn serve_fallback(uri: axum::http::Uri) -> axum::response::Response {
    frontend_assets::serve_fallback(&uri, &SpaAssets::get)
}
