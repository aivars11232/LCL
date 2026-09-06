//! Import, extension, version, checksum and namespace records.
//!
//! `04_GRAMMAR/05`: "IMPORT/EXTENSION require ID, SOURCE, NAMESPACE, and exact
//! VERSION. URI sources also require CHECKSUM. Imported declarations resolve as
//! namespace.local_id. Unqualified REF resolves local namespace only. Wildcard
//! imports and implicit namespace merging are invalid. Cycles are invalid."
//!
//! The grammar stage already enforced that those fields are present and
//! well-shaped, including "URI source requires CHECKSUM" as a registered
//! conditional requirement. What is left is exactly what needs another
//! document to decide: does the source resolve, do the bytes match the
//! checksum, does the loaded specification carry the requested version, is the
//! loaded kind the right kind, and is the import graph acyclic.

use lcl_lexer::Span;
use std::collections::BTreeMap;

use crate::declarations::FullId;
use crate::source::{SourceId, SourceRef};

/// Whether a block imports a document or loads a vocabulary extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImportKind {
    /// `IMPORT`: load one exact LCL document under a mandatory namespace.
    Import,
    /// `EXTENSION`: load a versioned vocabulary extension that adds
    /// definitions only. `07_VERSIONING_AND_EXTENSIONS/03`.
    Extension,
}

impl ImportKind {
    pub fn block(self) -> &'static str {
        match self {
            ImportKind::Import => "IMPORT",
            ImportKind::Extension => "EXTENSION",
        }
    }
}

/// What happened to one `IMPORT` or `EXTENSION` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    /// The source resolved and every contract held. The unit is loaded under
    /// the named identity.
    Loaded(SourceId),
    /// The provider could not supply the source: `error.import.not_found`.
    NotFound,
    /// The bytes did not match `CHECKSUM`: `error.import.checksum`.
    ChecksumMismatch,
    /// Loading this source would close a cycle: `error.import.cycle`.
    Cycle,
    /// The block itself was rejected before a request was made, e.g. an
    /// invalid namespace. No source was requested.
    NotRequested,
}

/// One resolved `IMPORT` or `EXTENSION` declaration.
///
/// `REQUIRED` is recorded but not acted on. No canonical rule makes a failed
/// import tolerable: `error.import.not_found` is registered unconditionally as
/// "IMPORT or EXTENSION source cannot be resolved", and
/// `07_VERSIONING_AND_EXTENSIONS/05` forbids an ignore-unknown mode for
/// normative content. A later layer that is given a rule for optionality has
/// the flag it would need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    pub kind: ImportKind,
    /// The unit whose block this is.
    pub origin: SourceId,
    /// The block's own `ID`.
    pub id: FullId,
    /// The declared namespace prefix.
    pub namespace: String,
    /// Locus of the `NAMESPACE` value.
    pub namespace_span: Span,
    /// The `SOURCE` value, when it names another unit.
    pub reference: Option<SourceRef>,
    /// Locus of the `SOURCE` value.
    pub source_span: Span,
    /// The requested exact `VERSION`.
    pub version: String,
    /// Locus of the `VERSION` value.
    pub version_span: Span,
    /// The declared `CHECKSUM`, if any.
    pub checksum: Option<String>,
    /// `REQUIRED`, recorded for later layers. See the type documentation.
    pub required: Option<bool>,
    /// What happened.
    pub outcome: ImportOutcome,
}

impl ImportRecord {
    /// The loaded unit, when one was loaded.
    pub fn loaded(&self) -> Option<&SourceId> {
        match &self.outcome {
            ImportOutcome::Loaded(id) => Some(id),
            _ => None,
        }
    }
}

/// Which declaration owns one namespace prefix in one unit.
///
/// `07_VERSIONING_AND_EXTENSIONS/02`: "An IMPORT or EXTENSION namespace owns
/// its complete first identifier segment in the importing document. No local
/// declaration ID or second imported namespace may occupy that segment."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceOwner {
    pub kind: ImportKind,
    /// The owning block's `ID`.
    pub id: FullId,
    /// Locus of the owning `NAMESPACE` value.
    pub span: Span,
    /// The unit loaded under this prefix, when it loaded.
    pub unit: Option<SourceId>,
}

