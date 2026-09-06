//! The closed grammar and schema vocabulary of LCL Core 0.1.0, loaded as data.
//!
//! Nothing in this module transcribes a normative table. `block_schemas` and
//! `field_signatures` are read out of the verified package, and [`Grammar::load`]
//! refuses any package that is not the approved release, exactly as
//! `Lexicon::load` does.
//!
//! ## What the grammar stage may decide about a value
//!
//! `error.field.type` is registered at `grammar_or_schema`, but most value
//! kinds name a *domain* rather than a *shape*. The registry itself draws the
//! line: `value_kind_templates` splits every templated expression into
//! `accepted_forms` — the syntactic shapes — and an `argument_kind` — the exact
//! string, integer range, domain, or reference target. This crate enforces the
//! forms and defers every argument.
//!
//! The canonical package proves the split is the intended one.
//! `08_EXAMPLES/INVALID/12_FLOATING_VERSION.invalid.lcl` writes
//! `VERSION: "latest"` where `LCL.VERSION` is `exact_string("0.1.0")`, and pins
//! `error.version.unsupported`, which the error registry stages at
//! **resolution**. The form (a `STRING`) is satisfied; the exact value is not a
//! grammar-stage question. `types_v0.1.0.json#/source_type_contract` maps the
//! same split for types: a malformed type expression is `error.field.type`,
//! while `unresolved`, `wrong_kind` and `cycle` are resolution-stage.
//!
//! So a form violation here is `error.field.type`; an argument violation
//! belongs to M3 resolution or M4 static checking and is not attempted.

use crate::diagnostic::GrammarError;
use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A syntactic shape a field value may take.
///
/// The named constants mirror `value_kind_templates.*.accepted_forms` plus the
/// literal token kinds the EBNF distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct FormSet(u16);

impl FormSet {
    /// A single-line `STRING` literal.
    pub const STRING: FormSet = FormSet(1 << 0);
    /// A `"""` literal.
    pub const MULTILINE_STRING: FormSet = FormSet(1 << 1);
    /// An `INTEGER_LITERAL`, with or without a leading `-`.
    pub const INTEGER: FormSet = FormSet(1 << 2);
    /// Exactly `TRUE` or `FALSE`.
    pub const BOOLEAN: FormSet = FormSet(1 << 3);
    /// A `SIMPLE_IDENTIFIER`: one lowercase segment, no `.`.
    pub const SIMPLE_IDENTIFIER: FormSet = FormSet(1 << 4);
    /// A `QUALIFIED_IDENTIFIER`: two or more dot-separated segments.
    pub const QUALIFIED_IDENTIFIER: FormSet = FormSet(1 << 5);
    /// One `REFERENCE_CALL`: `REF(identifier)`.
    pub const REFERENCE: FormSet = FormSet(1 << 6);
    /// A bracket literal whose every member is a `REFERENCE_CALL`.
    pub const REFERENCE_LIST: FormSet = FormSet(1 << 7);
    /// An indented body rather than an inline value.
    pub const NESTED: FormSet = FormSet(1 << 8);
    /// A `TYPE_EXPRESSION` in the sense of the EBNF: a non-null type
    /// expression, a `REFERENCE_CALL`, or `NULL`.
    pub const TYPE_EXPRESSION: FormSet = FormSet(1 << 9);
    /// Any inline `EXPRESSION`. Set by every inline value, so a value kind that
    /// accepts it imposes no grammar-stage shape at all.
    pub const EXPRESSION: FormSet = FormSet(1 << 10);

    pub const fn empty() -> FormSet {
        FormSet(0)
    }

    pub const fn union(self, other: FormSet) -> FormSet {
        FormSet(self.0 | other.0)
    }

    pub const fn intersects(self, other: FormSet) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn contains(self, other: FormSet) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Human names of the set members, for diagnostic detail only.
    pub fn names(self) -> Vec<&'static str> {
        const NAMES: [(FormSet, &str); 11] = [
            (FormSet::STRING, "STRING"),
            (FormSet::MULTILINE_STRING, "MULTILINE_STRING"),
            (FormSet::INTEGER, "INTEGER"),
            (FormSet::BOOLEAN, "BOOLEAN"),
            (FormSet::SIMPLE_IDENTIFIER, "SIMPLE_IDENTIFIER"),
            (FormSet::QUALIFIED_IDENTIFIER, "QUALIFIED_IDENTIFIER"),
            (FormSet::REFERENCE, "REF(identifier)"),
            (FormSet::REFERENCE_LIST, "list of REF(identifier)"),
            (FormSet::NESTED, "indented block"),
            (FormSet::TYPE_EXPRESSION, "TYPE_EXPRESSION"),
            (FormSet::EXPRESSION, "expression"),
        ];
        NAMES
            .iter()
            .filter(|(bit, _)| self.intersects(*bit))
            .map(|(_, n)| *n)
            .collect()
    }
}

