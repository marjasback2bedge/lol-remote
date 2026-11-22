//! The state snapshot pushed to the phone (spec §8) and the shared server context.

use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use lcu::champ_select::{current_action, local_member, ActionKind};
use lcu::types::{ChampSelectSession, ChampionSummary, ReadyCheck, SummonerSpell};
use lcu::LcuClient;
use serde::Serialize;
use tokio::sync::watch;

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Connection {
    #[default]
    Idle,
    Connecting,
    Active,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub connection: Connection,
    pub phase: String,
    pub auto_accept: bool,
    pub ready_check: Option<ReadyCheckState>,
    pub champ_select: Option<ChampSelectState>,
    /// Server clock (ms) when this snapshot was sent; lets the phone correct
    /// for clock skew when counting down `champSelect.timerMs`.
    pub server_time: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyCheckState {
    pub state: String,
    pub player_response: String,
    pub timer: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampSelectState {
    /// `CLASSIC`, `ARAM`, ...; empty if the client didn't report one.
    pub game_mode: String,
    pub timer_ms: i64,
    pub timer_phase: String,
    pub received_at: i64,
    pub my_action: Option<MyActionState>,
    pub my_selection: MySelection,
    pub my_team: Vec<TeamSlot>,
    pub their_team: Vec<EnemySlot>,
    pub bans: BansState,
    /// Bench champions; `None` when the mode has no bench.
    pub bench: Option<Vec<i64>>,
    /// `None` when the mode doesn't allow rerolling.
    pub rerolls_remaining: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MyActionState {
    pub kind: ActionKind,
    pub champion_id: i64,
    pub completed: bool,
    pub is_my_turn: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MySelection {
    pub champion_id: i64,
    pub spell1_id: u32,
    pub spell2_id: u32,
    pub position: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSlot {
    pub cell_id: i64,
    pub champion_id: i64,
    pub pick_intent: i64,
    pub position: String,
    pub is_me: bool,
}

/// Enemy slots expose the champion only: no names, no stats (Riot policy).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemySlot {
    pub champion_id: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BansState {
    pub my_team: Vec<i64>,
    pub their_team: Vec<i64>,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Unset spells come through as `u64::MAX`; the phone gets 0 instead.
fn spell_id(id: u64) -> u32 {
    u32::try_from(id).unwrap_or(0)
}

impl ReadyCheckState {
    /// `None` when there is no ready check to show.
    pub fn from_lcu(rc: &ReadyCheck) -> Option<Self> {
        (rc.state != "Invalid" && !rc.state.is_empty()).then(|| Self {
            state: rc.state.clone(),
            player_response: rc.player_response.clone(),
            timer: rc.timer,
        })
    }
}

impl ChampSelectState {
    pub fn from_session(s: &ChampSelectSession, game_mode: &str, received_at: i64) -> Self {
        let me = local_member(s);
        Self {
            game_mode: game_mode.to_owned(),
            timer_ms: s.timer.adjusted_time_left_in_phase,
            timer_phase: s.timer.phase.clone(),
            received_at,
            my_action: current_action(s).map(|a| MyActionState {
                kind: a.kind,
                champion_id: a.champion_id,
                completed: a.completed,
                is_my_turn: a.is_my_turn(),
            }),
            my_selection: me
                .map(|m| MySelection {
                    champion_id: if m.champion_id > 0 {
                        m.champion_id
                    } else {
                        m.champion_pick_intent
                    },
                    spell1_id: spell_id(m.spell1_id),
                    spell2_id: spell_id(m.spell2_id),
                    position: m.assigned_position.clone(),
                })
                .unwrap_or_default(),
            my_team: s
                .my_team
                .iter()
                .map(|m| TeamSlot {
                    cell_id: m.cell_id,
                    champion_id: m.champion_id,
                    pick_intent: m.champion_pick_intent,
                    position: m.assigned_position.clone(),
                    is_me: m.cell_id == s.local_player_cell_id,
                })
                .collect(),
            their_team: s
                .their_team
                .iter()
                .map(|m| EnemySlot {
                    champion_id: m.champion_id,
                })
                .collect(),
            bans: BansState {
                my_team: s.bans.my_team_bans.clone(),
                their_team: s.bans.their_team_bans.clone(),
            },
            bench: s.bench_enabled.then(|| {
                s.bench_champions
                    .iter()
                    .map(|b| b.champion_id)
                    .filter(|&id| id > 0)
                    .collect()
            }),
            rerolls_remaining: s.allow_rerolling.then_some(s.rerolls_remaining),
        }
    }
}

/// Data fetched once per LCU connection.
#[derive(Default)]
pub struct Cache {
    pub champions: Vec<ChampionSummary>,
    pub spells: Vec<SummonerSpell>,
}

/// Everything the HTTP handlers and the supervisor share.
pub struct Ctx {
    pub config: RwLock<Config>,
    pub config_path: PathBuf,
    pub state: watch::Sender<State>,
    pub lcu: RwLock<Option<LcuClient>>,
    pub cache: RwLock<Cache>,
}

impl Ctx {
    pub fn config(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    pub fn lcu(&self) -> Option<LcuClient> {
        self.lcu.read().unwrap().clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut State)) {
        self.state.send_modify(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ChampSelectSession {
        serde_json::from_str(include_str!(
            "../../lcu/tests/fixtures/champ_select_session.json"
        ))
        .unwrap()
    }

    #[test]
    fn champ_select_view_from_fixture() {
        let view = ChampSelectState::from_session(&fixture(), "CLASSIC", 42);
        assert_eq!(view.received_at, 42);
        assert_eq!(view.timer_phase, "BAN_PICK");
        let action = view.my_action.unwrap();
        assert_eq!(action.kind, ActionKind::Ban);
        assert!(action.is_my_turn);
        assert_eq!(view.my_team.len(), 5);
        assert_eq!(view.my_team.iter().filter(|m| m.is_me).count(), 1);
        assert_eq!(view.their_team.len(), 5);
        assert_eq!(view.my_selection.position, "middle");
        assert_eq!(view.my_selection.spell1_id, 4);
        assert!(view.bench.is_none());
        assert!(view.rerolls_remaining.is_none());
    }

    #[test]
    fn aram_bench_and_rerolls() {
        let s: ChampSelectSession = serde_json::from_str(
            r#"{
                "localPlayerCellId": 0,
                "actions": [],
                "myTeam": [{"cellId": 0, "championId": 22, "assignedPosition": ""}],
                "benchEnabled": true,
                "benchChampions": [{"championId": 1, "isPriority": false}, {"championId": 51}],
                "allowRerolling": true,
                "rerollsRemaining": 1
            }"#,
        )
        .unwrap();
        let view = ChampSelectState::from_session(&s, "ARAM", 0);
        assert_eq!(view.game_mode, "ARAM");
        assert!(view.my_action.is_none());
        assert_eq!(view.my_selection.champion_id, 22);
        assert_eq!(view.bench, Some(vec![1, 51]));
        assert_eq!(view.rerolls_remaining, Some(1));
    }

    #[test]
    fn unset_spell_becomes_zero() {
        let mut s = fixture();
        s.my_team[2].spell2_id = u64::MAX;
        assert_eq!(
            ChampSelectState::from_session(&s, "CLASSIC", 0)
                .my_selection
                .spell2_id,
            0
        );
    }

    #[test]
    fn enemy_slots_only_carry_champion() {
        let json =
            serde_json::to_value(ChampSelectState::from_session(&fixture(), "CLASSIC", 0)).unwrap();
        let enemy = json["theirTeam"][0].as_object().unwrap();
        assert_eq!(enemy.keys().collect::<Vec<_>>(), ["championId"]);
    }

    #[test]
    fn invalid_ready_check_is_hidden() {
        let rc = ReadyCheck {
            state: "Invalid".into(),
            ..Default::default()
        };
        assert!(ReadyCheckState::from_lcu(&rc).is_none());
    }
}
