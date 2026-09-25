//! The PC's long-lived identity, and certificates in general.
//!
//! The identity is an ECDSA P-256 key made once, on first start, and a
//! self-signed certificate carrying its public half. Its **fingerprint** — the
//! SHA-256 of the certificate's DER bytes — is what a paired device pins and
//! what the pairing QR code carries. Nothing about it depends on an address,
//! a network or a host name, so the PC stays the same PC when its address
//! changes, and a phone recognises it wherever it is reached.
//!
//! The private key never leaves `identity.key`, which is `0600` in a `0700`
//! directory, and is never written anywhere else or printed.

use crate::der;
use crate::paths::{self, Paths};
use lcl_protocol::json::{Node, Object};
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};

/// The PC this service runs as.
#[derive(Clone)]
pub struct Identity {
    /// A random, stable identifier, 32 hex digits.
    pub pc_id: String,
    /// A name for people, the host name by default.
    pub name: String,
    /// The certificate, DER.
    pub certificate: Vec<u8>,
    /// SHA-256 of `certificate`, 64 hex digits.
    pub fingerprint: String,
    key_pkcs8: Vec<u8>,
}

impl std::fmt::Debug for Identity {
    // The key is never printed, not even in a debug dump.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("pc_id", &self.pc_id)
            .field("name", &self.name)
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

impl Identity {
    /// Load the identity, or make it on first start.
    pub fn load_or_create(paths: &Paths) -> Result<Identity, String> {
        match Identity::load(paths)? {
            Some(identity) => Ok(identity),
            None => Identity::create(paths),
        }
    }

    /// Load the identity, if one was made.
    pub fn load(paths: &Paths) -> Result<Option<Identity>, String> {
        let key_path = paths.config.join("identity.key");
        if !key_path.exists() {
            return Ok(None);
        }
        let read = |name: &str| {
            std::fs::read(paths.config.join(name))
                .map_err(|e| format!("{}: {e}", paths.config.join(name).display()))
        };
        let key_pkcs8 = read("identity.key")?;
        let certificate = read("identity.crt")?;
        let meta = String::from_utf8(read("identity.json")?)
            .map_err(|_| "identity.json is not UTF-8".to_string())?;
        let meta = lcl_spec::json::parse(&meta).map_err(|e| format!("identity.json: {e}"))?;
        let field = |name: &str| {
            meta.get(name)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| format!("identity.json has no {name}"))
        };
        let fingerprint = fingerprint(&certificate);
        if field("fingerprint")? != fingerprint {
            return Err("identity.json does not describe identity.crt; refusing to guess".into());
        }
        // The key must be the one the certificate carries.
        let rng = SystemRandom::new();
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &key_pkcs8, &rng)
            .map_err(|_| "identity.key is not an ECDSA P-256 key".to_string())?;
        let spki = der::certificate_spki(&certificate).ok_or("identity.crt is malformed")?;
        if der::ec_p256_point(spki) != Some(pair.public_key().as_ref()) {
            return Err("identity.key does not match identity.crt".into());
        }
        Ok(Some(Identity {
            pc_id: field("pc_id")?,
            name: field("name")?,
            certificate,
            fingerprint,
            key_pkcs8,
        }))
    }

    fn create(paths: &Paths) -> Result<Identity, String> {
        let rng = SystemRandom::new();
        let pc_id = hex(&random::<16>(&rng)?);
        let name = host_name().unwrap_or_else(|| "LCL PC".to_string());
        let (key_pkcs8, certificate) = new_certificate(&format!("LCL PC {}", &pc_id[..8]), &rng)?;
        let identity = Identity {
            fingerprint: fingerprint(&certificate),
            pc_id,
            name,
            certificate,
            key_pkcs8,
        };
        paths::private_dir(&paths.config).map_err(|e| e.to_string())?;
        let write = |name: &str, bytes: &[u8]| {
            paths::write_private(&paths.config.join(name), bytes)
                .map_err(|e| format!("{}: {e}", paths.config.join(name).display()))
        };
        write("identity.key", &identity.key_pkcs8)?;
        write("identity.crt", &identity.certificate)?;
        identity.store_meta(paths)?;
        Ok(identity)
    }

    fn store_meta(&self, paths: &Paths) -> Result<(), String> {
        let meta = Object::new()
            .with("version", Node::u64(1))
            .with("pc_id", Node::string(&self.pc_id))
            .with("name", Node::string(&self.name))
            .with("fingerprint", Node::string(&self.fingerprint))
            .pretty();
        paths::write_private(&paths.config.join("identity.json"), meta.as_bytes())
            .map_err(|e| e.to_string())
    }

    /// Give the PC another name. The fingerprint, and so every pairing, stays.
    pub fn rename(&mut self, paths: &Paths, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.len() > 64 || name.chars().any(char::is_control) {
            return Err("a PC name is 1 to 64 printable characters".into());
        }
        self.name = name.to_string();
        self.store_meta(paths)
    }

    /// The private key, for the TLS server.
    pub fn private_key(&self) -> rustls::pki_types::PrivateKeyDer<'static> {
        rustls::pki_types::PrivateKeyDer::Pkcs8(self.key_pkcs8.clone().into())
    }
}

