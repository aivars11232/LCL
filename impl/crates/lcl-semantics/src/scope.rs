//! `SCOPE`, `WORKSPACE` and containment.
//!
//! Authority: `05_SEMANTICS/02_SCOPE_TARGET_WORKSPACE_AND_SOURCE.txt`.
//!
//! > SCOPE is computed as INCLUDE minus EXCLUDE. Exact references/paths
//! > identify one entity. GLOB/REGEX select a finite set resolved before
//! > affected execution. A GLOB is evaluated only against workspace-relative
//! > paths under its closed profile; it cannot select an absolute path or
//! > escape the WORKSPACE. EXCLUDE wins within the same SCOPE. Nested scopes
//! > intersect unless a higher-authority rule explicitly replaces a referenced
//! > scope.
//!
//! > WORKSPACE supplies an explicit base path and access MODE; it grants no
//! > operation by itself. A workspace-relative PATH is legal only when its
//! > resolved target is the WORKSPACE root or a descendant; textual prefix
//! > alone does not establish containment. A resolved escape produces
//! > error.value.out_of_range.
//!
//! ## Textual prefix is not containment
//!
//! The rule that "textual prefix alone does not establish containment" is the
//! whole of [`contains`]. `/ws` is not the parent of `/ws-other` even though
//! one string starts with the other, and `/ws/a/../../etc` is not inside `/ws`
//! even though it starts with it. Both are decided by resolving the path into
//! segments and comparing segments, never by comparing bytes.

use crate::authority::{ScopeRecord, Selector, WorkspaceRecord};
use crate::diagnostic::PreflightError;
use crate::engine::Engine;
use crate::syntax;
use lcl_parser::syntax::{
    Body, Call, Conditional, Executable, Expr, ForEach, Statement, Value as InlineValue,
};

/// Resolve every `SCOPE` and `WORKSPACE`, and check declared containment.
pub(crate) fn resolve(engine: &mut Engine) {
    collect_workspaces(engine);
    collect_scopes(engine);
    check_workspace_containment(engine);
    check_workspace_paths(engine);
}

fn collect_workspaces(engine: &mut Engine) {
    let mut found = Vec::new();
    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        if declaration.block != "WORKSPACE" {
            continue;
        }
        let Some(block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        let path = block
            .field("PATH")
            .and_then(|f| syntax::inline_expr(&f.body))
            .and_then(constructor_text);
        let mode = syntax::field_text(block, "MODE");
        if let (Some(path), Some(mode)) = (path, mode) {
            found.push(WorkspaceRecord {
                declaration: index,
                id: declaration.id.qualified(),
                source: declaration.source.clone(),
                span: declaration.id_span,
                path,
                mode,
            });
        }
    }
    found.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.span.start.cmp(&b.span.start))
    });
    engine.workspaces = found;
}

fn collect_scopes(engine: &mut Engine) {
    let mut found = Vec::new();
    // A document declaring exactly one WORKSPACE resolves its relative paths
    // against it. Several workspaces make the choice ambiguous, and inventing
    // one would decide a rule the language leaves to the declaration.
    let workspace = if engine.workspaces.len() == 1 {
        Some(engine.workspaces[0].declaration)
    } else {
        None
    };

    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        if declaration.block != "SCOPE" {
            continue;
        }
        let Some(block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        found.push(ScopeRecord {
            declaration: index,
            id: declaration.id.qualified(),
            source: declaration.source.clone(),
            span: declaration.id_span,
            include: selectors(block, "INCLUDE"),
            exclude: selectors(block, "EXCLUDE"),
            operation: syntax::field_text(block, "OPERATION"),
            workspace,
        });
    }
    found.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.span.start.cmp(&b.span.start))
    });
    engine.scopes = found;
}

/// Every selector one `INCLUDE` or `EXCLUDE` field declares, in source order.
fn selectors(block: syntax::DeclBlock, field: &str) -> Vec<Selector> {
    let Some(field) = block.field(field) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(collection) = syntax::inline_collection(&field.body) {
        for member in &collection.members {
            if let Some(selector) = selector_of(member) {
                out.push(selector);
            }
        }
        return out;
    }
    if let Some(expr) = syntax::inline_expr(&field.body) {
        if let Some(selector) = selector_of(expr) {
            out.push(selector);
        }
    }
    out
}

