//! The closed resolution vocabulary of LCL Core 0.1.0, loaded as data.
//!
//! Nothing here transcribes a normative table. The reserved namespaces, the
//! core operation identifiers, the reference-target domains, the exact
//! supported LCL version and every reference slot's legal target set are read
//! out of the verified package, and [`Rules::load`] refuses any package that is
//! not the approved release, exactly as `Lexicon::load` and `Grammar::load` do.
//!
//! ## Where a reference slot's legal targets come from
//!
//! `field_signatures_v0.1.0.json#/value_kind_templates` gives each templated
//! kind an `argument_kind`. The four reference templates carry
//! `reference_target_union` or `block_name`, so `reference(TASK|PHASE|SEQUENCE|
//! ACTION|TEST)` *is* the legal target set for `EXECUTE.REFERENCE`, written in
//! the registry. A union member is either a block name or one of the
//! `reference_domains` of `semantic_meta_types_v0.1.0.json`, which expand to
//! their exact members.
//!
//! Four further value kinds constrain a reference without using a template.
//! Their constraints are stated in `#/value_kind_registry` as prose, so each
//! derivation below quotes the exact sentence it depends on and [`Rules::load`]
//! **verifies that sentence is still present**. A registry that no longer says
//! it refuses to load rather than being silently reinterpreted.

use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_parser::Grammar;
use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::diagnostic::ResolutionError;

/// The checksum algorithm Core 0.1.0 recognizes, with its separator.
///
/// `07_VERSIONING_AND_EXTENSIONS/02`: "Checksum form is
/// algorithm:lowercase_hex; Core 0.1.0 recognizes only sha256."
/// `#/value_kind_registry/sha256_string`: "A STRING containing 'sha256:'
/// followed by exactly 64 lowercase hexadecimal digits."
pub const CHECKSUM_PREFIX: &str = "sha256:";

/// Length of a SHA-256 digest in lowercase hexadecimal digits.
pub const CHECKSUM_HEX_LEN: usize = 64;

/// What a reference in one slot is allowed to resolve to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefTarget {
    /// A declaration written as this block, e.g. `ACTION`.
    Block(String),
    /// A `DEFINE` declaration whose `KIND` is exactly this, e.g. `kind.type`.
    Definition(String),
}

impl fmt::Display for RefTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefTarget::Block(b) => f.write_str(b),
            RefTarget::Definition(k) => write!(f, "DEFINE {k}"),
        }
    }
}

/// One field slot that receives a reference identity.
#[derive(Debug, Clone)]
pub struct ReferenceSlot {
    /// The registered `value_kind` expression, verbatim.
    pub value_kind: String,
    /// Legal targets. Empty means "any declaration": the bare `reference` kind
    /// is defined as "REF(identifier) resolving exactly once", with no target
    /// restriction.
    pub targets: BTreeSet<RefTarget>,
}

impl ReferenceSlot {
    /// True when this slot places no kind restriction on its target.
    pub fn accepts_any(&self) -> bool {
        self.targets.is_empty()
    }

    pub fn accepts(&self, block: &str, definition_kind: Option<&str>) -> bool {
        if self.accepts_any() {
            return true;
        }
        self.targets.iter().any(|t| match t {
            RefTarget::Block(b) => b == block,
            RefTarget::Definition(k) => {
                block == "DEFINE" && definition_kind.is_some_and(|d| d == k)
            }
        })
    }

