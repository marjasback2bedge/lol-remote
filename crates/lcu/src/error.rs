use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum LcuError {
    #[error("failed to read lockfile {path}: {source}")]
    LockfileIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("malformed lockfile: {0}")]
    LockfileFormat(String),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("LCU returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("websocket error: {0}")]
    WebSocket(Box<tokio_tungstenite::tungstenite::Error>),
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<tokio_tungstenite::tungstenite::Error> for LcuError {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::WebSocket(Box::new(e))
    }
}
