//! The devices this PC trusts.
//!
//! A device is trusted by the fingerprint of the certificate it presented when
//! it paired — the SHA-256 of its exact bytes, whose key lives in that
//! device's own keystore and never leaves it. Nothing about a device's
//! address, network or name decides whether it is trusted.
//!
//! A device is added here only when the person at the PC approved its pairing
//! request (see [`crate::pairing`]); holding a pairing code never adds one.
//!
//! Revoking a device keeps its record, marked revoked, so the list still says
//! what happened; a revoked fingerprint never authenticates again. The same
//! device can be trusted again only by pairing afresh from a new QR code,
//! which makes a new record.
//!
//! The file is `devices.json` in the configuration directory, `0600`. A file
//! that cannot be read is an error, never an empty list: a registry nobody can
//! read trusts nobody, rather than being quietly replaced.

use crate::identity::{hex, random};
use crate::paths::{self, Paths};
use lcl_protocol::json::{Node, Object};
use lcl_spec::json::Json;
use ring::rand::SystemRandom;
use std::path::PathBuf;

/// One paired device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// A random identifier, 16 hex digits.
    pub id: String,
    pub name: String,
    /// SHA-256 of the device's certificate, 64 hex digits.
    pub fingerprint: String,
    pub paired_at: u64,
    pub last_seen: Option<u64>,
    /// The remote protocol it paired with.
    pub protocol: String,
    pub revoked_at: Option<u64>,
}

impl Device {
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }

    fn to_node(&self) -> Node {
        let optional = |v: Option<u64>| v.map(Node::u64).unwrap_or(Node::Null);
        Object::new()
            .with("id", Node::string(&self.id))
            .with("name", Node::string(&self.name))
            .with("fingerprint", Node::string(&self.fingerprint))
            .with("paired_at", Node::u64(self.paired_at))
            .with("last_seen", optional(self.last_seen))
            .with("protocol", Node::string(&self.protocol))
            .with("revoked_at", optional(self.revoked_at))
            .into()
    }

    fn from_json(value: &Json) -> Option<Device> {
        let text = |k: &str| value.get(k).and_then(Json::as_str).map(str::to_string);
        let number = |k: &str| value.get(k).and_then(Json::as_u64);
        Some(Device {
            id: text("id")?,
            name: text("name")?,
            fingerprint: text("fingerprint")?,
            paired_at: number("paired_at")?,
            last_seen: number("last_seen"),
            protocol: text("protocol")?,
            revoked_at: number("revoked_at"),
        })
    }
}

/// Why a device was not let in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Nothing with this fingerprint was ever paired.
    Unknown,
    /// It was paired, and the PC revoked it.
    Revoked,
    /// The registry could not be read, so nobody is trusted.
    Unreadable(String),
}

/// The registry file.
#[derive(Debug, Clone)]
pub struct Registry {
    file: PathBuf,
    lock: PathBuf,
}

impl Registry {
    pub fn new(paths: &Paths) -> Registry {
        Registry {
            file: paths.config.join("devices.json"),
            lock: paths.config.join("devices.lock"),
        }
    }

