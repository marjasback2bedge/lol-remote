//! Idle → Connecting → Active state machine (spec §7.3) and LCU event
//! handling, including ready-check notifications (spec §7.4).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use file_watch::Watcher;
use lcu::types::{Phase, ReadyCheck};
use lcu::{Credentials, LcuClient, LcuEvent};
use tokio::sync::{broadcast, mpsc};
use tokio::time::{sleep, timeout, Instant};

use crate::notify::Push;
use crate::state::{now_ms, ChampSelectState, Connection, Ctx, ReadyCheckState};

const RETRY_EVERY: Duration = Duration::from_secs(2);
const CONNECT_DEADLINE: Duration = Duration::from_secs(60);
const POLL_EVERY: Duration = Duration::from_secs(3);

pub async fn run(ctx: Arc<Ctx>, lockfile: PathBuf) {
    tracing::info!("watching for the LoL client at {}", lockfile.display());
    let mut failed: Option<Credentials> = None;
    loop {
        ctx.update(|s| s.connection = Connection::Idle);
        let creds = wait_for_lockfile(&lockfile, failed.as_ref()).await;
        tracing::info!(
            "lockfile found (pid {}, port {}), connecting",
            creds.pid,
            creds.port
        );
        ctx.update(|s| s.connection = Connection::Connecting);

        let Some((client, events, phase)) = connect(&creds, &lockfile).await else {
            tracing::warn!("could not connect to the LoL client; waiting for a new lockfile");
            failed = Some(creds);
            continue;
        };
        failed = None;
        tracing::info!("connected to the LoL client, phase = {phase}");
        Active::new(ctx.clone(), client)
            .run(events, phase, &lockfile, creds.pid)
            .await;
        tracing::info!("disconnected from the LoL client");

        *ctx.lcu.write().unwrap() = None;
        ctx.update(|s| {
            s.phase = Phase::None.to_string();
            s.ready_check = None;
            s.champ_select = None;
        });
    }
}

/// Waits until the lockfile exists and parses, skipping credentials that
/// already failed (a stale lockfile left behind by a crashed client).
async fn wait_for_lockfile(path: &Path, skip: Option<&Credentials>) -> Credentials {
    let (tx, mut rx) = mpsc::channel(8);
    let _watcher = watch_dir(path, tx.clone());
    loop {
        if let Ok(creds) = Credentials::read(path) {
            if skip != Some(&creds) {
                return creds;
            }
        }
        // FS events wake us early; the timeout is the polling fallback.
        let _ = timeout(POLL_EVERY, rx.recv()).await;
    }
}

fn watch_dir(path: &Path, tx: mpsc::Sender<()>) -> Option<file_watch::RecommendedWatcher> {
    let dir = path.parent().filter(|d| d.is_dir())?;
    let mut watcher = file_watch::recommended_watcher(move |_| {
        let _ = tx.try_send(());
    })
    .ok()?;
    watcher
        .watch(dir, file_watch::RecursiveMode::NonRecursive)
        .ok()?;
    Some(watcher)
}

async fn connect(
    creds: &Credentials,
    lockfile: &Path,
) -> Option<(LcuClient, broadcast::Receiver<LcuEvent>, Phase)> {
    let deadline = Instant::now() + CONNECT_DEADLINE;
    loop {
        let attempt = async {
            let client = LcuClient::new(creds)?;
            let phase = client.gameflow_phase().await?;
            let events = lcu::connect_events(creds).await?;
            Ok::<_, lcu::LcuError>((client, events, phase))
        };
        match attempt.await {
            Ok(connected) => return Some(connected),
            Err(e) => tracing::debug!("LCU not ready yet: {e}"),
        }
        if Instant::now() >= deadline || !lockfile.exists() {
            return None;
        }
        sleep(RETRY_EVERY).await;
    }
}

struct Active {
    ctx: Arc<Ctx>,
    client: LcuClient,
    phase: Phase,
    /// Set once the current ready check has been handled (notified or
    /// auto-accepted); cleared when the phase leaves ReadyCheck.
    ready_check_handled: bool,
}

impl Active {
    fn new(ctx: Arc<Ctx>, client: LcuClient) -> Self {
        Self {
            ctx,
            client,
            phase: Phase::None,
            ready_check_handled: false,
        }
    }

