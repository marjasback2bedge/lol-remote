use std::path::{Path, PathBuf};

use anyhow::Context;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub port: u16,
    /// Empty = platform default.
    pub lockfile_path: String,
    pub auth_token: String,
    pub ntfy_server: String,
    pub ntfy_topic: String,
    pub auto_accept: bool,
    /// Empty = system hostname.
    pub hostname: String,
    /// Push a low-priority "match cancelled" notification.
    pub notify_cancel: bool,
    /// Use the LAN IP instead of `<host>.local` in notification links.
    pub notify_use_ip: bool,
}

impl Default for Config {
    fn default() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            port: 3000,
            lockfile_path: String::new(),
            auth_token: Alphanumeric.sample_string(&mut rng, 32),
            ntfy_server: "https://ntfy.sh".into(),
            ntfy_topic: format!("lolr-{}", Alphanumeric.sample_string(&mut rng, 24)),
            auto_accept: false,
            hostname: String::new(),
            notify_cancel: true,
            notify_use_ip: false,
        }
    }
}

const HEADER: &str = "# lol-remote configuration. Keep auth_token and ntfy_topic secret.\n\n";

impl Config {
    pub fn default_path() -> anyhow::Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("dev", "lol-remote", "lol-remote")
            .context("could not determine a config directory")?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    /// Loads the config, creating it on first run. Returns `(config, created)`.
    pub fn load_or_create(path: &Path) -> anyhow::Result<(Self, bool)> {
        if !path.exists() {
            let config = Self::default();
            config.save(path)?;
            return Ok((config, true));
        }
        let contents =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let table: toml::Table =
            toml::from_str(&contents).with_context(|| format!("parsing {}", path.display()))?;
        let mut config: Self = table.clone().try_into()?;
        if config.auth_token.is_empty() {
            config.auth_token = Self::default().auth_token;
        }
        // Persist generated values (token, topic) for keys missing from the file.
        let complete = toml::Table::try_from(&config)?;
        if complete.keys().any(|k| !table.contains_key(k)) || config.auth_token.len() < 16 {
            config.save(path)?;
        }
        Ok((config, false))
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = toml::to_string(self)?;
        std::fs::write(path, format!("{HEADER}{body}"))
            .with_context(|| format!("writing {}", path.display()))
    }

    pub fn lockfile(&self) -> PathBuf {
        if self.lockfile_path.is_empty() {
            lcu::lockfile::default_path()
        } else {
            PathBuf::from(&self.lockfile_path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_then_reloads_same_values() {
        let dir = std::env::temp_dir().join(format!("lolr-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let (a, created) = Config::load_or_create(&path).unwrap();
        assert!(created);
        assert_eq!(a.auth_token.len(), 32);
        assert!(a.ntfy_topic.starts_with("lolr-"));
        let (b, created) = Config::load_or_create(&path).unwrap();
        assert!(!created);
        assert_eq!(a.auth_token, b.auth_token);
        assert_eq!(a.ntfy_topic, b.ntfy_topic);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn fills_in_missing_keys() {
        let dir = std::env::temp_dir().join(format!("lolr-test-partial-{}", std::process::id()));
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "port = 4000\n").unwrap();
        let (a, _) = Config::load_or_create(&path).unwrap();
        assert_eq!(a.port, 4000);
        let (b, _) = Config::load_or_create(&path).unwrap();
        assert_eq!(a.auth_token, b.auth_token);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