    /// Every device ever paired, revoked ones included.
    pub fn list(&self) -> Result<Vec<Device>, String> {
        let text = match std::fs::read_to_string(&self.file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("{}: {e}", self.file.display())),
        };
        let json = lcl_spec::json::parse(&text)
            .map_err(|e| format!("{} is not valid JSON: {e}", self.file.display()))?;
        if json.get("version").and_then(Json::as_u64) != Some(1) {
            return Err(format!("{} is not version 1", self.file.display()));
        }
        json.get("devices")
            .and_then(Json::as_array)
            .ok_or_else(|| format!("{} has no device list", self.file.display()))?
            .iter()
            .map(|d| {
                Device::from_json(d)
                    .ok_or_else(|| format!("{} holds a malformed device", self.file.display()))
            })
            .collect()
    }

    fn store(&self, devices: &[Device]) -> Result<(), String> {
        let text = Object::new()
            .with("version", Node::u64(1))
            .with("devices", Node::array(devices.iter().map(Device::to_node)))
            .pretty();
        paths::write_private(&self.file, text.as_bytes()).map_err(|e| e.to_string())
    }

    /// Change the registry under its lock.
    fn update<T>(
        &self,
        change: impl FnOnce(&mut Vec<Device>) -> Result<T, String>,
    ) -> Result<T, String> {
        let _held = paths::lock(&self.lock).map_err(|e| e.to_string())?;
        let mut devices = self.list()?;
        let out = change(&mut devices)?;
        self.store(&devices)?;
        Ok(out)
    }

    /// Trust one more device, under a new id.
    pub fn add(
        &self,
        name: &str,
        fingerprint: &str,
        protocol: &str,
        now: u64,
    ) -> Result<Device, String> {
        let id = hex(&random::<8>(&SystemRandom::new())?);
        self.enroll(&id, name, fingerprint, protocol, now)
    }

    /// Trust one more device under the id `id`, which pairing reserved for it
    /// before calling this. Called again for the same id and certificate — a
    /// pairing finished again after a crash — it finds the record it made and
    /// makes no second one.
    pub fn enroll(
        &self,
        id: &str,
        name: &str,
        fingerprint: &str,
        protocol: &str,
        now: u64,
    ) -> Result<Device, String> {
        let name = clean_name(name);
        self.update(|devices| {
            if let Some(done) = devices
                .iter()
                .find(|d| d.id == id && d.fingerprint == fingerprint && !d.is_revoked())
            {
                return Ok(done.clone());
            }
            if devices.iter().any(|d| d.id == id) {
                return Err(format!("the device id {id} is already taken"));
            }
            // One live record per key: pairing the same key again (from a new
            // QR code) replaces an older live record rather than doubling it.
            for device in devices.iter_mut() {
                if device.fingerprint == fingerprint && !device.is_revoked() {
                    device.revoked_at = Some(now);
                }
            }
            let device = Device {
                id: id.to_string(),
                name,
                fingerprint: fingerprint.to_string(),
                paired_at: now,
                last_seen: Some(now),
                protocol: protocol.to_string(),
                revoked_at: None,
            };
            devices.push(device.clone());
            Ok(device)
        })
    }

    /// The live device a certificate fingerprint belongs to.
    pub fn authorize(&self, fingerprint: &str) -> Result<Device, Refusal> {
        let devices = self.list().map_err(Refusal::Unreadable)?;
        let mut revoked = false;
        for device in devices.iter().rev() {
            if !constant_time_eq(device.fingerprint.as_bytes(), fingerprint.as_bytes()) {
                continue;
            }
            if device.is_revoked() {
                revoked = true;
            } else {
                return Ok(device.clone());
            }
        }
        Err(if revoked {
            Refusal::Revoked
        } else {
            Refusal::Unknown
        })
    }

    /// Record that a device was seen now.
    pub fn touch(&self, id: &str, now: u64) -> Result<(), String> {
        self.update(|devices| {
            if let Some(device) = devices.iter_mut().find(|d| d.id == id) {
                device.last_seen = Some(now);
            }
            Ok(())
        })
    }

    /// Revoke one device. Its record stays, marked revoked.
    pub fn revoke(&self, id: &str, now: u64) -> Result<Device, String> {
        self.update(|devices| {
            let device = devices
                .iter_mut()
                .find(|d| d.id == id)
                .ok_or_else(|| format!("no paired device has the id {id}"))?;
            if device.revoked_at.is_none() {
                device.revoked_at = Some(now);
            }
            Ok(device.clone())
        })
    }

    /// Rename one device.
    pub fn rename(&self, id: &str, name: &str) -> Result<Device, String> {
        let name = clean_name(name);
        self.update(|devices| {
            let device = devices
                .iter_mut()
                .find(|d| d.id == id)
                .ok_or_else(|| format!("no paired device has the id {id}"))?;
            device.name = name;
            Ok(device.clone())
        })
    }
}

/// A device name as a person would read it: printable, trimmed, bounded.
pub fn clean_name(name: &str) -> String {
    let cleaned: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        "Android device".to_string()
    } else {
        cleaned
    }
}

/// Equality that takes the same time whatever the contents.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry(name: &str) -> (Registry, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("lcl-remote-devices-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        (Registry::new(&Paths::under(&root)), root)
    }

    #[test]
    fn a_paired_device_is_trusted_until_it_is_revoked() {
        let (registry, root) = registry("revoke");
        let a = registry
            .add("Phone A", &"a".repeat(64), "lcl.remote/1", 10)
            .unwrap();
        let b = registry
            .add("Phone B", &"b".repeat(64), "lcl.remote/1", 11)
            .unwrap();
        assert_eq!(registry.authorize(&"a".repeat(64)).unwrap().id, a.id);
        assert_eq!(registry.authorize(&"c".repeat(64)), Err(Refusal::Unknown));
        registry.revoke(&a.id, 12).unwrap();
        assert_eq!(registry.authorize(&"a".repeat(64)), Err(Refusal::Revoked));
        // Revoking A leaves B alone.
        assert_eq!(registry.authorize(&"b".repeat(64)).unwrap().id, b.id);
        // Pairing A's key again from a new code makes a new, live record.
        let again = registry
            .add("Phone A", &"a".repeat(64), "lcl.remote/1", 13)
            .unwrap();
        assert_ne!(again.id, a.id);
        assert_eq!(registry.authorize(&"a".repeat(64)).unwrap().id, again.id);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn enrolling_again_after_a_crash_makes_no_second_record() {
        let (registry, root) = registry("enroll");
        let first = registry
            .enroll("00aa", "Phone", &"a".repeat(64), "lcl.remote/1", 10)
            .unwrap();
        let again = registry
            .enroll("00aa", "Phone", &"a".repeat(64), "lcl.remote/1", 11)
            .unwrap();
        assert_eq!(first, again);
        assert_eq!(registry.list().unwrap().len(), 1);
        // The id is the reserved one; it cannot be taken by another key.
        assert!(registry
            .enroll("00aa", "Other", &"b".repeat(64), "lcl.remote/1", 12)
            .is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_unreadable_registry_trusts_nobody() {
        let (registry, root) = registry("corrupt");
        registry
            .add("Phone", &"a".repeat(64), "lcl.remote/1", 1)
            .unwrap();
        std::fs::write(root.join("config/lcl/remote/devices.json"), b"{broken").unwrap();
        assert!(matches!(
            registry.authorize(&"a".repeat(64)),
            Err(Refusal::Unreadable(_))
        ));
        assert!(registry
            .add("Other", &"b".repeat(64), "lcl.remote/1", 2)
            .is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
