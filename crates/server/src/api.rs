//! HTTP API for the phone (spec §7.9). Only the operations listed here reach
//! the LCU; there is deliberately no generic proxy.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use lcu::{current_action, LcuClient, LcuError, Phase};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::{now_ms, Ctx};

pub const TOKEN_COOKIE: &str = "lolr_token";

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<LcuError> for ApiError {
    fn from(e: LcuError) -> Self {
        tracing::warn!("LCU request failed: {e}");
        ApiError(StatusCode::BAD_GATEWAY, format!("LoL 客戶端回應錯誤：{e}"))
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

fn conflict(msg: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, msg.into())
}

fn lcu(ctx: &Ctx) -> Result<LcuClient, ApiError> {
    ctx.lcu().ok_or_else(|| {
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "尚未連上 LoL 客戶端".into(),
        )
    })
}

pub fn router() -> Router<Arc<Ctx>> {
    Router::new()
        .route("/state", get(get_state))
        .route("/ready-check/accept", post(accept))
        .route("/ready-check/decline", post(decline))
        .route("/champions", get(champions))
        .route("/champ-select/hover", post(hover))
        .route("/champ-select/lock", post(lock))
        .route("/champ-select/spells", post(set_spells))
        .route("/spells", get(spells))
        .route("/perks/pages", get(perk_pages))
        .route("/perks/current", post(set_perk_page))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/pair", get(pair_info))
}

/// Constant-time comparison so the token can't be guessed byte by byte.
fn token_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn presented_token(headers: &HeaderMap) -> Option<&str> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let cookie = || {
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .find_map(|c| c.trim().strip_prefix(TOKEN_COOKIE)?.strip_prefix('='))
    };
    bearer.or_else(cookie)
}

pub async fn require_token(State(ctx): State<Arc<Ctx>>, req: Request, next: Next) -> Response {
    let expected = ctx.config.read().unwrap().auth_token.clone();
    match presented_token(req.headers()) {
        Some(token) if token_eq(token, &expected) => next.run(req).await,
        _ => ApiError(StatusCode::UNAUTHORIZED, "invalid or missing token".into()).into_response(),
    }
}

async fn get_state(State(ctx): State<Arc<Ctx>>) -> Json<crate::state::State> {
    let mut state = ctx.state.borrow().clone();
    state.server_time = now_ms();
    Json(state)
}

async fn accept(State(ctx): State<Arc<Ctx>>) -> ApiResult<serde_json::Value> {
    lcu(&ctx)?.accept_ready_check().await?;
    tracing::info!("ready check accepted from phone");
    Ok(Json(json!({ "ok": true })))
}