/// Either identifier shape.
const ANY_IDENTIFIER: FormSet = FormSet::SIMPLE_IDENTIFIER.union(FormSet::QUALIFIED_IDENTIFIER);

/// `SCALAR_TYPE` of `04_GRAMMAR/10_COMPLETE_EBNF.ebnf`.
///
/// These EBNF productions are grammar, not registry tables, and the package
/// ships the grammar as text rather than as data. M1 set the precedent with
/// `TYPE_ARGUMENT_WORDS`: name the production here and cross-check it against
/// the shipped EBNF in `tests/ebnf_authority.rs`, so a grammar change fails a
/// test instead of drifting silently.
pub(crate) const SCALAR_TYPE_WORDS: [&str; 17] = [
    "STRING",
    "INTEGER",
    "DECIMAL",
    "BOOLEAN",
    "OBJECT",
    "ENUM",
    "PATH",
    "URI",
    "GLOB",
    "REGEX",
    "DATE",
    "TIME",
    "DATETIME",
    "DURATION",
    "PERCENTAGE",
    "BYTES",
    "MEASURE",
];

/// The four bracketed alternatives of `NON_NULL_TYPE_EXPRESSION`.
pub(crate) const BRACKET_TYPE_WORDS: [&str; 4] = ["LIST", "SET", "OBJECT", "REFERENCE"];

/// The word-shaped alternatives of `LITERAL`.
pub(crate) const LITERAL_WORDS: [&str; 5] = ["TRUE", "FALSE", "NULL", "MISSING", "UNKNOWN"];

/// How this build classifies one registered value kind's accepted shapes.
///
/// The templated kinds are derived from `value_kind_templates`; the named kinds
/// of `value_kind_registry` carry the canonical pointer that justifies their
/// classification. A kind absent from both fails the load rather than being
/// guessed, per the fail-closed rule for unregistered material.
fn named_kind_forms(name: &str) -> Option<FormSet> {
    // Shape-bearing named kinds. Each comment names the authority that makes
    // the shape decidable without resolution or typing.
    let forms = match name {
        // "One exact BOOLEAN value: TRUE or FALSE."
        "boolean" => FormSet::BOOLEAN,
        // "One single-line STRING value."
        "string" => FormSet::STRING,
        // "One STRING or MULTILINE_STRING value."
        "string_or_multiline_string" => FormSet::STRING.union(FormSet::MULTILINE_STRING),
        // "STRING matching MAJOR.MINOR.PATCH with no wildcard." The exact
        // spelling is resolution-stage: see the 12_FLOATING_VERSION proof.
        "semantic_version_string" => FormSet::STRING,
        // "A STRING containing 'sha256:' followed by 64 lowercase hex digits."
        // The content is a resolution-stage import check
        // (07_VERSIONING_AND_EXTENSIONS/02).
        "sha256_string" => FormSet::STRING,
        // "One lowercase identifier segment." A dotted spelling is a shape
        // violation the token stream already distinguishes.
        "simple_identifier" => FormSet::SIMPLE_IDENTIFIER,
        // "Lowercase dot-separated identifier resolved under namespace rules."
        // Namespace resolution is step 4, so only the identifier shape is
        // checked here.
        "qualified_identifier" => ANY_IDENTIFIER,
        // "One property path made of one or more SIMPLE_IDENTIFIER segments
        // separated by '.'."
        "property_path" => ANY_IDENTIFIER,
        // "One STRING or QUALIFIED_IDENTIFIER value."
        "string_or_qualified_identifier" => FormSet::STRING.union(ANY_IDENTIFIER),
        // "A core_operation_ids member or the identifier of a DEFINE whose KIND
        // is kind.operation." Membership is resolution-stage.
        "operation_identifier" => ANY_IDENTIFIER,
        // "One REF resolving to HANDLER, or one operation_identifier ..."
        "operation_identifier_or_handler_reference" => ANY_IDENTIFIER.union(FormSet::REFERENCE),
        // "One REF resolving to a defined OBJECT type, or a local nested
        // sequence of one or more FIELD blocks."
        "schema_reference_or_nested_schema" => FormSet::REFERENCE.union(FormSet::NESTED),
        // "One TYPE_EXPRESSION under types_v0.1.0.json#/source_type_contract."
        // 04_GRAMMAR/11 rule 8: "A bare identifier is never a type."
        "type_expression" => FormSet::TYPE_EXPRESSION,
        // For kind.type one TYPE_EXPRESSION; for kind.format/error/event/status
        // one qualified identifier in the corresponding domain. Which arm
        // applies depends on the sibling KIND, so both shapes are admitted and
        // the domain is left to resolution.
        "type_or_format_base" => FormSet::TYPE_EXPRESSION.union(ANY_IDENTIFIER),
        // "Inline value expression or indented lowercase-key object value."
        "value_or_object_expression" => FormSet::EXPRESSION.union(FormSet::NESTED),
        // Everything else names a value *family* — a material value, a boolean
        // result, a target, a selector, a source, an ordered value, a duration,
        // a path, a pattern, a numeric bound, an effect-class list, a
        // dependency-class list. Each admits any expression whose family is
        // decided by M4 static checking, so the grammar stage imposes no shape.
        "value_expression"
        | "boolean_expression"
        | "boolean_or_reference_list"
        | "target_expression"
        | "selector_expression"
        | "source_expression"
        | "ordered_value"
        | "duration"
        | "path"
        | "regex_or_glob"
        | "nonnegative_numeric_or_measure"
        | "side_effect_declaration"
        | "dependency_class_list"
        | "string_uri_or_evidence_reference"
        | "reference" => {
            if name == "reference" {
                FormSet::REFERENCE
            } else {
                FormSet::EXPRESSION
            }
        }
        _ => return None,
    };
    Some(forms)
}

