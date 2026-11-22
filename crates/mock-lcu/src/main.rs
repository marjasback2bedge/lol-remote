//! A fake LCU for developing lol-remote without running League.
//!
//! Serves HTTPS + WAMP-over-WSS with a self-signed certificate on 127.0.0.1,
//! writes a lockfile pointing at itself, and lets you drive the gameflow from
//! stdin. Run `lol-remote run --lockfile <same path>` against it.

mod state;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use base64::Engine;
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::service::TowerToHyperService;
use rand::distributions::{Alphanumeric, DistString};
use serde_json::{json, Value};
use tokio::io::AsyncBufReadExt;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_rustls::TlsAcceptor;

use state::{Mock, WampEvent};

#[derive(Parser)]
#[command(about = "Fake League client (LCU) for lol-remote development")]
struct Args {
    /// Where to write the lockfile.
    #[arg(long, default_value = "mock-lockfile")]
    lockfile: PathBuf,
    /// Port to listen on (0 = random, like the real client).
    #[arg(long, default_value_t = 0)]
    port: u16,
}

#[derive(Clone)]
struct App {
    mock: Arc<Mutex<Mock>>,
    events: broadcast::Sender<WampEvent>,
    auth: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mock_lcu=info".into()),
        )
        .init();
    let args = Args::parse();

    let password = Alphanumeric.sample_string(&mut rand::thread_rng(), 22);
    let auth = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("riot:{password}"))
    );
    let (events, _) = broadcast::channel(256);
    let app = App {
        mock: Arc::new(Mutex::new(Mock::new(events.clone()))),
        events,
        auth,
    };

    let listener = TcpListener::bind(("127.0.0.1", args.port)).await?;
    let port = listener.local_addr()?.port();
    std::fs::write(
        &args.lockfile,
        format!(
            "LeagueClient:{}:{port}:{password}:https",
            std::process::id()
        ),
    )
    .with_context(|| format!("writing {}", args.lockfile.display()))?;
    let lockfile = std::fs::canonicalize(&args.lockfile)?;
    println!("mock LCU listening on https://127.0.0.1:{port}");
    println!("lockfile: {}", lockfile.display());
    println!("run: lol-remote run --lockfile {}", lockfile.display());
    print_help();

    tokio::spawn(serve(listener, router(app.clone()), tls_acceptor()?));
    tokio::spawn(tick(app.mock.clone()));

    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { break };
                if !command(&app, line.trim()) {
                    break;
                }
            }
        }
    }
    let _ = std::fs::remove_file(&lockfile);
    println!("lockfile removed, bye");
    Ok(())
}

fn print_help() {
    println!(
        "commands: lobby | queue (ready check in 3s) | rc | cs (champ select) | game | none | others-decline | mode classic|aram | quit"
    );
}

/// Returns false to quit.
fn command(app: &App, cmd: &str) -> bool {
    let mut mock = app.mock.lock().unwrap();
    match cmd {
        "" => {}
        "lobby" => mock.set_phase("Lobby"),
        "queue" => mock.start_queue(),
        "rc" | "readycheck" => mock.start_ready_check(),
        "cs" | "champselect" => mock.start_champ_select(),
        "game" => mock.start_game(),
        "none" => mock.set_phase("None"),
        "others-decline" => mock.others_decline(),
        "mode classic" => mock.game_mode = "CLASSIC",
        "mode aram" => mock.game_mode = "ARAM",
        "quit" | "exit" => return false,
        _ => print_help(),
    }
    println!("phase = {}, mode = {}", mock.phase, mock.game_mode);
    true
}

async fn tick(mock: Arc<Mutex<Mock>>) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        mock.lock().unwrap().tick();
    }
}

fn tls_acceptor() -> anyhow::Result<TlsAcceptor> {
    use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
    let rcgen::CertifiedKey { cert, key_pair } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into(), "localhost".into()])?;
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(vec![cert.der().clone()], key)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(config)))
}

