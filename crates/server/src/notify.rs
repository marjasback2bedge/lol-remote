//! Push notifications through ntfy (spec §7.5). Never include summoner names
//! or other personal data: ntfy.sh topics are public to anyone who knows them.

use std::time::Duration;

use serde_json::{json, Value};

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Push {
    MatchFound,
    AutoAccepted,
    Cancelled,
}

pub struct Notifier {
    http: reqwest::Client,
}

impl Notifier {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .expect("default reqwest client");
        Self { http }
    }

    /// Sends a push. Failures are logged and otherwise ignored.
    pub async fn send(&self, config: &Config, base_url: &str, push: Push) {
        if config.ntfy_topic.is_empty() {
            return;
        }
        let body = message(config, base_url, push);
        let url = format!("{}/", config.ntfy_server.trim_end_matches('/'));
        match self.http.post(&url).json(&body).send().await {
            Ok(res) if res.status().is_success() => tracing::info!("sent {push:?} notification"),
            Ok(res) => tracing::warn!("ntfy answered {} for {push:?}", res.status()),
            Err(e) => tracing::warn!("ntfy request failed: {e}"),
        }
    }
}

pub fn message(config: &Config, base_url: &str, push: Push) -> Value {
    let open_url = format!("{base_url}/?t={}", config.auth_token);
    let open = json!({ "action": "view", "label": "開啟", "url": open_url });
    let (title, message, priority, tags, actions) = match push {
        Push::MatchFound => (
            "找到對戰！",
            "10 秒內接受",
            5,
            json!(["crossed_swords"]),
            json!([
                {
                    "action": "http",
                    "label": "接受",
                    "url": format!("{base_url}/api/ready-check/accept"),
                    "method": "POST",
                    "headers": { "Authorization": format!("Bearer {}", config.auth_token) },
                    "clear": true
                },
                open
            ]),
        ),
        Push::AutoAccepted => (
            "已自動接受對戰",
            "準備進入選角",
            4,
            json!(["white_check_mark"]),
            json!([open]),
        ),
        Push::Cancelled => ("配對取消", "已回到佇列或大廳", 2, json!(["x"]), json!([])),
    };
    json!({
        "topic": config.ntfy_topic,
        "title": title,
        "message": message,
        "priority": priority,
        "tags": tags,
        "click": open_url,
        "actions": actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_found_has_accept_action() {
        let config = Config::default();
        let msg = message(&config, "http://pc.local:3000", Push::MatchFound);
        assert_eq!(msg["topic"], config.ntfy_topic.as_str());
        assert_eq!(msg["priority"], 5);
        let accept = &msg["actions"][0];
        assert_eq!(accept["url"], "http://pc.local:3000/api/ready-check/accept");
        assert_eq!(
            accept["headers"]["Authorization"],
            format!("Bearer {}", config.auth_token)
        );
        assert_eq!(
            msg["click"],
            format!("http://pc.local:3000/?t={}", config.auth_token)
        );
    }
}
