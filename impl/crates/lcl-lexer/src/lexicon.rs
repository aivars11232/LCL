//! The lexical vocabulary, derived from the verified canonical package.
//!
//! Nothing in this module is a transcription. The reserved words, the adopted
//! symbols, the excluded lexemes, the excluded notation patterns, the excluded
//! source classes, the block names, the object-data and type-designator field
//! signatures, the literal-constructor contracts, and every error identifier's
//! meaning, default status, specificity rank and supersession edges are read
//! out of the registries at load time. If the release changed, the load fails;
//! it does not adapt.
//!
//! ## Fail-closed gates
//!
//! [`Lexicon::load`] refuses unless all of the following hold.
//!
//! 1. The package is [`lcl_spec::Authority::Authoritative`] — internal
//!    integrity **and** the external trust anchor. An unverified package is
//!    never normative input, so it never becomes a lexer's vocabulary.
//! 2. `keywords_v0.1.0.json` declares `closed: true`, and every registered word
//!    matches `[A-Z][A-Z0-9_]*`. A lowercase or mixed-case "reserved word"
//!    would silently break the case rules of `02_LEXICAL/02`.
//! 3. Every adopted symbol and excluded lexeme is non-empty ASCII. A non-ASCII
//!    symbol cannot occur outside a string, so one would mean the registry and
//!    `02_LEXICAL/01` disagree.
//! 4. The registry's declared `excluded_notation_patterns` and
//!    `excluded_source_classes` are exactly the ones this lexer implements. A
//!    new class the lexer does not enforce must not pass silently.
//! 5. The set of `stage == "lexical"` error identifiers equals
//!    [`LexicalError::ALL`] exactly.
//! 6. No field name is a type designator in one block and something else in
//!    another; the type-position rule is keyed by field name.
//! 7. Every literal constructor this lexer validates is registered with
//!    `error.literal.invalid`, has at least one all-`STRING` overload, and its
//!    profile names `error.literal.invalid` as the invalid-literal error.

use crate::diagnostic::LexicalError;
use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The one excluded notation pattern this lexer implements, by registry key.
const IMPLEMENTED_NOTATION_PATTERNS: &[&str] = &["xml_tag"];
/// The one excluded source class this lexer implements, by registry key.
const IMPLEMENTED_SOURCE_CLASSES: &[&str] = &["U+0009"];
/// The error every closed literal profile must name for a malformed literal.
const LITERAL_INVALID: &str = "error.literal.invalid";

/// `field_signatures_v0.1.0.json#/value_kind_registry` kinds that make a field
/// a built-in type position ("a built-in type position", `02_LEXICAL/02`).
const TYPE_POSITION_KINDS: &[&str] = &["type_expression", "type_or_format_base"];
/// The value kind whose indented form is object data: "Inline value expression
/// or indented lowercase-key object value."
const OBJECT_DATA_KIND: &str = "value_or_object_expression";

#[derive(Debug)]
pub enum LexiconError {
    /// The package did not establish authority. See [`lcl_spec::Authority`].
    UnverifiedPackage(lcl_spec::Authority),
    MissingRegistry(&'static str),
    Malformed(String),
    Diagnostics(lcl_diagnostics::DiagnosticsError),
    /// The registry's lexical error set is not the set this build implements.
    LexicalErrorSetMismatch {
        missing_from_build: Vec<String>,
        missing_from_registry: Vec<String>,
    },
    /// The registry declares a symbol class or notation pattern this lexer does
    /// not enforce, or omits one it does.
    UnimplementedSymbolClass {
        kind: &'static str,
        declared: Vec<String>,
        implemented: Vec<String>,
    },
}

impl fmt::Display for LexiconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LexiconError::UnverifiedPackage(a) => write!(
                f,
                "refusing to build a lexicon from a {a} package: only the approved release is normative input"
            ),
            LexiconError::MissingRegistry(n) => write!(f, "registry {n:?} not loaded"),
            LexiconError::Malformed(m) => write!(f, "malformed registry data: {m}"),
            LexiconError::Diagnostics(e) => write!(f, "diagnostic registry: {e}"),
            LexiconError::LexicalErrorSetMismatch {
                missing_from_build,
                missing_from_registry,
            } => write!(
                f,
                "lexical error set mismatch: registry has {missing_from_build:?} that this build does not implement; this build has {missing_from_registry:?} that the registry does not register"
            ),
            LexiconError::UnimplementedSymbolClass {
                kind,
                declared,
                implemented,
            } => write!(
                f,
                "{kind} mismatch: registry declares {declared:?}, this lexer implements {implemented:?}"
            ),
        }
    }
}

