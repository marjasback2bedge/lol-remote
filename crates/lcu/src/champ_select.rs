//! Champ select helpers: finding the local player's action and acting on it.

use serde::Serialize;
use serde_json::json;

use crate::types::{Action, ChampSelectSession, TeamMember};
use crate::{LcuClient, LcuError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionKind {
    Pick,
    Ban,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MyAction {
    pub id: i64,
    pub kind: ActionKind,
    pub champion_id: i64,
    pub completed: bool,
    pub is_in_progress: bool,
}

impl MyAction {
    pub fn is_my_turn(&self) -> bool {
        self.is_in_progress && !self.completed
    }
}

/// The local player's most relevant action: the one currently in progress,
/// otherwise the next pending one, otherwise the last completed one.
pub fn current_action(session: &ChampSelectSession) -> Option<MyAction> {
    let mine: Vec<(&Action, ActionKind)> = session
        .actions
        .iter()
        .flatten()
        .filter(|a| a.actor_cell_id == session.local_player_cell_id)
        .filter_map(|a| match a.kind.as_str() {
            "pick" => Some((a, ActionKind::Pick)),
            "ban" => Some((a, ActionKind::Ban)),
            _ => None,
        })
        .collect();
    let (action, kind) = mine
        .iter()
        .find(|(a, _)| a.is_in_progress && !a.completed)
        .or_else(|| mine.iter().find(|(a, _)| !a.completed))
        .or_else(|| mine.last())?;
    Some(MyAction {
        id: action.id,
        kind: *kind,
        champion_id: action.champion_id,
        completed: action.completed,
        is_in_progress: action.is_in_progress,
    })
}

pub fn local_member(session: &ChampSelectSession) -> Option<&TeamMember> {
    session
        .my_team
        .iter()
        .find(|m| m.cell_id == session.local_player_cell_id)
}

impl LcuClient {
    pub async fn champ_select_session(&self) -> Result<ChampSelectSession, LcuError> {
        self.get("/lol-champ-select/v1/session").await
    }

    pub async fn pickable_champion_ids(&self) -> Result<Vec<i64>, LcuError> {
        self.get("/lol-champ-select/v1/pickable-champion-ids").await
    }

    pub async fn bannable_champion_ids(&self) -> Result<Vec<i64>, LcuError> {
        self.get("/lol-champ-select/v1/bannable-champion-ids").await
    }

    async fn patch_action(
        &self,
        action_id: i64,
        champion_id: i64,
        completed: bool,
    ) -> Result<(), LcuError> {
        self.patch::<serde_json::Value, _>(
            &format!("/lol-champ-select/v1/session/actions/{action_id}"),
            &json!({ "championId": champion_id, "completed": completed }),
        )
        .await
        .map(drop)
    }

    pub async fn hover(&self, action_id: i64, champion_id: i64) -> Result<(), LcuError> {
        self.patch_action(action_id, champion_id, false).await
    }

    pub async fn lock(&self, action_id: i64, champion_id: i64) -> Result<(), LcuError> {
        self.patch_action(action_id, champion_id, true).await
    }

    pub async fn set_summoner_spells(
        &self,
        spell1_id: u64,
        spell2_id: u64,
    ) -> Result<(), LcuError> {
        self.patch::<serde_json::Value, _>(
            "/lol-champ-select/v1/session/my-selection",
            &json!({ "spell1Id": spell1_id, "spell2Id": spell2_id }),
        )
        .await
        .map(drop)
    }

    /// Swaps the local player's champion with one from the bench (ARAM).
    pub async fn bench_swap(&self, champion_id: i64) -> Result<(), LcuError> {
        self.post_empty::<serde_json::Value>(&format!(
            "/lol-champ-select/v1/session/bench/swap/{champion_id}"
        ))
        .await
        .map(drop)
    }

    pub async fn reroll(&self) -> Result<(), LcuError> {
        self.post_empty::<serde_json::Value>("/lol-champ-select/v1/session/my-selection/reroll")
            .await
            .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ChampSelectSession {
        serde_json::from_str(include_str!("../tests/fixtures/champ_select_session.json")).unwrap()
    }

    fn set(session: &mut ChampSelectSession, id: i64, in_progress: bool, completed: bool) {
        for a in session.actions.iter_mut().flatten() {
            if a.id == id {
                a.is_in_progress = in_progress;
                a.completed = completed;
            }
        }
    }

    #[test]
    fn ban_in_progress_is_my_turn() {
        let a = current_action(&fixture()).unwrap();
        assert_eq!(a.kind, ActionKind::Ban);
        assert_eq!(a.id, 3);
        assert!(a.is_my_turn());
    }

    #[test]
    fn pick_not_my_turn_yet() {
        let mut s = fixture();
        set(&mut s, 3, false, true);
        let a = current_action(&s).unwrap();
        assert_eq!(a.kind, ActionKind::Pick);
        assert!(!a.is_my_turn());
    }

    #[test]
    fn pick_my_turn() {
        let mut s = fixture();
        set(&mut s, 3, false, true);
        set(&mut s, 16, true, false);
        let a = current_action(&s).unwrap();
        assert_eq!(a.kind, ActionKind::Pick);
        assert!(a.is_my_turn());
    }

    #[test]
    fn all_completed_returns_last() {
        let mut s = fixture();
        set(&mut s, 3, false, true);
        set(&mut s, 16, false, true);
        let a = current_action(&s).unwrap();
        assert_eq!(a.id, 16);
        assert!(a.completed);
        assert!(!a.is_my_turn());
    }

    #[test]
    fn no_actions_for_spectator() {
        let mut s = fixture();
        s.local_player_cell_id = 42;
        assert!(current_action(&s).is_none());
    }

    #[test]
    fn local_member_found() {
        let s = fixture();
        assert_eq!(local_member(&s).unwrap().assigned_position, "middle");
    }
}