    /// The legal targets, rendered for a diagnostic detail.
    pub fn describe(&self) -> String {
        if self.accepts_any() {
            return "any declaration".to_string();
        }
        self.targets
            .iter()
            .map(RefTarget::to_string)
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// Registry facts about one resolution-stage error identifier.
#[derive(Debug, Clone)]
pub struct RegisteredResolutionError {
    pub id: ResolutionError,
    pub meaning: String,
    pub default_status: String,
    pub specificity_rank: u64,
    pub supersedes: BTreeSet<ResolutionError>,
}

#[derive(Debug)]
pub enum RulesLoadError {
    /// The package did not establish authority. See [`lcl_spec::Authority`].
    UnverifiedPackage(lcl_spec::Authority),
    MissingRegistry(&'static str),
    Malformed(String),
    Diagnostics(lcl_diagnostics::DiagnosticsError),
    /// The registry's resolution error set is not the set this build mirrors.
    ResolutionErrorSetMismatch {
        missing_from_build: Vec<String>,
        missing_from_registry: Vec<String>,
    },
    /// A reference target this build cannot map to a block or a registered
    /// reference domain. Unknown material fails closed.
    UnknownReferenceTarget {
        value_kind: String,
        target: String,
    },
    /// A `#/value_kind_registry` sentence a derivation depends on is no longer
    /// present. The rule is not reinterpreted; the load refuses.
    RegistryTextChanged {
        value_kind: &'static str,
        expected_phrase: &'static str,
    },
    /// The exact supported LCL version disagrees between the block schema and
    /// the package's own formal version.
    VersionAuthorityConflict {
        schema: String,
        package: String,
    },
}

impl fmt::Display for RulesLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RulesLoadError::UnverifiedPackage(a) => write!(
                f,
                "refusing to build resolution rules from a {a} package: only the approved release is normative input"
            ),
            RulesLoadError::MissingRegistry(n) => write!(f, "registry {n:?} not loaded"),
            RulesLoadError::Malformed(m) => write!(f, "malformed registry data: {m}"),
            RulesLoadError::Diagnostics(e) => write!(f, "diagnostic registry: {e}"),
            RulesLoadError::ResolutionErrorSetMismatch {
                missing_from_build,
                missing_from_registry,
            } => write!(
                f,
                "resolution error set mismatch: registry has {missing_from_build:?} that this build does not mirror; this build has {missing_from_registry:?} that the registry does not register"
            ),
            RulesLoadError::UnknownReferenceTarget { value_kind, target } => write!(
                f,
                "value kind {value_kind:?} names reference target {target:?}, which is neither a block nor a registered reference domain: refusing to guess"
            ),
            RulesLoadError::RegistryTextChanged {
                value_kind,
                expected_phrase,
            } => write!(
                f,
                "value kind {value_kind:?} no longer states {expected_phrase:?}: refusing to apply a derivation the registry no longer supports"
            ),
            RulesLoadError::VersionAuthorityConflict { schema, package } => write!(
                f,
                "LCL.VERSION schema pins {schema:?} but the package's formal version is {package:?}"
            ),
        }
    }
}

impl std::error::Error for RulesLoadError {}

/// The closed resolution vocabulary of LCL Core 0.1.0.
pub struct Rules {
    lcl_version: String,
    reserved_namespaces: BTreeSet<String>,
    core_operation_ids: BTreeSet<String>,
    definition_kinds: BTreeSet<String>,
    document_kinds: BTreeSet<String>,
    reference_domains: BTreeMap<String, BTreeSet<String>>,
    /// `(block, field)` -> slot, for every field that receives a reference.
    reference_slots: BTreeMap<(String, String), ReferenceSlot>,
    /// `(block, field)` for every field whose value kind is an operation
    /// identifier.
    operation_slots: BTreeSet<(String, String)>,
    /// Blocks that declare an identity, i.e. carry an `ID` field.
    declaring_blocks: BTreeSet<String>,
    /// Blocks an extension document may contain.
    extension_blocks: BTreeSet<String>,
    /// Registered domain members per alias domain, for `DEFINE` BASE chains.
    alias_domains: BTreeMap<String, BTreeSet<String>>,
    errors: BTreeMap<ResolutionError, RegisteredResolutionError>,
    supersedes: BTreeMap<ResolutionError, BTreeSet<ResolutionError>>,
}

