//! Documents of the open project: read, save over a known revision, create, delete.

use super::*;

impl Routes {
    pub(super) fn read_document(&self, workspace: &Workspace, request: &Request) -> Response {
        let Some(id) = request.param("id") else {
            return Response::error(400, "a document id is required");
        };
        match workspace.read(id) {
            Ok(document) => Response::json(
                Object::new()
                    .with("id", Node::string(&document.id))
                    .with("text", Node::string(&document.text))
                    .with("digest", Node::string(&document.digest))
                    .pretty(),
            ),
            Err(e) => Response::error(404, &e.to_string()),
        }
    }

    /// Save one document. The body is the exact bytes to write, and `base` is
    /// the SHA-256 of the revision the edit started from.
    ///
    /// A save that does not name its revision is refused (428): a stale
    /// buffer must never replace a newer phone or external edit. When the
    /// file no longer holds `base` nothing is written and the reply is 409
    /// with `conflict: true` and what the disk holds now — `digest` and
    /// `exists` — so the person can choose, explicitly, to reload the disk
    /// version or to keep theirs by saving again over that exact revision.
    /// Nothing is ever merged. A refusal by the encoding rule is 422 and says
    /// which rule and why. The file is not touched in any refusal.
    pub(super) fn save_document(&self, workspace: &Workspace, request: &Request) -> Response {
        let Some(id) = request.param("id") else {
            return Response::error(400, "a document id is required");
        };
        let Some(base) = request.param("base").filter(|b| !b.is_empty()) else {
            return Response::error(
                428,
                "a save must name the revision it edited (base): nothing was written",
            );
        };
        let text = match request.text() {
            Ok(text) => text,
            Err(e) => return Response::error(422, &e.to_string()),
        };
        match workspace.save_expecting(id, text, base) {
            Ok(document) => Response::json(
                Object::new()
                    .with("id", Node::string(&document.id))
                    .with("digest", Node::string(&document.digest))
                    .with("bytes", Node::usize(document.text.len()))
                    .with(
                        "final_line_feed_added",
                        Node::Bool(document.text.len() != text.len()),
                    )
                    .pretty(),
            ),
            // A newer save of this exact document was published while this one
            // was still in flight. 409 and not 422: the request was well formed
            // and the state moved under it, which is what a conflict is. It is
            // reported rather than swallowed, so a client is never told a write
            // succeeded when the newer content is what is on disk.
            Err(crate::WorkspaceError::Document(crate::DocumentError::Superseded(_))) => {
                Response::error(
                    409,
                    &format!("{id} was saved again while this write was in flight"),
                )
            }
            Err(crate::WorkspaceError::Document(
                crate::DocumentError::Changed(_) | crate::DocumentError::NotFound(_),
            )) => {
                let now = workspace.read(id).ok();
                Response::json_status(
                    409,
                    Object::new()
                        .with(
                            "error",
                            Node::string(format!(
                                "{id} changed on disk since this revision was loaded; nothing was written"
                            )),
                        )
                        .with("conflict", Node::Bool(true))
                        .with("id", Node::string(id))
                        .with("exists", Node::Bool(now.is_some()))
                        .with(
                            "digest",
                            now.as_ref()
                                .map_or(Node::Null, |d| Node::string(&d.digest)),
                        )
                        .pretty(),
                )
            }
            Err(e) => Response::error(422, &e.to_string()),
        }
    }

