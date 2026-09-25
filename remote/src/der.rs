//! The little DER this service needs: writing one self-signed certificate,
//! and reading the public key back out of a certificate a peer presents.
//!
//! A certificate here is an envelope for a public key, not a claim anybody
//! vouches for. Trust is decided by pinning the SHA-256 of the exact
//! certificate bytes, and the key that must sign each TLS handshake is read
//! from those same bytes, so a peer cannot present one key and sign with
//! another.

/// OID bodies, already DER-encoded.
pub const OID_EC_PUBLIC_KEY: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
pub const OID_PRIME256V1: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
pub const OID_ECDSA_SHA256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02];
pub const OID_COMMON_NAME: &[u8] = &[0x55, 0x04, 0x03];
pub const OID_BASIC_CONSTRAINTS: &[u8] = &[0x55, 0x1d, 0x13];

const SEQUENCE: u8 = 0x30;
const SET: u8 = 0x31;
const INTEGER: u8 = 0x02;
const BIT_STRING: u8 = 0x03;
const OCTET_STRING: u8 = 0x04;
const OID: u8 = 0x06;
const UTF8_STRING: u8 = 0x0c;
const UTC_TIME: u8 = 0x17;
const GENERALIZED_TIME: u8 = 0x18;
const BOOLEAN: u8 = 0x01;

/// One element: tag, length and value.
pub fn tlv(tag: u8, value: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = value.len();
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes: Vec<u8> = len
            .to_be_bytes()
            .iter()
            .copied()
            .skip_while(|&b| b == 0)
            .collect();
        out.push(0x80 | bytes.len() as u8);
        out.extend(bytes);
    }
    out.extend_from_slice(value);
    out
}

pub fn sequence(parts: &[&[u8]]) -> Vec<u8> {
    tlv(SEQUENCE, &parts.concat())
}

pub fn set(parts: &[&[u8]]) -> Vec<u8> {
    tlv(SET, &parts.concat())
}

pub fn oid(body: &[u8]) -> Vec<u8> {
    tlv(OID, body)
}

/// A non-negative INTEGER from big-endian magnitude bytes.
pub fn unsigned_integer(bytes: &[u8]) -> Vec<u8> {
    let trimmed: Vec<u8> = bytes.iter().copied().skip_while(|&b| b == 0).collect();
    let mut body = Vec::with_capacity(trimmed.len() + 1);
    if trimmed.is_empty() || trimmed[0] & 0x80 != 0 {
        body.push(0);
    }
    body.extend(trimmed);
    tlv(INTEGER, &body)
}

pub fn bit_string(bytes: &[u8]) -> Vec<u8> {
    let mut body = vec![0u8];
    body.extend_from_slice(bytes);
    tlv(BIT_STRING, &body)
}

pub fn octet_string(bytes: &[u8]) -> Vec<u8> {
    tlv(OCTET_STRING, bytes)
}

pub fn boolean(value: bool) -> Vec<u8> {
    tlv(BOOLEAN, &[if value { 0xff } else { 0x00 }])
}

pub fn utf8(text: &str) -> Vec<u8> {
    tlv(UTF8_STRING, text.as_bytes())
}

/// `YYMMDDHHMMSSZ`, for dates 1950–2049.
pub fn utc_time(text: &str) -> Vec<u8> {
    tlv(UTC_TIME, text.as_bytes())
}

/// `YYYYMMDDHHMMSSZ`, for dates from 2050 on.
pub fn generalized_time(text: &str) -> Vec<u8> {
    tlv(GENERALIZED_TIME, text.as_bytes())
}

/// `[n] EXPLICIT`, constructed context-specific.
pub fn explicit(n: u8, inner: &[u8]) -> Vec<u8> {
    tlv(0xa0 | n, inner)
}

/// One element read from DER.
#[derive(Debug, Clone, Copy)]
pub struct Element<'a> {
    pub tag: u8,
    pub value: &'a [u8],
    /// The whole encoding, tag and length included.
    pub encoded: &'a [u8],
}

