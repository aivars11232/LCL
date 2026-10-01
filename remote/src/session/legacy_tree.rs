//! The `tree` operation of the apps published before `children` existed: a
//! whole project in one answer.

use super::operations::Asked;
use lcl_protocol::json::{Node, Object};
use lcl_spec::json::Json;

/// How much of a project the `tree` operation of apps published before
/// `children` existed is given: their bounds, so an old app is never sent
/// more than it was built for.
const LEGACY_TREE_DEPTH: usize = 12;
const LEGACY_TREE_ENTRIES: usize = 4096;

/// The old `tree` answer, `{"entries": [...], "truncated": bool}`, assembled
/// from the explorer's per-folder listings: every folder and document to
/// [`LEGACY_TREE_DEPTH`], at most [`LEGACY_TREE_ENTRIES`] entries, each
/// folder's contents right after it (parents before children, depth first),
/// `truncated` when anything was left out. A folder that cannot be listed is
/// left out and counts as such.
pub(super) fn legacy_tree(asked: &Asked) -> (u16, String) {
    let (status, root) = asked.call("GET", "/api/tree", &[("parent", String::new())], "");
    if status != 200 {
        return (status, root);
    }
    let mut entries: Vec<Node> = Vec::new();
    let mut truncated = false;
    legacy_place(asked, "", 0, Some(root), &mut entries, &mut truncated);
    let body = Object::new()
        .with("entries", Node::array(entries))
        .with("truncated", Node::Bool(truncated))
        .compact();
    (200, body)
}

/// One folder of [`legacy_tree`]: its entries, and below each subfolder its
/// own, in place.
fn legacy_place(
    asked: &Asked,
    folder: &str,
    depth: usize,
    answered: Option<String>,
    entries: &mut Vec<Node>,
    truncated: &mut bool,
) {
    let body = answered.unwrap_or_else(|| {
        let (status, body) = asked.call("GET", "/api/tree", &[("parent", folder.to_string())], "");
        if status == 200 {
            body
        } else {
            String::new()
        }
    });
    let Ok(listing) = lcl_spec::json::parse(&body) else {
        *truncated = true;
        return;
    };
    if listing.get("truncated").and_then(Json::as_bool) == Some(true) {
        *truncated = true;
    }
    let Some(children) = listing.get("entries").and_then(Json::as_array) else {
        return;
    };
    for child in children {
        if entries.len() >= LEGACY_TREE_ENTRIES {
            *truncated = true;
            return;
        }
        let (Some(id), Some(directory)) = (
            child.get("id").and_then(Json::as_str),
            child.get("directory").and_then(Json::as_bool),
        ) else {
            continue;
        };
        let mut flat = Object::new()
            .with("id", Node::string(id))
            .with("directory", Node::Bool(directory));
        if let Some(kind) = child.get("kind").and_then(Json::as_str) {
            flat = flat.with("kind", Node::string(kind));
        }
        entries.push(flat.into());
        if directory {
            if depth + 1 > LEGACY_TREE_DEPTH {
                *truncated = true;
            } else {
                legacy_place(asked, id, depth + 1, None, entries, truncated);
            }
        }
    }
}
