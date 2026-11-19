//! LCU event subscription over WAMP-on-WebSocket.

use futures_util::{SinkExt, StreamExt};
use reqwest::header::{HeaderValue, AUTHORIZATION};
use serde_json::Value;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async_tls_with_config, Connector};

use crate::types::{ChampSelectSession, Phase, ReadyCheck};
use crate::{tls, Credentials, LcuError};

pub const GAMEFLOW_PHASE: &str = "OnJsonApiEvent_lol-gameflow_v1_gameflow-phase";
pub const READY_CHECK: &str = "OnJsonApiEvent_lol-matchmaking_v1_ready-check";
pub const CHAMP_SELECT_SESSION: &str = "OnJsonApiEvent_lol-champ-select_v1_session";

const WAMP_SUBSCRIBE: u64 = 5;
const WAMP_EVENT: u64 = 8;

#[derive(Debug, Clone)]
pub enum LcuEvent {
    GameflowPhase(Phase),
    /// `None` when the ready check resource was deleted.
    ReadyCheck(Option<ReadyCheck>),
    /// `None` when champ select ended.
    ChampSelect(Option<Box<ChampSelectSession>>),
}

/// Connects to the LCU event socket and subscribes to the events lol-remote
/// needs. The returned receiver closes when the socket disconnects.
pub async fn connect_events(
    creds: &Credentials,
) -> Result<broadcast::Receiver<LcuEvent>, LcuError> {
    let mut request = format!("wss://127.0.0.1:{}/", creds.port).into_client_request()?;
    let auth = HeaderValue::from_str(&tls::basic_auth(&creds.password))
        .map_err(|_| LcuError::LockfileFormat("password is not a valid header".into()))?;
    request.headers_mut().insert(AUTHORIZATION, auth);

    let connector = Connector::Rustls(tls::localhost_client_config());
    let (ws, _) = connect_async_tls_with_config(request, None, false, Some(connector)).await?;
    let (mut sink, mut stream) = ws.split();
    for event in [GAMEFLOW_PHASE, READY_CHECK, CHAMP_SELECT_SESSION] {
        let msg = serde_json::json!([WAMP_SUBSCRIBE, event]).to_string();
        sink.send(Message::Text(msg)).await?;
    }

    let (tx, rx) = broadcast::channel(64);
    tokio::spawn(async move {
        // Keep the write half alive so the connection stays open.
        let _sink = sink;
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Some(event) = parse_message(&text) {
                        if tx.send(event).is_err() {
                            break;
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!("LCU websocket error: {e}");
                    break;
                }
            }
        }
        tracing::debug!("LCU event stream closed");
    });
    Ok(rx)
}

/// Parses `[8, "<event>", { "eventType": ..., "data": ... }]`.
pub(crate) fn parse_message(text: &str) -> Option<LcuEvent> {
    let Value::Array(parts) = serde_json::from_str::<Value>(text).ok()? else {
        return None;
    };
    if parts.first()?.as_u64()? != WAMP_EVENT {
        return None;
    }
    let name = parts.get(1)?.as_str()?;
    let payload = parts.get(2)?;
    let deleted = payload.get("eventType").and_then(Value::as_str) == Some("Delete");
    let data = payload.get("data").cloned().unwrap_or(Value::Null);

    let parse = |data: Value| -> Option<Value> { (!deleted && !data.is_null()).then_some(data) };
    match name {
        GAMEFLOW_PHASE => {
            let phase = match parse(data) {
                Some(v) => serde_json::from_value(v).ok()?,
                None => Phase::None,
            };
            Some(LcuEvent::GameflowPhase(phase))
        }
        READY_CHECK => Some(LcuEvent::ReadyCheck(
            parse(data).and_then(|v| serde_json::from_value(v).ok()),
        )),
        CHAMP_SELECT_SESSION => Some(LcuEvent::ChampSelect(
            parse(data)
                .and_then(|v| serde_json::from_value(v).ok())
                .map(Box::new),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_phase_event() {
        let msg = r#"[8,"OnJsonApiEvent_lol-gameflow_v1_gameflow-phase",{"data":"Matchmaking","eventType":"Update","uri":"/lol-gameflow/v1/gameflow-phase"}]"#;
        assert!(matches!(
            parse_message(msg),
            Some(LcuEvent::GameflowPhase(Phase::Matchmaking))
        ));
    }

    #[test]
    fn parses_ready_check_event() {
        let msg = r#"[8,"OnJsonApiEvent_lol-matchmaking_v1_ready-check",{"data":{"state":"InProgress","playerResponse":"None","timer":1.0},"eventType":"Update","uri":"/lol-matchmaking/v1/ready-check"}]"#;
        match parse_message(msg) {
            Some(LcuEvent::ReadyCheck(Some(rc))) => assert!(rc.awaiting_my_response()),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn delete_yields_none() {
        let msg = r#"[8,"OnJsonApiEvent_lol-champ-select_v1_session",{"data":null,"eventType":"Delete","uri":"/lol-champ-select/v1/session"}]"#;
        assert!(matches!(
            parse_message(msg),
            Some(LcuEvent::ChampSelect(None))
        ));
    }

    #[test]
    fn ignores_other_messages() {
        assert!(parse_message("[0,\"welcome\",1,\"x\"]").is_none());
        assert!(parse_message("not json").is_none());
        assert!(parse_message(r#"[8,"OnJsonApiEvent_other",{"data":1}]"#).is_none());
    }
}