    async fn run(
        mut self,
        mut events: broadcast::Receiver<LcuEvent>,
        phase: Phase,
        lockfile: &Path,
        pid: u32,
    ) {
        *self.ctx.lcu.write().unwrap() = Some(self.client.clone());
        self.load_cache().await;
        let _caffeinate = caffeinate(pid);
        self.ctx.update(|s| s.connection = Connection::Active);

        self.on_phase(phase.clone());
        if phase == Phase::ReadyCheck {
            if let Ok(rc) = self.client.ready_check().await {
                self.on_ready_check(Some(rc));
            }
        }
        if phase == Phase::ChampSelect {
            if let Ok(session) = self.client.champ_select_session().await {
                self.on_champ_select(Some(&session));
            }
        }

        let mut check = tokio::time::interval(POLL_EVERY);
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Ok(LcuEvent::GameflowPhase(p)) => self.on_phase(p),
                    Ok(LcuEvent::ReadyCheck(rc)) => self.on_ready_check(rc),
                    Ok(LcuEvent::ChampSelect(s)) => self.on_champ_select(s.as_deref()),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("dropped {n} LCU events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                _ = check.tick() => {
                    if !lockfile.exists() {
                        break;
                    }
                }
            }
        }
    }

    async fn load_cache(&self) {
        let (champions, spells) = tokio::join!(
            self.client.champion_summary(),
            self.client.summoner_spells()
        );
        let mut cache = self.ctx.cache.write().unwrap();
        match champions {
            Ok(c) => cache.champions = c.into_iter().filter(|c| c.id > 0).collect(),
            Err(e) => tracing::warn!("could not load champion list: {e}"),
        }
        match spells {
            Ok(s) => cache.spells = s,
            Err(e) => tracing::warn!("could not load summoner spells: {e}"),
        }
    }

    fn on_phase(&mut self, phase: Phase) {
        if phase == self.phase {
            return;
        }
        tracing::info!("phase: {} → {phase}", self.phase);
        let previous = std::mem::replace(&mut self.phase, phase.clone());
        self.ctx.update(|s| {
            s.phase = phase.to_string();
            if phase != Phase::ReadyCheck {
                s.ready_check = None;
            }
            if phase != Phase::ChampSelect {
                s.champ_select = None;
            }
        });

        match phase {
            Phase::ReadyCheck => self.start_ready_check(),
            _ if previous == Phase::ReadyCheck => {
                let cancelled = matches!(phase, Phase::Matchmaking | Phase::Lobby | Phase::None);
                if cancelled && self.ready_check_handled && self.ctx.config().notify_cancel {
                    self.push(Push::Cancelled);
                }
                self.ready_check_handled = false;
            }
            _ => self.ready_check_handled = false,
        }
    }

    fn on_ready_check(&mut self, rc: Option<ReadyCheck>) {
        let view = rc.as_ref().and_then(ReadyCheckState::from_lcu);
        self.ctx.update(|s| s.ready_check = view);
        if rc.is_some_and(|rc| rc.awaiting_my_response()) {
            self.start_ready_check();
        }
    }

    fn on_champ_select(&mut self, session: Option<&lcu::types::ChampSelectSession>) {
        let view = session.map(|s| ChampSelectState::from_session(s, now_ms()));
        self.ctx.update(|s| s.champ_select = view);
    }

    /// Notifies or auto-accepts once per ready check.
    fn start_ready_check(&mut self) {
        if self.ready_check_handled {
            return;
        }
        self.ready_check_handled = true;
        if self.ctx.config().auto_accept {
            let client = self.client.clone();
            let ctx = self.ctx.clone();
            tokio::spawn(async move {
                match client.accept_ready_check().await {
                    Ok(()) => {
                        tracing::info!("auto-accepted ready check");
                        let config = ctx.config();
                        ctx.notifier
                            .send(&config, ctx.public_url(), Push::AutoAccepted)
                            .await;
                    }
                    Err(e) => {
                        tracing::warn!("auto-accept failed: {e}");
                        let config = ctx.config();
                        ctx.notifier
                            .send(&config, ctx.public_url(), Push::MatchFound)
                            .await;
                    }
                }
            });
        } else {
            self.push(Push::MatchFound);
        }
    }

    fn push(&self, push: Push) {
        let ctx = self.ctx.clone();
        tokio::spawn(async move {
            let config = ctx.config();
            ctx.notifier.send(&config, ctx.public_url(), push).await;
        });
    }
}

/// Keeps the Mac awake while the client runs; killed when dropped.
fn caffeinate(pid: u32) -> Option<tokio::process::Child> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    tokio::process::Command::new("caffeinate")
        .args(["-i", "-w", &pid.to_string()])
        .kill_on_drop(true)
        .spawn()
        .inspect_err(|e| tracing::warn!("could not start caffeinate: {e}"))
        .ok()
}
