use std::{ffi::OsStr, path::Path};

use axum::{
    http::{HeaderValue, Uri},
    response::{IntoResponse, Response},
};
use reqwest::header;

const CACHE_IMMUTABLE: HeaderValue =
    HeaderValue::from_static("public, max-age=31536000, immutable");
const CACHE_NO_CACHE: HeaderValue = HeaderValue::from_static("no-cache, max-age=0");
const CACHE_SHORT: HeaderValue = HeaderValue::from_static("public, max-age=3600, s-maxage=3600");

fn cache_control_for_path(path: &str) -> HeaderValue {
    let path = Path::new(path);

    let first_segment = path.iter().next().unwrap_or_else(|| OsStr::new("/"));

    if first_segment.eq_ignore_ascii_case("_static") || first_segment.eq_ignore_ascii_case("assets")
    {
        return CACHE_IMMUTABLE;
    }

    if path.is_dir()
        || path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
    {
        return CACHE_NO_CACHE;
    }

    CACHE_SHORT
}

#[inline]
pub fn serve_fallback<T>(uri: &Uri, get_asset_fn: &T) -> Response
where
    T: Fn(&str) -> Option<rust_embed::EmbeddedFile>,
{
    let path = uri.path().trim_start_matches('/');

    if !path.is_empty()
        && let Some(resp) = serve_path(path, get_asset_fn)
    {
        return resp;
    }

    if path
        .split('/')
        .next_back()
        .is_some_and(|x| !x.contains('.'))
        && let Some(resp) = serve_path("index.html", get_asset_fn)
    {
        return resp;
    }

    if let Some(resp) = serve_path(&format!("{}index.html", path), get_asset_fn) {
        return resp;
    }

    (reqwest::StatusCode::NOT_FOUND, "file not found").into_response()
}

fn serve_path<T>(path: &str, get_asset_fn: &T) -> Option<Response>
where
    T: Fn(&str) -> Option<rust_embed::EmbeddedFile>,
{
    if path.is_empty() {
        return None;
    }

    let file = get_asset_fn(path)?;

    let Ok(mime) = HeaderValue::from_str(file.metadata.mimetype()) else {
        return Some(
            (
                reqwest::StatusCode::INTERNAL_SERVER_ERROR,
                "failed to serialize file mime type",
            )
                .into_response(),
        );
    };

    let mut resp = (
        reqwest::StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, cache_control_for_path(path)),
        ],
        file.data,
    )
        .into_response();

    if let Some(last_modified) = file.metadata.last_modified()
        && last_modified > 0
        && let Ok(last_modified) = HeaderValue::from_str(&format!("{}", last_modified))
    {
        resp.headers_mut()
            .append(header::LAST_MODIFIED, last_modified);
    }

    Some(resp)
}