impl std::error::Error for LexiconError {}

/// Registry facts about one lexical error identifier.
#[derive(Debug, Clone)]
pub struct RegisteredLexicalError {
    pub id: LexicalError,
    pub meaning: String,
    pub default_status: String,
    pub specificity_rank: u64,
    pub supersedes: BTreeSet<LexicalError>,
}

/// The closed REGEX flags contract, from
/// `types_v0.1.0.json#/pattern_profiles/REGEX`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexFlagContract {
    /// `allowed_flags`.
    pub allowed: Vec<char>,
    /// `canonical_flag_order`.
    pub canonical_order: Vec<char>,
    /// `duplicate_flags_allowed`.
    pub duplicates_allowed: bool,
    /// `unknown_flags_allowed`.
    pub unknown_allowed: bool,
}

/// Which closed literal profile a constructor's `STRING` arguments follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LiteralProfile {
    /// `pattern_profiles.REGEX`: a pattern, optionally followed by flags.
    Regex,
    /// `pattern_profiles.GLOB`.
    Glob,
    /// `temporal_literal_contract.date`.
    Date,
    /// `temporal_literal_contract.time`.
    Time,
    /// `temporal_literal_contract.datetime`.
    Datetime,
    /// `constructors.URI.profile`: RFC 3986 absolute-URI with a scheme.
    Uri,
}

/// A registered constructor whose all-`STRING` overloads carry a closed
/// literal profile with `error.literal.invalid`.
#[derive(Debug, Clone)]
pub struct LiteralConstructor {
    pub name: String,
    pub profile: LiteralProfile,
    /// Argument counts of the all-`STRING` overloads.
    pub literal_arities: BTreeSet<usize>,
}

/// The closed lexical vocabulary of LCL Core 0.1.0.
pub struct Lexicon {
    formal_version: String,
    reserved_words: BTreeSet<String>,
    /// Lowercase spelling to registered word, for the case rules.
    case_folded: BTreeMap<String, String>,
    /// Registered words usable as a call target: `CALLABLE` plus `REF`.
    callables: BTreeSet<String>,
    /// Registered words of a literal category: the sentinel and Boolean
    /// literals that end an operand.
    literal_words: BTreeSet<String>,
    /// Registered words of a type category: the words a type-argument `[`
    /// may follow.
    type_words: BTreeSet<String>,
    /// The 41 block names of `field_signatures_v0.1.0.json#/blocks`.
    block_names: BTreeSet<String>,
    /// `(block, field)` pairs whose indented form is object data.
    object_data_fields: BTreeSet<(String, String)>,
    /// Field names whose value is a built-in type position.
    type_position_fields: BTreeSet<String>,
    /// Adopted symbols, longest first, for the longest-lexeme selection rule.
    adopted: Vec<String>,
    /// Excluded exact lexemes, longest first.
    excluded: Vec<String>,
    longest_lexeme: usize,
    regex_flags: RegexFlagContract,
    literal_constructors: BTreeMap<String, LiteralConstructor>,
    errors: BTreeMap<LexicalError, RegisteredLexicalError>,
    supersedes: BTreeMap<LexicalError, BTreeSet<LexicalError>>,
}

impl fmt::Debug for Lexicon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lexicon")
            .field("formal_version", &self.formal_version)
            .field("reserved_words", &self.reserved_words.len())
            .field("callables", &self.callables.len())
            .field("block_names", &self.block_names.len())
            .field("object_data_fields", &self.object_data_fields.len())
            .field("type_position_fields", &self.type_position_fields.len())
            .field("adopted_symbols", &self.adopted.len())
            .field("excluded_lexemes", &self.excluded.len())
            .field("literal_constructors", &self.literal_constructors.len())
            .field("lexical_errors", &self.errors.len())
            .finish()
    }
}

