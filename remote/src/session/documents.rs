//! The documents a device has open: read, written and removed through the
//! workspace, and watched, so the device hears when one changes on this PC.

use super::operations::{Answer, Asked};
use super::wire::{error_body, End, Wire};
use super::Session;
use lcl_protocol::json::{Node, Object};
use lcl_spec::json::Json;

impl Session {
    /// Read a document, and watch it from then on.
    pub(super) fn open(&mut self, asked: &Asked) -> Answer {
        let doc = asked.need("document", "a document")?;
        let (status, body) = asked.call("GET", "/api/document", &[("id", doc.clone())], "");
        if status == 200 {
            let digest = lcl_spec::json::parse(&body)
                .ok()
                .and_then(|b| b.get("digest").and_then(Json::as_str).map(str::to_string));
            self.watched.insert((asked.project.id.clone(), doc), digest);
        }
        Ok((status, body))
    }

    /// Stop watching a document.
    pub(super) fn close(&mut self, asked: &Asked) -> Answer {
        let doc = asked.need("document", "a document")?;
        self.watched.remove(&(asked.project.id.clone(), doc));
        Ok((200, "{}".to_string()))
    }

    /// Save only over the revision the device started from.
    pub(super) fn save(&mut self, asked: &Asked) -> Answer {
        use lcl_workspace::{DocumentError as D, WorkspaceError as W};
        let doc = asked.need("document", "a document")?;
        let base = asked.need("base", "the revision the edit started from (base)")?;
        let text = asked.need("text", "the text")?;
        let workspace = asked.routes.workspace();
        Ok(match workspace.save_expecting(&doc, &text, &base) {
            Ok(saved) => {
                self.watched
                    .insert((asked.project.id.clone(), doc), Some(saved.digest.clone()));
                let body = Object::new()
                    .with("id", Node::string(&saved.id))
                    .with("digest", Node::string(&saved.digest))
                    .with("bytes", Node::usize(saved.text.len()))
                    .with(
                        "final_line_feed_added",
                        Node::Bool(saved.text.len() != text.len()),
                    )
                    .compact();
                (200, body)
            }
            Err(W::Document(D::Changed(_))) => {
                // Stop, and hand back what is on disk so the device can
                // reconcile. Nothing was written.
                let mut body = error_body(format!(
                    "{doc} changed on this PC since the revision this edit started from; \
                     nothing was saved"
                ))
                .with("conflict", Node::Bool(true));
                if let Ok(current) = workspace.read(&doc) {
                    body = body
                        .with("text", Node::string(current.text))
                        .with("digest", Node::string(current.digest));
                }
                (409, body.compact())
            }
            Err(W::Document(D::NotFound(_))) => (
                404,
                error_body(format!("{doc} no longer exists on this PC")).compact(),
            ),
            Err(W::Document(D::Superseded(_))) => (
                409,
                error_body(format!(
                    "{doc} was saved again while this save was in flight"
                ))
                .with("conflict", Node::Bool(true))
                .compact(),
            ),
            Err(e) => (422, error_body(e.to_string()).compact()),
        })
    }

    /// Create a document, and watch it from then on.
    ///
    /// With a role, the PC writes the role's scaffold or Master and the phone
    /// sends no text — only the digest of the scaffold it obtained with
    /// `scaffold`, which the PC requires to be the text it would write now.
    pub(super) fn create(&mut self, asked: &Asked) -> Answer {
        let mut query = vec![("id", asked.need("name", "a name")?)];
        let text = match asked.param("role") {
            Some(role) => {
                query.push(("role", role.to_string()));
                for key in ["mode", "master", "source", "scaffold_digest"] {
                    if let Some(value) = asked.param(key) {
                        query.push((key, value.to_string()));
                    }
                }
                String::new()
            }
            None => asked.need("text", "the text")?,
        };
        let (status, reply) = asked.call("POST", "/api/document", &query, &text);
        if status == 200 {
            if let Ok(created) = lcl_spec::json::parse(&reply) {
                if let (Some(doc), Some(digest)) = (
                    created.get("id").and_then(Json::as_str),
                    created.get("digest").and_then(Json::as_str),
                ) {
                    self.watched.insert(
                        (asked.project.id.clone(), doc.to_string()),
                        Some(digest.to_string()),
                    );
                }
            }
        }
        Ok((status, reply))
    }

    /// Delete a document as it is at the digest the device names, and stop
    /// watching it.
    pub(super) fn delete(&mut self, asked: &Asked) -> Answer {
        let doc = asked.need("document", "a document")?;
        let digest = asked.need("digest", "the digest")?;
        let (status, reply) = asked.call(
            "DELETE",
            "/api/document",
            &[("id", doc.clone()), ("digest", digest)],
            "",
        );
        if status == 200 {
            self.watched.remove(&(asked.project.id.clone(), doc));
        }
        Ok((status, reply))
    }

    /// Tell the device about open documents that changed on this PC.
    pub(super) fn check_documents(&mut self, wire: &mut Wire) -> Result<(), End> {
        for ((project, doc), known) in self.watched.iter_mut() {
            let Ok(Some((_, routes))) = self.shared.projects.open(&self.shared.paths, project)
            else {
                continue;
            };
            let now = routes.workspace().read(doc).ok().map(|d| d.digest);
            if &now != known {
                *known = now.clone();
                wire.send(
                    &Object::new()
                        .with("type", Node::string("event"))
                        .with("event", Node::string("document_changed"))
                        .with("project", Node::string(project))
                        .with("document", Node::string(doc))
                        .with("digest", Node::optional(now))
                        .compact(),
                )?;
            }
        }
        Ok(())
    }
}
