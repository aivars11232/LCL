//! The service's settings: where it listens and what it shares.
//!
//! `remote.json` in the configuration directory, written by the command line
//! (`lcl-remote projects add`, `lcl-remote address add`) and read by the
//! service. A missing file is the defaults. A file that cannot be read stops
//! the service with the reason, rather than starting it with settings nobody
//! chose.

use crate::paths::{self, Paths};
use lcl_protocol::json::{Node, Object};
use lcl_spec::json::Json;
use std::path::PathBuf;

/// The TCP port paired devices connect to by default.
pub const DEFAULT_PORT: u16 = 47300;
/// The UDP port the service answers local discovery on.
pub const DEFAULT_DISCOVERY_PORT: u16 = 47301;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The address to listen on. `0.0.0.0` by default, so a phone on the same
    /// network can reach it; nothing is served to anybody who has not paired.
    pub listen: String,
    pub port: u16,
    pub discovery_port: u16,
    /// Projects shared besides the default workspace.
    pub projects: Vec<PathBuf>,
    /// Addresses a device can use from other networks — a name or public
    /// address forwarded to this PC — put into pairing codes after the local
    /// ones. The service does not make itself reachable from the Internet;
    /// this only tells a device where it already is.
    pub public_addresses: Vec<String>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            listen: "0.0.0.0".into(),
            port: DEFAULT_PORT,
            discovery_port: DEFAULT_DISCOVERY_PORT,
            projects: Vec::new(),
            public_addresses: Vec::new(),
        }
    }
}

impl Config {
    fn file(paths: &Paths) -> PathBuf {
        paths.config.join("remote.json")
    }

    pub fn load(paths: &Paths) -> Result<Config, String> {
        let file = Config::file(paths);
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(e) => return Err(format!("{}: {e}", file.display())),
        };
        let json = lcl_spec::json::parse(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        if json.get("version").and_then(Json::as_u64) != Some(1) {
            return Err(format!("{} is not version 1", file.display()));
        }
        let mut config = Config::default();
        if let Some(listen) = json.get("listen").and_then(Json::as_str) {
            config.listen = listen.to_string();
        }
        let port = |key: &str, default: u16| -> Result<u16, String> {
            match json.get(key) {
                None => Ok(default),
                Some(v) => v
                    .as_u64()
                    .and_then(|n| u16::try_from(n).ok())
                    .filter(|&n| n > 0)
                    .ok_or_else(|| format!("{key} in {} is not a port", file.display())),
            }
        };
        config.port = port("port", DEFAULT_PORT)?;
        config.discovery_port = port("discovery_port", DEFAULT_DISCOVERY_PORT)?;
        let strings = |key: &str| -> Vec<String> {
            json.get(key)
                .and_then(Json::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        config.projects = strings("projects").into_iter().map(PathBuf::from).collect();
        config.public_addresses = strings("public_addresses");
        Ok(config)
    }

    pub fn store(&self, paths: &Paths) -> Result<(), String> {
        let text = Object::new()
            .with("version", Node::u64(1))
            .with("listen", Node::string(&self.listen))
            .with("port", Node::u64(u64::from(self.port)))
            .with("discovery_port", Node::u64(u64::from(self.discovery_port)))
            .with(
                "projects",
                Node::array(
                    self.projects
                        .iter()
                        .map(|p| Node::string(p.display().to_string())),
                ),
            )
            .with(
                "public_addresses",
                Node::array(self.public_addresses.iter().map(Node::string)),
            )
            .pretty();
        paths::write_private(&Config::file(paths), text.as_bytes()).map_err(|e| e.to_string())
    }
}