impl Lexicon {
    /// Build the vocabulary from a verified package.
    pub fn load(spec: &SpecPackage) -> Result<Self, LexiconError> {
        if !spec.is_authoritative() {
            return Err(LexiconError::UnverifiedPackage(spec.authority()));
        }

        let keywords = spec
            .registry("keywords")
            .ok_or(LexiconError::MissingRegistry("keywords"))?;
        let symbols = spec
            .registry("symbols")
            .ok_or(LexiconError::MissingRegistry("symbols"))?;
        let field_signatures = spec
            .registry("field_signatures")
            .ok_or(LexiconError::MissingRegistry("field_signatures"))?;
        let types = spec
            .registry("types")
            .ok_or(LexiconError::MissingRegistry("types"))?;
        let operators = spec
            .registry("operators_and_functions")
            .ok_or(LexiconError::MissingRegistry("operators_and_functions"))?;

        // --- Reserved words -------------------------------------------------
        if keywords.get("closed").and_then(Json::as_bool) != Some(true) {
            return Err(LexiconError::Malformed(
                "keywords registry does not declare closed: true".into(),
            ));
        }
        let entries = keywords
            .get("keywords")
            .and_then(Json::as_object)
            .ok_or_else(|| LexiconError::Malformed("keywords.keywords is not an object".into()))?;

        let mut reserved_words = BTreeSet::new();
        let mut case_folded = BTreeMap::new();
        let mut callables = BTreeSet::new();
        let mut literal_words = BTreeSet::new();
        let mut type_words = BTreeSet::new();
        for (word, body) in entries {
            if !is_registered_word_shape(word) {
                return Err(LexiconError::Malformed(format!(
                    "reserved word {word:?} is not [A-Z][A-Z0-9_]*"
                )));
            }
            let category = body
                .get("category")
                .and_then(Json::as_str)
                .ok_or_else(|| LexiconError::Malformed(format!("{word}: missing category")))?;
            // `04_GRAMMAR/10_COMPLETE_EBNF.ebnf` derives CALLABLE from exactly
            // the function and constructor categories; REF is the constructor
            // of REFERENCE_CALL. Deriving the set keeps it closed under a
            // registry change instead of pinning a copied list.
            if category.contains("function") || category.contains("constructor") {
                callables.insert(word.clone());
            }
            if category.contains("literal") {
                literal_words.insert(word.clone());
            }
            if category.contains("type") {
                type_words.insert(word.clone());
            }
            if let Some(previous) = case_folded.insert(word.to_lowercase(), word.clone()) {
                return Err(LexiconError::Malformed(format!(
                    "reserved words {previous:?} and {word:?} case-fold together"
                )));
            }
            reserved_words.insert(word.clone());
        }

        // --- Symbols --------------------------------------------------------
        let adopted_obj = symbols
            .get("adopted")
            .and_then(Json::as_object)
            .ok_or_else(|| LexiconError::Malformed("symbols.adopted is not an object".into()))?;
        let mut adopted: Vec<String> = adopted_obj.iter().map(|(k, _)| k.clone()).collect();

        let excluded_arr = symbols
            .get("excluded_exact_lexemes")
            .and_then(Json::as_array)
            .ok_or_else(|| {
                LexiconError::Malformed("symbols.excluded_exact_lexemes is not an array".into())
            })?;
        let mut excluded: Vec<String> = excluded_arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        if excluded.len() != excluded_arr.len() {
            return Err(LexiconError::Malformed(
                "symbols.excluded_exact_lexemes holds a non-string".into(),
            ));
        }

        for lexeme in adopted.iter().chain(excluded.iter()) {
            if lexeme.is_empty() || !lexeme.is_ascii() {
                return Err(LexiconError::Malformed(format!(
                    "symbol lexeme {lexeme:?} is empty or non-ASCII, which cannot occur outside a string"
                )));
            }
        }
        // Longest exact lexeme wins, per symbols_v0.1.0.json#/selection_rule.
        adopted.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        excluded.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        let longest_lexeme = adopted
            .iter()
            .chain(excluded.iter())
            .map(String::len)
            .max()
            .unwrap_or(0);

        check_declared_keys(
            "excluded_notation_patterns",
            symbols.get("excluded_notation_patterns"),
            IMPLEMENTED_NOTATION_PATTERNS,
        )?;
        check_declared_keys(
            "excluded_source_classes",
            symbols.get("excluded_source_classes"),
            IMPLEMENTED_SOURCE_CLASSES,
        )?;

        // --- Block and field contexts ---------------------------------------
        // `02_LEXICAL/02` keys error.keyword.case on "a block or field key
        // outside object data" and "a built-in type position". Which fields
        // hold object data and which are type positions is a fact of
        // `field_signatures_v0.1.0.json`, read here rather than assumed.
        let blocks = field_signatures
            .get("blocks")
            .and_then(Json::as_object)
            .ok_or_else(|| {
                LexiconError::Malformed("field_signatures.blocks is not an object".into())
            })?;
        let mut block_names = BTreeSet::new();
        let mut object_data_fields = BTreeSet::new();
        let mut type_position_fields = BTreeSet::new();
        let mut non_type_fields = BTreeSet::new();
        for (block, body) in blocks {
            if !reserved_words.contains(block) {
                return Err(LexiconError::Malformed(format!(
                    "block {block:?} is not a reserved word"
                )));
            }
            block_names.insert(block.clone());
            let fields = body
                .get("fields")
                .and_then(Json::as_object)
                .ok_or_else(|| LexiconError::Malformed(format!("{block}: fields missing")))?;
            for (field, signature) in fields {
                if !reserved_words.contains(field) {
                    return Err(LexiconError::Malformed(format!(
                        "field {block}.{field} is not a reserved word"
                    )));
                }
                let kind = signature
                    .get("value_kind")
                    .and_then(Json::as_str)
                    .ok_or_else(|| {
                        LexiconError::Malformed(format!("{block}.{field}: value_kind missing"))
                    })?;
                if kind == OBJECT_DATA_KIND {
                    object_data_fields.insert((block.clone(), field.clone()));
                }
                if TYPE_POSITION_KINDS.contains(&kind) {
                    type_position_fields.insert(field.clone());
                } else {
                    non_type_fields.insert(field.clone());
                }
            }
        }
        if let Some(ambiguous) = type_position_fields.intersection(&non_type_fields).next() {
            return Err(LexiconError::Malformed(format!(
                "field {ambiguous:?} is a type position in one block and not in another"
            )));
        }

        // --- Literal constructor contracts ----------------------------------
        let regex_profile = types
            .get("pattern_profiles")
            .and_then(|p| p.get("REGEX"))
            .ok_or_else(|| {
                LexiconError::Malformed("types.pattern_profiles.REGEX missing".into())
            })?;
        let regex_flags = RegexFlagContract {
            allowed: char_list(regex_profile.get("allowed_flags"), "REGEX.allowed_flags")?,
            canonical_order: regex_profile
                .get("canonical_flag_order")
                .and_then(Json::as_str)
                .map(|s| s.chars().collect())
                .ok_or_else(|| {
                    LexiconError::Malformed("REGEX.canonical_flag_order missing".into())
                })?,
            duplicates_allowed: bool_field(regex_profile, "duplicate_flags_allowed", "REGEX")?,
            unknown_allowed: bool_field(regex_profile, "unknown_flags_allowed", "REGEX")?,
        };
        let order_set: BTreeSet<char> = regex_flags.canonical_order.iter().copied().collect();
        let allowed_set: BTreeSet<char> = regex_flags.allowed.iter().copied().collect();
        if order_set != allowed_set || order_set.len() != regex_flags.canonical_order.len() {
            return Err(LexiconError::Malformed(
                "REGEX canonical_flag_order does not order allowed_flags exactly once each".into(),
            ));
        }
        expect_error(
            regex_profile.get("invalid_pattern_error"),
            "types.pattern_profiles.REGEX.invalid_pattern_error",
        )?;
        expect_error(
            types
                .get("pattern_profiles")
                .and_then(|p| p.get("GLOB"))
                .and_then(|g| g.get("invalid_pattern_error")),
            "types.pattern_profiles.GLOB.invalid_pattern_error",
        )?;
        expect_error(
            types
                .get("temporal_literal_contract")
                .and_then(|t| t.get("error")),
            "types.temporal_literal_contract.error",
        )?;

        let constructors = operators
            .get("constructors")
            .and_then(Json::as_object)
            .ok_or_else(|| {
                LexiconError::Malformed("operators_and_functions.constructors missing".into())
            })?;
        let mut literal_constructors = BTreeMap::new();
        for (name, profile) in [
            ("REGEX", LiteralProfile::Regex),
            ("GLOB", LiteralProfile::Glob),
            ("DATE", LiteralProfile::Date),
            ("TIME", LiteralProfile::Time),
            ("DATETIME", LiteralProfile::Datetime),
            ("URI", LiteralProfile::Uri),
        ] {
            let entry = constructors
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v)
                .ok_or_else(|| LexiconError::Malformed(format!("constructor {name} missing")))?;
            if !callables.contains(name) {
                return Err(LexiconError::Malformed(format!(
                    "constructor {name} is not a callable reserved word"
                )));
            }
            let errors = entry
                .get("errors")
                .and_then(Json::as_array)
                .ok_or_else(|| LexiconError::Malformed(format!("{name}: errors missing")))?;
            if !errors.iter().any(|e| e.as_str() == Some(LITERAL_INVALID)) {
                return Err(LexiconError::Malformed(format!(
                    "constructor {name} does not register {LITERAL_INVALID}"
                )));
            }
            let overloads = entry
                .get("overloads")
                .and_then(Json::as_array)
                .ok_or_else(|| LexiconError::Malformed(format!("{name}: overloads missing")))?;
            let mut literal_arities = BTreeSet::new();
            for overload in overloads {
                let params = overload
                    .get("parameters")
                    .and_then(Json::as_array)
                    .ok_or_else(|| {
                        LexiconError::Malformed(format!("{name}: overload parameters missing"))
                    })?;
                if !params.is_empty() && params.iter().all(|p| p.as_str() == Some("STRING")) {
                    literal_arities.insert(params.len());
                }
            }
            if literal_arities.is_empty() {
                return Err(LexiconError::Malformed(format!(
                    "constructor {name} has no all-STRING overload"
                )));
            }
            literal_constructors.insert(
                name.to_string(),
                LiteralConstructor {
                    name: name.to_string(),
                    profile,
                    literal_arities,
                },
            );
        }