async fn decline(State(ctx): State<Arc<Ctx>>) -> ApiResult<serde_json::Value> {
    lcu(&ctx)?.decline_ready_check().await?;
    tracing::info!("ready check declined from phone");
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChampionOut {
    id: i64,
    name: String,
    alias: String,
    icon_url: String,
    pickable: bool,
    bannable: bool,
}

pub fn champion_icon_url(id: i64) -> String {
    format!("/assets/lcu/lol-game-data/assets/v1/champion-icons/{id}.png")
}

async fn champions(State(ctx): State<Arc<Ctx>>) -> ApiResult<Vec<ChampionOut>> {
    let client = lcu(&ctx)?;
    let in_champ_select = ctx.state.borrow().phase == Phase::ChampSelect.as_str();
    let (pickable, bannable) = if in_champ_select {
        let (p, b) = tokio::join!(
            client.pickable_champion_ids(),
            client.bannable_champion_ids()
        );
        (p.unwrap_or_default(), b.unwrap_or_default())
    } else {
        Default::default()
    };
    let mut out: Vec<ChampionOut> = ctx
        .cache
        .read()
        .unwrap()
        .champions
        .iter()
        .map(|c| ChampionOut {
            id: c.id,
            name: c.name.clone(),
            alias: c.alias.clone(),
            icon_url: champion_icon_url(c.id),
            pickable: pickable.contains(&c.id),
            bannable: bannable.contains(&c.id),
        })
        .collect();
    out.sort_by(|a, b| a.alias.cmp(&b.alias));
    Ok(Json(out))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChampionBody {
    champion_id: i64,
}

async fn hover(
    State(ctx): State<Arc<Ctx>>,
    Json(body): Json<ChampionBody>,
) -> ApiResult<serde_json::Value> {
    let client = lcu(&ctx)?;
    let session = client.champ_select_session().await?;
    let action = current_action(&session).ok_or_else(|| conflict("目前沒有你的選角動作"))?;
    if action.completed {
        return Err(conflict("已經鎖定"));
    }
    client.hover(action.id, body.champion_id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn lock(
    State(ctx): State<Arc<Ctx>>,
    Json(body): Json<ChampionBody>,
) -> ApiResult<serde_json::Value> {
    let client = lcu(&ctx)?;
    let session = client.champ_select_session().await?;
    let action = current_action(&session).ok_or_else(|| conflict("目前沒有你的選角動作"))?;
    if !action.is_my_turn() {
        return Err(conflict("還沒輪到你"));
    }
    client.lock(action.id, body.champion_id).await?;
    tracing::info!("locked {:?} {} from phone", action.kind, body.champion_id);
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpellOut {
    id: u64,
    name: String,
    icon_url: String,
}

async fn spells(State(ctx): State<Arc<Ctx>>) -> ApiResult<Vec<SpellOut>> {
    lcu(&ctx)?;
    let out = ctx
        .cache
        .read()
        .unwrap()
        .spells
        .iter()
        .filter(|s| s.game_modes.iter().any(|m| m == "CLASSIC"))
        .map(|s| SpellOut {
            id: s.id,
            name: s.name.clone(),
            icon_url: format!("/assets/lcu{}", s.icon_path),
        })
        .collect();
    Ok(Json(out))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpellsBody {
    spell1_id: u64,
    spell2_id: u64,
}

async fn set_spells(
    State(ctx): State<Arc<Ctx>>,
    Json(body): Json<SpellsBody>,
) -> ApiResult<serde_json::Value> {
    if body.spell1_id == body.spell2_id {
        return Err(ApiError(StatusCode::BAD_REQUEST, "兩個技能不能相同".into()));
    }
    lcu(&ctx)?
        .set_summoner_spells(body.spell1_id, body.spell2_id)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PerkPageOut {
    id: i64,
    name: String,
    is_current: bool,
    is_editable: bool,
}

async fn perk_pages(State(ctx): State<Arc<Ctx>>) -> ApiResult<Vec<PerkPageOut>> {
    let pages = lcu(&ctx)?.perk_pages().await?;
    Ok(Json(
        pages
            .into_iter()
            .map(|p| PerkPageOut {
                id: p.id,
                name: p.name,
                is_current: p.current,
                is_editable: p.is_editable,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PerkBody {
    page_id: i64,
}

async fn set_perk_page(
    State(ctx): State<Arc<Ctx>>,
    Json(body): Json<PerkBody>,
) -> ApiResult<serde_json::Value> {
    lcu(&ctx)?.set_current_perk_page(body.page_id).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    auto_accept: bool,
}

async fn get_settings(State(ctx): State<Arc<Ctx>>) -> Json<Settings> {
    Json(Settings {
        auto_accept: ctx.config.read().unwrap().auto_accept,
    })
}

async fn put_settings(
    State(ctx): State<Arc<Ctx>>,
    Json(body): Json<Settings>,
) -> ApiResult<Settings> {
    let config = {
        let mut config = ctx.config.write().unwrap();
        config.auto_accept = body.auto_accept;
        config.clone()
    };
    config.save(&ctx.config_path).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("無法寫入設定檔：{e}"),
        )
    })?;
    ctx.update(|s| s.auto_accept = body.auto_accept);
    tracing::info!("auto accept set to {}", body.auto_accept);
    Ok(Json(body))
}

async fn pair_info(State(ctx): State<Arc<Ctx>>) -> Json<serde_json::Value> {
    let config = ctx.config();
    let server = config.ntfy_server.trim_end_matches('/');
    Json(json!({
        "ntfyServer": server,
        "ntfyTopic": config.ntfy_topic,
        "subscribeUrl": format!("{server}/{}", config.ntfy_topic),
        "mdnsUrl": ctx.mdns_url,
        "ipUrl": ctx.ip_url,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_comparison() {
        assert!(token_eq("abc", "abc"));
        assert!(!token_eq("abc", "abd"));
        assert!(!token_eq("abc", "abcd"));
    }

    #[test]
    fn token_from_bearer_or_cookie() {
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, "Bearer xyz".parse().unwrap());
        assert_eq!(presented_token(&h), Some("xyz"));

        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, "a=1; lolr_token=tok; b=2".parse().unwrap());
        assert_eq!(presented_token(&h), Some("tok"));

        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, "lolr_token_other=nope".parse().unwrap());
        assert_eq!(presented_token(&h), None);
    }
}