/// Classify one selector by what it is written as.
///
/// `GLOB` and `REGEX` are the two registered pattern constructors; anything
/// else is exact. Classifying by the written constructor keeps a `PATH` from
/// being treated as a pattern, which would silently widen a scope.
pub(crate) fn selector_of(expr: &Expr) -> Option<Selector> {
    if let Some(id) = syntax::reference_target(expr) {
        return Some(Selector::Reference(id.to_string()));
    }
    match expr {
        Expr::Call(call) => {
            let literal = call.arguments.first().and_then(literal_string);
            Some(match (call.callable.text.as_str(), literal) {
                ("GLOB", Some(text)) => Selector::Glob(text),
                ("REGEX", Some(text)) => Selector::Regex(text),
                // A constructor of exactly one string literal identifies the
                // entity that literal names: `PATH("/ws/src")` selects
                // `/ws/src`, which is what containment compares with the root.
                (_, Some(text)) if call.arguments.len() == 1 => Selector::Exact(text),
                // `PATH(REF(workspace), "relative")`: the one form whose parts
                // both matter — the workspace it is rooted in, and the exact
                // decoded relative STRING a GLOB consumes.
                ("PATH", None) if call.arguments.len() == 2 => {
                    match (
                        call.arguments.first().and_then(syntax::reference_target),
                        call.arguments.get(1).and_then(literal_string),
                    ) {
                        (Some(workspace), Some(relative)) => Selector::Relative {
                            workspace: workspace.to_string(),
                            relative,
                        },
                        _ => Selector::Expression(crate::eval::render_static(expr)),
                    }
                }
                // Anything else identifies one entity by the form it is
                // written in, kept exactly as written.
                _ => Selector::Expression(crate::eval::render_static(expr)),
            })
        }
        Expr::Literal(literal) => Some(Selector::Exact(literal.text.clone())),
        Expr::Group(group) => selector_of(&group.inner),
        _ => None,
    }
}

/// Whether `scope` admits the entity `target` names.
///
/// `05_SEMANTICS/02`: "SCOPE is computed as INCLUDE minus EXCLUDE. Exact
/// references/paths identify one entity. GLOB/REGEX select a finite set
/// resolved before affected execution. A GLOB is evaluated only against
/// workspace-relative paths under its closed profile; it cannot select an
/// absolute path or escape the WORKSPACE. EXCLUDE wins within the same SCOPE."
///
/// Both sides are normalized by [`selector_of`], so an `ACTION` naming an
/// entity and a `SCOPE` selecting it are compared as the same written form.
///
/// ## A pattern decides, and it decides about *this* target
///
/// Deciding whether one known entity is in a pattern's set is not enumerating
/// that set: `pattern_profiles/GLOB` matches a `full_workspace_relative_path`
/// and `pattern_profiles/REGEX` matches a `full_string`, both over a subject
/// this layer already has in hand. Nothing here reads a filesystem, and no root
/// is inferred — a target that cannot supply a workspace-relative subject is
/// simply not selected by a `GLOB`, which is what "it cannot select an absolute
/// path" says.
///
/// The compiled matchers are `lcl-checker`'s, the ones the checker uses for a
/// declared `PATTERN` and the runtime uses for `MATCHES`. There is no second
/// pattern implementation.
pub(crate) fn admits(scope: &ScopeRecord, target: &Selector) -> Admission {
    // "EXCLUDE wins within the same SCOPE", so it is asked first and its
    // undecided answer is a refusal rather than a silent permission.
    match selected_by(&scope.exclude, target) {
        Selected::Yes => return Admission::Refused,
        Selected::Undecided(reason) => return Admission::Undecided(reason),
        Selected::No => {}
    }
    match selected_by(&scope.include, target) {
        Selected::Yes => Admission::Admitted,
        Selected::No => Admission::Refused,
        Selected::Undecided(reason) => Admission::Undecided(reason),
    }
}

/// What [`admits`] concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Admission {
    Admitted,
    Refused,
    /// The restriction could not be decided before effects. It is never treated
    /// as absent: the caller refuses, carrying this reason.
    Undecided(String),
}

