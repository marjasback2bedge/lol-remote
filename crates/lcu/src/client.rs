use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::RequestBuilder;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::types::{ChampionSummary, GameflowSession, PerkPage, Phase, ReadyCheck, SummonerSpell};
use crate::{tls, Credentials, LcuError};

/// REST client for the LCU. Cheap to clone.
#[derive(Clone)]
pub struct LcuClient {
    http: reqwest::Client,
    base: String,
}

impl LcuClient {
    pub fn new(creds: &Credentials) -> Result<Self, LcuError> {
        let mut headers = HeaderMap::new();
        let mut auth = HeaderValue::from_str(&tls::basic_auth(&creds.password))
            .map_err(|_| LcuError::LockfileFormat("password is not a valid header".into()))?;
        auth.set_sensitive(true);
        headers.insert(AUTHORIZATION, auth);
        let http = reqwest::Client::builder()
            .use_preconfigured_tls((*tls::localhost_client_config()).clone())
            .default_headers(headers)
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            http,
            base: format!("https://127.0.0.1:{}", creds.port),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    async fn send<T: DeserializeOwned>(&self, req: RequestBuilder) -> Result<T, LcuError> {
        let res = req.send().await?;
        let status = res.status();
        let body = res.bytes().await?;
        if !status.is_success() {
            return Err(LcuError::Status {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body).into_owned(),
            });
        }
        // Many LCU actions answer 204 with an empty body.
        if body.iter().all(u8::is_ascii_whitespace) {
            return Ok(serde_json::from_str("null")?);
        }
        Ok(serde_json::from_slice(&body)?)
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, LcuError> {
        self.send(self.http.get(self.url(path))).await
    }

    pub async fn post<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LcuError> {
        self.send(self.http.post(self.url(path)).json(body)).await
    }

    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, LcuError> {
        self.send(self.http.post(self.url(path))).await
    }

    pub async fn patch<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LcuError> {
        self.send(self.http.patch(self.url(path)).json(body)).await
    }

    pub async fn put<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, LcuError> {
        self.send(self.http.put(self.url(path)).json(body)).await
    }

    pub async fn delete(&self, path: &str) -> Result<(), LcuError> {
        self.send::<serde_json::Value>(self.http.delete(self.url(path)))
            .await
            .map(drop)
    }

    /// Raw GET for binary assets; returns `(content-type, body)`.
    pub async fn get_bytes(&self, path: &str) -> Result<(Option<String>, Vec<u8>), LcuError> {
        let res = self.http.get(self.url(path)).send().await?;
        let status = res.status();
        if !status.is_success() {
            return Err(LcuError::Status {
                status: status.as_u16(),
                body: String::new(),
            });
        }
        let content_type = res
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        Ok((content_type, res.bytes().await?.to_vec()))
    }

    pub async fn gameflow_phase(&self) -> Result<Phase, LcuError> {
        self.get("/lol-gameflow/v1/gameflow-phase").await
    }

    pub async fn gameflow_session(&self) -> Result<GameflowSession, LcuError> {
        self.get("/lol-gameflow/v1/session").await
    }

    pub async fn ready_check(&self) -> Result<ReadyCheck, LcuError> {
        self.get("/lol-matchmaking/v1/ready-check").await
    }

    pub async fn accept_ready_check(&self) -> Result<(), LcuError> {
        self.post_empty::<serde_json::Value>("/lol-matchmaking/v1/ready-check/accept")
            .await
            .map(drop)
    }

    pub async fn decline_ready_check(&self) -> Result<(), LcuError> {
        self.post_empty::<serde_json::Value>("/lol-matchmaking/v1/ready-check/decline")
            .await
            .map(drop)
    }

    pub async fn champion_summary(&self) -> Result<Vec<ChampionSummary>, LcuError> {
        self.get("/lol-game-data/assets/v1/champion-summary.json")
            .await
    }

    pub async fn summoner_spells(&self) -> Result<Vec<SummonerSpell>, LcuError> {
        self.get("/lol-game-data/assets/v1/summoner-spells.json")
            .await
    }

    pub async fn perk_pages(&self) -> Result<Vec<PerkPage>, LcuError> {
        self.get("/lol-perks/v1/pages").await
    }

    pub async fn set_current_perk_page(&self, page_id: i64) -> Result<(), LcuError> {
        self.put::<serde_json::Value, _>("/lol-perks/v1/currentpage", &page_id)
            .await
            .map(drop)
    }
}
