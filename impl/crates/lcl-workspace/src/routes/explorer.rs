//! The project explorer: one folder's direct children at a time, and New Folder.

use super::*;

impl Routes {
    /// `GET /api/tree?parent=`: the direct children of one folder of the
    /// project, the root for no `parent`, and nothing below them. Folders
    /// first, then documents, each in name order; every folder, empty or not.
    /// `truncated` is about this folder alone: it held more than
    /// [`crate::project::MAX_CHILDREN`] entries, and these are the first.
    pub(super) fn tree(&self, workspace: &Workspace, request: &Request) -> Response {
        if self.is_home() {
            return Response::error(
                409,
                "no project is open: choose one on the Projects home first",
            );
        }
        let parent = request.param("parent").unwrap_or("");
        match workspace.children(parent) {
            Ok(children) => {
                // The `SPECIFICATION` `KIND` each listed document declares,
                // as its engine parses it: the role of a project part,
                // `kind.project` for an entry; `null` for a folder, an
                // unreadable or unparsable file, or one over 1 MiB.
                let kinds = workspace.tree_kinds(&children);
                Response::json(
                    Object::new()
                        .with("parent", Node::string(&children.parent))
                        .with(
                            "entries",
                            Node::array(children.entries.iter().zip(kinds).map(|(e, kind)| {
                                Object::new()
                                    .with("id", Node::string(&e.id))
                                    .with("name", Node::string(&e.name))
                                    .with("directory", Node::Bool(e.directory))
                                    .with(
                                        "bytes",
                                        match e.bytes {
                                            Some(n) => Node::u64(n),
                                            None => Node::Null,
                                        },
                                    )
                                    .with("kind", kind.as_deref().map_or(Node::Null, Node::string))
                                    .into()
                            })),
                        )
                        .with("truncated", Node::Bool(children.truncated))
                        .pretty(),
                )
            }
            Err(crate::WorkspaceError::Document(crate::DocumentError::Outside(_))) => {
                Response::error(400, "that folder is not inside the project")
            }
            Err(e) => Response::error(404, &e.to_string()),
        }
    }

    /// `POST /api/tree/folder?id=`: make one empty folder in the project,
    /// under a folder that exists. The explorer shows it at once.
    pub(super) fn create_folder(&self, workspace: &Workspace, request: &Request) -> Response {
        if self.is_home() {
            return Response::error(409, "no project is open");
        }
        let Some(id) = request
            .param("id")
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            return Response::error(400, "a folder name is required");
        };
        match workspace.create_folder(id) {
            Ok(id) => Response::json(
                Object::new()
                    .with("id", Node::string(&id))
                    .with("directory", Node::Bool(true))
                    .pretty(),
            ),
            Err(crate::WorkspaceError::Document(crate::DocumentError::AlreadyExists(_))) => {
                Response::error(409, &format!("{id} already exists"))
            }
            Err(crate::WorkspaceError::Document(crate::DocumentError::Outside(_))) => {
                Response::error(400, "that folder would be outside the project")
            }
            Err(e) => Response::error(422, &e.to_string()),
        }
    }
}