enum Selected {
    Yes,
    No,
    Undecided(String),
}

/// Whether any selector of one side selects `target`.
fn selected_by(selectors: &[Selector], target: &Selector) -> Selected {
    let mut undecided = None;
    for selector in selectors {
        match selects(selector, target) {
            Selected::Yes => return Selected::Yes,
            Selected::No => {}
            Selected::Undecided(reason) => undecided = Some(reason),
        }
    }
    match undecided {
        Some(reason) => Selected::Undecided(reason),
        None => Selected::No,
    }
}

fn selects(selector: &Selector, target: &Selector) -> Selected {
    match selector {
        Selector::Glob(pattern) => match glob_subject(target) {
            // "it cannot select an absolute path or escape the WORKSPACE", and
            // `pattern_profiles/GLOB/input`: "An input that cannot supply this
            // form ... no root is inferred."
            None => Selected::No,
            Some(subject) => match lcl_checker::glob_selects(pattern, &subject) {
                Some(true) => Selected::Yes,
                Some(false) => Selected::No,
                None => Selected::Undecided(format!(
                    "GLOB({pattern:?}) could not be decided against {subject:?} within the \
                     declared finite pattern-resource limit"
                )),
            },
        },
        Selector::Regex(text) => {
            let (pattern, flags) = text
                .split_once(crate::value::REGEX_FLAG_SEPARATOR)
                .unwrap_or((text.as_str(), ""));
            let subject = regex_subject(target);
            match lcl_checker::regex_selects(pattern, flags, &subject) {
                Some(true) => Selected::Yes,
                Some(false) => Selected::No,
                None => Selected::Undecided(format!(
                    "REGEX({pattern:?}) could not be decided against {subject:?} within the \
                     declared finite pattern-resource limit"
                )),
            }
        }
        exact if exact == target => Selected::Yes,
        _ => Selected::No,
    }
}

/// The normalized relative segment sequence a `GLOB` consumes, when the target
/// can supply one.
///
/// `types_v0.1.0.json#/pattern_profiles/GLOB/input`: "A PATH operand requires an
/// explicit WORKSPACE root retained by that value ... and is compared using its
/// normalized relative segment sequence."
fn glob_subject(target: &Selector) -> Option<String> {
    let Selector::Relative { relative, .. } = target else {
        return None;
    };
    let mut segments: Vec<&str> = Vec::new();
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            // A relative path that leaves its root is already
            // `error.value.out_of_range` where the path is written.
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    Some(segments.join("/"))
}

/// The full string a `REGEX` selector matches: the text that identifies the
/// entity — a relative path under its workspace, a written path, or the
/// declaration id a reference names.
fn regex_subject(target: &Selector) -> String {
    match target {
        Selector::Relative { relative, .. } => {
            glob_subject(target).unwrap_or_else(|| relative.clone())
        }
        other => other.as_str().to_string(),
    }
}

/// The scope an action names, when that scope governs this invocation.
///
/// `02_LEXICAL/06`: SCOPE declares "the exact set of entities to which a clause
/// may apply". A `SCOPE` that names an `OPERATION` states the invocation it
/// governs, so it is not the applicable scope of an action invoking another
/// operation.
pub(crate) fn applicable<'a>(
    scopes: &'a [ScopeRecord],
    named: &str,
    operation: &str,
) -> Option<&'a ScopeRecord> {
    scopes
        .iter()
        .find(|scope| scope.id == named)
        .filter(|scope| match &scope.operation {
            Some(restricted) => restricted == operation,
            None => true,
        })
}

/// The single string argument of a constructor such as `PATH("/a")`.
fn constructor_text(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Call(call) => call.arguments.first().and_then(literal_string),
        Expr::Group(group) => constructor_text(&group.inner),
        _ => None,
    }
}

fn literal_string(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Literal(literal) => Some(literal.text.clone()),
        Expr::Group(group) => literal_string(&group.inner),
        _ => None,
    }
}

