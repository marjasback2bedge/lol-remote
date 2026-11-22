//! LCU payload types. Only fields lol-remote reads are declared; everything
//! else is ignored and missing fields fall back to defaults.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    None,
    Lobby,
    Matchmaking,
    CheckedIntoTournament,
    ReadyCheck,
    ChampSelect,
    GameStart,
    FailedToLaunch,
    InProgress,
    Reconnect,
    WaitingForStats,
    PreEndOfGame,
    EndOfGame,
    TerminatedInError,
    Unknown(String),
}

impl Phase {
    pub fn as_str(&self) -> &str {
        match self {
            Phase::None => "None",
            Phase::Lobby => "Lobby",
            Phase::Matchmaking => "Matchmaking",
            Phase::CheckedIntoTournament => "CheckedIntoTournament",
            Phase::ReadyCheck => "ReadyCheck",
            Phase::ChampSelect => "ChampSelect",
            Phase::GameStart => "GameStart",
            Phase::FailedToLaunch => "FailedToLaunch",
            Phase::InProgress => "InProgress",
            Phase::Reconnect => "Reconnect",
            Phase::WaitingForStats => "WaitingForStats",
            Phase::PreEndOfGame => "PreEndOfGame",
            Phase::EndOfGame => "EndOfGame",
            Phase::TerminatedInError => "TerminatedInError",
            Phase::Unknown(s) => s,
        }
    }
}

impl From<&str> for Phase {
    fn from(s: &str) -> Self {
        match s {
            "None" => Phase::None,
            "Lobby" => Phase::Lobby,
            "Matchmaking" => Phase::Matchmaking,
            "CheckedIntoTournament" => Phase::CheckedIntoTournament,
            "ReadyCheck" => Phase::ReadyCheck,
            "ChampSelect" => Phase::ChampSelect,
            "GameStart" => Phase::GameStart,
            "FailedToLaunch" => Phase::FailedToLaunch,
            "InProgress" => Phase::InProgress,
            "Reconnect" => Phase::Reconnect,
            "WaitingForStats" => Phase::WaitingForStats,
            "PreEndOfGame" => Phase::PreEndOfGame,
            "EndOfGame" => Phase::EndOfGame,
            "TerminatedInError" => Phase::TerminatedInError,
            other => Phase::Unknown(other.to_owned()),
        }
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Phase {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Phase::from(String::deserialize(d)?.as_str()))
    }
}

impl Serialize for Phase {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReadyCheck {
    /// `Invalid`, `InProgress`, `EveryoneReady`, `StrangerNotReady`, ...
    pub state: String,
    /// `None`, `Accepted`, `Declined`.
    pub player_response: String,
    /// Seconds elapsed since the ready check started.
    pub timer: f64,
}

impl ReadyCheck {
    pub fn awaiting_my_response(&self) -> bool {
        self.state == "InProgress" && self.player_response == "None"
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChampSelectSession {
    pub local_player_cell_id: i64,
    pub actions: Vec<Vec<Action>>,
    pub my_team: Vec<TeamMember>,
    pub their_team: Vec<TeamMember>,
    pub bans: Bans,
    pub timer: Timer,
    /// ARAM-style modes: champions anyone on the team can swap for.
    pub bench_enabled: bool,
    pub bench_champions: Vec<BenchChampion>,
    pub allow_rerolling: bool,
    pub rerolls_remaining: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BenchChampion {
    pub champion_id: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Action {
    pub id: i64,
    pub actor_cell_id: i64,
    pub champion_id: i64,
    pub completed: bool,
    pub is_in_progress: bool,
    /// `pick`, `ban`, `ten_bans_reveal`, ...
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TeamMember {
    pub cell_id: i64,
    pub champion_id: i64,
    pub champion_pick_intent: i64,
    pub assigned_position: String,
    /// Unset spells are reported as `u64::MAX`.
    pub spell1_id: u64,
    pub spell2_id: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Bans {
    pub my_team_bans: Vec<i64>,
    pub their_team_bans: Vec<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Timer {
    pub adjusted_time_left_in_phase: i64,
    /// `PLANNING`, `BAN_PICK`, `FINALIZATION`, `GAME_STARTING`.
    pub phase: String,
}

/// `/lol-gameflow/v1/session`, reduced to the game mode.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameflowSession {
    pub map: GameflowMap,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameflowMap {
    /// `CLASSIC`, `ARAM`, `CHERRY` (Arena), `URF`, ...
    pub game_mode: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChampionSummary {
    pub id: i64,
    pub name: String,
    pub alias: String,
    pub square_portrait_path: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SummonerSpell {
    pub id: u64,
    pub name: String,
    pub icon_path: String,
    pub game_modes: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PerkPage {
    pub id: i64,
    pub name: String,
    pub is_editable: bool,
    pub current: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_round_trips() {
        let p: Phase = serde_json::from_str("\"ChampSelect\"").unwrap();
        assert_eq!(p, Phase::ChampSelect);
        let u: Phase = serde_json::from_str("\"SomethingNew\"").unwrap();
        assert_eq!(u, Phase::Unknown("SomethingNew".into()));
        assert_eq!(serde_json::to_string(&u).unwrap(), "\"SomethingNew\"");
    }

    #[test]
    fn ready_check_ignores_unknown_fields() {
        let rc: ReadyCheck = serde_json::from_str(
            r#"{"state":"InProgress","playerResponse":"None","timer":3.0,"dodgeWarning":"None"}"#,
        )
        .unwrap();
        assert!(rc.awaiting_my_response());
    }
}
