//! Minimal client for the League of Legends Client Update (LCU) API.
//!
//! Only the endpoints and fields lol-remote needs are modelled. The LCU API is
//! unofficial and changes between patches, so every type ignores unknown fields
//! and defaults missing ones.

pub mod champ_select;
pub mod client;
pub mod error;
pub mod events;
pub mod lockfile;
mod tls;
pub mod types;

pub use champ_select::{current_action, ActionKind, MyAction};
pub use client::LcuClient;
pub use error::LcuError;
pub use events::{connect_events, LcuEvent};
pub use lockfile::Credentials;
pub use types::Phase;
