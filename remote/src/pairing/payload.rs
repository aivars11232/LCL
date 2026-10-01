//! Pairing text: what a pairing QR code says, written and read. The format
//! is described in [`crate::pairing`].

use crate::b64;

/// What pairing text starts with.
pub const PREFIX: &str = "LCLPAIR|";
/// The pairing-text format this build writes and reads.
pub const PAYLOAD_VERSION: u32 = 2;

/// The earlier pairing link, which trusted whoever used the code first.
const LEGACY_PREFIX: &str = "lclpair://";
/// Said when a person has pairing text or a device from before approval.
pub const OLDER_FLOW: &str =
    "This pairing code uses the older pairing flow. Update LCL on the PC and show a new QR code.";

/// What a pairing QR code says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    pub version: u32,
    pub pc_id: String,
    pub pc_name: String,
    pub fingerprint: String,
    pub addresses: Vec<String>,
    pub code: String,
    pub expires: u64,
}

impl Payload {
    pub fn to_text(&self) -> String {
        let mut text = format!(
            "{PREFIX}v={}&pc={}&n={}&fp={}",
            self.version,
            self.pc_id,
            percent_encode(&self.pc_name),
            self.fingerprint
        );
        for address in &self.addresses {
            text.push_str("&a=");
            text.push_str(&percent_encode(address));
        }
        text.push_str(&format!("&c={}&e={}", self.code, self.expires));
        text
    }

    /// Read pairing text, refusing anything malformed, from another version,
    /// or missing a field. Mirrors what the Android app accepts.
    pub fn parse(text: &str) -> Result<Payload, String> {
        let text = text.trim();
        if text.starts_with(LEGACY_PREFIX) {
            return Err(OLDER_FLOW.into());
        }
        let query = text
            .strip_prefix(PREFIX)
            .ok_or("this is not LCL pairing text")?;
        let mut version = None;
        let mut pc_id = None;
        let mut pc_name = None;
        let mut fingerprint = None;
        let mut addresses = Vec::new();
        let mut code = None;
        let mut expires = None;
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').ok_or("a pairing field has no value")?;
            let value = percent_decode(value).ok_or("a pairing field is badly encoded")?;
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
                return Err(format!("the pairing text names {key} twice"));
            }
        }
        let version: u32 = version
            .ok_or("no version")?
            .parse()
            .map_err(|_| "a bad version")?;
        if version < PAYLOAD_VERSION {
            return Err(OLDER_FLOW.into());
        }
        if version != PAYLOAD_VERSION {
            return Err(format!("pairing text version {version} is not supported"));
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
            return Err("the pairing text names no address to reach the PC at".into());
        }
        Ok(Payload {
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

    #[test]
    fn pairing_text_round_trips_and_malformed_or_older_text_is_refused() {
        let payload = Payload {
            version: 2,
            pc_id: "0123456789abcdef0123456789abcdef".into(),
            pc_name: "Aivars' PC & more | yes".into(),
            fingerprint: "a".repeat(64),
            addresses: vec!["192.168.1.20:47300".into(), "[fe80::1]:47300".into()],
            code: b64::encode(&[7u8; 32]),
            expires: 1_790_000_000,
        };
        let good = payload.to_text();
        assert!(good.starts_with("LCLPAIR|v=2&"), "{good}");
        assert!(!good.contains("://"), "{good}");
        assert_eq!(good.matches('|').count(), 1, "{good}");
        assert!(good.is_ascii());
        assert_eq!(Payload::parse(&good).unwrap(), payload);
        for bad in [
            "https://example.com/pair?v=2".to_string(),
            good.replace("LCLPAIR|", "LCLPAIR:"),
            good.replace("LCLPAIR|", "lclpair|"),
            good.replace("v=2", "v=3"),
            good.replace(&"a".repeat(64), &"A".repeat(64)),
            good.replace(&"a".repeat(64), &"a".repeat(63)),
            good.replace(&payload.code, "short"),
            good.replace("&a=192.168.1.20:47300&a=[fe80::1]:47300", ""),
            format!("{good}&c=again"),
            good.replace("e=1790000000", "e=soon"),
        ] {
            assert!(Payload::parse(&bad).is_err(), "{bad} was accepted");
        }
        // The earlier flow, as a link or as version 1 text, is refused with
        // a reason a person can act on.
        let legacy = good
            .replace("LCLPAIR|", "lclpair://pair?")
            .replace("v=2", "v=1");
        assert_eq!(Payload::parse(&legacy).unwrap_err(), OLDER_FLOW);
        assert_eq!(
            Payload::parse(&good.replace("v=2", "v=1")).unwrap_err(),
            OLDER_FLOW
        );
    }
}
