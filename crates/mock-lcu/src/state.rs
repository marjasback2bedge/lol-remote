//! Simulated client state: gameflow, ready check and a draft champ select in
//! which the other nine players act automatically.

use rand::seq::SliceRandom;
use serde_json::{json, Value};
use tokio::sync::broadcast;

const GAMEFLOW: &str = "OnJsonApiEvent_lol-gameflow_v1_gameflow-phase";
const READY_CHECK: &str = "OnJsonApiEvent_lol-matchmaking_v1_ready-check";
const CHAMP_SELECT: &str = "OnJsonApiEvent_lol-champ-select_v1_session";

const SESSION_FIXTURE: &str = include_str!("../../lcu/tests/fixtures/champ_select_session.json");
const STAGE_MS: i64 = 30_000;
const FINALIZATION_MS: i64 = 15_000;
/// Other players act this long after their turn starts.
const BOT_DELAY_MS: i64 = 3_000;
const READY_CHECK_SECS: f64 = 12.0;

#[derive(Clone, Debug)]
pub struct WampEvent {
    pub name: &'static str,
    pub uri: &'static str,
    pub event_type: &'static str,
    pub data: Value,
}

struct ReadyCheck {
    state: &'static str,
    player_response: &'static str,
    timer: f64,
}

struct ChampSelect {
    session: Value,
    stage: usize,
    stage_elapsed_ms: i64,
}

pub struct Mock {
    pub phase: &'static str,
    events: broadcast::Sender<WampEvent>,
    queue_secs: u32,
    ready_check: Option<ReadyCheck>,
    champ_select: Option<ChampSelect>,
    champions: Vec<i64>,
    perk_pages: Vec<(i64, &'static str, bool)>,
    current_page: i64,
}

impl Mock {
    pub fn new(events: broadcast::Sender<WampEvent>) -> Self {
        let summary: Vec<Value> =
            serde_json::from_str(include_str!("../data/champion-summary.json")).unwrap();
        let champions = summary
            .iter()
            .filter_map(|c| c["id"].as_i64())
            .filter(|&id| id > 0)
            .collect();
        Self {
            phase: "None",
            events,
            queue_secs: 0,
            ready_check: None,
            champ_select: None,
            champions,
            perk_pages: vec![
                (101, "中路 - 電刑", true),
                (102, "下路 - 致命節奏", true),
                (103, "上路 - 不滅之握", true),
                (50, "推薦頁面", false),
            ],
            current_page: 101,
        }
    }

    fn emit(&self, name: &'static str, uri: &'static str, data: Option<Value>) {
        let (event_type, data) = match data {
            Some(d) => ("Update", d),
            None => ("Delete", Value::Null),
        };
        let _ = self.events.send(WampEvent {
            name,
            uri,
            event_type,
            data,
        });
    }

    pub fn set_phase(&mut self, phase: &'static str) {
        if phase != "ReadyCheck" && self.ready_check.take().is_some() {
            self.emit(READY_CHECK, "/lol-matchmaking/v1/ready-check", None);
        }
        if phase != "ChampSelect" && self.champ_select.take().is_some() {
            self.emit(CHAMP_SELECT, "/lol-champ-select/v1/session", None);
        }
        self.phase = phase;
        self.emit(
            GAMEFLOW,
            "/lol-gameflow/v1/gameflow-phase",
            Some(json!(phase)),
        );
    }

    pub fn start_queue(&mut self) {
        self.queue_secs = 0;
        self.set_phase("Matchmaking");
    }

    pub fn start_ready_check(&mut self) {
        self.ready_check = Some(ReadyCheck {
            state: "InProgress",
            player_response: "None",
            timer: 0.0,
        });
        self.set_phase("ReadyCheck");
        self.emit_ready_check();
    }

    pub fn others_decline(&mut self) {
        if let Some(rc) = &mut self.ready_check {
            rc.state = "StrangerNotReady";
            self.emit_ready_check();
            self.start_queue();
        }
    }

    pub fn start_champ_select(&mut self) {
        let mut session: Value = serde_json::from_str(SESSION_FIXTURE).unwrap();
        session["timer"]["adjustedTimeLeftInPhase"] = json!(STAGE_MS);
        self.champ_select = Some(ChampSelect {
            session,
            stage: 0,
            stage_elapsed_ms: 0,
        });
        self.set_phase("ChampSelect");
        self.emit_session();
    }

    pub fn start_game(&mut self) {
        self.set_phase("GameStart");
        self.set_phase("InProgress");
    }

    fn emit_ready_check(&self) {
        self.emit(
            READY_CHECK,
            "/lol-matchmaking/v1/ready-check",
            self.ready_check_json(),
        );
    }

    fn emit_session(&self) {
        self.emit(
            CHAMP_SELECT,
            "/lol-champ-select/v1/session",
            self.session().cloned(),
        );
    }

