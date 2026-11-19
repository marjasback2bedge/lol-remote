//! lol-remote: control the League of Legends client from your phone.

mod api;
mod assets;
mod config;
mod install;
mod mdns;
mod net;
mod notify;
mod pairing;
mod state;
mod supervisor;
mod ws;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use anyhow::Context;
use axum::routing::get;
use axum::{middleware, Router};
use clap::{Parser, Subcommand};
use tokio::sync::watch;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::state::{Cache, Ctx, State};

#[derive(Parser)]
#[command(
    version,
    about = "Control the League of Legends client from your phone"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Use this lockfile instead of the configured one (e.g. a mock-lcu lockfile).
    #[arg(long, global = true)]
    lockfile: Option<PathBuf>,
    /// Use this config file instead of the default location.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Run in the foreground (default).
    Run,
    /// Print the pairing QR codes and URLs.
    Pair,
    /// Start lol-remote automatically at login.
    Install,
    /// Remove the login item.
    Uninstall,
    /// Config file helpers.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Print the config file path.
    Path,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config_path = match &cli.config {
        Some(p) => p.clone(),
        None => Config::default_path()?,
    };

    match cli.command.unwrap_or(Command::Run) {
        Command::Run => run(config_path, cli.lockfile).await,
        Command::Pair => {
            let (config, _) = Config::load_or_create(&config_path)?;
            let (mdns_url, ip_url) = urls(&config);
            pairing::print(&config, &mdns_url, &ip_url);
            Ok(())
        }
        Command::Install => {
            let mut extra = Vec::new();
            if let Some(p) = &cli.config {
                extra.extend(["--config".into(), absolute(p)?]);
            }
            if let Some(p) = &cli.lockfile {
                extra.extend(["--lockfile".into(), absolute(p)?]);
            }
            install::install(&extra)
        }
        Command::Uninstall => install::uninstall(),
        Command::Config {
            command: ConfigCommand::Path,
        } => {
            println!("{}", config_path.display());
            Ok(())
        }
    }
}

fn absolute(p: &std::path::Path) -> anyhow::Result<String> {
    Ok(std::path::absolute(p)?.display().to_string())
}

fn hostname(config: &Config) -> String {
    if config.hostname.is_empty() {
        net::system_hostname()
    } else {
        config.hostname.trim_end_matches(".local").to_owned()
    }
}

/// `(http://<host>.local:<port>, http://<lan ip>:<port>)`
fn urls(config: &Config) -> (String, String) {
    (
        format!("http://{}.local:{}", hostname(config), config.port),
        format!("http://{}:{}", net::lan_ip(), config.port),
    )
}

async fn run(config_path: PathBuf, lockfile: Option<PathBuf>) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lol_remote=info,lcu=info".into()),
        )
        .init();

    let (config, created) = Config::load_or_create(&config_path)?;
    tracing::info!("config: {}", config_path.display());
    let (mdns_url, ip_url) = urls(&config);
    if created {
        pairing::print(&config, &mdns_url, &ip_url);
    }
    let lockfile = lockfile.unwrap_or_else(|| config.lockfile());
    let port = config.port;

    let _mdns = mdns::advertise(&hostname(&config), net::lan_ip(), port)
        .inspect_err(|e| tracing::warn!("mDNS advertisement failed: {e}"))
        .ok();

    let (state, _) = watch::channel(State {
        auto_accept: config.auto_accept,
        phase: lcu::Phase::None.to_string(),
        ..State::default()
    });
    let ctx = Arc::new(Ctx {
        config: RwLock::new(config),
        config_path,
        state,
        lcu: RwLock::new(None),
        cache: RwLock::new(Cache::default()),
        notifier: notify::Notifier::new(),
        mdns_url: mdns_url.clone(),
        ip_url: ip_url.clone(),
    });

    tokio::spawn(supervisor::run(ctx.clone(), lockfile));

    let protected = Router::new()
        .nest("/api", api::router())
        .route("/ws", get(ws::handler))
        .route("/assets/lcu/*path", get(assets::lcu_asset))
        .layer(middleware::from_fn_with_state(
            ctx.clone(),
            api::require_token,
        ));
    let app = protected
        .fallback(assets::static_file)
        .layer(TraceLayer::new_for_http())
        .with_state(ctx);

    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!("serving on {ip_url} and {mdns_url} (run `lol-remote pair` for the QR code)");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