        // --- Diagnostics ----------------------------------------------------
        let diagnostics = DiagnosticRegistry::load(spec).map_err(LexiconError::Diagnostics)?;
        let registered_lexical: BTreeSet<&str> = diagnostics
            .errors_by_stage(Stage::Lexical)
            .into_iter()
            .map(|e| e.id.as_str())
            .collect();
        let implemented: BTreeSet<&str> = LexicalError::ALL
            .iter()
            .map(|e| e.as_registry_str())
            .collect();
        if registered_lexical != implemented {
            return Err(LexiconError::LexicalErrorSetMismatch {
                missing_from_build: registered_lexical
                    .difference(&implemented)
                    .map(|s| (*s).to_string())
                    .collect(),
                missing_from_registry: implemented
                    .difference(&registered_lexical)
                    .map(|s| (*s).to_string())
                    .collect(),
            });
        }

        let selection = diagnostics.selection_contract();
        let default_rank = selection
            .get("specificity_rank")
            .and_then(|v| v.get("default_for_every_error"))
            .and_then(Json::as_u64)
            .ok_or_else(|| {
                LexiconError::Malformed("missing specificity_rank.default_for_every_error".into())
            })?;
        let rank_overrides = selection
            .get("specificity_rank")
            .and_then(|v| v.get("overrides"))
            .and_then(Json::as_object)
            .map(|o| o.to_vec())
            .unwrap_or_default();
        let supersede_overrides = selection
            .get("supersedes")
            .and_then(|v| v.get("overrides"))
            .and_then(Json::as_object)
            .map(|o| o.to_vec())
            .unwrap_or_default();

