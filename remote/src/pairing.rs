//! One-time pairing: the QR code a device scans, and the challenge behind it.
//!
//! Pairing establishes long-lived trust once; it is not how a device connects
//! afterwards. The QR code carries a **link**:
//!
//! ```text
//! lclpair://pair?v=1&pc=<pc id>&n=<pc name>&fp=<certificate SHA-256>
//!               &a=<host:port>[&a=...]&c=<one-time code>&e=<expiry>
//! ```
//!
//! * `fp` lets the device authenticate the PC before saying anything to it:
//!   the TLS connection is refused unless the PC presents exactly that
//!   certificate.
//! * `a` is where to try reaching it. Addresses are hints for finding the PC,
//!   never part of who it is.
//! * `c` is a 32-byte random secret, good for one pairing and a few minutes.
//!   The PC keeps only its SHA-256, marks it used the moment a device pairs
//!   with it, and refuses it after its expiry. It is not a password: it never
//!   authenticates anything again, and a device that has paired connects with
//!   its own key.
//!
//! No private key is ever in a link.

use crate::b64;
use crate::devices::constant_time_eq;
use crate::identity::{hex, random};
use crate::paths::{self, Paths};
use lcl_protocol::json::{Node, Object};
use lcl_spec::json::Json;
use ring::rand::SystemRandom;
use std::path::PathBuf;

/// The link scheme a QR code carries.
pub const SCHEME: &str = "lclpair";
/// The link format this build writes and reads.
pub const LINK_VERSION: u32 = 1;
/// How long a code is good for by default.
pub const DEFAULT_TTL_SECONDS: u64 = 300;

/// One pairing challenge, as the PC stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub id: String,
    /// SHA-256 of the code, hex. The code itself is never stored.
    pub hash: String,
    pub created: u64,
    pub expires: u64,
    pub consumed_at: Option<u64>,
    /// The fingerprint of the device that paired with it.
    pub consumed_by: Option<String>,
}

/// Why a code was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No challenge has this code.
    Unknown,
    Expired,
    /// Somebody already paired with it.
    Used,
    Unreadable(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Unknown => f.write_str("this pairing code is not one this PC issued"),
            Refusal::Expired => f.write_str("this pairing code has expired; show a new QR code"),
            Refusal::Used => f.write_str("this pairing code was already used; show a new QR code"),
            Refusal::Unreadable(detail) => write!(f, "pairing is unavailable: {detail}"),
        }
    }
}

/// The pending challenges, in the state directory.
#[derive(Debug, Clone)]
pub struct Pairing {
    file: PathBuf,
    lock: PathBuf,
}

impl Pairing {
    pub fn new(paths: &Paths) -> Pairing {
        Pairing {
            file: paths.state.join("pairing.json"),
            lock: paths.state.join("pairing.lock"),
        }
    }