/// The accepted forms of one template head, from
/// `value_kind_templates.<head>.accepted_forms`.
fn template_form(accepted: &str) -> Option<FormSet> {
    Some(match accepted {
        "STRING" => FormSet::STRING,
        "INTEGER" => FormSet::INTEGER,
        "QUALIFIED_IDENTIFIER" => ANY_IDENTIFIER,
        "single_reference" => FormSet::REFERENCE,
        "reference_list" => FormSet::REFERENCE_LIST,
        "nested_block" => FormSet::NESTED,
        _ => return None,
    })
}

/// One field's exact signature.
#[derive(Debug, Clone)]
pub struct FieldSignature {
    pub name: String,
    pub required: bool,
    pub minimum_occurrences: u64,
    /// `None` means unbounded source repetition within the containing block.
    pub maximum_occurrences: Option<u64>,
    /// The registered `value_kind` expression, verbatim.
    pub value_kind: String,
    /// The shapes this build accepts for that kind.
    pub forms: FormSet,
    /// The `BLOCK` argument of a `nested_block`-bearing template, when the kind
    /// names one.
    pub nested_block: Option<String>,
    /// True when the registry records a default. The parser never applies one:
    /// `04_GRAMMAR/13` applies a default only to a MISSING field, which is a
    /// later-stage determination.
    pub has_default: bool,
}

/// One block's exact schema, merged from both registries after a parity check.
#[derive(Debug, Clone)]
pub struct BlockSchema {
    pub name: String,
    /// `contexts` / `legal_parents`: the containers this block may occur in.
    pub parents: Vec<String>,
    /// `occurrence` / `block_occurrence`.
    pub occurrence: Occurrence,
    /// Field signatures, in registry order.
    pub fields: Vec<FieldSignature>,
    /// Required child *blocks*, from `block_schemas.required` entries that name
    /// a block rather than a field.
    pub required: Vec<String>,
    /// `block_schemas.repeatable`.
    pub repeatable: BTreeSet<String>,
    /// `conditional_requirements`, verbatim. Prose; this crate enforces only
    /// the structurally decidable ones and names each in `conditional.rs`.
    pub conditional_requirements: Vec<String>,
    /// True when `unknown_fields` is `"forbidden"`.
    pub unknown_fields_forbidden: bool,
}

impl BlockSchema {
    pub fn field(&self, name: &str) -> Option<&FieldSignature> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn accepts_parent(&self, parent: &str) -> bool {
        self.parents.iter().any(|p| p == parent)
    }
}

/// A block's occurrence limit inside one container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Occurrence {
    ExactlyOne,
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
}

