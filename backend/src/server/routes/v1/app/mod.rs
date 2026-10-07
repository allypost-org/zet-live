use std::sync::LazyLock;

use axum::{Json, http::header, response::IntoResponse};

use crate::config::project::ProjectConfig;

pub async fn get_version() -> impl IntoResponse {
    static INFO: LazyLock<serde_json::Value> = LazyLock::new(|| {
        let info = ProjectConfig::build_info();

        let commit = info
            .version_control
            .as_ref()
            .and_then(|x| match x {
                build_info::VersionControl::Git(git_info) => Some(git_info),
                #[allow(unreachable_patterns)]
                _ => None,
            })
            .map(|x| x.commit_id.clone());

        let id = encode_hex(ProjectConfig::app_and_build_date().as_bytes());

        let mut ret = serde_json::json!({
            "name": ProjectConfig::app_name(),
            "version": ProjectConfig::app_version(),
            "built": ProjectConfig::build_date(),
            "id": id,
            "frontendId": crate::server::routes::frontend::SpaAssets::version_id(),
        });
        let ret = ret.as_object_mut().expect("Value is object");

        if cfg!(debug_assertions) {
            ret.insert("_build".to_string(), serde_json::json!(info));
        }

        if let Some(commit) = commit {
            ret.insert("commit".to_string(), serde_json::json!(commit));
        }

        serde_json::json!(ret)
    });

    ([(header::CACHE_CONTROL, "no-store")], Json(INFO.clone()))
}

fn encode_hex(data: &[u8]) -> String {
    const HEX_CHARS_LOWER: &[u8; 16] = b"0123456789abcdef";
    let mut ret = String::with_capacity(data.len() * 2);
    for &b in data {
        ret.push(HEX_CHARS_LOWER[(b >> 4) as usize] as char);
        ret.push(HEX_CHARS_LOWER[(b & 0x0F) as usize] as char);
    }
    ret
}