        let mut supersedes: BTreeMap<LexicalError, BTreeSet<LexicalError>> = BTreeMap::new();
        for (id, targets) in &supersede_overrides {
            // Only lexical-to-lexical edges are in this crate's scope; an edge
            // whose source is a later-stage error is another layer's business.
            let Some(source) = LexicalError::from_registry_str(id) else {
                continue;
            };
            let list = targets.as_array().ok_or_else(|| {
                LexiconError::Malformed(format!("supersedes.overrides.{id} is not an array"))
            })?;
            let mut set = BTreeSet::new();
            for target in list {
                let name = target.as_str().ok_or_else(|| {
                    LexiconError::Malformed(format!("supersedes.overrides.{id} holds a non-string"))
                })?;
                // A cross-stage target is recorded by the registry but cannot be
                // raised in a source-validation run, so it is not represented.
                if let Some(t) = LexicalError::from_registry_str(name) {
                    set.insert(t);
                }
            }
            supersedes.insert(source, set);
        }

        let mut errors = BTreeMap::new();
        for id in LexicalError::ALL {
            let def = diagnostics
                .error(id.as_registry_str())
                .ok_or_else(|| LexiconError::Malformed(format!("{id}: not in the registry")))?;
            let specificity_rank = rank_overrides
                .iter()
                .find(|(k, _)| k == id.as_registry_str())
                .and_then(|(_, v)| v.as_u64())
                .unwrap_or(default_rank);
            errors.insert(
                id,
                RegisteredLexicalError {
                    id,
                    meaning: def.meaning.clone(),
                    default_status: def.default_status.clone(),
                    specificity_rank,
                    supersedes: supersedes.get(&id).cloned().unwrap_or_default(),
                },
            );
        }