impl Occurrence {
    fn parse(s: &str) -> Option<Occurrence> {
        Some(match s {
            "exactly_one" => Occurrence::ExactlyOne,
            "zero_or_one" => Occurrence::ZeroOrOne,
            "zero_or_more" => Occurrence::ZeroOrMore,
            "one_or_more" => Occurrence::OneOrMore,
            _ => return None,
        })
    }

    /// Largest legal count, or `None` when unbounded.
    pub fn maximum(self) -> Option<u64> {
        match self {
            Occurrence::ExactlyOne | Occurrence::ZeroOrOne => Some(1),
            Occurrence::ZeroOrMore | Occurrence::OneOrMore => None,
        }
    }

    pub fn as_registry_str(self) -> &'static str {
        match self {
            Occurrence::ExactlyOne => "exactly_one",
            Occurrence::ZeroOrOne => "zero_or_one",
            Occurrence::ZeroOrMore => "zero_or_more",
            Occurrence::OneOrMore => "one_or_more",
        }
    }
}

/// The registered error metadata and the supersession edges between them.
type ErrorTables = (
    BTreeMap<GrammarError, RegisteredGrammarError>,
    BTreeMap<GrammarError, BTreeSet<GrammarError>>,
);

/// Registry facts about one grammar-or-schema error identifier.
#[derive(Debug, Clone)]
pub struct RegisteredGrammarError {
    pub id: GrammarError,
    pub meaning: String,
    pub default_status: String,
    pub specificity_rank: u64,
    pub supersedes: BTreeSet<GrammarError>,
}

#[derive(Debug)]
pub enum GrammarLoadError {
    /// The package did not establish authority. See [`lcl_spec::Authority`].
    UnverifiedPackage(lcl_spec::Authority),
    MissingRegistry(&'static str),
    Malformed(String),
    Diagnostics(lcl_diagnostics::DiagnosticsError),
    /// The registry's grammar error set is not the set this build implements.
    GrammarErrorSetMismatch {
        missing_from_build: Vec<String>,
        missing_from_registry: Vec<String>,
    },
    /// `block_schemas` and `field_signatures` disagree about a block.
    RegistryParity {
        block: String,
        field: &'static str,
        block_schemas: String,
        field_signatures: String,
    },
    /// A `value_kind` expression this build cannot classify. Unknown material
    /// fails closed rather than being interpreted.
    UnknownValueKind(String),
}

impl fmt::Display for GrammarLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GrammarLoadError::UnverifiedPackage(a) => write!(
                f,
                "refusing to build a grammar from a {a} package: only the approved release is normative input"
            ),
            GrammarLoadError::MissingRegistry(n) => write!(f, "registry {n:?} not loaded"),
            GrammarLoadError::Malformed(m) => write!(f, "malformed registry data: {m}"),
            GrammarLoadError::Diagnostics(e) => write!(f, "diagnostic registry: {e}"),
            GrammarLoadError::GrammarErrorSetMismatch {
                missing_from_build,
                missing_from_registry,
            } => write!(
                f,
                "grammar error set mismatch: registry has {missing_from_build:?} that this build does not implement; this build has {missing_from_registry:?} that the registry does not register"
            ),
            GrammarLoadError::RegistryParity {
                block,
                field,
                block_schemas,
                field_signatures,
            } => write!(
                f,
                "registry parity failure for block {block}: block_schemas {field} is {block_schemas}, field_signatures says {field_signatures}"
            ),
            GrammarLoadError::UnknownValueKind(k) => {
                write!(f, "unclassifiable value kind {k:?}: refusing to guess a shape")
            }
        }
    }
}

impl std::error::Error for GrammarLoadError {}

/// The closed block and field vocabulary of LCL Core 0.1.0.
pub struct Grammar {
    formal_version: String,
    /// `CALLABLE` plus `REF`: every word that may stand before an opening
    /// parenthesis. Derived from the keyword registry's function and
    /// constructor categories, exactly as the lexicon derives it.
    callables: BTreeSet<String>,
    schemas: BTreeMap<String, BlockSchema>,
    /// `document_kind_blocks`: legal top-level blocks per `SPECIFICATION.KIND`.
    document_kind_blocks: BTreeMap<String, BTreeSet<String>>,
    errors: BTreeMap<GrammarError, RegisteredGrammarError>,
    supersedes: BTreeMap<GrammarError, BTreeSet<GrammarError>>,
}