    fn list(&self) -> Result<Vec<Challenge>, String> {
        let text = match std::fs::read_to_string(&self.file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.to_string()),
        };
        let json = lcl_spec::json::parse(&text).map_err(|e| e.to_string())?;
        let items = json
            .get("challenges")
            .and_then(Json::as_array)
            .ok_or("pairing.json has no challenge list")?;
        items
            .iter()
            .map(|c| {
                let text = |k: &str| c.get(k).and_then(Json::as_str).map(str::to_string);
                let number = |k: &str| c.get(k).and_then(Json::as_u64);
                (|| {
                    Some(Challenge {
                        id: text("id")?,
                        hash: text("hash")?,
                        created: number("created")?,
                        expires: number("expires")?,
                        consumed_at: number("consumed_at"),
                        consumed_by: text("consumed_by"),
                    })
                })()
                .ok_or_else(|| "pairing.json holds a malformed challenge".to_string())
            })
            .collect()
    }

    fn store(&self, challenges: &[Challenge]) -> Result<(), String> {
        let items = challenges.iter().map(|c| {
            Object::new()
                .with("id", Node::string(&c.id))
                .with("hash", Node::string(&c.hash))
                .with("created", Node::u64(c.created))
                .with("expires", Node::u64(c.expires))
                .with(
                    "consumed_at",
                    c.consumed_at.map(Node::u64).unwrap_or(Node::Null),
                )
                .with("consumed_by", Node::optional(c.consumed_by.clone()))
                .into()
        });
        let text = Object::new()
            .with("version", Node::u64(1))
            .with("challenges", Node::array(items))
            .pretty();
        paths::write_private(&self.file, text.as_bytes()).map_err(|e| e.to_string())
    }

    /// Issue a code good for `ttl` seconds from `now`. Returns the challenge
    /// and the code, which exists nowhere else once this returns.
    pub fn create(&self, now: u64, ttl: u64) -> Result<(Challenge, String), String> {
        let rng = SystemRandom::new();
        let code = b64::encode(&random::<32>(&rng)?);
        let challenge = Challenge {
            id: hex(&random::<8>(&rng)?),
            hash: lcl_spec::sha256::hex_digest(code.as_bytes()),
            created: now,
            expires: now.saturating_add(ttl),
            consumed_at: None,
            consumed_by: None,
        };
        let _held = paths::lock(&self.lock).map_err(|e| e.to_string())?;
        let mut challenges = self.list()?;
        // Keep a day of history, and nothing older.
        challenges.retain(|c| c.expires.saturating_add(86_400) > now);
        challenges.push(challenge.clone());
        self.store(&challenges)?;
        Ok((challenge, code))
    }

    /// Use a code, once. On success the challenge is marked used before this
    /// returns, so a second attempt with the same code — a replayed QR code —
    /// is refused whatever happens next.
    pub fn consume(&self, code: &str, device: &str, now: u64) -> Result<String, Refusal> {
        let hash = lcl_spec::sha256::hex_digest(code.as_bytes());
        let _held = paths::lock(&self.lock).map_err(|e| Refusal::Unreadable(e.to_string()))?;
        let mut challenges = self.list().map_err(Refusal::Unreadable)?;
        let challenge = challenges
            .iter_mut()
            .find(|c| constant_time_eq(c.hash.as_bytes(), hash.as_bytes()))
            .ok_or(Refusal::Unknown)?;
        if challenge.consumed_at.is_some() {
            return Err(Refusal::Used);
        }
        if now >= challenge.expires {
            return Err(Refusal::Expired);
        }
        challenge.consumed_at = Some(now);
        challenge.consumed_by = Some(device.to_string());
        let id = challenge.id.clone();
        self.store(&challenges).map_err(Refusal::Unreadable)?;
        Ok(id)
    }

    /// Challenges still waiting to be used.
    pub fn pending(&self, now: u64) -> Result<Vec<Challenge>, String> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|c| c.consumed_at.is_none() && c.expires > now)
            .collect())
    }
}

/// What a pairing QR code says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub version: u32,
    pub pc_id: String,
    pub pc_name: String,
    pub fingerprint: String,
    pub addresses: Vec<String>,
    pub code: String,
    pub expires: u64,
}

impl Link {
    pub fn to_uri(&self) -> String {
        let mut uri = format!(
            "{SCHEME}://pair?v={}&pc={}&n={}&fp={}",
            self.version,
            self.pc_id,
            percent_encode(&self.pc_name),
            self.fingerprint
        );
        for address in &self.addresses {
            uri.push_str("&a=");
            uri.push_str(&percent_encode(address));
        }
        uri.push_str(&format!("&c={}&e={}", self.code, self.expires));
        uri
    }