        Ok(Self {
            formal_version: spec.formal_version().to_string(),
            reserved_words,
            case_folded,
            callables,
            literal_words,
            type_words,
            block_names,
            object_data_fields,
            type_position_fields,
            adopted,
            excluded,
            longest_lexeme,
            regex_flags,
            literal_constructors,
            errors,
            supersedes,
        })
    }

    pub fn formal_version(&self) -> &str {
        &self.formal_version
    }

    /// The closed reserved word list.
    pub fn reserved_words(&self) -> impl Iterator<Item = &str> {
        self.reserved_words.iter().map(String::as_str)
    }

    pub fn is_reserved_word(&self, word: &str) -> bool {
        self.reserved_words.contains(word)
    }

    /// The registered word a spelling case-folds to, if any.
    pub fn case_folded_word(&self, word: &str) -> Option<&str> {
        self.case_folded
            .get(&word.to_lowercase())
            .map(String::as_str)
    }

    /// `CALLABLE` plus `REF`: the words that may precede an opening `(`.
    pub fn callables(&self) -> impl Iterator<Item = &str> {
        self.callables.iter().map(String::as_str)
    }

    pub fn is_callable(&self, word: &str) -> bool {
        self.callables.contains(word)
    }

    /// Registered words of a literal category (`TRUE`, `FALSE`, `NULL`,
    /// `MISSING`, `UNKNOWN`), which complete an operand.
    pub fn literal_words(&self) -> impl Iterator<Item = &str> {
        self.literal_words.iter().map(String::as_str)
    }

    pub fn is_literal_word(&self, word: &str) -> bool {
        self.literal_words.contains(word)
    }

    /// Registered words of a type category.
    pub fn type_words(&self) -> impl Iterator<Item = &str> {
        self.type_words.iter().map(String::as_str)
    }

    pub fn is_type_word(&self, word: &str) -> bool {
        self.type_words.contains(word)
    }

    /// The block names of `field_signatures_v0.1.0.json#/blocks`.
    pub fn block_names(&self) -> impl Iterator<Item = &str> {
        self.block_names.iter().map(String::as_str)
    }

    pub fn is_block_name(&self, word: &str) -> bool {
        self.block_names.contains(word)
    }

    /// `(block, field)` pairs whose indented form is lowercase-key object data.
    pub fn object_data_fields(&self) -> impl Iterator<Item = (&str, &str)> {
        self.object_data_fields
            .iter()
            .map(|(b, f)| (b.as_str(), f.as_str()))
    }

    /// True when `field` under `block` opens object data in its indented form.
    pub fn is_object_data_field(&self, block: Option<&str>, field: &str) -> bool {
        block.is_some_and(|b| {
            self.object_data_fields
                .contains(&(b.to_string(), field.to_string()))
        })
    }

    /// Field names whose inline value is a built-in type position.
    pub fn type_position_fields(&self) -> impl Iterator<Item = &str> {
        self.type_position_fields.iter().map(String::as_str)
    }

    pub fn is_type_position_field(&self, field: &str) -> bool {
        self.type_position_fields.contains(field)
    }

    pub fn adopted_symbols(&self) -> impl Iterator<Item = &str> {
        self.adopted.iter().map(String::as_str)
    }

    pub fn excluded_lexemes(&self) -> impl Iterator<Item = &str> {
        self.excluded.iter().map(String::as_str)
    }

    pub fn is_adopted_symbol(&self, lexeme: &str) -> bool {
        self.adopted.iter().any(|s| s == lexeme)
    }

    /// The closed REGEX flags contract.
    pub fn regex_flags(&self) -> &RegexFlagContract {
        &self.regex_flags
    }

    /// The constructors whose literal `STRING` arguments this lexer validates.
    pub fn literal_constructors(&self) -> impl Iterator<Item = &LiteralConstructor> {
        self.literal_constructors.values()
    }

    pub fn literal_constructor(&self, word: &str) -> Option<&LiteralConstructor> {
        self.literal_constructors.get(word)
    }

    /// The registry facts for one lexical error.
    pub fn error(&self, id: LexicalError) -> &RegisteredLexicalError {
        // Total: `load` populates every variant of the closed enum or fails.
        self.errors
            .get(&id)
            .unwrap_or_else(|| unreachable!("Lexicon::load populates every LexicalError"))
    }

    pub(crate) fn supersedes(&self) -> &BTreeMap<LexicalError, BTreeSet<LexicalError>> {
        &self.supersedes
    }

    /// Longest exact lexeme selection across adopted and excluded symbols.
    ///
    /// Implements `symbols_v0.1.0.json#/selection_rule` directly: "In NORMAL
    /// mode choose the longest exact lexeme across adopted and
    /// excluded_exact_lexemes before accepting or rejecting it."
    pub(crate) fn longest_lexeme_at<'a>(&'a self, rest: &str) -> Option<(&'a str, bool)> {
        let window = rest.len().min(self.longest_lexeme);
        for length in (1..=window).rev() {
            let Some(candidate) = rest.get(..length) else {
                continue;
            };
            if let Some(found) = self.adopted.iter().find(|s| *s == candidate) {
                return Some((found.as_str(), true));
            }
            if let Some(found) = self.excluded.iter().find(|s| *s == candidate) {
                return Some((found.as_str(), false));
            }
        }
        None
    }
}

