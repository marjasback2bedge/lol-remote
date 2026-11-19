use std::fmt;
use std::path::{Path, PathBuf};

use crate::LcuError;

/// Connection details read from the LoL client's lockfile.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub name: String,
    pub pid: u32,
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

// Hand-written so the password never ends up in logs.
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("name", &self.name)
            .field("pid", &self.pid)
            .field("port", &self.port)
            .field("password", &"<redacted>")
            .field("protocol", &self.protocol)
            .finish()
    }
}

impl Credentials {
    /// Parses `<name>:<pid>:<port>:<password>:<protocol>`.
    pub fn parse(contents: &str) -> Result<Self, LcuError> {
        let contents = contents.trim();
        if contents.is_empty() {
            return Err(LcuError::LockfileFormat("lockfile is empty".into()));
        }
        let fields: Vec<&str> = contents.split(':').collect();
        let [name, pid, port, password, protocol] = fields[..] else {
            return Err(LcuError::LockfileFormat(format!(
                "expected 5 ':'-separated fields, found {}",
                fields.len()
            )));
        };
        let pid = pid
            .parse()
            .map_err(|_| LcuError::LockfileFormat(format!("invalid pid {pid:?}")))?;
        let port = port
            .parse()
            .map_err(|_| LcuError::LockfileFormat(format!("invalid port {port:?}")))?;
        if password.is_empty() {
            return Err(LcuError::LockfileFormat("empty password".into()));
        }
        Ok(Self {
            name: name.to_owned(),
            pid,
            port,
            password: password.to_owned(),
            protocol: protocol.to_owned(),
        })
    }

    pub fn read(path: &Path) -> Result<Self, LcuError> {
        let contents = std::fs::read_to_string(path).map_err(|source| LcuError::LockfileIo {
            path: path.to_owned(),
            source,
        })?;
        Self::parse(&contents)
    }
}

/// Where the lockfile lives on a default install.
pub fn default_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        PathBuf::from(r"C:\Riot Games\League of Legends\lockfile")
    } else {
        PathBuf::from("/Applications/League of Legends.app/Contents/LoL/lockfile")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_lockfile() {
        let c = Credentials::parse("LeagueClient:12345:54321:abcXYZ:https\n").unwrap();
        assert_eq!(c.name, "LeagueClient");
        assert_eq!(c.pid, 12345);
        assert_eq!(c.port, 54321);
        assert_eq!(c.password, "abcXYZ");
        assert_eq!(c.protocol, "https");
    }

    #[test]
    fn rejects_missing_fields() {
        let err = Credentials::parse("LeagueClient:12345:54321").unwrap_err();
        assert!(matches!(err, LcuError::LockfileFormat(_)));
    }

    #[test]
    fn rejects_empty_file() {
        assert!(matches!(
            Credentials::parse("  \n").unwrap_err(),
            LcuError::LockfileFormat(_)
        ));
    }

    #[test]
    fn rejects_bad_port() {
        assert!(Credentials::parse("LeagueClient:1:notaport:pw:https").is_err());
    }

    #[test]
    fn debug_hides_password() {
        let c = Credentials::parse("LeagueClient:1:2:secret:https").unwrap();
        assert!(!format!("{c:?}").contains("secret"));
    }

    #[test]
    fn missing_file_is_an_error() {
        let err = Credentials::read(Path::new("/definitely/not/here/lockfile")).unwrap_err();
        assert!(matches!(err, LcuError::LockfileIo { .. }));
    }
}