    /// Read a link, refusing anything malformed, from another version, or
    /// missing a field. Mirrors what the Android app accepts.
    pub fn parse(uri: &str) -> Result<Link, String> {
        let query = uri
            .strip_prefix(&format!("{SCHEME}://pair?"))
            .ok_or("not an LCL pairing link")?;
        let mut version = None;
        let mut pc_id = None;
        let mut pc_name = None;
        let mut fingerprint = None;
        let mut addresses = Vec::new();
        let mut code = None;
        let mut expires = None;
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').ok_or("a link field has no value")?;
            let value = percent_decode(value).ok_or("a link field is badly encoded")?;
            let slot = match key {
                "v" => &mut version,
                "pc" => &mut pc_id,
                "n" => &mut pc_name,
                "fp" => &mut fingerprint,
                "c" => &mut code,
                "e" => &mut expires,
                "a" => {
                    addresses.push(value);
                    continue;
                }
                _ => continue, // a later version's extra field
            };
            if slot.replace(value).is_some() {
                return Err(format!("the link names {key} twice"));
            }
        }
        let version: u32 = version
            .ok_or("no version")?
            .parse()
            .map_err(|_| "a bad version")?;
        if version != LINK_VERSION {
            return Err(format!("pairing link version {version} is not supported"));
        }
        let fingerprint = fingerprint.ok_or("no PC fingerprint")?;
        if fingerprint.len() != 64
            || !fingerprint
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err("the PC fingerprint is not 64 lowercase hex digits".into());
        }
        let code = code.ok_or("no pairing code")?;
        if b64::decode(&code).map(|b| b.len()) != Some(32) {
            return Err("the pairing code is malformed".into());
        }
        if addresses.is_empty() {
            return Err("the link names no address to reach the PC at".into());
        }
        Ok(Link {
            version,
            pc_id: pc_id.ok_or("no PC id")?,
            pc_name: pc_name.unwrap_or_default(),
            fingerprint,
            addresses,
            code,
            expires: expires
                .ok_or("no expiry")?
                .parse()
                .map_err(|_| "a bad expiry")?,
        })
    }
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~:[]".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairing(name: &str) -> (Pairing, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("lcl-remote-pairing-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        (Pairing::new(&Paths::under(&root)), root)
    }

    #[test]
    fn a_code_pairs_once_and_only_before_it_expires() {
        let (pairing, root) = pairing("once");
        let (_, code) = pairing.create(1_000, 300).unwrap();
        assert_eq!(pairing.consume(&code, "dev", 1_299).map(|_| ()), Ok(()));
        assert_eq!(
            pairing.consume(&code, "dev", 1_299),
            Err(Refusal::Used),
            "a replayed code"
        );
        let (_, late) = pairing.create(2_000, 300).unwrap();
        assert_eq!(pairing.consume(&late, "dev", 2_300), Err(Refusal::Expired));
        assert_eq!(
            pairing.consume("not-a-code", "dev", 2_000),
            Err(Refusal::Unknown)
        );
        // Only a hash is stored.
        let stored = std::fs::read_to_string(root.join("state/lcl/remote/pairing.json")).unwrap();
        assert!(!stored.contains(&code) && !stored.contains(&late));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_link_round_trips_and_malformed_links_are_refused() {
        let link = Link {
            version: 1,
            pc_id: "0123456789abcdef0123456789abcdef".into(),
            pc_name: "Aivars' PC & more".into(),
            fingerprint: "a".repeat(64),
            addresses: vec!["192.168.1.20:47300".into(), "[fe80::1]:47300".into()],
            code: b64::encode(&[7u8; 32]),
            expires: 1_790_000_000,
        };
        assert_eq!(Link::parse(&link.to_uri()).unwrap(), link);
        let good = link.to_uri();
        for bad in [
            "https://example.com/pair?v=1".to_string(),
            good.replace("v=1", "v=2"),
            good.replace(&"a".repeat(64), &"A".repeat(64)),
            good.replace(&"a".repeat(64), &"a".repeat(63)),
            good.replace(&link.code, "short"),
            good.replace("&a=192.168.1.20:47300&a=%5Bfe80::1%5D:47300", "")
                .replace("&a=192.168.1.20:47300&a=[fe80::1]:47300", ""),
            format!("{good}&c=again"),
            good.replace("e=1790000000", "e=soon"),
        ] {
            assert!(Link::parse(&bad).is_err(), "{bad} was accepted");
        }
    }
}
