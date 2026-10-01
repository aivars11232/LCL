//! Settings → Android devices: what `lcl-remote` reports and does, asked on the person's behalf.

use super::*;

/// The paired devices and whether the service is running, as `lcl-remote`
/// reports them; `{"installed":false}` when it is not installed.
pub(super) fn remote_devices() -> Response {
    let Some(program) = remote::binary() else {
        return Response::json("{\"installed\":false}".to_string());
    };
    match remote::run(&program, &["devices", "--json"]) {
        Ok(json) => Response::json(json),
        Err(detail) => Response::error(502, &detail),
    }
}

/// A new one-time pairing code: its pairing text, its QR code as SVG, and
/// when it expires. Each call makes a new one. Scanning it trusts nothing: a
/// phone that asks with it waits until the person approves it here.
pub(super) fn remote_pair() -> Response {
    let Some(program) = remote::binary() else {
        return Response::error(503, "lcl-remote is not installed on this PC");
    };
    match remote::run(&program, &["pair", "--json"]) {
        Ok(json) => Response::json(json),
        Err(detail) => Response::error(502, &detail),
    }
}

/// The pairing requests waiting for the person at this PC, each with the
/// verification code its phone shows, as `lcl-remote pending --json` reports
/// them.
pub(super) fn remote_pending() -> Response {
    let Some(program) = remote::binary() else {
        return Response::json("{\"installed\":false}".to_string());
    };
    match remote::run(&program, &["pending", "--json"]) {
        Ok(json) => Response::json(json),
        Err(detail) => Response::error(502, &detail),
    }
}

/// Approve or deny exactly one pairing request, by its id. Approving is what
/// trusts a new device; the page asks the person to confirm it first.
pub(super) fn remote_decide(request: &Request, decision: &str) -> Response {
    let Some(id) = request.param("id") else {
        return Response::error(400, "a request id is required");
    };
    if !remote::valid_request_id(id) {
        return Response::error(400, "that is not a pairing request id");
    }
    let Some(program) = remote::binary() else {
        return Response::error(503, "lcl-remote is not installed on this PC");
    };
    match remote::run(&program, &[decision, id]) {
        Ok(said) => Response::json(
            Object::new()
                .with("request", Node::string(id))
                .with("decision", Node::string(decision))
                .with("message", Node::string(said.trim()))
                .compact(),
        ),
        Err(detail) => Response::error(502, &detail),
    }
}

/// Stop trusting one device. It is disconnected and must pair again.
pub(super) fn remote_revoke(request: &Request) -> Response {
    let Some(id) = request.param("id") else {
        return Response::error(400, "a device id is required");
    };
    if !remote::valid_device_id(id) {
        return Response::error(400, "that is not a device id");
    }
    let Some(program) = remote::binary() else {
        return Response::error(503, "lcl-remote is not installed on this PC");
    };
    match remote::run(&program, &["revoke", id]) {
        Ok(said) => Response::json(
            Object::new()
                .with("revoked", Node::string(id))
                .with("message", Node::string(said.trim()))
                .compact(),
        ),
        Err(detail) => Response::error(502, &detail),
    }
}