async fn serve(listener: TcpListener, router: Router, acceptor: TlsAcceptor) {
    loop {
        let Ok((tcp, _)) = listener.accept().await else {
            continue;
        };
        let acceptor = acceptor.clone();
        let service = TowerToHyperService::new(router.clone());
        tokio::spawn(async move {
            let Ok(tls) = acceptor.accept(tcp).await else {
                return;
            };
            let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(TokioIo::new(tls), service)
                .await;
        });
    }
}

fn router(app: App) -> Router {
    Router::new()
        .route("/", get(websocket))
        .route("/lol-gameflow/v1/gameflow-phase", get(gameflow_phase))
        .route("/lol-gameflow/v1/session", get(gameflow_session))
        .route("/lol-summoner/v1/current-summoner", get(current_summoner))
        .route("/lol-matchmaking/v1/ready-check", get(ready_check))
        .route("/lol-matchmaking/v1/ready-check/accept", post(accept))
        .route("/lol-matchmaking/v1/ready-check/decline", post(decline))
        .route("/lol-champ-select/v1/session", get(session))
        .route("/lol-champ-select/v1/pickable-champion-ids", get(pickable))
        .route("/lol-champ-select/v1/bannable-champion-ids", get(bannable))
        .route(
            "/lol-champ-select/v1/session/actions/:id",
            patch(patch_action),
        )
        .route(
            "/lol-champ-select/v1/session/my-selection",
            patch(my_selection),
        )
        .route(
            "/lol-champ-select/v1/session/my-selection/reroll",
            post(reroll),
        )
        .route(
            "/lol-champ-select/v1/session/bench/swap/:id",
            post(bench_swap),
        )
        .route("/lol-perks/v1/pages", get(perk_pages))
        .route(
            "/lol-perks/v1/currentpage",
            get(current_page).put(set_current_page),
        )
        .route(
            "/lol-game-data/assets/v1/champion-summary.json",
            get(|| async { json_file(include_str!("../data/champion-summary.json")) }),
        )
        .route(
            "/lol-game-data/assets/v1/summoner-spells.json",
            get(|| async { json_file(include_str!("../data/summoner-spells.json")) }),
        )
        .route("/lol-game-data/assets/*path", get(icon))
        .layer(middleware::from_fn_with_state(app.clone(), check_auth))
        .with_state(app)
}

async fn check_auth(State(app): State<App>, req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        == Some(app.auth.as_str());
    if !ok {
        return (StatusCode::UNAUTHORIZED, "bad credentials").into_response();
    }
    next.run(req).await
}

type ApiResult = Result<Response, (StatusCode, Json<Value>)>;

fn not_found(msg: &str) -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "errorCode": "RPC_ERROR", "httpStatus": 404, "message": msg })),
    )
}

fn bad_request(msg: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "errorCode": "RPC_ERROR", "httpStatus": 400, "message": msg })),
    )
}

fn json_file(contents: &'static str) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], contents).into_response()
}

async fn websocket(State(app): State<App>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| wamp(socket, app.events.subscribe()))
}

