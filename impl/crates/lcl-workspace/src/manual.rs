//! The Users Manual, packaged into this binary.
//!
//! The manual is documentation, not language authority: nothing here reaches
//! the engine, a document, a project or a setting. The snapshot is every
//! top-level `*.md` file of the repository's `users_manual/` at build time,
//! and `users_manual/MANIFEST.json` records its version and digest. The
//! Android app packages the same files and the same viewer
//! (`assets/manual/`), and both prove the same digest.
//!
//! Only these embedded bytes are served. A name is looked up in the embedded
//! list, never on a filesystem, so no route can reach anything else.

use crate::http::Response;
use lcl_protocol::json::{Node, Object};

include!(concat!(env!("OUT_DIR"), "/manual_files.rs"));

const VIEWER_HTML: &str = include_str!("../assets/manual/manual.html");
pub const VIEWER_JS: &str = include_str!("../assets/manual/manual.js");
pub const VIEWER_CSS: &str = include_str!("../assets/manual/manual.css");

/// SHA-256 over every file, in the order given: for each, its name, a NUL,
/// its length in decimal, a NUL, then its bytes. `users_manual/tools/
/// manual_manifest.py` and the Android app compute the same.
pub fn digest<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> String {
    let mut bytes = Vec::new();
    for (name, content) in files {
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(content.len().to_string().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(content);
    }
    lcl_spec::sha256::hex_digest(&bytes)
}

/// The digest of the embedded snapshot.
pub fn embedded_digest() -> String {
    digest(FILES.iter().map(|(name, text)| (*name, text.as_bytes())))
}

/// The manifest's `version`.
pub fn version() -> String {
    lcl_spec::json::parse(MANIFEST)
        .ok()
        .and_then(|m| {
            m.get("version")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// The manual window. `{{TOKEN}}` is the session token and `{{Q}}` the query
/// that carries it to the viewer's own files; the Android app fills `{{Q}}`
/// with nothing.
pub fn page(token: &str) -> Response {
    Response::html(
        &VIEWER_HTML
            .replace("{{TOKEN}}", token)
            .replace("{{Q}}", &format!("?t={token}")),
    )
}

/// `GET /manual/snapshot`: the whole manual in one reply.
pub fn snapshot() -> Response {
    Response::json(
        Object::new()
            .with("version", Node::string(version()))
            .with("digest", Node::string(embedded_digest()))
            .with(
                "files",
                Node::array(FILES.iter().map(|(name, text)| {
                    Object::new()
                        .with("name", Node::string(*name))
                        .with("text", Node::string(*text))
                        .into()
                })),
            )
            .pretty(),
    )
}
