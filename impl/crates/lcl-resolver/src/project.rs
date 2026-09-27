//! Core 0.3.0 projects: one specification over an entry and its declared parts.
//!
//! `05_SEMANTICS/13_PROJECTS_PARTS_AND_PROJECT_ADMISSION.txt`, mirroring
//! `10_REGISTRIES/block_schemas_v0.3.0.json#/project_contract`:
//!
//! > The project parts are exactly the source units the entry names with PART,
//! > in PART source order.
//!
//! > The entry and its parts share one project namespace, as if their top-level
//! > declarations were written in one document in project source order.
//!
//! This module decides which units are parts, in which order, and whether the
//! project namespace is complete. It adds each obtained part to the program
//! under the entry's own (empty) namespace prefix chain, so the existing
//! declaration index, duplicate-ID check and reference binder treat the whole
//! project as one declaring document without a second mechanism. Nothing here
//! enumerates: a part enters only because a `PART` names it, through the same
//! [`SourceProvider`] every import uses.
//!
//! A package that registers no project diagnostics (Core 0.1.0 and 0.2.0) has
//! no `kind.project` and no `PART`, and every function here returns without
//! effect for it: [`Rules::supports_projects`] gates them all.

use lcl_lexer::Span;
use lcl_parser::syntax::TopLevel;
use std::collections::BTreeSet;

use crate::field;
use crate::imports::{check_lcl_version, declared_kind, declared_specification_version};
use crate::source::{SourceId, SourceProvider, SourceRef, SourceRequest};
use crate::{Diagnostic, Emitter, ResolutionError, Resolved, Resolver, Rules, UnitPath};

/// The kind of a project entry.
pub const PROJECT_KIND: &str = "kind.project";

/// What became of one `PART` declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PartState {
    /// Obtained, admitted by every part rule, and in the project namespace.
    Loaded,
    /// Optional and absent: "omitted: it contributes no source unit,
    /// declaration or diagnostic, and the evaluation records the omission."
    Omitted,
    /// Required, or optional but present-and-unobtainable:
    /// `error.project.part_missing`.
    Missing,
    /// Obtained but rejected: an earlier stage, its LCL version, its kind,
    /// its placement or its SPECIFICATION VERSION.
    Rejected,
    /// Names a source unit an earlier PART already names:
    /// `error.project.part_duplicate`.
    Duplicate,
}

impl PartState {
    pub fn as_str(self) -> &'static str {
        match self {
            PartState::Loaded => "loaded",
            PartState::Omitted => "omitted",
            PartState::Missing => "missing",
            PartState::Rejected => "rejected",
            PartState::Duplicate => "duplicate",
        }
    }
}

/// One `PART` declaration of the entry, in PART source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPart {
    /// The PART's own `ID`.
    pub id: String,
    /// The relative path its `SOURCE` names, exactly as written.
    pub source: String,
    /// Locus of the `SOURCE` value in the entry.
    pub source_span: Span,
    /// The part kind its `KIND` names: the part's role.
    pub kind: String,
    /// `REQUIRED`, TRUE when omitted.
    pub required: bool,
    /// The unit the source resolved to, when it was obtained.
    pub unit: Option<SourceId>,
    pub state: PartState,
}

/// The project a `kind.project` evaluation root declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The entry: the evaluation root.
    pub entry: SourceId,
    /// Every `PART`, in PART source order.
    pub parts: Vec<ProjectPart>,
    /// True when every required part was obtained and every obtained part was
    /// admitted: the project namespace is resolved only when it is complete.
    pub complete: bool,
}

impl Project {
    /// The units that joined the project namespace, in project source order,
    /// the entry first.
    pub fn units(&self) -> impl Iterator<Item = &SourceId> {
        std::iter::once(&self.entry).chain(
            self.parts
                .iter()
                .filter(|p| p.state == PartState::Loaded)
                .filter_map(|p| p.unit.as_ref()),
        )
    }

    /// True when `unit` is the entry or a loaded part.
    pub fn contains(&self, unit: &SourceId) -> bool {
        self.units().any(|u| u == unit)
    }
}