async fn wamp(socket: WebSocket, mut events: broadcast::Receiver<WampEvent>) {
    let (mut tx, mut rx) = socket.split();
    let mut subscribed = HashSet::new();
    loop {
        tokio::select! {
            msg = rx.next() => match msg {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(Value::Array(parts)) = serde_json::from_str::<Value>(&text) {
                        if parts.first().and_then(Value::as_u64) == Some(5) {
                            if let Some(name) = parts.get(1).and_then(Value::as_str) {
                                tracing::info!("client subscribed to {name}");
                                subscribed.insert(name.to_owned());
                            }
                        }
                    }
                }
                Some(Ok(_)) => {}
                _ => break,
            },
            event = events.recv() => {
                let Ok(event) = event else { continue };
                if !subscribed.contains(event.name) {
                    continue;
                }
                let msg = json!([8, event.name, {
                    "uri": event.uri,
                    "eventType": event.event_type,
                    "data": event.data,
                }]);
                if tx.send(Message::Text(msg.to_string())).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn gameflow_phase(State(app): State<App>) -> Json<Value> {
    Json(json!(app.mock.lock().unwrap().phase))
}

async fn gameflow_session(State(app): State<App>) -> Json<Value> {
    Json(app.mock.lock().unwrap().gameflow_session())
}

async fn current_summoner() -> Json<Value> {
    Json(json!({ "gameName": "MockPlayer", "tagLine": "MOCK", "summonerLevel": 30 }))
}

async fn ready_check(State(app): State<App>) -> ApiResult {
    let mock = app.mock.lock().unwrap();
    mock.ready_check_json()
        .map(|v| Json(v).into_response())
        .ok_or_else(|| not_found("Not attached to a matchmaking queue."))
}

async fn accept(State(app): State<App>) -> ApiResult {
    app.mock
        .lock()
        .unwrap()
        .respond_ready_check("Accepted")
        .map(|()| StatusCode::NO_CONTENT.into_response())
        .map_err(not_found)
}

async fn decline(State(app): State<App>) -> ApiResult {
    app.mock
        .lock()
        .unwrap()
        .respond_ready_check("Declined")
        .map(|()| StatusCode::NO_CONTENT.into_response())
        .map_err(not_found)
}

async fn session(State(app): State<App>) -> ApiResult {
    let mock = app.mock.lock().unwrap();
    mock.session()
        .map(|s| Json(s.clone()).into_response())
        .ok_or_else(|| not_found("No active delegate"))
}

async fn pickable(State(app): State<App>) -> Json<Value> {
    Json(json!(app.mock.lock().unwrap().available_champions()))
}

async fn bannable(State(app): State<App>) -> Json<Value> {
    Json(json!(app.mock.lock().unwrap().available_champions()))
}

async fn patch_action(
    State(app): State<App>,
    Path(id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult {
    app.mock
        .lock()
        .unwrap()
        .patch_action(id, &body)
        .map(|()| StatusCode::NO_CONTENT.into_response())
        .map_err(bad_request)
}

async fn my_selection(State(app): State<App>, Json(body): Json<Value>) -> ApiResult {
    app.mock
        .lock()
        .unwrap()
        .patch_my_selection(&body)
        .map(|()| StatusCode::NO_CONTENT.into_response())
        .map_err(bad_request)
}

async fn reroll(State(app): State<App>) -> ApiResult {
    app.mock
        .lock()
        .unwrap()
        .reroll()
        .map(|()| StatusCode::NO_CONTENT.into_response())
        .map_err(bad_request)
}

async fn bench_swap(State(app): State<App>, Path(id): Path<i64>) -> ApiResult {
    app.mock
        .lock()
        .unwrap()
        .bench_swap(id)
        .map(|()| StatusCode::NO_CONTENT.into_response())
        .map_err(bad_request)
}

async fn perk_pages(State(app): State<App>) -> Json<Value> {
    Json(app.mock.lock().unwrap().perk_pages())
}

async fn current_page(State(app): State<App>) -> Json<Value> {
    let mock = app.mock.lock().unwrap();
    let pages = mock.perk_pages();
    let current = pages
        .as_array()
        .and_then(|p| p.iter().find(|p| p["current"] == true).cloned())
        .unwrap_or(Value::Null);
    Json(current)
}

async fn set_current_page(State(app): State<App>, Json(id): Json<i64>) -> ApiResult {
    app.mock
        .lock()
        .unwrap()
        .set_current_page(id)
        .map(|()| StatusCode::NO_CONTENT.into_response())
        .map_err(bad_request)
}

/// Every other game-data asset is answered with a generated SVG placeholder.
async fn icon(Path(path): Path<String>) -> Response {
    let label = path
        .rsplit('/')
        .next()
        .unwrap_or("?")
        .trim_end_matches(".png")
        .trim_start_matches("Summoner")
        .trim_start_matches('_');
    let hue = path
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b.into()))
        % 360;
    let text: String = label.chars().take(3).collect();
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="hsl({hue},45%,35%)"/><text x="32" y="40" font-family="sans-serif" font-size="20" fill="#fff" text-anchor="middle">{text}</text></svg>"##
    );
    ([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response()
}
