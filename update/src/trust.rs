//! Which update signing keys this build trusts, and checking a manifest's
//! signature against them.
//!
//! The list is `trusted_keys.txt`, compiled in. A signature is checked against
//! each trusted key before anything in the manifest is read, so no field of an
//! unverified manifest — the key id it names included — is ever believed, and
//! a key a manifest brings along is never trusted. Only a test build may add
//! keys, from `LCL_UPDATE_TEST_KEYS`.

use ring::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_ASN1};

/// The compiled-in list of trusted keys.
pub const KEYS_FILE: &str = include_str!("../trusted_keys.txt");

/// What a build that trusts no key says instead of checking.
pub const NOT_CONFIGURED: &str = "this build of LCL trusts no update signing key yet, so it \
     cannot verify, and never installs, any update";

/// The DER prefix every P-256 SubjectPublicKeyInfo shares:
/// `SEQUENCE { SEQUENCE { id-ecPublicKey, prime256v1 }, BIT STRING }`,
/// followed by the 65-byte uncompressed point.
const P256_SPKI_PREFIX: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

/// One trusted ECDSA P-256 public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedKey {
    pub id: String,
    /// The uncompressed point, `04 || X || Y`.
    point: Vec<u8>,
}

impl TrustedKey {
    /// A key from its SubjectPublicKeyInfo DER, which must be exactly a P-256
    /// key's.
    pub fn from_spki(id: &str, spki: &[u8]) -> Result<TrustedKey, String> {
        let valid_id = !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !valid_id {
            return Err(format!("{id:?} is not a key id: 1 to 64 of a-z, 0-9 and -"));
        }
        let point = spki
            .strip_prefix(&P256_SPKI_PREFIX[..])
            .filter(|point| point.len() == 65 && point[0] == 0x04)
            .ok_or_else(|| format!("key {id} is not an ECDSA P-256 public key"))?;
        Ok(TrustedKey {
            id: id.to_string(),
            point: point.to_vec(),
        })
    }
}

/// The keys a key-list text names: `<id> <hex SPKI DER>` per line, `#`
/// comments and blank lines ignored. Anything else, or one id twice, is
/// refused.
pub fn parse_keys(text: &str) -> Result<Vec<TrustedKey>, String> {
    let mut keys: Vec<TrustedKey> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [id, spki] = fields[..] else {
            return Err(format!(
                "trusted key line {}: expected <id> <hex>",
                number + 1
            ));
        };
        let key = TrustedKey::from_spki(id, &hex_decode(spki)?)?;
        if keys.iter().any(|k| k.id == key.id) {
            return Err(format!("trusted key {id} is listed twice"));
        }
        keys.push(key);
    }
    Ok(keys)
}

/// The keys this build trusts.
pub fn trusted_keys() -> Result<Vec<TrustedKey>, String> {
    #[allow(unused_mut)]
    let mut keys = parse_keys(KEYS_FILE)?;
    #[cfg(feature = "test-endpoint")]
    if let Some(path) = std::env::var_os("LCL_UPDATE_TEST_KEYS") {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("test keys {}: {e}", std::path::Path::new(&path).display()))?;
        for key in parse_keys(&text)? {
            if keys.iter().any(|k| k.id == key.id) {
                return Err(format!("trusted key {} is listed twice", key.id));
            }
            keys.push(key);
        }
    }
    Ok(keys)
}

/// The id of the trusted key that signed exactly `message`: `signature` is a
/// DER-encoded ECDSA P-256 SHA-256 signature. Checked by `ring`.
pub fn verify(message: &[u8], signature: &[u8], keys: &[TrustedKey]) -> Result<String, String> {
    if keys.is_empty() {
        return Err(NOT_CONFIGURED.to_string());
    }
    keys.iter()
        .find(|key| {
            UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, &key.point)
                .verify(message, signature)
                .is_ok()
        })
        .map(|key| key.id.clone())
        .ok_or_else(|| {
            "the update manifest's signature is not valid for any key this build trusts".to_string()
        })
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_decode(text: &str) -> Result<Vec<u8>, String> {
    let digit = |c: u8| match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        _ => Err("expected lowercase hexadecimal".to_string()),
    };
    if !text.len().is_multiple_of(2) {
        return Err("expected an even number of hexadecimal digits".to_string());
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| Ok(digit(pair[0])? << 4 | digit(pair[1])?))
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};

    /// A throwaway signing key, made in memory for one test.
    pub struct TestSigner {
        pair: EcdsaKeyPair,
        pub id: String,
    }

    impl TestSigner {
        pub fn new(id: &str) -> TestSigner {
            let rng = SystemRandom::new();
            let pkcs8 =
                EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
            let pair =
                EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
                    .unwrap();
            TestSigner {
                pair,
                id: id.to_string(),
            }
        }

        pub fn sign(&self, message: &[u8]) -> Vec<u8> {
            self.pair
                .sign(&SystemRandom::new(), message)
                .unwrap()
                .as_ref()
                .to_vec()
        }

        pub fn spki(&self) -> Vec<u8> {
            let mut spki = P256_SPKI_PREFIX.to_vec();
            spki.extend_from_slice(self.pair.public_key().as_ref());
            spki
        }

        /// This key as a line of a key list.
        pub fn line(&self) -> String {
            format!("{} {}\n", self.id, hex(&self.spki()))
        }
    }

    #[test]
    fn a_manifest_signed_by_a_trusted_key_verifies_and_nothing_else_does() {
        let current = TestSigner::new("current");
        let next = TestSigner::new("next");
        let stranger = TestSigner::new("stranger");
        let keys = parse_keys(&format!("# list\n\n{}{}", current.line(), next.line())).unwrap();
        let manifest = b"{\"format\": 1}";
        assert_eq!(
            verify(manifest, &current.sign(manifest), &keys).unwrap(),
            "current"
        );
        // Every listed key verifies (a test build lists its test keys beside
        // the build's own); a key that is not listed never does.
        assert_eq!(
            verify(manifest, &next.sign(manifest), &keys).unwrap(),
            "next"
        );
        for (message, signature) in [
            (&b"{\"format\": 2}"[..], current.sign(manifest)),
            (&manifest[..], stranger.sign(manifest)),
            (&manifest[..], Vec::new()),
            (&manifest[..], b"garbage".to_vec()),
        ] {
            assert!(verify(message, &signature, &keys).is_err());
        }
        assert_eq!(
            verify(manifest, &current.sign(manifest), &[]).unwrap_err(),
            NOT_CONFIGURED
        );
    }

    #[test]
    fn a_key_list_holds_only_well_formed_p256_keys_once_each() {
        let key = TestSigner::new("k");
        let spki = hex(&key.spki());
        for bad in [
            format!("k {spki} extra"),
            format!("K {spki}"),
            format!("k {}", &spki[2..]),
            format!("k {}", spki.to_uppercase()),
            format!("k {spki}\nk {spki}"),
            "k 3059".to_string(),
        ] {
            assert!(parse_keys(&bad).is_err(), "{bad}");
        }
        parse_keys(KEYS_FILE).expect("the compiled-in key list is well formed");
    }
}
