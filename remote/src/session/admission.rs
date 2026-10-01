//! Who a device is: its `hello`, answered with `welcome` or with one refusal.

use super::wire::{error_message, Wire};
use super::{Shared, PROTOCOL};
use crate::devices::{Device, Refusal};
use crate::identity::Identity;
use crate::pairing::{Presented, Refusal as PairingRefusal, PAIRING_VERSION};
use crate::paths;
use lcl_protocol::json::{Node, Object};
use lcl_spec::json::Json;

/// Decide who a device is from its `hello`, and tell it.
pub(super) fn admit(
    wire: &mut Wire,
    shared: &Shared,
    hello: &str,
    fingerprint: &str,
) -> Option<Device> {
    let Ok(hello) = lcl_spec::json::parse(hello) else {
        return refuse(wire, "malformed", "the first message is not JSON");
    };
    let field = |k: &str| hello.get(k).and_then(Json::as_str);
    if field("type") != Some("hello") {
        return refuse(wire, "malformed", "the first message must be hello");
    }
    if field("protocol") != Some(PROTOCOL) {
        return refuse(
            wire,
            "unsupported_protocol",
            &format!("this PC speaks {PROTOCOL} only"),
        );
    }
    let device = match field("intent") {
        Some("pair") => pair(wire, shared, &hello, fingerprint)?,
        Some("connect") => connect(wire, shared, &hello, fingerprint)?,
        _ => {
            return refuse(
                wire,
                "malformed",
                "hello must say whether it is to pair or to connect",
            )
        }
    };
    let welcome = Object::new()
        .with("type", Node::string("welcome"))
        .with("protocol", Node::string(PROTOCOL))
        .with("pc", pc_node(&shared.identity))
        .with("device", device_node(&device))
        .compact();
    wire.send(&welcome).ok()?;
    Some(device)
}

/// A device asking to be trusted with a code. Only the request the person at
/// the PC approved is let in, and is told `paired` first. Any other is told
/// where it stands, and gets no session.
fn pair(wire: &mut Wire, shared: &Shared, hello: &Json, fingerprint: &str) -> Option<Device> {
    let now = paths::now();
    // Only the flow in which the PC approves each new device. A hello from
    // before it — or one that leaves the version out — is refused here,
    // whatever client sent it.
    match hello.get("pairing_version").and_then(Json::as_u64) {
        Some(PAIRING_VERSION) => {}
        Some(version) if version > PAIRING_VERSION => {
            return refuse(
                wire,
                "unsupported_pairing_version",
                &format!(
                    "this PC pairs with pairing version {PAIRING_VERSION}; update LCL on the PC"
                ),
            )
        }
        _ => {
            return refuse(
                wire,
                "pairing_upgrade_required",
                "this device uses the older pairing flow, which this PC no longer accepts; \
                 update LCL for Android and scan a new QR code",
            )
        }
    }
    let field = |k: &str| hello.get(k).and_then(Json::as_str);
    let (Some(code), Some(name)) = (field("code"), field("name")) else {
        return refuse(wire, "malformed", "pairing needs a code and a device name");
    };
    let presented = shared.pairing.present(
        code,
        fingerprint,
        name,
        &shared.identity.fingerprint,
        &shared.registry,
        PROTOCOL,
        now,
    );
    match presented {
        // Not trusted: the device is told what to show and to ask again, and
        // the connection ends. It holds no session, and no connection is kept
        // open while the person decides.
        Ok(Presented::Pending(candidate)) => {
            let _ = wire.send(
                &Object::new()
                    .with("type", Node::string("pairing_pending"))
                    .with("request", Node::string(&candidate.request))
                    .with("verification", Node::string(&candidate.verification))
                    .with("expires", Node::u64(candidate.expires))
                    .with("pc", pc_node(&shared.identity))
                    .compact(),
            );
            None
        }
        Ok(Presented::Paired(device)) => {
            let paired = Object::new()
                .with("type", Node::string("paired"))
                .with("device", device_node(&device))
                .with("pc", pc_node(&shared.identity))
                .compact();
            wire.send(&paired).ok()?;
            Some(device)
        }
        Err(PairingRefusal::Denied) => {
            refuse(wire, "pairing_denied", &PairingRefusal::Denied.to_string())
        }
        Err(PairingRefusal::Busy) => {
            refuse(wire, "pairing_busy", &PairingRefusal::Busy.to_string())
        }
        Err(PairingRefusal::Unreadable(e)) => refuse(wire, "unavailable", &e),
        Err(refusal) => refuse(wire, "pairing_refused", &refusal.to_string()),
    }
}

/// A device that says it is already paired. It is let in only if its
/// certificate's fingerprint belongs to a device that is paired and not
/// revoked.
fn connect(wire: &mut Wire, shared: &Shared, hello: &Json, fingerprint: &str) -> Option<Device> {
    match shared.registry.authorize(fingerprint) {
        Ok(device) => {
            // A device may say which record it believes it is; it must be
            // the one its certificate belongs to.
            if let Some(claimed) = hello.get("device").and_then(Json::as_str) {
                if claimed != device.id {
                    return refuse(
                        wire,
                        "identity_mismatch",
                        "this certificate belongs to another device",
                    );
                }
            }
            Some(device)
        }
        Err(Refusal::Unknown) => refuse(
            wire,
            "not_paired",
            "this device is not paired with this PC; scan a new QR code",
        ),
        Err(Refusal::Revoked) => refuse(
            wire,
            "revoked",
            "this PC revoked this device; pair it again with a new QR code",
        ),
        Err(Refusal::Unreadable(e)) => refuse(wire, "unavailable", &e),
    }
}

/// Answer a `hello` with one `error`. The device is not let in.
fn refuse(wire: &mut Wire, code: &str, message: &str) -> Option<Device> {
    let _ = wire.send(&error_message(code, message));
    None
}

fn device_node(device: &Device) -> Node {
    Object::new()
        .with("id", Node::string(&device.id))
        .with("name", Node::string(&device.name))
        .into()
}

pub(super) fn pc_node(identity: &Identity) -> Node {
    Object::new()
        .with("id", Node::string(&identity.pc_id))
        .with("name", Node::string(&identity.name))
        .with("fingerprint", Node::string(&identity.fingerprint))
        .into()
}
