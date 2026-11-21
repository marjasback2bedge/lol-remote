//! WebSocket pushing full state snapshots to the phone (spec §7.9).

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use serde_json::json;
use tokio::sync::watch;

use crate::state::{self, now_ms, Ctx};

const PING_EVERY: Duration = Duration::from_secs(20);

pub async fn handler(State(ctx): State<Arc<Ctx>>, ws: WebSocketUpgrade) -> Response {
    let rx = ctx.state.subscribe();
    ws.on_upgrade(move |socket| push_state(socket, rx))
}

fn snapshot(rx: &mut watch::Receiver<state::State>) -> Message {
    let mut state = rx.borrow_and_update().clone();
    state.server_time = now_ms();
    Message::Text(json!({ "type": "state", "data": state }).to_string())
}

async fn push_state(mut socket: WebSocket, mut rx: watch::Receiver<state::State>) {
    if socket.send(snapshot(&mut rx)).await.is_err() {
        return;
    }
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    loop {
        let msg = tokio::select! {
            changed = rx.changed() => match changed {
                Ok(()) => snapshot(&mut rx),
                Err(_) => break,
            },
            _ = ping.tick() => Message::Ping(Vec::new()),
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => continue,
            },
        };
        if socket.send(msg).await.is_err() {
            break;
        }
    }
}
