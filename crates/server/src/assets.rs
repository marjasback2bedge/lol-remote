//! Embedded frontend (spec §9) and the LCU image proxy.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use crate::state::Ctx;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
#[allow_missing = true]
struct Web;

/// Only game-data assets may be proxied; nothing else of the LCU is exposed.
const LCU_ASSET_PREFIX: &str = "lol-game-data/assets/";

fn is_allowed_asset(path: &str) -> bool {
    path.starts_with(LCU_ASSET_PREFIX)
        && !path.split('/').any(|seg| seg == ".." || seg == ".")
        && !path.contains(['?', '#', '\\'])
}

pub async fn lcu_asset(State(ctx): State<Arc<Ctx>>, Path(path): Path<String>) -> Response {
    if !is_allowed_asset(&path) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(client) = ctx.lcu() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match client.get_bytes(&format!("/{path}")).await {
        Ok((content_type, body)) => (
            [
                (
                    header::CONTENT_TYPE,
                    content_type.unwrap_or_else(|| "application/octet-stream".into()),
                ),
                (header::CACHE_CONTROL, "max-age=86400".into()),
            ],
            body,
        )
            .into_response(),
        Err(lcu::LcuError::Status { status, .. }) => StatusCode::from_u16(status)
            .unwrap_or(StatusCode::BAD_GATEWAY)
            .into_response(),
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
}

/// Serves the SPA, falling back to `index.html` for unknown routes.
pub async fn static_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if let Some(file) = Web::get(path).filter(|_| !path.is_empty()) {
        // Vite fingerprints everything under assets/.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return (
            [
                (header::CONTENT_TYPE, file.metadata.mimetype().to_owned()),
                (header::CACHE_CONTROL, cache.to_owned()),
            ],
            file.data,
        )
            .into_response();
    }
    match Web::get("index.html") {
        Some(index) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            index.data,
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "frontend not built: run `npm run build` in web/ and rebuild lol-remote",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_allowlist() {
        assert!(is_allowed_asset(
            "lol-game-data/assets/v1/champion-icons/1.png"
        ));
        assert!(!is_allowed_asset("lol-summoner/v1/current-summoner"));
        assert!(!is_allowed_asset(
            "lol-game-data/assets/../../lol-summoner/v1/x"
        ));
        assert!(!is_allowed_asset("lol-game-data/assets/x?y=1"));
    }
}