impl fmt::Debug for Grammar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Grammar")
            .field("formal_version", &self.formal_version)
            .field("blocks", &self.schemas.len())
            .field("fields", &self.field_count())
            .field("document_kinds", &self.document_kind_blocks.len())
            .field("grammar_errors", &self.errors.len())
            .finish()
    }
}

impl Grammar {
    /// Build the block and field vocabulary from a verified package.
    pub fn load(spec: &SpecPackage) -> Result<Self, GrammarLoadError> {
        if !spec.is_authoritative() {
            return Err(GrammarLoadError::UnverifiedPackage(spec.authority()));
        }

        let block_schemas = spec
            .registry("block_schemas")
            .ok_or(GrammarLoadError::MissingRegistry("block_schemas"))?;
        let field_signatures = spec
            .registry("field_signatures")
            .ok_or(GrammarLoadError::MissingRegistry("field_signatures"))?;

        if field_signatures.get("closed").and_then(Json::as_bool) != Some(true) {
            return Err(GrammarLoadError::Malformed(
                "field_signatures registry does not declare closed: true".into(),
            ));
        }

        let templates = field_signatures
            .get("value_kind_templates")
            .and_then(Json::as_object)
            .ok_or_else(|| {
                GrammarLoadError::Malformed("field_signatures.value_kind_templates missing".into())
            })?;
        let template_forms = Self::template_forms(templates)?;

        let bs_schemas = block_schemas
            .get("schemas")
            .and_then(Json::as_object)
            .ok_or_else(|| GrammarLoadError::Malformed("block_schemas.schemas missing".into()))?;
        let fs_blocks = field_signatures
            .get("blocks")
            .and_then(Json::as_object)
            .ok_or_else(|| GrammarLoadError::Malformed("field_signatures.blocks missing".into()))?;

        let mut schemas = BTreeMap::new();
        for (name, bs) in bs_schemas {
            let fs = fs_blocks
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v)
                .ok_or_else(|| {
                    GrammarLoadError::Malformed(format!(
                        "block {name} missing from field_signatures"
                    ))
                })?;
            schemas.insert(name.clone(), Self::block(name, bs, fs, &template_forms)?);
        }
        for (name, _) in fs_blocks {
            if !schemas.contains_key(name) {
                return Err(GrammarLoadError::Malformed(format!(
                    "block {name} missing from block_schemas"
                )));
            }
        }

        let mut document_kind_blocks = BTreeMap::new();
        let kinds = block_schemas
            .get("document_kind_blocks")
            .and_then(Json::as_object)
            .ok_or_else(|| {
                GrammarLoadError::Malformed("block_schemas.document_kind_blocks missing".into())
            })?;
        for (kind, list) in kinds {
            let members = list.as_array().ok_or_else(|| {
                GrammarLoadError::Malformed(format!("document_kind_blocks.{kind} is not an array"))
            })?;
            let mut set = BTreeSet::new();
            for m in members {
                let block = m.as_str().ok_or_else(|| {
                    GrammarLoadError::Malformed(format!("document_kind_blocks.{kind} member"))
                })?;
                if !schemas.contains_key(block) {
                    return Err(GrammarLoadError::Malformed(format!(
                        "document_kind_blocks.{kind} names unregistered block {block}"
                    )));
                }
                set.insert(block.to_string());
            }
            document_kind_blocks.insert(kind.clone(), set);
        }

        let (errors, supersedes) = Self::errors(spec)?;
        let callables = Self::load_callables(spec)?;