/// Check every declared workspace-relative path against its workspace root.
///
/// Only paths the document actually declares are checked. This layer resolves
/// no filesystem and enumerates nothing: containment is a question about the
/// two written paths, which is exactly what makes it answerable before effects.
fn check_workspace_containment(engine: &mut Engine) {
    if engine.workspaces.len() != 1 {
        return;
    }
    let root = engine.workspaces[0].path.clone();
    let mut escapes = Vec::new();

    for scope in &engine.scopes {
        for selector in scope.include.iter().chain(scope.exclude.iter()) {
            // Only an exact absolute path can be compared against the
            // workspace root here. A `GLOB` cannot escape by construction:
            // `02_LEXICAL` gives it a closed profile that the lexer already
            // enforces, rejecting a leading slash and any `.` or `..` segment
            // as `error.literal.invalid`. Re-checking it here would be a second
            // implementation of a rule an earlier stage already owns, and the
            // earliest-stage rule forbids a later stage re-deciding it.
            let Selector::Exact(path) = selector else {
                continue;
            };
            if !path.starts_with('/') {
                // A workspace-relative exact path resolves under the root.
                if !contains(&root, &join(&root, path)) {
                    escapes.push((
                        scope.source.clone(),
                        scope.span,
                        format!("{}|{}", scope.id, path),
                        format!(
                            "scope `{}` selects {path:?}, which resolves outside WORKSPACE `{root}`",
                            scope.id
                        ),
                    ));
                }
                continue;
            }
            if !contains(&root, path) {
                escapes.push((
                    scope.source.clone(),
                    scope.span,
                    format!("{}|{}", scope.id, path),
                    format!(
                        "scope `{}` selects {path:?}, which is neither WORKSPACE `{root}` nor a descendant of it",
                        scope.id
                    ),
                ));
            }
        }
    }

    for (source, span, cause, detail) in escapes {
        engine.emit(
            PreflightError::ValueOutOfRange,
            &source,
            span,
            cause,
            detail,
        );
    }
}

/// Every WORKSPACE-form `PATH(REF(workspace), "relative")` a declaration writes
/// must resolve to that WORKSPACE root or a descendant.
///
/// `03_TYPES_AND_VALUES/04`: "The WORKSPACE form must resolve to the workspace
/// root or one of its descendants. Containment is checked on the resolved
/// target, not by textual prefix ... Escape produces error.value.out_of_range."
fn check_workspace_paths(engine: &mut Engine) {
    let mut escapes = std::collections::BTreeMap::new();
    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        let Some(block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        let mut calls = Vec::new();
        statement_paths(block.statements(), &mut calls);
        for call in calls {
            if let Some(detail) = workspace_escape(&engine.workspaces, call) {
                // A nested declaration is also walked from its parent.
                escapes
                    .entry((declaration.source.clone(), call.span.start, call.span.end))
                    .or_insert((call.span, detail));
            }
        }
    }
    for ((source, _, _), (span, detail)) in escapes {
        engine.emit(
            PreflightError::ValueOutOfRange,
            &source,
            span,
            "workspace_path",
            detail,
        );
    }
}

/// The escape one WORKSPACE-form `PATH` call writes, when it writes one.
fn workspace_escape(workspaces: &[WorkspaceRecord], call: &Call) -> Option<String> {
    let [target, relative] = call.arguments.as_slice() else {
        return None;
    };
    let id = syntax::reference_target(target)?;
    let relative = syntax::literal_text(relative)?;
    let root = &workspaces.iter().find(|w| w.id == id)?.path;
    if !relative.starts_with('/') && contains(root, &join(root, &relative)) {
        return None;
    }
    Some(format!(
        "PATH(REF({id}), {relative:?}) resolves outside WORKSPACE `{root}`"
    ))
}

/// Every `PATH` call these statements write, nested bodies included.
fn statement_paths<'a>(statements: &'a [Statement], out: &mut Vec<&'a Call>) {
    for statement in statements {
        match statement {
            Statement::Field(field) => body_paths(&field.body, out),
            Statement::Property(property) => body_paths(&property.body, out),
            Statement::Conditional(conditional) => conditional_paths(conditional, out),
            Statement::ForEach(for_each) => for_each_paths(for_each, out),
        }
    }
}