/// The declared `SPECIFICATION` `KIND` of the root, when it has one.
fn root_kind(resolved: &Resolved) -> Option<(String, Span)> {
    declared_kind(resolved, &resolved.root)
}

/// `error.project.placement` for a root that is a project part:
/// "A document whose KIND is a project part kind is legal only as a part of
/// the evaluation root's project; as the evaluation root ... it uses
/// error.project.placement at its SPECIFICATION KIND value."
pub(crate) fn check_root(resolver: &Resolver<'_>, resolved: &Resolved, raw: &mut Vec<Diagnostic>) {
    if !resolver.rules().supports_projects() {
        return;
    }
    let Some((kind, span)) = root_kind(resolved) else {
        return;
    };
    if resolver.rules().is_project_part_kind(&kind) {
        Emitter::new(resolver.rules(), &resolved.units).emit(
            raw,
            ResolutionError::ProjectPlacement,
            &resolved.root,
            span,
            "part-as-root",
            format!(
                "a {kind} document is a project part and is legal only through its project entry"
            ),
        );
    }
}

/// Load every `PART` of a `kind.project` root, in PART source order.
///
/// Called after the root's LCL version is accepted and before any import is
/// expanded, so `resolved.order` and `resolved.paths` are in project source
/// order: the entry, then each part, then imported units.
pub(crate) fn load_parts(
    resolver: &Resolver<'_>,
    provider: &dyn SourceProvider,
    resolved: &mut Resolved,
    raw: &mut Vec<Diagnostic>,
) {
    let rules = resolver.rules();
    if !rules.supports_projects() {
        return;
    }
    match root_kind(resolved) {
        Some((kind, _)) if kind == PROJECT_KIND => {}
        _ => return,
    }
    let root = resolved.root.clone();
    let entry_version = declared_specification_version(resolved, &root).map(|(v, _)| v);
    let declarations = part_declarations(resolved);

    let mut project = Project {
        entry: root.clone(),
        parts: Vec::new(),
        complete: true,
    };
    let mut named: BTreeSet<SourceId> = BTreeSet::new();

    for declaration in declarations {
        let mut part = ProjectPart {
            id: declaration.id,
            source: declaration.path.clone(),
            source_span: declaration.source_span,
            kind: declaration.kind.clone(),
            required: declaration.required,
            unit: None,
            state: PartState::Loaded,
        };
        let request = SourceRequest {
            origin: root.clone(),
            reference: SourceRef::Path(declaration.path.clone()),
            span: declaration.source_span,
        };
        let loaded = match provider.load(&request) {
            Ok(unit) => unit,
            // "A part declared REQUIRED FALSE whose source is absent ... is
            // omitted". Absence is the host's report that nothing exists at
            // the resolved path; every other failure is part_missing.
            Err(error) if error.is_absent() && !declaration.required => {
                part.state = PartState::Omitted;
                project.parts.push(part);
                continue;
            }
            Err(error) => {
                Emitter::new(rules, &resolved.units).emit(
                    raw,
                    ResolutionError::ProjectPartMissing,
                    &root,
                    declaration.source_span,
                    &format!("part-missing:{}", declaration.path),
                    format!(
                        "the part {:?} cannot be obtained: {error}",
                        declaration.path
                    ),
                );
                project.complete = false;
                part.state = PartState::Missing;
                project.parts.push(part);
                continue;
            }
        };
        let unit_id = loaded.id().clone();
        part.unit = Some(unit_id.clone());

        // The entry named as its own part is a kind.project document occurring
        // other than as the evaluation root.
        if unit_id == root {
            let (kind, span) = root_kind(resolved).expect("the root declared kind.project");
            Emitter::new(rules, &resolved.units).emit(
                raw,
                ResolutionError::ProjectPlacement,
                &root,
                span,
                "entry-as-part",
                format!("the entry, a {kind} document, is named as one of its own parts"),
            );
            project.complete = false;
            part.state = PartState::Rejected;
            project.parts.push(part);
            continue;
        }
        if !named.insert(unit_id.clone()) {
            Emitter::new(rules, &resolved.units).emit(
                raw,
                ResolutionError::ProjectPartDuplicate,
                &root,
                declaration.source_span,
                &format!("part-duplicate:{unit_id}"),
                format!("{unit_id} is already a part of this project"),
            );
            part.state = PartState::Duplicate;
            project.parts.push(part);
            continue;
        }

        let staged = resolver.stage(&loaded);
        resolved.order.push(unit_id.clone());
        resolved.units.insert(unit_id.clone(), staged);

        // "Every project unit declares LCL VERSION 0.3.0; a part declaring
        // another LCL version uses error.version.unsupported."
        let supported = check_lcl_version(resolver, resolved, raw, &unit_id);
        let usable = resolved
            .units
            .get(&unit_id)
            .is_some_and(crate::ResolvedUnit::is_usable);
        if !supported || !usable {
            project.complete = false;
            part.state = PartState::Rejected;
            project.parts.push(part);
            continue;
        }

        let emitter = Emitter::new(rules, &resolved.units);
        let (kind, kind_span) =
            declared_kind(resolved, &unit_id).unwrap_or_else(|| (String::new(), Span::empty(0)));
        let problem = if kind == PROJECT_KIND {
            Some((
                ResolutionError::ProjectPlacement,
                kind_span,
                "project-as-part",
                format!("{unit_id} is a kind.project document; a project cannot be a part"),
            ))
        } else if kind != declaration.kind {
            Some((
                ResolutionError::ProjectPartKind,
                kind_span,
                "part-kind",
                format!(
                    "{unit_id} declares KIND {kind}, but its PART declares {}",
                    declaration.kind
                ),
            ))
        } else {
            match declared_specification_version(resolved, &unit_id) {
                Some((version, span)) if Some(&version) != entry_version.as_ref() => Some((
                    ResolutionError::VersionMismatch,
                    span,
                    "part-version",
                    format!(
                        "{unit_id} declares SPECIFICATION VERSION {version:?}, not the entry's {:?}",
                        entry_version.clone().unwrap_or_default()
                    ),
                )),
                _ => None,
            }
        };
        if let Some((error, span, cause, detail)) = problem {
            emitter.emit(
                raw,
                error,
                &unit_id,
                span,
                &format!("{cause}:{unit_id}"),
                detail,
            );
            project.complete = false;
            part.state = PartState::Rejected;
            project.parts.push(part);
            continue;
        }

        // The part joins the one project namespace: the entry's own empty
        // prefix chain, so its IDs are unqualified project IDs.
        resolved.paths.push(UnitPath::root(unit_id));
        project.parts.push(part);
    }

    resolved.project = Some(project);
}