        Ok(Self {
            formal_version: spec.formal_version().to_string(),
            callables,
            schemas,
            document_kind_blocks,
            errors,
            supersedes,
        })
    }

    fn template_forms(
        templates: &[(String, Json)],
    ) -> Result<BTreeMap<String, FormSet>, GrammarLoadError> {
        let mut out = BTreeMap::new();
        for (head, body) in templates {
            let accepted = body
                .get("accepted_forms")
                .and_then(Json::as_array)
                .ok_or_else(|| {
                    GrammarLoadError::Malformed(format!("template {head} has no accepted_forms"))
                })?;
            let mut forms = FormSet::empty();
            for a in accepted {
                let name = a.as_str().ok_or_else(|| {
                    GrammarLoadError::Malformed(format!("template {head} accepted_forms member"))
                })?;
                let bit = template_form(name).ok_or_else(|| {
                    GrammarLoadError::Malformed(format!(
                        "template {head} declares accepted form {name:?} that this build does not implement"
                    ))
                })?;
                forms = forms.union(bit);
            }
            out.insert(head.clone(), forms);
        }
        Ok(out)
    }

    fn block(
        name: &str,
        bs: &Json,
        fs: &Json,
        templates: &BTreeMap<String, FormSet>,
    ) -> Result<BlockSchema, GrammarLoadError> {
        let parity = |field: &'static str, a: String, b: String| {
            if a == b {
                Ok(())
            } else {
                Err(GrammarLoadError::RegistryParity {
                    block: name.to_string(),
                    field,
                    block_schemas: a,
                    field_signatures: b,
                })
            }
        };

        let contexts = string_array(bs, "contexts", name)?;
        let legal_parents = string_array(fs, "legal_parents", name)?;
        parity(
            "contexts",
            format!("{contexts:?}"),
            format!("{legal_parents:?}"),
        )?;

        let bs_occurrence = str_field(bs, "occurrence", name)?;
        let fs_occurrence = str_field(fs, "block_occurrence", name)?;
        parity("occurrence", bs_occurrence.clone(), fs_occurrence.clone())?;
        let occurrence = Occurrence::parse(&bs_occurrence).ok_or_else(|| {
            GrammarLoadError::Malformed(format!("{name}: unknown occurrence {bs_occurrence:?}"))
        })?;

        let repeatable: BTreeSet<String> =
            string_array(bs, "repeatable", name)?.into_iter().collect();

        let field_obj = fs
            .get("fields")
            .and_then(Json::as_object)
            .ok_or_else(|| GrammarLoadError::Malformed(format!("{name}.fields missing")))?;

        let mut fields = Vec::new();
        for (field_name, body) in field_obj {
            let value_kind = str_field(body, "value_kind", field_name)?;
            let (forms, nested_block) = Self::classify(&value_kind, templates)?;
            let maximum_occurrences = match body.get("maximum_occurrences") {
                Some(Json::Null) | None => None,
                Some(v) => Some(v.as_u64().ok_or_else(|| {
                    GrammarLoadError::Malformed(format!(
                        "{name}.{field_name}: maximum_occurrences is not a non-negative integer"
                    ))
                })?),
            };
            // The two registries must agree on repeatability: an unbounded
            // maximum is exactly a `repeatable` entry.
            let unbounded = maximum_occurrences.is_none();
            if unbounded != repeatable.contains(field_name) {
                return Err(GrammarLoadError::RegistryParity {
                    block: name.to_string(),
                    field: "repeatable",
                    block_schemas: format!(
                        "{field_name} repeatable={}",
                        repeatable.contains(field_name)
                    ),
                    field_signatures: format!("{field_name} unbounded={unbounded}"),
                });
            }
            fields.push(FieldSignature {
                name: field_name.clone(),
                required: body
                    .get("required")
                    .and_then(Json::as_bool)
                    .ok_or_else(|| {
                        GrammarLoadError::Malformed(format!(
                            "{name}.{field_name}: missing required"
                        ))
                    })?,
                minimum_occurrences: body
                    .get("minimum_occurrences")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| {
                        GrammarLoadError::Malformed(format!(
                            "{name}.{field_name}: missing minimum_occurrences"
                        ))
                    })?,
                maximum_occurrences,
                value_kind,
                forms,
                nested_block,
                has_default: !matches!(body.get("default"), Some(Json::Null) | None),
            });
        }

        // `block_schemas.required` mixes required fields with required child
        // blocks. A name that is also a field of this block is a field
        // requirement and is already carried by the field signature.
        let required: Vec<String> = string_array(bs, "required", name)?
            .into_iter()
            .filter(|r| !fields.iter().any(|f| &f.name == r))
            .collect();

        Ok(BlockSchema {
            name: name.to_string(),
            parents: legal_parents,
            occurrence,
            fields,
            required,
            repeatable,
            conditional_requirements: string_array(fs, "conditional_requirements", name)?,
            unknown_fields_forbidden: str_field(fs, "unknown_fields", name)? == "forbidden",
        })
    }

    /// Split a `value_kind` expression into its accepted forms and its
    /// `nested_block` argument, if any.
    fn classify(
        value_kind: &str,
        templates: &BTreeMap<String, FormSet>,
    ) -> Result<(FormSet, Option<String>), GrammarLoadError> {
        match value_kind.find(['[', '(']) {
            // A named kind from `value_kind_registry`.
            None => named_kind_forms(value_kind)
                .map(|f| (f, None))
                .ok_or_else(|| GrammarLoadError::UnknownValueKind(value_kind.to_string())),
            // A template instance: `head(ARGUMENT)` or `head[RANGE]`.
            Some(split) => {
                let head = &value_kind[..split];
                let forms = *templates
                    .get(head)
                    .ok_or_else(|| GrammarLoadError::UnknownValueKind(value_kind.to_string()))?;
                let argument = value_kind
                    .get(split + 1..value_kind.len().saturating_sub(1))
                    .unwrap_or_default();
                let nested = forms
                    .contains(FormSet::NESTED)
                    .then(|| argument.to_string());
                Ok((forms, nested))
            }
        }
    }

    /// `CALLABLE`, derived from the closed keyword registry rather than copied.
    ///
    /// `04_GRAMMAR/10_COMPLETE_EBNF.ebnf` derives `CALLABLE` from exactly the
    /// function and constructor categories; `REF` is the constructor of
    /// `REFERENCE_CALL`.
    fn load_callables(spec: &SpecPackage) -> Result<BTreeSet<String>, GrammarLoadError> {
        let keywords = spec
            .registry("keywords")
            .ok_or(GrammarLoadError::MissingRegistry("keywords"))?;
        if keywords.get("closed").and_then(Json::as_bool) != Some(true) {
            return Err(GrammarLoadError::Malformed(
                "keywords registry does not declare closed: true".into(),
            ));
        }
        let entries = keywords
            .get("keywords")
            .and_then(Json::as_object)
            .ok_or_else(|| GrammarLoadError::Malformed("keywords.keywords".into()))?;
        let mut out = BTreeSet::new();
        for (word, body) in entries {
            let category = body
                .get("category")
                .and_then(Json::as_str)
                .ok_or_else(|| GrammarLoadError::Malformed(format!("{word}: missing category")))?;
            if category.contains("function") || category.contains("constructor") {
                out.insert(word.clone());
            }
        }
        Ok(out)
    }

    fn errors(spec: &SpecPackage) -> Result<ErrorTables, GrammarLoadError> {
        let registry = lcl_diagnostics::DiagnosticRegistry::load(spec)
            .map_err(GrammarLoadError::Diagnostics)?;

        // The enum must equal the registry's grammar_or_schema set exactly.
        let registered: BTreeSet<String> = registry
            .errors_by_stage(lcl_diagnostics::Stage::GrammarOrSchema)
            .into_iter()
            .map(|e| e.id.clone())
            .collect();
        let built: BTreeSet<String> = GrammarError::ALL
            .iter()
            .map(|e| e.as_registry_str().to_string())
            .collect();
        if registered != built {
            return Err(GrammarLoadError::GrammarErrorSetMismatch {
                missing_from_build: registered.difference(&built).cloned().collect(),
                missing_from_registry: built.difference(&registered).cloned().collect(),
            });
        }

        let raw = spec
            .registry("statuses_and_errors")
            .ok_or(GrammarLoadError::MissingRegistry("statuses_and_errors"))?;
        let selection = raw
            .get("diagnostic_selection")
            .ok_or_else(|| GrammarLoadError::Malformed("missing diagnostic_selection".into()))?;

        let rank = selection.get("specificity_rank").ok_or_else(|| {
            GrammarLoadError::Malformed("missing diagnostic_selection.specificity_rank".into())
        })?;
        let default_rank = rank
            .get("default_for_every_error")
            .and_then(Json::as_u64)
            .ok_or_else(|| {
                GrammarLoadError::Malformed("specificity_rank.default_for_every_error".into())
            })?;
        let rank_overrides = rank.get("overrides").and_then(Json::as_object);

        let supersede_overrides = selection
            .get("supersedes")
            .and_then(|s| s.get("overrides"))
            .and_then(Json::as_object);

        let mut errors = BTreeMap::new();
        let mut supersedes = BTreeMap::new();
        for id in GrammarError::ALL {
            let key = id.as_registry_str();
            let def = registry
                .error(key)
                .ok_or_else(|| GrammarLoadError::Malformed(format!("{key} not registered")))?;
            let specificity_rank = rank_overrides
                .and_then(|o| o.iter().find(|(k, _)| k == key))
                .and_then(|(_, v)| v.as_u64())
                .unwrap_or(default_rank);
            let mut edges = BTreeSet::new();
            if let Some((_, targets)) = supersede_overrides
                .and_then(|o| o.iter().find(|(k, _)| k == key))
                .map(|(k, v)| (k, v))
            {
                for t in targets.as_array().unwrap_or_default() {
                    // Only same-stage edges are reachable from this crate; an
                    // edge to another stage's identifier is not this stage's to
                    // apply.
                    if let Some(target) = t.as_str().and_then(GrammarError::from_registry_str) {
                        edges.insert(target);
                    }
                }
            }
            if !edges.is_empty() {
                supersedes.insert(id, edges.clone());
            }
            errors.insert(
                id,
                RegisteredGrammarError {
                    id,
                    meaning: def.meaning.clone(),
                    default_status: def.default_status.clone(),
                    specificity_rank,
                    supersedes: edges,
                },
            );
        }
        Ok((errors, supersedes))
    }

    pub fn formal_version(&self) -> &str {
        &self.formal_version
    }

    /// True for a word that may stand immediately before `(`.
    pub fn is_callable(&self, word: &str) -> bool {
        self.callables.contains(word)
    }

    pub fn callables(&self) -> impl Iterator<Item = &str> {
        self.callables.iter().map(String::as_str)
    }

    /// True for a `SCALAR_TYPE` word.
    pub fn is_scalar_type(&self, word: &str) -> bool {
        SCALAR_TYPE_WORDS.contains(&word)
    }

    /// The `SCALAR_TYPE` alternatives this build implements.
    pub fn scalar_types(&self) -> impl Iterator<Item = &'static str> {
        SCALAR_TYPE_WORDS.into_iter()
    }

    /// The four bracketed `NON_NULL_TYPE_EXPRESSION` constructors.
    pub fn bracket_types(&self) -> impl Iterator<Item = &'static str> {
        BRACKET_TYPE_WORDS.into_iter()
    }

    /// The word-shaped `LITERAL` alternatives.
    pub fn literal_words(&self) -> impl Iterator<Item = &'static str> {
        LITERAL_WORDS.into_iter()
    }

    /// True for one of the four bracketed type constructors.
    pub fn is_bracket_type(&self, word: &str) -> bool {
        BRACKET_TYPE_WORDS.contains(&word)
    }

    /// True for a word-shaped `LITERAL` alternative.
    pub fn is_literal_word(&self, word: &str) -> bool {
        LITERAL_WORDS.contains(&word)
    }

    pub fn schema(&self, block: &str) -> Option<&BlockSchema> {
        self.schemas.get(block)
    }

    pub fn schemas(&self) -> impl Iterator<Item = &BlockSchema> {
        self.schemas.values()
    }

    pub fn block_names(&self) -> impl Iterator<Item = &str> {
        self.schemas.keys().map(String::as_str)
    }

    pub fn is_block(&self, name: &str) -> bool {
        self.schemas.contains_key(name)
    }

    pub fn block_count(&self) -> usize {
        self.schemas.len()
    }

    pub fn field_count(&self) -> usize {
        self.schemas.values().map(|s| s.fields.len()).sum()
    }

    /// Every distinct registered `value_kind` expression, in sorted order.
    pub fn value_kinds(&self) -> BTreeSet<&str> {
        self.schemas
            .values()
            .flat_map(|s| s.fields.iter().map(|f| f.value_kind.as_str()))
            .collect()
    }

    /// Blocks legal at top level for a document kind, or `None` when the kind
    /// is not registered.
    pub fn document_kind_blocks(&self, kind: &str) -> Option<&BTreeSet<String>> {
        self.document_kind_blocks.get(kind)
    }

    pub fn document_kinds(&self) -> impl Iterator<Item = &str> {
        self.document_kind_blocks.keys().map(String::as_str)
    }

    pub fn error(&self, id: GrammarError) -> &RegisteredGrammarError {
        self.errors
            .get(&id)
            .expect("load validated every registered grammar error")
    }

    pub(crate) fn supersedes(&self) -> &BTreeMap<GrammarError, BTreeSet<GrammarError>> {
        &self.supersedes
    }
}

fn str_field(body: &Json, key: &str, owner: &str) -> Result<String, GrammarLoadError> {
    body.get(key)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            GrammarLoadError::Malformed(format!("{owner}: missing string field {key:?}"))
        })
}

fn string_array(body: &Json, key: &str, owner: &str) -> Result<Vec<String>, GrammarLoadError> {
    let Some(value) = body.get(key) else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| GrammarLoadError::Malformed(format!("{owner}.{key} is not an array")))?;
    array
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| GrammarLoadError::Malformed(format!("{owner}.{key} member")))
        })
        .collect()
}