/// SHA-256 of a certificate's DER bytes, 64 lowercase hex digits.
pub fn fingerprint(certificate: &[u8]) -> String {
    lcl_spec::sha256::hex_digest(certificate)
}

/// A fresh ECDSA P-256 key and a self-signed certificate for it.
///
/// Returns the key as PKCS#8 and the certificate as DER. Valid from 2020 and
/// with RFC 5280's "no well-defined expiration" end date, because nothing
/// here trusts it by date: it is pinned by its exact bytes.
pub fn new_certificate(
    common_name: &str,
    rng: &SystemRandom,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, rng)
        .map_err(|_| "no ECDSA key could be generated".to_string())?;
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), rng)
        .map_err(|_| "the generated key is unusable".to_string())?;
    let mut serial = random::<16>(rng)?;
    serial[0] &= 0x7f;
    serial[0] |= 0x01; // positive and non-zero, in 16 bytes
    let algorithm = der::sequence(&[&der::oid(der::OID_ECDSA_SHA256)]);
    let name = der::sequence(&[&der::set(&[&der::sequence(&[
        &der::oid(der::OID_COMMON_NAME),
        &der::utf8(common_name),
    ])])]);
    let validity = der::sequence(&[
        &der::utc_time("200101000000Z"),
        &der::generalized_time("99991231235959Z"),
    ]);
    let spki = der::sequence(&[
        &der::sequence(&[
            &der::oid(der::OID_EC_PUBLIC_KEY),
            &der::oid(der::OID_PRIME256V1),
        ]),
        &der::bit_string(pair.public_key().as_ref()),
    ]);
    // basicConstraints: not a CA. Critical, as RFC 5280 asks of it.
    let extensions = der::explicit(
        3,
        &der::sequence(&[&der::sequence(&[
            &der::oid(der::OID_BASIC_CONSTRAINTS),
            &der::boolean(true),
            &der::octet_string(&der::sequence(&[])),
        ])]),
    );
    let tbs = der::sequence(&[
        &der::explicit(0, &der::unsigned_integer(&[2])),
        &der::unsigned_integer(&serial),
        &algorithm,
        &name,
        &validity,
        &name,
        &spki,
        &extensions,
    ]);
    let signature = pair
        .sign(rng, &tbs)
        .map_err(|_| "the certificate could not be signed".to_string())?;
    let certificate = der::sequence(&[&tbs, &algorithm, &der::bit_string(signature.as_ref())]);
    Ok((pkcs8.as_ref().to_vec(), certificate))
}

/// `N` random bytes from the operating system.
pub fn random<const N: usize>(rng: &SystemRandom) -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    rng.fill(&mut bytes)
        .map_err(|_| "the system random source failed".to_string())?;
    Ok(bytes)
}

/// Lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn host_name() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .ok()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_certificate_carries_the_key_that_made_it() {
        let rng = SystemRandom::new();
        let (pkcs8, certificate) = new_certificate("test", &rng).unwrap();
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &pkcs8, &rng).unwrap();
        let spki = der::certificate_spki(&certificate).unwrap();
        assert_eq!(
            der::ec_p256_point(spki).unwrap(),
            pair.public_key().as_ref()
        );
        // And its signature verifies under that key.
        let (outer, _) = der::read(&certificate).unwrap();
        let parts = der::children(outer.value).unwrap();
        let signature = &parts[2].value[1..];
        ring::signature::UnparsedPublicKey::new(
            &ring::signature::ECDSA_P256_SHA256_ASN1,
            pair.public_key().as_ref(),
        )
        .verify(parts[0].encoded, signature)
        .expect("self-signature verifies");
    }

    #[test]
    fn the_identity_survives_a_restart_and_keeps_its_fingerprint() {
        let root = std::env::temp_dir().join(format!("lcl-remote-identity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths::under(&root);
        let made = Identity::load_or_create(&paths).unwrap();
        let again = Identity::load_or_create(&paths).unwrap();
        assert_eq!(made.fingerprint, again.fingerprint);
        assert_eq!(made.pc_id, again.pc_id);
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(paths.config.join("identity.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "the private key is readable by others");
        let dir = std::fs::metadata(&paths.config)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(dir & 0o777, 0o700);
        // A certificate that no longer matches its description is refused.
        std::fs::write(paths.config.join("identity.crt"), b"not a certificate").unwrap();
        assert!(Identity::load(&paths).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