    pub fn ready_check_json(&self) -> Option<Value> {
        self.ready_check.as_ref().map(|rc| {
            json!({
                "state": rc.state,
                "playerResponse": rc.player_response,
                "timer": rc.timer,
                "dodgeWarning": "None",
                "declinerIds": [],
            })
        })
    }

    pub fn respond_ready_check(&mut self, response: &'static str) -> Result<(), &'static str> {
        let rc = self
            .ready_check
            .as_mut()
            .filter(|rc| rc.state == "InProgress")
            .ok_or("No ready check in progress")?;
        rc.player_response = response;
        println!("ready check: {response}");
        self.emit_ready_check();
        if response == "Declined" {
            self.set_phase("Lobby");
        }
        Ok(())
    }

    pub fn session(&self) -> Option<&Value> {
        self.champ_select.as_ref().map(|cs| &cs.session)
    }

    /// Champions not yet banned or picked by anyone.
    pub fn available_champions(&self) -> Vec<i64> {
        let taken = self.taken_champions();
        self.champions
            .iter()
            .copied()
            .filter(|id| !taken.contains(id))
            .collect()
    }

    fn taken_champions(&self) -> Vec<i64> {
        let Some(cs) = &self.champ_select else {
            return Vec::new();
        };
        all_actions(&cs.session)
            .filter(|a| a["completed"] == true)
            .filter_map(|a| a["championId"].as_i64())
            .filter(|&id| id > 0)
            .collect()
    }

    pub fn patch_action(&mut self, id: i64, body: &Value) -> Result<(), String> {
        let champion_id = body["championId"].as_i64();
        let complete = body["completed"].as_bool().unwrap_or(false);
        if let Some(cid) = champion_id {
            if cid > 0 && !self.available_champions().contains(&cid) {
                return Err(format!("champion {cid} is not available"));
            }
        }
        let cs = self.champ_select.as_mut().ok_or("No active delegate")?;
        let local = cs.session["localPlayerCellId"].clone();
        let action = all_actions_mut(&mut cs.session)
            .find(|a| a["id"] == id)
            .ok_or_else(|| format!("no action {id}"))?;
        if action["actorCellId"] != local {
            return Err("not your action".into());
        }
        if action["completed"] == true {
            return Err("action already completed".into());
        }
        if let Some(cid) = champion_id {
            action["championId"] = json!(cid);
        }
        if complete {
            if action["isInProgress"] != true {
                return Err("action is not in progress".into());
            }
            action["completed"] = json!(true);
        }
        let is_pick = action["type"] == "pick";
        let champ = action["championId"].clone();
        let done = action["completed"] == true;
        if is_pick {
            let member = local_member_mut(&mut cs.session);
            member["championPickIntent"] = if done { json!(0) } else { champ.clone() };
            member["championId"] = if done {
                champ
            } else {
                member["championId"].clone()
            };
        }
        println!(
            "my {}: champion {} {}",
            if is_pick { "pick" } else { "ban" },
            body["championId"],
            if done { "(locked)" } else { "(hover)" }
        );
        self.advance_if_stage_done();
        self.emit_session();
        Ok(())
    }

    pub fn patch_my_selection(&mut self, body: &Value) -> Result<(), String> {
        let cs = self.champ_select.as_mut().ok_or("No active delegate")?;
        let member = local_member_mut(&mut cs.session);
        for key in ["spell1Id", "spell2Id"] {
            if let Some(v) = body[key].as_u64() {
                member[key] = json!(v);
            }
        }
        println!("spells: {} / {}", member["spell1Id"], member["spell2Id"]);
        self.emit_session();
        Ok(())
    }

    pub fn perk_pages(&self) -> Value {
        Value::Array(
            self.perk_pages
                .iter()
                .map(|&(id, name, editable)| {
                    json!({
                        "id": id,
                        "name": name,
                        "isEditable": editable,
                        "isDeletable": editable,
                        "current": id == self.current_page,
                        "isActive": id == self.current_page,
                        "primaryStyleId": 8100,
                        "subStyleId": 8000,
                    })
                })
                .collect(),
        )
    }

    pub fn set_current_page(&mut self, id: i64) -> Result<(), String> {
        if !self.perk_pages.iter().any(|p| p.0 == id) {
            return Err(format!("no rune page {id}"));
        }
        self.current_page = id;
        println!("rune page: {id}");
        Ok(())
    }

    pub fn tick(&mut self) {
        match self.phase {
            "Matchmaking" => {
                self.queue_secs += 1;
                if self.queue_secs >= 3 {
                    self.start_ready_check();
                }
            }
            "ReadyCheck" => self.tick_ready_check(),
            "ChampSelect" => self.tick_champ_select(),
            _ => {}
        }
    }

    fn tick_ready_check(&mut self) {
        let Some(rc) = &mut self.ready_check else {
            return;
        };
        rc.timer += 1.0;
        match (rc.state, rc.player_response) {
            ("InProgress", "Accepted") if rc.timer >= 3.0 => {
                rc.state = "EveryoneReady";
                self.emit_ready_check();
            }
            ("EveryoneReady", _) => self.start_champ_select(),
            ("InProgress", "None") if rc.timer >= READY_CHECK_SECS => {
                println!("ready check timed out");
                rc.state = "PartyNotReady";
                self.emit_ready_check();
                self.set_phase("Lobby");
            }
            _ => self.emit_ready_check(),
        }
    }

    fn tick_champ_select(&mut self) {
        let Some(cs) = &mut self.champ_select else {
            return;
        };
        cs.stage_elapsed_ms += 1000;
        let left = cs.session["timer"]["adjustedTimeLeftInPhase"]
            .as_i64()
            .unwrap_or(0)
            - 1000;
        cs.session["timer"]["adjustedTimeLeftInPhase"] = json!(left.max(0));
        let finalizing = cs.session["timer"]["phase"] == "FINALIZATION";

        if left <= 0 {
            if finalizing {
                println!("champ select finished, starting game");
                self.start_game();
            } else {
                println!("timer ran out on your turn: dodge, back to lobby");
                self.set_phase("Lobby");
            }
            return;
        }
        if !finalizing && cs.stage_elapsed_ms >= BOT_DELAY_MS {
            self.bots_act();
        }
        // The real client re-sends the session periodically; this also keeps
        // the timer in sync for clients that rely on it.
        self.emit_session();
    }

    /// Other players complete their in-progress actions with a random champion.
    fn bots_act(&mut self) {
        let mut available = self.available_champions();
        available.shuffle(&mut rand::thread_rng());
        let cs = self.champ_select.as_mut().unwrap();
        let local = cs.session["localPlayerCellId"].clone();
        let mut picks = Vec::new();
        for action in all_actions_mut(&mut cs.session) {
            if action["isInProgress"] == true
                && action["completed"] == false
                && action["actorCellId"] != local
            {
                let champ = available.pop().unwrap_or(0);
                action["championId"] = json!(champ);
                action["completed"] = json!(true);
                if action["type"] == "pick" {
                    picks.push((action["actorCellId"].as_i64().unwrap_or(0), champ));
                }
            }
        }
        for (cell, champ) in picks {
            for team in ["myTeam", "theirTeam"] {
                if let Some(members) = cs.session[team].as_array_mut() {
                    for m in members.iter_mut().filter(|m| m["cellId"] == cell) {
                        m["championId"] = json!(champ);
                    }
                }
            }
        }
        self.advance_if_stage_done();
    }

    fn advance_if_stage_done(&mut self) {
        let Some(cs) = &mut self.champ_select else {
            return;
        };
        loop {
            let groups = cs.session["actions"].as_array_mut().unwrap();
            let Some(group) = groups.get_mut(cs.stage) else {
                break;
            };
            let group = group.as_array_mut().unwrap();
            if group[0]["type"] == "ten_bans_reveal" {
                group[0]["completed"] = json!(true);
            } else if group.iter().any(|a| a["completed"] == false) {
                for a in group.iter_mut() {
                    a["isInProgress"] = json!(true);
                }
                break;
            }
            for a in group.iter_mut() {
                a["isInProgress"] = json!(false);
            }
            if group[0]["type"] == "ten_bans_reveal" {
                reveal_bans(&mut cs.session);
            }
            cs.stage += 1;
            cs.stage_elapsed_ms = 0;
            cs.session["timer"]["adjustedTimeLeftInPhase"] = json!(STAGE_MS);
        }
        if cs.stage >= cs.session["actions"].as_array().map_or(0, Vec::len) {
            cs.session["timer"]["phase"] = json!("FINALIZATION");
            cs.session["timer"]["adjustedTimeLeftInPhase"] = json!(FINALIZATION_MS);
        }
    }
}

fn reveal_bans(session: &mut Value) {
    let (mut mine, mut theirs) = (Vec::new(), Vec::new());
    for a in all_actions(session).filter(|a| a["type"] == "ban") {
        let champ = a["championId"].as_i64().unwrap_or(0);
        if a["isAllyAction"] == true {
            mine.push(champ);
        } else {
            theirs.push(champ);
        }
    }
    session["bans"]["myTeamBans"] = json!(mine);
    session["bans"]["theirTeamBans"] = json!(theirs);
}

fn all_actions(session: &Value) -> impl Iterator<Item = &Value> {
    session["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
}

fn all_actions_mut(session: &mut Value) -> impl Iterator<Item = &mut Value> {
    session["actions"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .filter_map(Value::as_array_mut)
        .flatten()
}

fn local_member_mut(session: &mut Value) -> &mut Value {
    let local = session["localPlayerCellId"].clone();
    session["myTeam"]
        .as_array_mut()
        .and_then(|m| m.iter_mut().find(|m| m["cellId"] == local))
        .expect("fixture contains the local player")
}