fn executable_paths<'a>(executables: &'a [Executable], out: &mut Vec<&'a Call>) {
    for executable in executables {
        match executable {
            Executable::Block(block) => statement_paths(&block.body, out),
            Executable::Conditional(conditional) => conditional_paths(conditional, out),
            Executable::ForEach(for_each) => for_each_paths(for_each, out),
        }
    }
}

fn conditional_paths<'a>(conditional: &'a Conditional, out: &mut Vec<&'a Call>) {
    expression_paths(&conditional.condition, out);
    executable_paths(&conditional.then_body, out);
    if let Some(arm) = &conditional.else_body {
        executable_paths(&arm.body, out);
    }
}

fn for_each_paths<'a>(for_each: &'a ForEach, out: &mut Vec<&'a Call>) {
    expression_paths(&for_each.collection, out);
    executable_paths(&for_each.body, out);
}

fn body_paths<'a>(body: &'a Body, out: &mut Vec<&'a Call>) {
    match body {
        Body::Inline(InlineValue::Expression(expr)) => expression_paths(expr, out),
        Body::Inline(InlineValue::MultilineCollection(collection)) => {
            for member in &collection.members {
                expression_paths(member, out);
            }
        }
        Body::Nested(nested) => statement_paths(&nested.statements, out),
    }
}

/// Every `PATH` call inside one expression, walked with an explicit stack.
fn expression_paths<'a>(expr: &'a Expr, out: &mut Vec<&'a Call>) {
    let mut stack = vec![expr];
    while let Some(expr) = stack.pop() {
        match expr {
            Expr::Call(call) => {
                if call.callable.text == "PATH" {
                    out.push(call);
                }
                stack.extend(&call.arguments);
            }
            Expr::Collection(collection) => stack.extend(&collection.members),
            Expr::Group(group) => stack.push(&group.inner),
            Expr::Unary(unary) => stack.push(&unary.operand),
            Expr::Binary(binary) => {
                stack.push(&binary.left);
                stack.push(&binary.right);
            }
            Expr::Property(property) => stack.push(&property.base),
            Expr::Index(index) => {
                stack.push(&index.base);
                stack.push(&index.index);
            }
            Expr::Literal(_) | Expr::Identifier(_) | Expr::Type(_) => {}
        }
    }
}

/// Resolve `path` against `root` when it is relative.
fn join(root: &str, path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("{}/{}", root.trim_end_matches('/'), path)
    }
}

/// Is `candidate` the workspace root or a descendant of it?
///
/// Segment comparison after resolving `.` and `..`, never a textual prefix
/// test: "textual prefix alone does not establish containment".
pub(crate) fn contains(root: &str, candidate: &str) -> bool {
    let root = normalize(root);
    let candidate = normalize(candidate);
    let Some(root) = root else { return false };
    let Some(candidate) = candidate else {
        // A candidate that climbs above the filesystem root cannot be inside
        // any workspace.
        return false;
    };
    candidate.len() >= root.len() && candidate[..root.len()] == root[..]
}

/// Resolve a path into segments, applying `.` and `..`.
///
/// `None` when the path climbs above its own root, which is an escape by
/// construction rather than a path to compare.
fn normalize(path: &str) -> Option<Vec<&str>> {
    let mut out: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            other => out.push(other),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::contains;

    #[test]
    fn a_textual_prefix_is_not_containment() {
        assert!(!contains("/ws", "/ws-other/file"));
        assert!(!contains("/ws", "/wsx"));
    }

    #[test]
    fn a_descendant_is_contained_and_the_root_itself_is_too() {
        assert!(contains("/ws", "/ws"));
        assert!(contains("/ws", "/ws/a/b"));
        assert!(contains("/ws/", "/ws/a"));
    }

    #[test]
    fn a_resolved_escape_is_not_contained() {
        assert!(!contains("/ws", "/ws/a/../../etc"));
        assert!(!contains("/ws", "/etc/passwd"));
        assert!(!contains("/ws", "/ws/../ws2"));
    }

    #[test]
    fn interior_dot_segments_resolve_before_comparison() {
        assert!(contains("/ws", "/ws/a/../b"));
        assert!(contains("/ws", "/ws/./a"));
    }

    #[test]
    fn climbing_above_the_filesystem_root_is_never_contained() {
        assert!(!contains("/ws", "/../ws"));
    }
}