impl fmt::Debug for Rules {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rules")
            .field("lcl_version", &self.lcl_version)
            .field("reserved_namespaces", &self.reserved_namespaces.len())
            .field("core_operations", &self.core_operation_ids.len())
            .field("reference_slots", &self.reference_slots.len())
            .field("resolution_errors", &self.errors.len())
            .finish()
    }
}

/// A `#/value_kind_registry` sentence fragment each non-template derivation
/// depends on. Verified at load; see the module documentation.
const REGISTRY_ANCHORS: &[(&str, &str)] = &[
    (
        "type_expression",
        "a defined type is REF(identifier) resolving to DEFINE kind.type",
    ),
    (
        "schema_reference_or_nested_schema",
        "One REF resolving to a defined OBJECT type",
    ),
    (
        "string_uri_or_evidence_reference",
        "REF resolving to EVIDENCE",
    ),
    ("reference", "REF(identifier) resolving exactly once."),
    (
        "operation_identifier",
        "A core_operation_ids member or the identifier of a DEFINE declaration whose KIND is kind.operation.",
    ),
    ("sha256_string", "followed by exactly 64 lowercase hexadecimal digits"),
];

impl Rules {
    /// Build the resolution vocabulary from a verified package and the grammar
    /// already loaded from that same package.
    pub fn load(spec: &SpecPackage, grammar: &Grammar) -> Result<Self, RulesLoadError> {
        if !spec.is_authoritative() {
            return Err(RulesLoadError::UnverifiedPackage(spec.authority()));
        }

        let field_signatures = registry(spec, "field_signatures")?;
        let groups = registry(spec, "built_in_groups_and_results")?;
        let meta_types = registry(spec, "semantic_meta_types")?;

        for name in ["field_signatures", "semantic_meta_types"] {
            let r = registry(spec, name)?;
            if r.get("closed").and_then(Json::as_bool) != Some(true) {
                return Err(RulesLoadError::Malformed(format!(
                    "{name} registry does not declare closed: true"
                )));
            }
        }

        let value_kind_registry = field_signatures
            .get("value_kind_registry")
            .and_then(Json::as_object)
            .ok_or_else(|| {
                RulesLoadError::Malformed("field_signatures.value_kind_registry missing".into())
            })?;
        for (value_kind, phrase) in REGISTRY_ANCHORS {
            let text = value_kind_registry
                .iter()
                .find(|(k, _)| k == value_kind)
                .and_then(|(_, v)| v.as_str())
                .unwrap_or_default();
            if !text.contains(phrase) {
                return Err(RulesLoadError::RegistryTextChanged {
                    value_kind,
                    expected_phrase: phrase,
                });
            }
        }

        let reserved_namespaces = string_set(groups, "reserved_namespaces")?;
        let core_operation_ids = string_set(groups, "core_operation_ids")?;
        let enum_groups = groups.get("enum_groups").ok_or_else(|| {
            RulesLoadError::Malformed("built_in_groups_and_results.enum_groups missing".into())
        })?;
        let definition_kinds = string_set(enum_groups, "definition_kinds")?;
        let document_kinds = string_set(enum_groups, "document_kinds")?;

        // Alias domains: the closed member sets a DEFINE kind.error / kind.event
        // / kind.status BASE may terminate in. `03_TYPES_AND_VALUES/05`: "BASE
        // names one core identifier of the same domain or one acyclic same-kind
        // alias."
        let mut alias_domains = BTreeMap::new();
        alias_domains.insert("kind.error".to_string(), string_set(enum_groups, "errors")?);
        alias_domains.insert("kind.event".to_string(), string_set(enum_groups, "events")?);
        alias_domains.insert(
            "kind.status".to_string(),
            string_set(enum_groups, "statuses")?,
        );
        alias_domains.insert(
            "kind.format".to_string(),
            string_set(enum_groups, "formats")?,
        );

        let mut reference_domains = BTreeMap::new();
        let domains = meta_types
            .get("reference_domains")
            .and_then(Json::as_object)
            .ok_or_else(|| {
                RulesLoadError::Malformed("semantic_meta_types.reference_domains missing".into())
            })?;
        for (name, body) in domains {
            let members = string_set(body, "members")?;
            reference_domains.insert(name.clone(), members);
        }

        // The exact supported LCL version, from the block schema that pins it.
        let lcl_version = grammar
            .schema("LCL")
            .and_then(|s| s.field("VERSION"))
            .map(|f| f.value_kind.clone())
            .ok_or_else(|| RulesLoadError::Malformed("LCL.VERSION signature missing".into()))?;
        let lcl_version = exact_string_argument(&lcl_version).ok_or_else(|| {
            RulesLoadError::Malformed(format!(
                "LCL.VERSION value kind {lcl_version:?} is not an exact_string"
            ))
        })?;
        if lcl_version != spec.formal_version() {
            return Err(RulesLoadError::VersionAuthorityConflict {
                schema: lcl_version,
                package: spec.formal_version().to_string(),
            });
        }

        // Reference and operation slots, from every block's field signatures.
        let mut reference_slots = BTreeMap::new();
        let mut operation_slots = BTreeSet::new();
        let mut declaring_blocks = BTreeSet::new();
        for schema in grammar.schemas() {
            if schema.field("ID").is_some() {
                declaring_blocks.insert(schema.name.clone());
            }
            for field in &schema.fields {
                if field.value_kind.starts_with("operation_identifier") {
                    operation_slots.insert((schema.name.clone(), field.name.clone()));
                }
                if let Some(targets) = reference_targets(&field.value_kind, &reference_domains)? {
                    reference_slots.insert(
                        (schema.name.clone(), field.name.clone()),
                        ReferenceSlot {
                            value_kind: field.value_kind.clone(),
                            targets,
                        },
                    );
                }
            }
        }

        let extension_blocks = grammar
            .document_kind_blocks("kind.extension")
            .cloned()
            .ok_or_else(|| {
                RulesLoadError::Malformed("document_kind_blocks has no kind.extension".into())
            })?;

        // Diagnostic metadata for the fourteen registered resolution errors.
        let registry = DiagnosticRegistry::load(spec).map_err(RulesLoadError::Diagnostics)?;
        let registered: BTreeMap<&str, &lcl_diagnostics::ErrorDef> = registry
            .errors_by_stage(Stage::Resolution)
            .into_iter()
            .map(|e| (e.id.as_str(), e))
            .collect();

        let mirrored: BTreeSet<&str> = ResolutionError::ALL
            .iter()
            .map(|e| e.as_registry_str())
            .collect();
        let registered_ids: BTreeSet<&str> = registered.keys().copied().collect();
        if mirrored != registered_ids {
            return Err(RulesLoadError::ResolutionErrorSetMismatch {
                missing_from_build: registered_ids
                    .difference(&mirrored)
                    .map(|s| (*s).to_string())
                    .collect(),
                missing_from_registry: mirrored
                    .difference(&registered_ids)
                    .map(|s| (*s).to_string())
                    .collect(),
            });
        }

        let selection = registry.selection_contract();
        let ranks = selection.get("specificity_rank");
        let default_rank = ranks
            .and_then(|r| r.get("default_for_every_error"))
            .and_then(Json::as_u64)
            .ok_or_else(|| {
                RulesLoadError::Malformed(
                    "diagnostic_selection.specificity_rank.default_for_every_error missing".into(),
                )
            })?;
        let rank_overrides = ranks
            .and_then(|r| r.get("overrides"))
            .and_then(Json::as_object)
            .unwrap_or(&[]);
        let supersede_overrides = selection
            .get("supersedes")
            .and_then(|s| s.get("overrides"))
            .and_then(Json::as_object)
            .unwrap_or(&[]);

        let mut errors = BTreeMap::new();
        let mut supersedes = BTreeMap::new();
        for id in ResolutionError::ALL {
            let key = id.as_registry_str();
            let def = registered.get(key).ok_or_else(|| {
                RulesLoadError::Malformed(format!("{key} vanished from registry"))
            })?;
            let rank = rank_overrides
                .iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_u64())
                .unwrap_or(default_rank);
            let edges: BTreeSet<ResolutionError> = supersede_overrides
                .iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, v)| v.as_array())
                .unwrap_or(&[])
                .iter()
                .filter_map(Json::as_str)
                .filter_map(ResolutionError::from_registry_str)
                .collect();
            if !edges.is_empty() {
                supersedes.insert(id, edges.clone());
            }
            errors.insert(
                id,
                RegisteredResolutionError {
                    id,
                    meaning: def.meaning.clone(),
                    default_status: def.default_status.clone(),
                    specificity_rank: rank,
                    supersedes: edges,
                },
            );
        }

        Ok(Rules {
            lcl_version,
            reserved_namespaces,
            core_operation_ids,
            definition_kinds,
            document_kinds,
            reference_domains,
            reference_slots,
            operation_slots,
            declaring_blocks,
            extension_blocks,
            alias_domains,
            errors,
            supersedes,
        })
    }

    /// The one exact LCL version this build resolves, from `LCL.VERSION`'s
    /// `exact_string` argument.
    pub fn lcl_version(&self) -> &str {
        &self.lcl_version
    }

    pub fn reserved_namespaces(&self) -> impl Iterator<Item = &str> {
        self.reserved_namespaces.iter().map(String::as_str)
    }

    pub fn is_reserved_namespace(&self, segment: &str) -> bool {
        self.reserved_namespaces.contains(segment)
    }

    pub fn is_core_operation(&self, id: &str) -> bool {
        self.core_operation_ids.contains(id)
    }

    pub fn core_operation_count(&self) -> usize {
        self.core_operation_ids.len()
    }

    pub fn is_definition_kind(&self, kind: &str) -> bool {
        self.definition_kinds.contains(kind)
    }

    pub fn is_document_kind(&self, kind: &str) -> bool {
        self.document_kinds.contains(kind)
    }

    pub fn reference_domain(&self, name: &str) -> Option<&BTreeSet<String>> {
        self.reference_domains.get(name)
    }

    pub fn reference_slot(&self, block: &str, field: &str) -> Option<&ReferenceSlot> {
        self.reference_slots
            .get(&(block.to_string(), field.to_string()))
    }

    pub fn reference_slot_count(&self) -> usize {
        self.reference_slots.len()
    }

    pub fn is_operation_slot(&self, block: &str, field: &str) -> bool {
        self.operation_slots
            .contains(&(block.to_string(), field.to_string()))
    }

    pub fn is_declaring_block(&self, block: &str) -> bool {
        self.declaring_blocks.contains(block)
    }

    pub fn declaring_blocks(&self) -> impl Iterator<Item = &str> {
        self.declaring_blocks.iter().map(String::as_str)
    }

    pub fn extension_permits(&self, block: &str) -> bool {
        self.extension_blocks.contains(block)
    }

    pub fn extension_blocks(&self) -> impl Iterator<Item = &str> {
        self.extension_blocks.iter().map(String::as_str)
    }

    /// Registered core members of one alias domain, e.g. `kind.status`.
    pub fn alias_domain(&self, definition_kind: &str) -> Option<&BTreeSet<String>> {
        self.alias_domains.get(definition_kind)
    }

    pub fn error(&self, id: ResolutionError) -> &RegisteredResolutionError {
        self.errors
            .get(&id)
            .expect("every mirrored identifier is registered; load verified the set")
    }

    pub(crate) fn supersedes(&self) -> &BTreeMap<ResolutionError, BTreeSet<ResolutionError>> {
        &self.supersedes
    }
}