/// One `PART` block of the entry, read after the grammar stage accepted it.
struct PartDeclaration {
    id: String,
    path: String,
    source_span: Span,
    kind: String,
    required: bool,
}

/// Every `PART` of the root, in source order. The grammar stage already
/// required ID, SOURCE and KIND, a registered part kind, and the exact
/// `PATH("relative")` form, so a block missing any of them never reaches
/// resolution.
fn part_declarations(resolved: &Resolved) -> Vec<PartDeclaration> {
    let Some(document) = resolved
        .units
        .get(&resolved.root)
        .and_then(|u| u.document())
    else {
        return Vec::new();
    };
    document
        .items
        .iter()
        .filter_map(|item| match item {
            TopLevel::Block(block) if block.key.text == "PART" => {
                let path = field::expression(block, "SOURCE")
                    .and_then(field::constructor_string)
                    .filter(|(callable, _)| *callable == "PATH")
                    .map(|(_, path)| path.to_string())?;
                Some(PartDeclaration {
                    id: field::identifier(block, "ID").map(|(id, _)| id)?,
                    path,
                    source_span: field::field_or_header_span(block, "SOURCE"),
                    kind: field::identifier(block, "KIND").map(|(kind, _)| kind)?,
                    required: field::boolean(block, "REQUIRED").unwrap_or(true),
                })
            }
            _ => None,
        })
        .collect()
}

/// True when a loaded document is a `kind.project` or a project part: as an
/// `IMPORT` or `EXTENSION` source it is misplaced.
pub(crate) fn misplaced_import(rules: &Rules, kind: &str) -> bool {
    rules.supports_projects() && (kind == PROJECT_KIND || rules.is_project_part_kind(kind))
}