fn is_registered_word_shape(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn check_declared_keys(
    kind: &'static str,
    value: Option<&Json>,
    implemented: &[&str],
) -> Result<(), LexiconError> {
    let declared: Vec<String> = value
        .and_then(Json::as_object)
        .map(|o| o.iter().map(|(k, _)| k.clone()).collect())
        .unwrap_or_default();
    let declared_set: BTreeSet<&str> = declared.iter().map(String::as_str).collect();
    let implemented_set: BTreeSet<&str> = implemented.iter().copied().collect();
    if declared_set != implemented_set {
        return Err(LexiconError::UnimplementedSymbolClass {
            kind,
            declared: declared_set.iter().map(|s| (*s).to_string()).collect(),
            implemented: implemented_set.iter().map(|s| (*s).to_string()).collect(),
        });
    }
    Ok(())
}

fn char_list(value: Option<&Json>, what: &str) -> Result<Vec<char>, LexiconError> {
    let items = value
        .and_then(Json::as_array)
        .ok_or_else(|| LexiconError::Malformed(format!("{what} is not an array")))?;
    let mut out = Vec::new();
    for item in items {
        let s = item
            .as_str()
            .ok_or_else(|| LexiconError::Malformed(format!("{what} holds a non-string")))?;
        let mut chars = s.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => out.push(c),
            _ => {
                return Err(LexiconError::Malformed(format!(
                    "{what} entry {s:?} is not one character"
                )))
            }
        }
    }
    Ok(out)
}

fn bool_field(value: &Json, key: &str, what: &str) -> Result<bool, LexiconError> {
    value
        .get(key)
        .and_then(Json::as_bool)
        .ok_or_else(|| LexiconError::Malformed(format!("{what}.{key} missing or not boolean")))
}

fn expect_error(value: Option<&Json>, what: &str) -> Result<(), LexiconError> {
    match value.and_then(Json::as_str) {
        Some(LITERAL_INVALID) => Ok(()),
        other => Err(LexiconError::Malformed(format!(
            "{what} is {other:?}, expected {LITERAL_INVALID:?}"
        ))),
    }
}