/// Read one element from the front of `input`, returning it and the rest.
///
/// Definite lengths only, single-byte tags only, and a length must fit what is
/// there. That covers every level of a certificate this module walks, and a
/// structure outside it is refused rather than guessed at.
pub fn read(input: &[u8]) -> Option<(Element<'_>, &[u8])> {
    let (&tag, rest) = input.split_first()?;
    if tag & 0x1f == 0x1f {
        return None; // multi-byte tag
    }
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = if first < 0x80 {
        (first as usize, rest)
    } else {
        let count = (first & 0x7f) as usize;
        if count == 0 || count > 4 || rest.len() < count {
            return None; // indefinite, or absurd
        }
        let mut len = 0usize;
        for &b in &rest[..count] {
            len = (len << 8) | b as usize;
        }
        (len, &rest[count..])
    };
    if rest.len() < len {
        return None;
    }
    let header = input.len() - rest.len();
    let element = Element {
        tag,
        value: &rest[..len],
        encoded: &input[..header + len],
    };
    Some((element, &rest[len..]))
}

/// Every element directly inside a constructed value.
pub fn children(mut value: &[u8]) -> Option<Vec<Element<'_>>> {
    let mut out = Vec::new();
    while !value.is_empty() {
        let (element, rest) = read(value)?;
        out.push(element);
        value = rest;
    }
    Some(out)
}

/// A certificate's SubjectPublicKeyInfo, as its whole DER element.
pub fn certificate_spki(certificate: &[u8]) -> Option<&[u8]> {
    let (outer, rest) = read(certificate)?;
    if outer.tag != SEQUENCE || !rest.is_empty() {
        return None;
    }
    let parts = children(outer.value)?;
    let tbs = parts.first().filter(|e| e.tag == SEQUENCE)?;
    let fields = children(tbs.value)?;
    // version [0] is optional; after it: serial, signature, issuer, validity,
    // subject, subjectPublicKeyInfo.
    let skip = usize::from(fields.first()?.tag == 0xa0);
    let spki = fields.get(skip + 5)?;
    (spki.tag == SEQUENCE).then_some(spki.encoded)
}

/// The uncompressed P-256 point of an SPKI, if that is what it holds.
pub fn ec_p256_point(spki: &[u8]) -> Option<&[u8]> {
    let (outer, rest) = read(spki)?;
    if outer.tag != SEQUENCE || !rest.is_empty() {
        return None;
    }
    let parts = children(outer.value)?;
    let [algorithm, key] = parts.as_slice() else {
        return None;
    };
    let ids = children(algorithm.value)?;
    let [kind, curve] = ids.as_slice() else {
        return None;
    };
    if kind.tag != OID
        || kind.value != OID_EC_PUBLIC_KEY
        || curve.tag != OID
        || curve.value != OID_PRIME256V1
    {
        return None;
    }
    if key.tag != BIT_STRING {
        return None;
    }
    let (&unused, point) = key.value.split_first()?;
    (unused == 0 && point.len() == 65 && point[0] == 0x04).then_some(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_use_the_short_and_long_forms() {
        assert_eq!(tlv(0x04, &[1, 2]), vec![0x04, 2, 1, 2]);
        let long = tlv(0x04, &[0u8; 200]);
        assert_eq!(&long[..3], &[0x04, 0x81, 200]);
        let longer = tlv(0x04, &[0u8; 300]);
        assert_eq!(&longer[..4], &[0x04, 0x82, 0x01, 0x2c]);
        let (element, rest) = read(&longer).unwrap();
        assert_eq!(element.value.len(), 300);
        assert!(rest.is_empty());
    }

    #[test]
    fn integers_are_minimal_and_non_negative() {
        assert_eq!(unsigned_integer(&[0, 0, 5]), vec![0x02, 1, 5]);
        assert_eq!(unsigned_integer(&[0x80]), vec![0x02, 2, 0, 0x80]);
        assert_eq!(unsigned_integer(&[]), vec![0x02, 1, 0]);
    }

    #[test]
    fn truncated_or_indefinite_input_is_refused() {
        assert!(read(&[0x30, 0x05, 1, 2]).is_none());
        assert!(read(&[0x30, 0x80, 0, 0]).is_none());
        assert!(read(&[0x1f, 0x01, 0]).is_none());
        assert!(certificate_spki(&[0x30, 0x00]).is_none());
    }
}