    /// Create one document, under the name the naming default gives it.
    ///
    /// Separate from `PUT` on purpose. Saving must write exactly the name it
    /// was given, whichever ending it has; creating applies the naming default,
    /// which keeps an explicitly chosen `.lcl` or `.lcl.txt` and gives a name
    /// without either the default ending chosen in Settings — the native
    /// `.lcl` unless the person chose `.lcl.txt`. Keeping the two in one route
    /// would mean guessing which of them the caller meant.
    ///
    /// An existing file is never overwritten. Creation that silently replaced
    /// a document would be a data-loss path reachable by typing a name.
    pub(super) fn create_document(&self, workspace: &Workspace, request: &Request) -> Response {
        let Some(requested) = request.param("id") else {
            return Response::error(400, "a document id is required");
        };
        let ending = self.settings().settings.default_extension;
        let id = lcl_project::default_name_with(requested, ending);
        // A bare suffix (`.lcl`, `.lcl.txt`, or an empty name given `.lcl`) has
        // nothing in front of it, which is exactly what `is_document` refuses.
        if !lcl_project::is_document(&id) {
            return Response::error(400, "a document needs a name");
        }
        // Preserve the early response, but do not use this observation as a
        // reservation: create_document below is the atomic authority.
        if workspace.exists(&id) {
            return Response::error(409, &format!("{id} already exists"));
        }
        // With a role, the text is the role's scaffold or Master for exactly
        // this name, and a body would be a second, conflicting answer.
        let scaffolded;
        let text = match request.param("role") {
            Some(_) => {
                if !request.body.is_empty() {
                    return Response::error(
                        400,
                        "a file created by role takes its text from the scaffold; send no body",
                    );
                }
                scaffolded = match authoring::file(workspace, &self.masters(), request, &id) {
                    Ok(planned) => planned.scaffold.text,
                    Err(refusal) => return refusal,
                };
                // Created only from the exact text the person previewed
                // (`GET /api/scaffold` for this very name): a Master or
                // default that changed in between, or another name, gives
                // other bytes, and nothing is created (C03-AUDIT-04).
                let digest = lcl_spec::sha256::hex_digest(scaffolded.as_bytes());
                match request.param("scaffold_digest") {
                    Some(previewed) if previewed == digest => {}
                    Some(_) => {
                        return Response::error(
                            409,
                            &format!(
                                "the starting text of {id} changed since it was previewed (a \
                                 template, a default or the name changed); nothing was created"
                            ),
                        )
                    }
                    None => {
                        return Response::error(
                            428,
                            "a file created by role is created only from a preview: pass the \
                             scaffold_digest GET /api/scaffold returned; nothing was created",
                        )
                    }
                }
                scaffolded.as_str()
            }
            None => match request.text() {
                Ok(text) => text,
                Err(e) => return Response::error(422, &e.to_string()),
            },
        };
        match workspace.create_document(&id, text) {
            Ok(document) => Response::json(
                Object::new()
                    .with("id", Node::string(&document.id))
                    // What the caller asked for, so a frontend can say that the
                    // name it is about to show is not the one that was typed.
                    .with("requested", Node::string(requested))
                    .with("digest", Node::string(&document.digest))
                    .with("bytes", Node::usize(document.text.len()))
                    .pretty(),
            ),
            Err(crate::WorkspaceError::Document(crate::DocumentError::AlreadyExists(_))) => {
                Response::error(409, &format!("{id} already exists"))
            }
            Err(e) => Response::error(422, &e.to_string()),
        }
    }

    /// Delete one document, if it still holds the bytes that were confirmed.
    ///
    /// `digest` is required: it is the SHA-256 of the content the person saw
    /// when they confirmed, and a file that changed or was replaced since is
    /// left alone and reported. See [`crate::document::delete`] for what else
    /// is refused — anything but a regular `.lcl` or `.lcl.txt` file inside
    /// the project.
    pub(super) fn delete_document(&self, workspace: &Workspace, request: &Request) -> Response {
        use crate::DocumentError as D;
        let Some(id) = request.param("id") else {
            return Response::error(400, "a document id is required");
        };
        let Some(digest) = request.param("digest").filter(|d| !d.is_empty()) else {
            return Response::error(
                400,
                "a digest is required: a deletion removes only the content that was confirmed",
            );
        };
        match workspace.delete(id, digest) {
            Ok(()) => Response::json(
                Object::new()
                    .with("id", Node::string(id))
                    .with("deleted", Node::Bool(true))
                    .pretty(),
            ),
            Err(crate::WorkspaceError::Document(error)) => {
                let status = match &error {
                    D::NotFound(_) => 404,
                    D::Changed(_) | D::Superseded(_) => 409,
                    D::Io { .. } => 500,
                    _ => 400,
                };
                Response::error(status, &error.to_string())
            }
            Err(error) => Response::error(500, &error.to_string()),
        }
    }
}