fn registry<'a>(spec: &'a SpecPackage, name: &'static str) -> Result<&'a Json, RulesLoadError> {
    spec.registry(name)
        .ok_or(RulesLoadError::MissingRegistry(name))
}

fn string_set(parent: &Json, key: &str) -> Result<BTreeSet<String>, RulesLoadError> {
    let values = parent
        .get(key)
        .and_then(Json::as_array)
        .ok_or_else(|| RulesLoadError::Malformed(format!("{key} is not an array")))?;
    let mut out = BTreeSet::new();
    for v in values {
        let s = v
            .as_str()
            .ok_or_else(|| RulesLoadError::Malformed(format!("{key} holds a non-string member")))?;
        out.insert(s.to_string());
    }
    Ok(out)
}

/// The argument of `exact_string("...")`.
fn exact_string_argument(value_kind: &str) -> Option<String> {
    let rest = value_kind.strip_prefix("exact_string(\"")?;
    let arg = rest.strip_suffix("\")")?;
    Some(arg.to_string())
}

/// The legal targets of a reference-receiving value kind, or `None` when the
/// kind receives no reference identity.
fn reference_targets(
    value_kind: &str,
    domains: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Option<BTreeSet<RefTarget>>, RulesLoadError> {
    // The four non-template kinds. Each quotes `#/value_kind_registry`, whose
    // wording `Rules::load` has already verified.
    match value_kind {
        // "REF(identifier) resolving exactly once." — no target restriction.
        "reference" => return Ok(Some(BTreeSet::new())),
        // "a defined type is REF(identifier) resolving to DEFINE kind.type"
        "type_expression" => {
            return Ok(Some(
                [RefTarget::Definition("kind.type".into())]
                    .into_iter()
                    .collect(),
            ))
        }
        // "One REF resolving to a defined OBJECT type"
        "schema_reference_or_nested_schema" => {
            return Ok(Some(
                [RefTarget::Definition("kind.type".into())]
                    .into_iter()
                    .collect(),
            ))
        }
        // "One STRING, URI, or REF resolving to EVIDENCE."
        "string_uri_or_evidence_reference" => {
            return Ok(Some(
                [RefTarget::Block("EVIDENCE".into())].into_iter().collect(),
            ))
        }
        _ => {}
    }

    // `operation_identifier_or_handler_reference`: "One REF resolving to
    // HANDLER, or one operation_identifier ...". The operation alternative is
    // handled by the operation slot; the reference alternative targets HANDLER.
    if value_kind == "operation_identifier_or_handler_reference" {
        return Ok(Some(
            [RefTarget::Block("HANDLER".into())].into_iter().collect(),
        ));
    }

    let Some(argument) = reference_template_argument(value_kind) else {
        return Ok(None);
    };

    let mut targets = BTreeSet::new();
    for member in argument.split('|') {
        let member = member.trim();
        if let Some(domain) = domains.get(member) {
            targets.extend(domain.iter().map(|m| RefTarget::Block(m.clone())));
        } else if member.chars().all(|c| c.is_ascii_uppercase() || c == '_') && !member.is_empty() {
            targets.insert(RefTarget::Block(member.to_string()));
        } else {
            return Err(RulesLoadError::UnknownReferenceTarget {
                value_kind: value_kind.to_string(),
                target: member.to_string(),
            });
        }
    }
    Ok(Some(targets))
}

/// The `(...)` argument of one of the four reference templates.
fn reference_template_argument(value_kind: &str) -> Option<&str> {
    const TEMPLATES: [&str; 4] = [
        "reference_or_list_or_nested",
        "reference_or_list",
        "reference_or_nested",
        "reference",
    ];
    for template in TEMPLATES {
        if let Some(rest) = value_kind.strip_prefix(template) {
            return rest.strip_prefix('(').and_then(|r| r.strip_suffix(')'));
        }
    }
    None
}