/// One unit reached by one exact import path.
///
/// Identity is per *path*, not per unit. `07_VERSIONING_AND_EXTENSIONS/02`:
/// "Distinct acyclic import paths do not override one another", and nested
/// imports "prepend each prefix in order". A library reached through two
/// different prefixes therefore contributes two sets of fully qualified IDs,
/// while remaining one loaded unit with one digest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitPath {
    /// The loaded unit.
    pub unit: SourceId,
    /// Importing prefixes, outermost first. Empty for the root.
    pub prefixes: Vec<String>,
}

impl UnitPath {
    pub fn root(unit: SourceId) -> Self {
        UnitPath {
            unit,
            prefixes: Vec::new(),
        }
    }

    /// The identity an ID of this unit takes on.
    pub fn qualify(&self, local: &str) -> FullId {
        FullId {
            namespace_path: self.prefixes.clone(),
            local: local.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Phase C: loading explicitly referenced source units
// ---------------------------------------------------------------------------

use crate::field;
use crate::source::{SourceProvider, SourceRequest, SourceUnit};
use crate::{Diagnostic, Emitter, ResolutionError, Resolved, Resolver};
use lcl_parser::syntax::{Block, TopLevel};

/// One `IMPORT` or `EXTENSION` block, read out of the syntax tree.
struct ImportSpec {
    kind: ImportKind,
    id: String,
    namespace: Option<(String, Span)>,
    version: Option<(String, Span)>,
    checksum: Option<(String, Span)>,
    required: Option<bool>,
    reference: Option<SourceRef>,
    source_span: Span,
}

fn read_spec(block: &Block, kind: ImportKind) -> ImportSpec {
    let reference = field::expression(block, "SOURCE")
        .and_then(field::constructor_string)
        .and_then(|(callable, argument)| match callable {
            "PATH" => Some(SourceRef::Path(argument.to_string())),
            "URI" => Some(SourceRef::Uri(argument.to_string())),
            _ => None,
        });
    ImportSpec {
        kind,
        id: field::identifier(block, "ID")
            .map(|(t, _)| t)
            .unwrap_or_default(),
        namespace: field::identifier(block, "NAMESPACE"),
        version: field::string(block, "VERSION"),
        checksum: field::string(block, "CHECKSUM"),
        required: field::boolean(block, "REQUIRED"),
        reference,
        source_span: field::field_or_header_span(block, "SOURCE"),
    }
}

/// Every `IMPORT` and `EXTENSION` block of one unit, in source order.
fn specs_of(resolved: &Resolved, unit: &SourceId) -> Vec<ImportSpec> {
    let Some(document) = resolved.units.get(unit).and_then(|u| u.document()) else {
        return Vec::new();
    };
    document
        .items
        .iter()
        .filter_map(|item| match item {
            TopLevel::Block(b) if b.key.text == "IMPORT" => Some(read_spec(b, ImportKind::Import)),
            TopLevel::Block(b) if b.key.text == "EXTENSION" => {
                Some(read_spec(b, ImportKind::Extension))
            }
            _ => None,
        })
        .collect()
}

/// The declared `LCL.VERSION` of one unit, with its span.
fn declared_lcl_version(resolved: &Resolved, unit: &SourceId) -> Option<(String, Span)> {
    let document = resolved.units.get(unit).and_then(|u| u.document())?;
    let block = document.block("LCL")?;
    field::string(block, "VERSION")
}

/// The declared `SPECIFICATION` `VERSION` and `KIND` of one unit.
fn declared_specification(
    resolved: &Resolved,
    unit: &SourceId,
) -> (Option<String>, Option<String>) {
    let Some(document) = resolved.units.get(unit).and_then(|u| u.document()) else {
        return (None, None);
    };
    let Some(block) = document.block("SPECIFICATION") else {
        return (None, None);
    };
    (
        field::string(block, "VERSION").map(|(v, _)| v),
        field::identifier(block, "KIND").map(|(k, _)| k),
    )
}

/// Load every explicitly referenced source unit, depth first in declaration
/// order, enforcing namespace validity, checksums and acyclicity.
pub(crate) fn resolve_sources(
    resolver: &Resolver<'_>,
    provider: &dyn SourceProvider,
    resolved: &mut Resolved,
    raw: &mut Vec<Diagnostic>,
) {
    let root = resolved.root.clone();
    // The root's own language version is checked before anything is loaded on
    // its behalf: 07/05, "An interpreter validates only exact supported
    // language versions."
    if !check_lcl_version(resolver, resolved, raw, &root) {
        return;
    }
    let root_path = UnitPath::root(root.clone());
    let mut chain = vec![root];
    expand(resolver, provider, resolved, raw, &root_path, &mut chain);
}

/// `error.version.unsupported`, and whether the unit may take part in
/// resolution at all.
fn check_lcl_version(
    resolver: &Resolver<'_>,
    resolved: &mut Resolved,
    raw: &mut Vec<Diagnostic>,
    unit: &SourceId,
) -> bool {
    let expected = resolver.rules().lcl_version().to_string();
    let declared = declared_lcl_version(resolved, unit);
    let (found, span) = match declared {
        Some(pair) => pair,
        // The grammar stage guarantees an `LCL` block with a `STRING` VERSION
        // in every document that reached this stage.
        None => return true,
    };
    if found == expected {
        return true;
    }
    let emitter = Emitter::new(resolver.rules(), &resolved.units);
    emitter.emit(
        raw,
        ResolutionError::VersionUnsupported,
        unit,
        span,
        "lcl-version",
        format!("declared LCL version {found:?} is not the supported exact version {expected:?}"),
    );
    // Nothing later may reinterpret this unit under 0.1.0 semantics.
    if let Some(u) = resolved.units.get_mut(unit) {
        u.version_rejected = true;
    }
    false
}

/// Resolve one unit's imports, then recurse into each loaded child.
fn expand(
    resolver: &Resolver<'_>,
    provider: &dyn SourceProvider,
    resolved: &mut Resolved,
    raw: &mut Vec<Diagnostic>,
    path: &UnitPath,
    chain: &mut Vec<SourceId>,
) {
    let specs = specs_of(resolved, &path.unit);
    let mut owned: BTreeMap<String, FullId> = BTreeMap::new();

    for spec in specs {
        let id = path.qualify(&spec.id);
        let (namespace, namespace_span) = match &spec.namespace {
            Some((n, s)) => (n.clone(), *s),
            None => continue,
        };

        let mut record = ImportRecord {
            kind: spec.kind,
            origin: path.unit.clone(),
            id: id.clone(),
            namespace: namespace.clone(),
            namespace_span,
            reference: spec.reference.clone(),
            source_span: spec.source_span,
            version: spec.version.clone().map(|(v, _)| v).unwrap_or_default(),
            version_span: spec.version.map(|(_, s)| s).unwrap_or(namespace_span),
            checksum: spec.checksum.clone().map(|(c, _)| c),
            required: spec.required,
            outcome: ImportOutcome::NotRequested,
        };

        let emitter = Emitter::new(resolver.rules(), &resolved.units);

        // 07/02: "Namespace is mandatory, unique, lowercase, and not reserved."
        // "A prefix cannot be a reserved built-in namespace."
        if resolver.rules().is_reserved_namespace(&namespace) {
            emitter.emit(
                raw,
                ResolutionError::NamespaceInvalid,
                &path.unit,
                namespace_span,
                &format!("reserved-namespace:{namespace}"),
                format!(
                    "`{namespace}` is a reserved built-in namespace and cannot be claimed by an {}",
                    spec.kind.block()
                ),
            );
            resolved.imports.push(record);
            continue;
        }

        // "Two imports/extensions cannot own the same prefix ... Duplicate
        // namespace ownership also uses error.id.duplicate."
        if let Some(first) = owned.get(&namespace) {
            emitter.emit(
                raw,
                ResolutionError::IdDuplicate,
                &path.unit,
                namespace_span,
                &format!("duplicate-namespace:{namespace}"),
                format!("namespace `{namespace}` is already owned by `{first}`"),
            );
            resolved.imports.push(record);
            continue;
        }
        owned.insert(namespace.clone(), id.clone());

        // A SOURCE that names no document cannot be resolved.
        let Some(reference) = spec.reference.clone() else {
            emitter.emit(
                raw,
                ResolutionError::ImportNotFound,
                &path.unit,
                spec.source_span,
                &format!("unnamed-source:{}", id.qualified()),
                format!(
                    "{} SOURCE must name a document with PATH or URI",
                    spec.kind.block()
                ),
            );
            record.outcome = ImportOutcome::NotFound;
            register_namespace(resolved, path, &namespace, &record, None);
            resolved.imports.push(record);
            continue;
        };

        let request = SourceRequest {
            origin: path.unit.clone(),
            reference: reference.clone(),
            span: spec.source_span,
        };
        let loaded = match provider.load(&request) {
            Ok(unit) => unit,
            Err(error) => {
                emitter.emit(
                    raw,
                    ResolutionError::ImportNotFound,
                    &path.unit,
                    spec.source_span,
                    &format!("not-found:{reference}"),
                    format!("{reference} could not be resolved: {error}"),
                );
                record.outcome = ImportOutcome::NotFound;
                register_namespace(resolved, path, &namespace, &record, None);
                resolved.imports.push(record);
                continue;
            }
        };

        // 07/02: "Import cycles fail."
        if chain.contains(loaded.id()) {
            emitter.emit(
                raw,
                ResolutionError::ImportCycle,
                &path.unit,
                spec.source_span,
                &format!("cycle:{}", loaded.id()),
                format!(
                    "importing {} closes a cycle: {}",
                    loaded.id(),
                    chain
                        .iter()
                        .map(SourceId::to_string)
                        .collect::<Vec<_>>()
                        .join(" -> ")
                ),
            );
            record.outcome = ImportOutcome::Cycle;
            register_namespace(resolved, path, &namespace, &record, None);
            resolved.imports.push(record);
            continue;
        }

        // 07/02: "URI requires CHECKSUM. Checksum form is
        // algorithm:lowercase_hex; Core 0.1.0 recognizes only sha256."
        if let Some((declared, span)) = &spec.checksum {
            if !checksum_matches(declared, &loaded) {
                emitter.emit(
                    raw,
                    ResolutionError::ImportChecksum,
                    &path.unit,
                    *span,
                    &format!("checksum:{}", loaded.id()),
                    format!(
                        "{} has digest {}{}, which does not match the declared {declared}",
                        loaded.id(),
                        crate::rules::CHECKSUM_PREFIX,
                        loaded.digest()
                    ),
                );
                record.outcome = ImportOutcome::ChecksumMismatch;
                register_namespace(resolved, path, &namespace, &record, None);
                resolved.imports.push(record);
                continue;
            }
        }

        // Stage the unit once; a second import of the same unit reuses it.
        let loaded_id = loaded.id().clone();
        let first_load = !resolved.units.contains_key(&loaded_id);
        if first_load {
            let staged = resolver.stage(&loaded);
            resolved.order.push(loaded_id.clone());
            resolved.units.insert(loaded_id.clone(), staged);
            check_lcl_version(resolver, resolved, raw, &loaded_id);
        }

        record.outcome = ImportOutcome::Loaded(loaded_id.clone());
        register_namespace(resolved, path, &namespace, &record, Some(loaded_id.clone()));
        resolved.imports.push(record);

        let child = UnitPath {
            unit: loaded_id.clone(),
            prefixes: {
                let mut p = path.prefixes.clone();
                p.push(namespace.clone());
                p
            },
        };
        if resolved.paths.contains(&child) {
            continue;
        }
        let usable = resolved
            .units
            .get(&loaded_id)
            .is_some_and(crate::ResolvedUnit::is_usable);
        resolved.paths.push(child.clone());
        if usable {
            chain.push(loaded_id);
            expand(resolver, provider, resolved, raw, &child, chain);
            chain.pop();
        }
    }
}

fn register_namespace(
    resolved: &mut Resolved,
    path: &UnitPath,
    namespace: &str,
    record: &ImportRecord,
    unit: Option<SourceId>,
) {
    resolved.namespaces.insert(
        (path.unit.clone(), namespace.to_string()),
        NamespaceOwner {
            kind: record.kind,
            id: record.id.clone(),
            span: record.namespace_span,
            unit,
        },
    );
}

/// Exact `sha256:<64 lowercase hex>` comparison against the loaded bytes.
fn checksum_matches(declared: &str, loaded: &SourceUnit) -> bool {
    let Some(hex) = declared.strip_prefix(crate::rules::CHECKSUM_PREFIX) else {
        return false;
    };
    if hex.len() != crate::rules::CHECKSUM_HEX_LEN
        || !hex
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    {
        return false;
    }
    hex == loaded.digest()
}

/// Contracts that need the declaration index: import versions, the extension
/// contract, and namespace ownership against local declarations.
pub(crate) fn check_contracts(
    resolver: &Resolver<'_>,
    resolved: &mut Resolved,
    raw: &mut Vec<Diagnostic>,
) {
    let records = resolved.imports.clone();
    for record in &records {
        let Some(loaded) = record.loaded().cloned() else {
            continue;
        };
        if !resolved
            .units
            .get(&loaded)
            .is_some_and(crate::ResolvedUnit::is_usable)
        {
            continue;
        }
        let (version, kind) = declared_specification(resolved, &loaded);
        let emitter = Emitter::new(resolver.rules(), &resolved.units);

        // 07/02: "IMPORT VERSION must equal imported SPECIFICATION.VERSION."
        if let Some(found) = version {
            if found != record.version {
                emitter.emit(
                    raw,
                    ResolutionError::VersionMismatch,
                    &record.origin,
                    record.version_span,
                    &format!("import-version:{loaded}"),
                    format!(
                        "{loaded} declares SPECIFICATION VERSION {found:?}, not the requested {:?}",
                        record.version
                    ),
                );
            }
        }

        // 07/03: "An extension is kind.extension and may contain IMPORT,
        // DEFINE, DATA, COMMENT, and EXAMPLE only."
        if record.kind == ImportKind::Extension {
            check_extension(resolver, resolved, raw, record, &loaded, kind.as_deref());
        }
    }

    check_prefix_ownership(resolver, resolved, raw);
}

fn check_extension(
    resolver: &Resolver<'_>,
    resolved: &Resolved,
    raw: &mut Vec<Diagnostic>,
    record: &ImportRecord,
    loaded: &SourceId,
    kind: Option<&str>,
) {
    let emitter = Emitter::new(resolver.rules(), &resolved.units);
    if kind != Some("kind.extension") {
        emitter.emit(
            raw,
            ResolutionError::ExtensionInvalid,
            &record.origin,
            record.source_span,
            &format!("extension-kind:{loaded}"),
            format!(
                "{loaded} declares KIND {}, but an EXTENSION must load a kind.extension document",
                kind.unwrap_or("nothing")
            ),
        );
        return;
    }
    // The document kind already restricts top-level blocks at the grammar
    // stage, so a kind.extension document cannot contain a forbidden block and
    // still reach here. The check is kept because the extension contract is
    // stated independently of the document-kind table, and a future registry
    // that relaxed one without the other must still fail closed.
    let Some(document) = resolved.units.get(loaded).and_then(|u| u.document()) else {
        return;
    };
    for block in document.blocks() {
        let name = block.key.text.as_str();
        if name == "LCL" || name == "SPECIFICATION" {
            continue;
        }
        if !resolver.rules().extension_permits(name) {
            emitter.emit(
                raw,
                ResolutionError::ExtensionInvalid,
                loaded,
                block.key.span,
                &format!("extension-block:{name}"),
                format!("`{name}` is not permitted in a definition-only extension"),
            );
        }
    }
}

/// 07/02: "An IMPORT or EXTENSION namespace owns its complete first identifier
/// segment in the importing document. No local declaration ID or second
/// imported namespace may occupy that segment; for example, namespace library
/// and a local ID library.value collide and use error.id.duplicate."
fn check_prefix_ownership(
    resolver: &Resolver<'_>,
    resolved: &mut Resolved,
    raw: &mut Vec<Diagnostic>,
) {
    let emitter = Emitter::new(resolver.rules(), &resolved.units);
    for decl in resolved.declarations.all() {
        let first = decl.id.first_segment();
        let key = (decl.source.clone(), first.to_string());
        let Some(owner) = resolved.namespaces.get(&key) else {
            continue;
        };
        emitter.emit(
            raw,
            ResolutionError::IdDuplicate,
            &decl.source,
            decl.id_span,
            &format!("prefix-collision:{first}"),
            format!(
                "`{}` occupies the first identifier segment `{first}`, which the {} `{}` owns",
                decl.id.local,
                owner.kind.block(),
                owner.id
            ),
        );
    }
}
