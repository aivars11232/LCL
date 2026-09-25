//! TLS 1.3, with both ends authenticated by certificates pinned at pairing.
//!
//! Neither side trusts a certificate authority, and neither needs one. The PC
//! presents its identity certificate, and a device accepts it only if its
//! SHA-256 is the fingerprint the pairing QR code carried. The device presents
//! the certificate of the key in its own keystore, and the PC decides who that
//! is — a paired device, a revoked one, or somebody about to pair — by the
//! same kind of fingerprint, after the handshake has proved the device holds
//! the key.
//!
//! The handshake signatures are checked here with `ring`, against the public
//! key read from the exact certificate bytes that are fingerprinted, so a peer
//! cannot show one certificate and sign with another key. Only ECDSA P-256
//! with SHA-256 is accepted, which is what both ends use; TLS 1.2 is not
//! offered at all.

use crate::der;
use crate::identity::Identity;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{CertificateError, DigitallySignedStruct, DistinguishedName, Error, SignatureScheme};
use std::sync::Arc;

/// The application protocol both ends name in ALPN.
pub const ALPN: &[u8] = b"lcl.remote/1";

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The server side: this PC's certificate, and a device certificate required.
pub fn server_config(identity: &Identity) -> Result<Arc<rustls::ServerConfig>, String> {
    let mut config = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .with_client_cert_verifier(Arc::new(DeviceCertificate))
        .with_single_cert(
            vec![CertificateDer::from(identity.certificate.clone())],
            identity.private_key(),
        )
        .map_err(|e| format!("the PC identity is unusable for TLS: {e}"))?;
    config.alpn_protocols = vec![ALPN.to_vec()];
    // No session tickets and no early data: every connection proves the
    // device's key afresh, and nothing can be replayed into a new one.
    config.max_early_data_size = 0;
    config.send_tls13_tickets = 0;
    Ok(Arc::new(config))
}

/// The client side, for tests and tools: a pinned PC and this client's key.
pub fn client_config(
    pc_fingerprint: &str,
    certificate: Vec<u8>,
    key_pkcs8: Vec<u8>,
) -> Result<Arc<rustls::ClientConfig>, String> {
    let mut config = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedPc(pc_fingerprint.to_string())))
        .with_client_auth_cert(
            vec![CertificateDer::from(certificate)],
            PrivateKeyDer::Pkcs8(key_pkcs8.into()),
        )
        .map_err(|e| e.to_string())?;
    config.alpn_protocols = vec![ALPN.to_vec()];
    config.enable_early_data = false;
    Ok(Arc::new(config))
}

/// The name a client presents. Nothing checks it: identity is the pin.
pub fn server_name() -> ServerName<'static> {
    ServerName::try_from("lcl-remote.invalid").expect("a valid DNS name")
}

/// Verify one TLS 1.3 handshake signature against a certificate's own key.
fn verify_signature(
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
) -> Result<HandshakeSignatureValid, Error> {
    if dss.scheme != SignatureScheme::ECDSA_NISTP256_SHA256 {
        return Err(Error::InvalidCertificate(CertificateError::BadSignature));
    }
    let point = der::certificate_spki(cert.as_ref())
        .and_then(der::ec_p256_point)
        .ok_or(Error::InvalidCertificate(CertificateError::BadEncoding))?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ECDSA_P256_SHA256_ASN1, point)
        .verify(message, dss.signature())
        .map(|()| HandshakeSignatureValid::assertion())
        .map_err(|_| Error::InvalidCertificate(CertificateError::BadSignature))
}

fn schemes() -> Vec<SignatureScheme> {
    vec![SignatureScheme::ECDSA_NISTP256_SHA256]
}

/// Every device must present a certificate whose key can sign the handshake.
/// Which device it is — if any — is decided after, by fingerprint.
#[derive(Debug)]
struct DeviceCertificate;

impl ClientCertVerifier for DeviceCertificate {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        der::certificate_spki(end_entity.as_ref())
            .and_then(der::ec_p256_point)
            .map(|_| ClientCertVerified::assertion())
            .ok_or(Error::InvalidCertificate(CertificateError::BadEncoding))
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::General("TLS 1.2 is not offered".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        schemes()
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }
}

/// A PC certificate is accepted only if it is exactly the pinned one.
#[derive(Debug)]
struct PinnedPc(String);

impl ServerCertVerifier for PinnedPc {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let presented = crate::identity::fingerprint(end_entity.as_ref());
        if crate::devices::constant_time_eq(presented.as_bytes(), self.0.as_bytes()) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::General("TLS 1.2 is not offered".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        schemes()
    }
}
