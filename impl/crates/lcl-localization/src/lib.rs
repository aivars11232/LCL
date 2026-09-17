//! # lcl-localization — the LCL 0.2.0 localization stage
//!
//! LCL-FEATURE-04. Before any word of a document is interpreted, this crate
//! decides which locale profile, if any, spells that document's reserved words,
//! and whether every word outside string literals is acceptable under it.
//!
//! ## Authority
//!
//! The contract is `10_REGISTRIES/localization_surface_v0.2.0.json`,
//! `10_REGISTRIES/locale_profile_schema_v0.2.0.json` and
//! `02_LEXICAL/13_LOCALIZED_SOURCE_AND_LOCALE_DIRECTIVE.txt`. [`Contract::load`]
//! reads the repertoires, confusable skeletons, reserved words, localizable
//! words, display domains and provider classes out of the authoritative 0.2.0
//! package. The diagnostic identifiers in [`ids`] are checked against the
//! statuses registry at load, so a registry change fails closed.
//!
//! ## Scope
//!
//! The output is a selection record, localization-stage diagnostics, and the
//! selected profile whose spellings the lexer maps to canonical reserved words.
//! Nothing here parses grammar, builds an AST, resolves or executes. Source
//! bytes are never rewritten and no canonical-spelling copy is produced.
//!
//! ## Providers
//!
//! A [`LocaleDetector`] proposes candidate locales and a
//! [`LocaleProfileResolver`] supplies profile bytes. Both are provider-neutral
//! and untrusted: every profile is validated with [`validate_profile`] and bound
//! to its content identity before any word is interpreted.

use lcl_spec::json::{self, Json};
use lcl_spec::{sha256, SpecPackage};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The language version whose localization contract this crate implements.
pub const LANGUAGE_VERSION: &str = "0.2.0";

/// `locale_profile_schema_v0.2.0.json#/profile_format`.
pub const PROFILE_FORMAT: &str = "lcl-locale-profile/1";

/// Upper bound on the bytes read for one locale profile.
pub const MAX_PROFILE_BYTES: usize = 1 << 20;

/// Host ceiling on the locale profile files one engine is given. A product
/// limit on configuration, not a language rule.
pub const MAX_PROFILE_FILES: usize = 256;

/// Identity of the built-in [`CoverageDetector`].
pub const COVERAGE_DETECTOR_IDENTITY: &str = "lcl.detector.profile_coverage/1";

/// The registered diagnostics this stage selects, verified at [`Contract::load`].
pub mod ids {
    pub const DIRECTIVE_INVALID: &str = "error.localization.directive_invalid";
    pub const LOCALE_INVALID: &str = "error.localization.locale_invalid";
    pub const PROFILE_UNAVAILABLE: &str = "error.localization.profile_unavailable";
    pub const PROFILE_INVALID: &str = "error.localization.profile_invalid";
    pub const PROFILE_DRIFT: &str = "error.localization.profile_drift";
    pub const DETECTION_AMBIGUOUS: &str = "error.localization.detection_ambiguous";
    pub const DETECTION_FAILED: &str = "error.localization.detection_failed";
    pub const MIXED: &str = "error.localization.mixed";
    pub const CONFUSABLE: &str = "error.localization.confusable";
    /// Lexical, not localization-stage: an unmapped word under a profile.
    pub const KEYWORD_UNKNOWN: &str = "error.keyword.unknown";

    /// Every localization-stage identifier.
    pub const LOCALIZATION_STAGE: [&str; 9] = [
        DIRECTIVE_INVALID,
        LOCALE_INVALID,
        PROFILE_UNAVAILABLE,
        PROFILE_INVALID,
        PROFILE_DRIFT,
        DETECTION_AMBIGUOUS,
        DETECTION_FAILED,
        MIXED,
        CONFUSABLE,
    ];
}

const REQUIRED_PROFILE_FIELDS: [&str; 8] = [
    "coverage",
    "format",
    "lcl_version",
    "locale",
    "preferred",
    "provenance",
    "repertoires",
    "spellings",
];
const OPTIONAL_PROFILE_FIELDS: [&str; 1] = ["display_labels"];

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

/// Why the localization contract could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    /// The package did not establish authority.
    NotAuthoritative,
    /// The package is not the language version this crate implements.
    WrongVersion(String),
    /// A registry this contract reads is absent.
    MissingRegistry(&'static str),
    /// A registry does not have the shape this contract requires.
    Malformed(String),
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContractError::NotAuthoritative => {
                f.write_str("the localization contract requires an authoritative package")
            }
            ContractError::WrongVersion(v) => write!(
                f,
                "the localization contract is defined for LCL {LANGUAGE_VERSION}, not {v:?}"
            ),
            ContractError::MissingRegistry(name) => write!(f, "registry {name} is absent"),
            ContractError::Malformed(what) => write!(f, "malformed localization contract: {what}"),
        }
    }
}

impl std::error::Error for ContractError {}

/// Registered metadata of one diagnostic this stage selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorMetadata {
    /// `errors.<id>.meaning`, verbatim.
    pub meaning: String,
    /// `errors.<id>.default_status`, verbatim.
    pub default_status: String,
    /// `diagnostic_selection.specificity_rank` resolved for this identifier.
    pub specificity_rank: u64,
}

/// The localization contract of one authoritative LCL 0.2.0 package.
#[derive(Debug, Clone)]
pub struct Contract {
    error_metadata: BTreeMap<String, ErrorMetadata>,
    reserved: BTreeSet<String>,
    localizable: BTreeSet<String>,
    repertoires: BTreeMap<String, Vec<(u32, u32)>>,
    confusable: BTreeMap<char, char>,
    canonical_skeletons: BTreeMap<String, String>,
    display_members: BTreeSet<String>,
    provider_classes: BTreeSet<String>,
}

impl Contract {
    /// Read the contract from an authoritative 0.2.0 package.
    pub fn load(spec: &SpecPackage) -> Result<Self, ContractError> {
        if !spec.is_authoritative() {
            return Err(ContractError::NotAuthoritative);
        }
        if spec.formal_version() != LANGUAGE_VERSION {
            return Err(ContractError::WrongVersion(
                spec.formal_version().to_string(),
            ));
        }
        let registry = |name: &'static str| {
            spec.registry(name)
                .ok_or(ContractError::MissingRegistry(name))
        };
        let malformed = |what: &str| ContractError::Malformed(what.to_string());

        let reserved: BTreeSet<String> = registry("keywords")?
            .get("keywords")
            .and_then(Json::as_object)
            .ok_or_else(|| malformed("keywords.keywords is not an object"))?
            .iter()
            .map(|(word, _)| word.clone())
            .collect();

        let surface = registry("localization_surface")?;
        let words = surface
            .get("reserved_words")
            .and_then(Json::as_object)
            .ok_or_else(|| malformed("localization_surface.reserved_words is not an object"))?;
        let mut classified = BTreeSet::new();
        let mut localizable = BTreeSet::new();
        for (word, entry) in words {
            classified.insert(word.clone());
            if entry.get("classification").and_then(Json::as_str) == Some("localized_lexeme") {
                localizable.insert(word.clone());
            }
        }
        if classified != reserved {
            return Err(malformed(
                "the localization surface does not classify exactly the reserved words",
            ));
        }

        let schema = registry("locale_profile_schema")?;
        if schema.get("profile_format").and_then(Json::as_str) != Some(PROFILE_FORMAT) {
            return Err(malformed("unexpected profile_format"));
        }
        let mut repertoires = BTreeMap::new();
        for (name, entry) in schema
            .get("lexical_repertoires")
            .and_then(Json::as_object)
            .ok_or_else(|| malformed("lexical_repertoires is not an object"))?
        {
            let Some(letters) = entry.get("letters").and_then(Json::as_array) else {
                continue;
            };
            let ranges = parse_ranges(letters)
                .ok_or_else(|| malformed(&format!("repertoire {name} has a malformed range")))?;
            let count: u64 = ranges
                .iter()
                .map(|(start, end)| u64::from(end - start) + 1)
                .sum();
            let lowercase = ranges
                .iter()
                .any(|(start, end)| *start <= 0x7A && *end >= 0x61);
            if entry.get("letter_count").and_then(Json::as_u64) != Some(count) || lowercase {
                return Err(malformed(&format!(
                    "repertoire {name} letter count or ASCII lowercase exclusion differs"
                )));
            }
            repertoires.insert(name.clone(), ranges);
        }
        if repertoires.is_empty() {
            return Err(malformed("no lexical repertoire is registered"));
        }

        let mut confusable = BTreeMap::new();
        for (source, target) in schema
            .get("confusable_skeleton")
            .and_then(|c| c.get("map"))
            .and_then(Json::as_object)
            .ok_or_else(|| malformed("confusable_skeleton.map is not an object"))?
        {
            let from = parse_code_point(source);
            let to = target.as_str().and_then(parse_code_point);
            match (from, to) {
                (Some(from), Some(to)) => {
                    confusable.insert(from, to);
                }
                _ => return Err(malformed("confusable_skeleton.map has a malformed entry")),
            }
        }

        let provider_classes: BTreeSet<String> = schema
            .get("profile_document")
            .and_then(|d| d.get("provider_classes"))
            .and_then(Json::as_array)
            .ok_or_else(|| malformed("profile_document.provider_classes is not an array"))?
            .iter()
            .filter_map(|c| c.as_str().map(str::to_string))
            .collect();

        let errors = registry("statuses_and_errors")?
            .get("errors")
            .ok_or_else(|| malformed("statuses_and_errors.errors is absent"))?;
        let stage_of = |id: &str| {
            errors
                .get(id)
                .and_then(|e| e.get("stage"))
                .and_then(Json::as_str)
                .map(str::to_string)
        };
        for id in ids::LOCALIZATION_STAGE {
            if stage_of(id).as_deref() != Some("localization") {
                return Err(malformed(&format!(
                    "{id} is not a registered localization-stage error"
                )));
            }
        }
        if stage_of(ids::KEYWORD_UNKNOWN).as_deref() != Some("lexical") {
            return Err(malformed(
                "error.keyword.unknown is not a registered lexical error",
            ));
        }
        let declared: BTreeSet<&str> = schema
            .get("diagnostics")
            .and_then(Json::as_object)
            .ok_or_else(|| malformed("locale_profile_schema.diagnostics is not an object"))?
            .iter()
            .filter_map(|(_, id)| id.as_str())
            .collect();
        let known: BTreeSet<&str> = ids::LOCALIZATION_STAGE
            .iter()
            .copied()
            .chain([ids::KEYWORD_UNKNOWN])
            .collect();
        if declared != known {
            return Err(malformed(
                "the schema diagnostics differ from the identifiers this stage selects",
            ));
        }

        let statuses_and_errors = registry("statuses_and_errors")?;
        let ranks = statuses_and_errors
            .get("diagnostic_selection")
            .and_then(|s| s.get("specificity_rank"));
        let default_rank = ranks
            .and_then(|r| r.get("default_for_every_error"))
            .and_then(Json::as_u64)
            .ok_or_else(|| malformed("specificity_rank.default_for_every_error is absent"))?;
        let mut error_metadata = BTreeMap::new();
        for id in &known {
            let entry = errors
                .get(id)
                .ok_or_else(|| malformed(&format!("{id} is not registered")))?;
            let text = |key: &str| {
                entry
                    .get(key)
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| malformed(&format!("{id}.{key} is absent")))
            };
            let rank = ranks
                .and_then(|r| r.get("overrides"))
                .and_then(|o| o.get(id))
                .and_then(Json::as_u64)
                .unwrap_or(default_rank);
            error_metadata.insert(
                (*id).to_string(),
                ErrorMetadata {
                    meaning: text("meaning")?,
                    default_status: text("default_status")?,
                    specificity_rank: rank,
                },
            );
        }

        let mut display_members = BTreeSet::new();
        if let Some(groups) = registry("built_in_groups_and_results")?
            .get("enum_groups")
            .and_then(Json::as_object)
        {
            for (_, members) in groups {
                let members: Vec<&str> = members
                    .as_array()
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Json::as_str)
                    .collect();
                if !members.is_empty() && members.iter().all(|m| is_qualified_identifier(m)) {
                    display_members.extend(members.iter().map(|m| m.to_string()));
                }
            }
        }
        if let Some(contracts) = registry("operations")?
            .get("contracts")
            .and_then(Json::as_object)
        {
            display_members.extend(contracts.iter().map(|(id, _)| id.clone()));
        }
        let formats = registry("formats_encodings_units")?;
        for key in ["formats", "encodings", "units"] {
            if let Some(entries) = formats.get(key).and_then(Json::as_object) {
                display_members.extend(entries.iter().map(|(id, _)| id.clone()));
            }
        }

        let mut contract = Contract {
            error_metadata,
            reserved,
            localizable,
            repertoires,
            confusable,
            canonical_skeletons: BTreeMap::new(),
            display_members,
            provider_classes,
        };
        contract.canonical_skeletons = contract
            .reserved
            .iter()
            .map(|word| (contract.skeleton(word), word.clone()))
            .collect();
        Ok(contract)
    }

    /// Registered metadata of a diagnostic this stage selects.
    pub fn error_metadata(&self, id: &str) -> Option<&ErrorMetadata> {
        self.error_metadata.get(id)
    }

    /// The inclusive code point ranges of every registered repertoire.
    pub fn letter_ranges(&self) -> Vec<(u32, u32)> {
        self.repertoires.values().flatten().copied().collect()
    }

    /// The repertoire whose letters include `c`, if any.
    pub fn repertoire_of(&self, c: char) -> Option<&str> {
        let v = c as u32;
        self.repertoires
            .iter()
            .find(|(_, ranges)| {
                ranges
                    .iter()
                    .any(|(start, end)| (*start..=*end).contains(&v))
            })
            .map(|(name, _)| name.as_str())
    }

    /// True when `c` is a letter of a registered repertoire.
    pub fn is_repertoire_letter(&self, c: char) -> bool {
        self.repertoire_of(c).is_some()
    }

    /// True when `word` is a canonical reserved word.
    pub fn is_reserved(&self, word: &str) -> bool {
        self.reserved.contains(word)
    }

    /// The confusable skeleton of `word`.
    pub fn skeleton(&self, word: &str) -> String {
        word.chars()
            .map(|c| self.confusable.get(&c).copied().unwrap_or(c))
            .collect()
    }

    /// A scalar that can occur inside a source word.
    pub fn is_word_scalar(&self, c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_' || self.is_repertoire_letter(c)
    }

    fn repertoire_letters(&self, name: &str) -> Option<&[(u32, u32)]> {
        self.repertoires.get(name).map(Vec::as_slice)
    }
}

fn parse_code_point(text: &str) -> Option<char> {
    let hex = text.strip_prefix("U+")?;
    if !(4..=6).contains(&hex.len())
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
    {
        return None;
    }
    char::from_u32(u32::from_str_radix(hex, 16).ok()?)
}

fn parse_ranges(items: &[Json]) -> Option<Vec<(u32, u32)>> {
    items
        .iter()
        .map(|item| {
            let text = item.as_str()?;
            match text.split_once("..") {
                Some((start, end)) => {
                    let start = parse_code_point(start)? as u32;
                    let end = parse_code_point(end)? as u32;
                    (start <= end).then_some((start, end))
                }
                None => {
                    let single = parse_code_point(text)? as u32;
                    Some((single, single))
                }
            }
        })
        .collect()
}

fn is_simple_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_qualified_identifier(text: &str) -> bool {
    let segments: Vec<&str> = text.split('.').collect();
    segments.len() >= 2 && segments.iter().all(|s| is_simple_identifier(s))
}

// ---------------------------------------------------------------------------
// Locale tags
// ---------------------------------------------------------------------------

/// A normalized locale tag of the closed BCP 47 subset
/// `language ["-" script] ["-" region]`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocaleTag(String);

impl LocaleTag {
    /// Parse and normalize. Comparison is ASCII case-insensitive; the normalized
    /// form writes the language in lowercase, the script with an uppercase first
    /// letter, and the region in uppercase.
    pub fn parse(text: &str) -> Result<LocaleTag, String> {
        let invalid = || format!("{text:?} is not a well-formed locale tag");
        if !text.is_ascii() {
            return Err(invalid());
        }
        let parts: Vec<&str> = text.split('-').collect();
        let language = parts.first().copied().unwrap_or("");
        if !(2..=3).contains(&language.len()) || !language.bytes().all(|b| b.is_ascii_alphabetic())
        {
            return Err(invalid());
        }
        let mut normalized = language.to_ascii_lowercase();
        let mut rest = &parts[1..];
        if let Some(script) = rest.first() {
            if script.len() == 4 && script.bytes().all(|b| b.is_ascii_alphabetic()) {
                normalized.push('-');
                normalized.push_str(&script[..1].to_ascii_uppercase());
                normalized.push_str(&script[1..].to_ascii_lowercase());
                rest = &rest[1..];
            }
        }
        if let Some(region) = rest.first() {
            let letters = region.len() == 2 && region.bytes().all(|b| b.is_ascii_alphabetic());
            let digits = region.len() == 3 && region.bytes().all(|b| b.is_ascii_digit());
            if letters || digits {
                normalized.push('-');
                normalized.push_str(&region.to_ascii_uppercase());
                rest = &rest[1..];
            }
        }
        if !rest.is_empty() {
            return Err(invalid());
        }
        Ok(LocaleTag(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LocaleTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// Locale profiles
// ---------------------------------------------------------------------------

/// A rejected locale profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileError {
    /// `error.localization.profile_invalid` or `error.localization.locale_invalid`.
    pub id: &'static str,
    pub detail: String,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.id, self.detail)
    }
}

impl std::error::Error for ProfileError {}

/// A validated locale profile, bound to the identity of its exact bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    locale: LocaleTag,
    identity: String,
    repertoires: Vec<String>,
    spellings: BTreeMap<String, String>,
    preferred: BTreeMap<String, String>,
    skeletons: BTreeSet<String>,
    provider_class: String,
    provider_identity: String,
    complete: bool,
    mapped_reserved_words: usize,
}

impl Profile {
    pub fn locale(&self) -> &LocaleTag {
        &self.locale
    }

    /// `sha256:` followed by the lowercase hex digest of the validated bytes.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// The canonical reserved word `spelling` denotes under this profile.
    pub fn canonical(&self, spelling: &str) -> Option<&str> {
        self.spellings.get(spelling).map(String::as_str)
    }

    /// The preferred spelling of a canonical reserved word, for rendering.
    pub fn preferred(&self, word: &str) -> Option<&str> {
        self.preferred.get(word).map(String::as_str)
    }

    /// Every spelling and the canonical reserved word it denotes.
    pub fn spellings(&self) -> &BTreeMap<String, String> {
        &self.spellings
    }

    pub fn repertoires(&self) -> &[String] {
        &self.repertoires
    }

    pub fn provider_class(&self) -> &str {
        &self.provider_class
    }

    pub fn provider_identity(&self) -> &str {
        &self.provider_identity
    }

    /// Whether the profile maps every localizable reserved word.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn mapped_reserved_words(&self) -> usize {
        self.mapped_reserved_words
    }

    fn is_confusable_with_spelling(&self, skeleton: &str) -> bool {
        self.skeletons.contains(skeleton)
    }
}

/// The content identity of profile bytes.
pub fn content_identity(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256::hex_digest(bytes))
}

/// Validate one locale profile under `locale_profile_schema_v0.2.0.json`.
///
/// `expected` is the locale the caller selected; a profile for another locale is
/// invalid. A profile that fails any rule is rejected whole.
pub fn validate_profile(
    contract: &Contract,
    bytes: &[u8],
    expected: Option<&LocaleTag>,
) -> Result<Profile, ProfileError> {
    let invalid = |detail: String| ProfileError {
        id: ids::PROFILE_INVALID,
        detail,
    };
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(invalid(format!(
            "profile exceeds {MAX_PROFILE_BYTES} bytes"
        )));
    }
    if bytes.starts_with(b"\xEF\xBB\xBF") {
        return Err(invalid("profile has a byte-order mark".into()));
    }
    let identity = content_identity(bytes);
    let text =
        std::str::from_utf8(bytes).map_err(|e| invalid(format!("profile is not UTF-8: {e}")))?;
    let document =
        json::parse(text).map_err(|e| invalid(format!("profile is not strict JSON: {e}")))?;
    let fields = document
        .as_object()
        .ok_or_else(|| invalid("profile is not an object".into()))?;
    let names: BTreeSet<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
    let required: BTreeSet<&str> = REQUIRED_PROFILE_FIELDS.into_iter().collect();
    let allowed: BTreeSet<&str> = REQUIRED_PROFILE_FIELDS
        .into_iter()
        .chain(OPTIONAL_PROFILE_FIELDS)
        .collect();
    if !required.is_subset(&names) || !names.is_subset(&allowed) {
        return Err(invalid(format!("profile fields are not exact: {names:?}")));
    }
    let field = |name: &str| document.get(name).unwrap_or(&Json::Null);

    if field("format").as_str() != Some(PROFILE_FORMAT) {
        return Err(invalid("unknown profile format".into()));
    }
    if field("lcl_version").as_str() != Some(LANGUAGE_VERSION) {
        return Err(invalid("incompatible LCL version".into()));
    }
    let written = field("locale").as_str().ok_or_else(|| ProfileError {
        id: ids::LOCALE_INVALID,
        detail: "locale is not a string".into(),
    })?;
    let locale = LocaleTag::parse(written).map_err(|detail| ProfileError {
        id: ids::LOCALE_INVALID,
        detail,
    })?;
    if locale.as_str() != written {
        return Err(invalid("locale is not written in normalized form".into()));
    }
    if let Some(expected) = expected {
        if &locale != expected {
            return Err(invalid(format!(
                "profile locale {locale} is not the selected {expected}"
            )));
        }
    }

    let repertoires: Vec<String> = field("repertoires")
        .as_array()
        .ok_or_else(|| invalid("repertoires is not an array".into()))?
        .iter()
        .map(|r| r.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| invalid("repertoires holds a non-string".into()))?;
    let strictly_sorted = repertoires.windows(2).all(|w| w[0] < w[1]);
    if repertoires.is_empty()
        || !strictly_sorted
        || repertoires
            .iter()
            .any(|r| contract.repertoire_letters(r).is_none())
    {
        return Err(invalid(format!(
            "repertoires are not a sorted set of registered names: {repertoires:?}"
        )));
    }

    let pairs = field("spellings")
        .as_object()
        .ok_or_else(|| invalid("spellings is not an object".into()))?;
    if pairs.is_empty() {
        return Err(invalid("spellings is empty".into()));
    }
    let mut spellings = BTreeMap::new();
    let mut skeletons: BTreeMap<String, String> = BTreeMap::new();
    for (spelling, word) in pairs {
        let word = word
            .as_str()
            .filter(|w| contract.is_reserved(w))
            .ok_or_else(|| invalid(format!("{spelling:?} maps to an unknown reserved word")))?;
        if !contract.localizable.contains(word) {
            return Err(invalid(format!("{word} is not localizable")));
        }
        let length = spelling.chars().count();
        if !(1..=64).contains(&length) {
            return Err(invalid(format!("{spelling:?} length")));
        }
        let mut chars = spelling.chars();
        let first = chars.next().unwrap_or('\0');
        let repertoire = contract
            .repertoire_of(first)
            .filter(|r| repertoires.iter().any(|listed| listed == r))
            .ok_or_else(|| {
                invalid(format!(
                    "{spelling:?} does not start with a letter of a listed repertoire"
                ))
            })?;
        let letters = contract.repertoire_letters(repertoire).unwrap_or(&[]);
        for c in chars {
            let v = c as u32;
            let continuation = c.is_ascii_digit() || c == '_';
            let letter = letters
                .iter()
                .any(|(start, end)| (*start..=*end).contains(&v));
            if !continuation && !letter {
                return Err(invalid(format!(
                    "{spelling:?} has U+{v:04X} outside repertoire {repertoire}"
                )));
            }
        }
        if is_simple_identifier(spelling) || is_qualified_identifier(spelling) {
            return Err(invalid(format!("{spelling:?} has an identifier form")));
        }
        if contract.is_reserved(spelling) && spelling != word {
            return Err(invalid(format!(
                "{spelling:?} is the canonical reserved word {spelling}, not {word}"
            )));
        }
        let skeleton = contract.skeleton(spelling);
        if let Some(canonical) = contract.canonical_skeletons.get(&skeleton) {
            if canonical != word {
                return Err(invalid(format!(
                    "{spelling:?} is confusable with canonical {canonical}"
                )));
            }
        }
        if let Some(other) = skeletons.get(&skeleton) {
            return Err(invalid(format!(
                "{spelling:?} is confusable with {other:?}"
            )));
        }
        skeletons.insert(skeleton, spelling.clone());
        spellings.insert(spelling.clone(), word.to_string());
    }

    let mut preferred = BTreeMap::new();
    for (word, spelling) in field("preferred")
        .as_object()
        .ok_or_else(|| invalid("preferred is not an object".into()))?
    {
        let spelling = spelling.as_str().unwrap_or("");
        if spellings.get(spelling).map(String::as_str) != Some(word.as_str()) {
            return Err(invalid(
                "preferred spellings do not map to their reserved words".into(),
            ));
        }
        preferred.insert(word.clone(), spelling.to_string());
    }

    let mapped: BTreeSet<&str> = spellings.values().map(String::as_str).collect();
    let coverage = field("coverage");
    let coverage_keys: BTreeSet<&str> = coverage
        .as_object()
        .unwrap_or(&[])
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    let complete = mapped.len() == contract.reserved.len();
    if coverage_keys != BTreeSet::from(["complete", "mapped_reserved_words"])
        || coverage.get("mapped_reserved_words").and_then(Json::as_u64) != Some(mapped.len() as u64)
        || coverage.get("complete").and_then(Json::as_bool) != Some(complete)
    {
        return Err(invalid("coverage does not describe the mapping".into()));
    }

    let provenance = field("provenance");
    let provenance_keys: BTreeSet<&str> = provenance
        .as_object()
        .unwrap_or(&[])
        .iter()
        .map(|(k, _)| k.as_str())
        .collect();
    let provider_class = provenance
        .get("provider_class")
        .and_then(Json::as_str)
        .unwrap_or("");
    let provider_identity = provenance
        .get("provider_identity")
        .and_then(Json::as_str)
        .unwrap_or("");
    if provenance_keys != BTreeSet::from(["provider_class", "provider_identity"])
        || !contract.provider_classes.contains(provider_class)
        || provider_identity.is_empty()
        || !provider_identity.is_ascii()
    {
        return Err(invalid("provenance is not exact".into()));
    }

    if let Some(labels) = document.get("display_labels") {
        let entries = labels
            .as_object()
            .ok_or_else(|| invalid("display_labels is not an object".into()))?;
        for (id, label) in entries {
            if !contract.display_members.contains(id) || label.as_str().map_or(true, str::is_empty)
            {
                return Err(invalid(
                    "display_labels name unregistered identifiers".into(),
                ));
            }
        }
    }

    Ok(Profile {
        locale,
        identity,
        repertoires,
        mapped_reserved_words: mapped.len(),
        complete,
        spellings,
        preferred,
        skeletons: skeletons.into_keys().collect(),
        provider_class: provider_class.to_string(),
        provider_identity: provider_identity.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

/// A resolver or detector could not be consulted at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderUnavailable(pub String);

/// Supplies locale profile bytes. Output is untrusted until validated.
pub trait LocaleProfileResolver {
    /// A stable, non-secret identity for this resolver.
    fn identity(&self) -> &str;
    /// Locales this resolver can supply now, in ascending order.
    fn available_locales(&self) -> Result<Vec<LocaleTag>, ProviderUnavailable>;
    /// The profile bytes for `locale`, or `None` when it has none.
    fn resolve(&self, locale: &LocaleTag) -> Result<Option<Vec<u8>>, ProviderUnavailable>;
}

/// Proposes candidate locales for a document's non-canonical words.
pub trait LocaleDetector {
    /// A stable, non-secret identity for this detector.
    fn identity(&self) -> &str;
    /// Candidate locales in the detector's order. Proposing a locale selects
    /// nothing: selection still requires a validated profile that spells a word.
    fn propose(&self, words: &[&str], available: &[LocaleTag]) -> Vec<LocaleTag>;
}

/// The built-in detector: every available locale is a candidate, and profile
/// coverage of the document's words decides.
#[derive(Debug, Clone, Copy, Default)]
pub struct CoverageDetector;

impl LocaleDetector for CoverageDetector {
    fn identity(&self) -> &str {
        COVERAGE_DETECTOR_IDENTITY
    }

    fn propose(&self, _words: &[&str], available: &[LocaleTag]) -> Vec<LocaleTag> {
        let mut candidates = available.to_vec();
        candidates.sort();
        candidates.dedup();
        candidates
    }
}

/// Profiles held in memory: fixtures, caches, and dynamically obtained bytes.
#[derive(Debug, Clone, Default)]
pub struct MemoryResolver {
    identity: String,
    profiles: BTreeMap<LocaleTag, Vec<u8>>,
}

impl MemoryResolver {
    pub fn new(identity: impl Into<String>) -> Self {
        Self {
            identity: identity.into(),
            profiles: BTreeMap::new(),
        }
    }

    /// Hold `bytes` as the profile offered for `locale`. The bytes are not
    /// validated here; selection validates them.
    pub fn insert(&mut self, locale: LocaleTag, bytes: Vec<u8>) {
        self.profiles.insert(locale, bytes);
    }
}

impl LocaleProfileResolver for MemoryResolver {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn available_locales(&self) -> Result<Vec<LocaleTag>, ProviderUnavailable> {
        Ok(self.profiles.keys().cloned().collect())
    }

    fn resolve(&self, locale: &LocaleTag) -> Result<Option<Vec<u8>>, ProviderUnavailable> {
        Ok(self.profiles.get(locale).cloned())
    }
}

/// Profiles stored as `<normalized locale>.json` in one directory.
#[derive(Debug, Clone)]
pub struct DirectoryResolver {
    identity: String,
    dir: PathBuf,
}

impl DirectoryResolver {
    pub fn new(identity: impl Into<String>, dir: impl AsRef<Path>) -> Self {
        Self {
            identity: identity.into(),
            dir: dir.as_ref().to_path_buf(),
        }
    }
}

impl LocaleProfileResolver for DirectoryResolver {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn available_locales(&self) -> Result<Vec<LocaleTag>, ProviderUnavailable> {
        let entries = std::fs::read_dir(&self.dir)
            .map_err(|e| ProviderUnavailable(format!("{}: {e}", self.dir.display())))?;
        let mut locales = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(stem) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
                continue;
            };
            if let Ok(tag) = LocaleTag::parse(stem) {
                if tag.as_str() == stem {
                    locales.push(tag);
                }
            }
        }
        locales.sort();
        Ok(locales)
    }

    fn resolve(&self, locale: &LocaleTag) -> Result<Option<Vec<u8>>, ProviderUnavailable> {
        let path = self.dir.join(format!("{}.json", locale.as_str()));
        match read_profile_file(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ProviderUnavailable(format!("{}: {e}", path.display()))),
        }
    }
}

/// Read a locale profile file, bounded before allocation: at most
/// [`MAX_PROFILE_BYTES`] + 1 bytes, so an oversized or endless file is rejected
/// by [`validate_profile`] as too large instead of being read whole.
pub fn read_profile_file(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_PROFILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// A resolver that cannot be consulted.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableResolver;

impl LocaleProfileResolver for UnavailableResolver {
    fn identity(&self) -> &str {
        "lcl.resolver.unavailable"
    }

    fn available_locales(&self) -> Result<Vec<LocaleTag>, ProviderUnavailable> {
        Err(ProviderUnavailable(
            "no locale profile resolver is available".into(),
        ))
    }

    fn resolve(&self, _locale: &LocaleTag) -> Result<Option<Vec<u8>>, ProviderUnavailable> {
        Err(ProviderUnavailable(
            "no locale profile resolver is available".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Selection and word classification
// ---------------------------------------------------------------------------

/// How the locale of a document was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Explicit,
    Pinned,
    Auto,
    Canonical,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Explicit => "explicit",
            Method::Pinned => "pinned",
            Method::Auto => "auto",
            Method::Canonical => "canonical",
        }
    }
}

/// A recorded locale and profile identity that a document must reproduce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub locale: LocaleTag,
    pub identity: String,
}

/// Presentation and reproducibility metadata of one selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionRecord {
    pub method: Method,
    pub locale: Option<LocaleTag>,
    pub profile_identity: Option<String>,
    pub detector_identity: Option<String>,
    pub candidate_locales: Vec<LocaleTag>,
    pub lcl_version: &'static str,
}

/// One registered localization-stage diagnostic at a source byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizationDiagnostic {
    pub id: &'static str,
    /// Zero-based byte offset of the first offending byte.
    pub offset: usize,
    /// Exclusive end of the offending bytes; equal to `offset` when no source
    /// bytes are the cause.
    pub end: usize,
    pub detail: String,
}

/// A candidate word outside string literals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceWord {
    pub text: String,
    /// Zero-based byte offset of the word in the source.
    pub offset: usize,
}

/// The result of the localization stage for one source unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Localization {
    /// The selection, when one was made.
    pub record: Option<SelectionRecord>,
    /// The selected profile, when the method is not canonical.
    pub profile: Option<Profile>,
    /// Byte length of the directive line including its LINE FEED, or 0.
    pub directive_len: usize,
    /// Localization-stage diagnostics in stable order. Non-empty means the
    /// source unit stops before words are lexed.
    pub diagnostics: Vec<LocalizationDiagnostic>,
    /// Candidate words the lexer reports as `error.keyword.unknown`.
    pub unknown_words: Vec<SourceWord>,
    /// The canonical reserved word of every accepted candidate word, in order.
    pub canonical_words: Vec<String>,
}

impl Localization {
    /// True when no localization-stage diagnostic stops the source unit.
    pub fn is_accepted(&self) -> bool {
        self.diagnostics.is_empty()
    }

    fn failed(diagnostics: Vec<LocalizationDiagnostic>) -> Self {
        Localization {
            record: None,
            profile: None,
            directive_len: 0,
            diagnostics,
            unknown_words: Vec::new(),
            canonical_words: Vec::new(),
        }
    }
}

fn diagnostic(
    id: &'static str,
    offset: usize,
    end: usize,
    detail: impl Into<String>,
) -> LocalizationDiagnostic {
    LocalizationDiagnostic {
        id,
        offset,
        end: end.max(offset),
        detail: detail.into(),
    }
}

/// Run the localization stage over one source unit.
///
/// Selection precedence is explicit directive, then pin, then automatic
/// detection, then canonical spelling. Every failure fails closed.
pub fn localize(
    contract: &Contract,
    source: &[u8],
    resolver: &dyn LocaleProfileResolver,
    detector: &dyn LocaleDetector,
    pin: Option<&Pin>,
) -> Localization {
    // `01_FOUNDATION/03` decodes UTF-8 before this stage, and `02_LEXICAL/01`
    // forbids repairing source before validation: bytes that are not UTF-8 hold
    // no words to localize. Nothing is selected, and the lexical stage reports
    // `error.encoding.invalid` on the original offending bytes.
    let Ok(text) = std::str::from_utf8(source) else {
        return Localization::failed(Vec::new());
    };
    let chars: Vec<char> = text.chars().collect();
    let offsets = byte_offsets(&chars);

    let (tag, mut problems) = directive(text);
    if !problems.is_empty() {
        problems.sort_by(|a, b| (a.offset, a.id).cmp(&(b.offset, b.id)));
        return Localization::failed(problems);
    }
    let skip_chars = if tag.is_some() {
        chars
            .iter()
            .position(|c| *c == '\n')
            .map_or(chars.len(), |i| i + 1)
    } else {
        0
    };
    let directive_len = offsets.get(skip_chars).copied().unwrap_or(0);
    let mask = outside_string_mask(&chars);
    let candidates = words(contract, &chars, &offsets, &mask, skip_chars);
    let non_canonical: Vec<&SourceWord> = candidates
        .iter()
        .filter(|w| !contract.is_reserved(&w.text))
        .collect();
    let first_offset = non_canonical.first().map_or(0, |w| w.offset);
    let first_end = non_canonical.first().map_or(0, |w| w.offset + w.text.len());

    let obtain = |locale: &LocaleTag| -> Result<Profile, (&'static str, String)> {
        match resolver.resolve(locale) {
            Err(ProviderUnavailable(detail)) => Err((ids::PROFILE_UNAVAILABLE, detail)),
            Ok(None) => Err((
                ids::PROFILE_UNAVAILABLE,
                format!("no locale profile is available for {locale}"),
            )),
            Ok(Some(bytes)) => {
                validate_profile(contract, &bytes, Some(locale)).map_err(|e| (e.id, e.detail))
            }
        }
    };

    let (record, selected) = if let Some(tag) = tag {
        if let Some(pin) = pin {
            if pin.locale != tag {
                return Localization::failed(vec![diagnostic(
                    ids::PROFILE_DRIFT,
                    0,
                    0,
                    format!(
                        "the directive selects {tag}, the pin records {}",
                        pin.locale
                    ),
                )]);
            }
        }
        let profile = match obtain(&tag) {
            Ok(profile) => profile,
            Err((id, detail)) => return Localization::failed(vec![diagnostic(id, 0, 0, detail)]),
        };
        if let Some(pin) = pin {
            if pin.identity != profile.identity {
                return Localization::failed(vec![diagnostic(
                    ids::PROFILE_DRIFT,
                    0,
                    0,
                    format!("pinned {}, resolved {}", pin.identity, profile.identity),
                )]);
            }
        }
        let record = SelectionRecord {
            method: Method::Explicit,
            locale: Some(tag),
            profile_identity: Some(profile.identity.clone()),
            detector_identity: None,
            candidate_locales: Vec::new(),
            lcl_version: LANGUAGE_VERSION,
        };
        (record, Some(profile))
    } else if let Some(pin) = pin {
        let profile = match obtain(&pin.locale) {
            Ok(profile) => profile,
            Err((id, detail)) => return Localization::failed(vec![diagnostic(id, 0, 0, detail)]),
        };
        if pin.identity != profile.identity {
            return Localization::failed(vec![diagnostic(
                ids::PROFILE_DRIFT,
                0,
                0,
                format!("pinned {}, resolved {}", pin.identity, profile.identity),
            )]);
        }
        let record = SelectionRecord {
            method: Method::Pinned,
            locale: Some(pin.locale.clone()),
            profile_identity: Some(profile.identity.clone()),
            detector_identity: None,
            candidate_locales: Vec::new(),
            lcl_version: LANGUAGE_VERSION,
        };
        (record, Some(profile))
    } else if non_canonical.is_empty() {
        let record = SelectionRecord {
            method: Method::Canonical,
            locale: None,
            profile_identity: None,
            detector_identity: None,
            candidate_locales: Vec::new(),
            lcl_version: LANGUAGE_VERSION,
        };
        (record, None)
    } else {
        let available = match resolver.available_locales() {
            Ok(available) => available,
            Err(ProviderUnavailable(detail)) => {
                return Localization::failed(vec![diagnostic(
                    ids::PROFILE_UNAVAILABLE,
                    first_offset,
                    first_end,
                    detail,
                )])
            }
        };
        let word_texts: Vec<&str> = non_canonical.iter().map(|w| w.text.as_str()).collect();
        let candidates_locales = detector.propose(&word_texts, &available);
        let mut matches: Vec<Profile> = Vec::new();
        for locale in &candidates_locales {
            let profile = match obtain(locale) {
                Ok(profile) => profile,
                Err((ids::PROFILE_UNAVAILABLE, _)) => continue,
                Err((id, detail)) => {
                    return Localization::failed(vec![diagnostic(
                        id,
                        first_offset,
                        first_end,
                        detail,
                    )])
                }
            };
            if word_texts.iter().any(|w| profile.canonical(w).is_some()) {
                matches.push(profile);
            }
        }
        matches.sort_by(|a, b| a.locale.cmp(&b.locale));
        matches.dedup_by(|a, b| a.locale == b.locale);
        if matches.len() > 1 {
            let names: Vec<&str> = matches.iter().map(|p| p.locale.as_str()).collect();
            return Localization::failed(vec![diagnostic(
                ids::DETECTION_AMBIGUOUS,
                first_offset,
                first_end,
                format!("several profiles spell words of this source: {names:?}"),
            )]);
        }
        let Some(profile) = matches.pop() else {
            return Localization::failed(vec![diagnostic(
                ids::DETECTION_FAILED,
                first_offset,
                first_end,
                "no available validated profile spells a word of this source",
            )]);
        };
        let record = SelectionRecord {
            method: Method::Auto,
            locale: Some(profile.locale.clone()),
            profile_identity: Some(profile.identity.clone()),
            detector_identity: Some(detector.identity().to_string()),
            candidate_locales: candidates_locales,
            lcl_version: LANGUAGE_VERSION,
        };
        (record, Some(profile))
    };

    let mut diagnostics = Vec::new();
    let mut unknown_words = Vec::new();
    let mut canonical_words = Vec::new();
    for word in candidates {
        match &selected {
            None => {
                if contract.is_reserved(&word.text) {
                    canonical_words.push(word.text.clone());
                } else {
                    unknown_words.push(word);
                }
            }
            Some(profile) => {
                if let Some(canonical) = profile.canonical(&word.text) {
                    canonical_words.push(canonical.to_string());
                } else if contract.is_reserved(&word.text) {
                    diagnostics.push(diagnostic(
                        ids::MIXED,
                        word.offset,
                        word.offset + word.text.len(),
                        format!(
                            "`{}` is a canonical reserved word that {} does not spell",
                            word.text, profile.locale
                        ),
                    ));
                } else if mixes_repertoires(contract, &word.text)
                    || profile.is_confusable_with_spelling(&contract.skeleton(&word.text))
                {
                    diagnostics.push(diagnostic(
                        ids::CONFUSABLE,
                        word.offset,
                        word.offset + word.text.len(),
                        format!(
                            "`{}` mixes repertoires or is confusable with a spelling of {}",
                            word.text, profile.locale
                        ),
                    ));
                } else {
                    unknown_words.push(word);
                }
            }
        }
    }
    diagnostics.sort_by(|a, b| (a.offset, a.id).cmp(&(b.offset, b.id)));
    Localization {
        record: Some(record),
        profile: selected,
        directive_len,
        diagnostics,
        unknown_words,
        canonical_words,
    }
}

/// True when any line of `source` begins with `@locale`, well formed or not.
pub fn has_locale_directive(source: &[u8]) -> bool {
    source
        .split(|b| *b == b'\n')
        .any(|line| line.starts_with(b"@locale"))
}

/// Whether a rejected localization decides a document's result.
///
/// Owner decision D9 (2026-09-15): the localization-stage diagnostic is the
/// result whenever the source has a locale directive, or localization failed for
/// any reason other than `error.localization.detection_failed` without one. A
/// source in which no available profile recognised any word keeps the result of
/// its canonical reading.
pub fn localization_decides(localization: &Localization, source: &[u8]) -> bool {
    !localization.is_accepted()
        && (has_locale_directive(source)
            || localization
                .diagnostics
                .iter()
                .any(|d| d.id != ids::DETECTION_FAILED))
}

fn mixes_repertoires(contract: &Contract, word: &str) -> bool {
    let repertoires: BTreeSet<&str> = word
        .chars()
        .filter(|c| !(c.is_ascii_digit() || *c == '_'))
        .filter_map(|c| contract.repertoire_of(c))
        .collect();
    repertoires.len() > 1
}

fn byte_offsets(chars: &[char]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut total = 0usize;
    for c in chars {
        offsets.push(total);
        total += c.len_utf8();
    }
    offsets.push(total);
    offsets
}

/// The locale directive of `02_LEXICAL/13`: the normalized tag of a valid first
/// line, and every directive problem found.
fn directive(text: &str) -> (Option<LocaleTag>, Vec<LocalizationDiagnostic>) {
    const KEYWORD: &str = "@locale";
    const PREFIX: &str = "@locale ";
    let mut tag = None;
    let mut problems = Vec::new();
    let mut start = 0usize;
    for (number, line) in text.split('\n').enumerate() {
        if line.starts_with(KEYWORD) {
            if number != 0 {
                problems.push(diagnostic(
                    ids::DIRECTIVE_INVALID,
                    start,
                    start + line.len(),
                    "a locale directive occurs after the first line",
                ));
            } else if !line.starts_with(PREFIX)
                || line.matches(' ').count() != 1
                || line.chars().count() == PREFIX.len()
            {
                problems.push(diagnostic(
                    ids::DIRECTIVE_INVALID,
                    start,
                    start + line.len(),
                    "the locale directive is not `@locale`, one SPACE and one tag",
                ));
            } else {
                match LocaleTag::parse(&line[PREFIX.len()..]) {
                    Ok(parsed) => tag = Some(parsed),
                    Err(detail) => problems.push(diagnostic(
                        ids::LOCALE_INVALID,
                        start + PREFIX.len(),
                        start + line.len(),
                        detail,
                    )),
                }
            }
        }
        start += line.len() + 1;
    }
    (tag, problems)
}

/// Which scalars lie outside STRING and MULTILINE_STRING literals.
///
/// The same bounded string recognition as `TOOLS/validate_source_fixtures.py`:
/// `"""` opens and closes a multiline string, `"` a single-line string, and a
/// backslash escapes the next scalar inside either.
fn outside_string_mask(chars: &[char]) -> Vec<bool> {
    #[derive(PartialEq)]
    enum Mode {
        Normal,
        String,
        Multiline,
    }
    let triple = |i: usize| chars.get(i..i + 3) == Some(&['"', '"', '"'][..]);
    let mut outside = vec![true; chars.len()];
    let mut mode = Mode::Normal;
    let mut escaped = false;
    let mut index = 0usize;
    while index < chars.len() {
        if mode == Mode::Normal {
            if triple(index) {
                for slot in outside.iter_mut().skip(index).take(3) {
                    *slot = false;
                }
                mode = Mode::Multiline;
                index += 3;
                continue;
            }
            if chars[index] == '"' {
                outside[index] = false;
                mode = Mode::String;
            }
            index += 1;
            continue;
        }
        outside[index] = false;
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if chars[index] == '\\' {
            escaped = true;
            index += 1;
            continue;
        }
        if mode == Mode::String && chars[index] == '"' {
            mode = Mode::Normal;
            index += 1;
            continue;
        }
        if mode == Mode::Multiline && triple(index) {
            for slot in outside.iter_mut().skip(index).take(3) {
                *slot = false;
            }
            mode = Mode::Normal;
            index += 3;
            continue;
        }
        index += 1;
    }
    outside
}

/// Candidate words outside string literals, after the directive line.
fn words(
    contract: &Contract,
    chars: &[char],
    offsets: &[usize],
    mask: &[bool],
    skip: usize,
) -> Vec<SourceWord> {
    let mut found = Vec::new();
    let mut index = skip;
    while index < chars.len() {
        if mask[index] && contract.is_word_scalar(chars[index]) {
            let mut end = index;
            while end < chars.len() && mask[end] && contract.is_word_scalar(chars[end]) {
                end += 1;
            }
            let word: String = chars[index..end].iter().collect();
            let first = chars[index];
            let starts = first.is_ascii_uppercase() || !first.is_ascii();
            if starts && !word.chars().any(|c| c.is_ascii_lowercase()) {
                found.push(SourceWord {
                    text: word,
                    offset: offsets[index],
                });
            }
            index = end;
        } else {
            index += 1;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_tags_normalize_and_reject_malformed_forms() {
        assert_eq!(LocaleTag::parse("lv-LV").unwrap().as_str(), "lv-LV");
        assert_eq!(LocaleTag::parse("LV-lv").unwrap().as_str(), "lv-LV");
        assert_eq!(
            LocaleTag::parse("zh-hans-cn").unwrap().as_str(),
            "zh-Hans-CN"
        );
        assert_eq!(LocaleTag::parse("es-419").unwrap().as_str(), "es-419");
        for bad in [
            "",
            "l",
            "lv_LV",
            "lv-",
            "lv-LVV",
            "latvian",
            "lv-LV-x",
            "l\u{0101}-LV",
        ] {
            assert!(LocaleTag::parse(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn the_directive_is_first_line_only_and_exact() {
        let (tag, problems) = directive("@locale lv-LV\nLCL:\n");
        assert_eq!(tag.map(|t| t.0), Some("lv-LV".into()));
        assert!(problems.is_empty());

        let (_, problems) = directive("LCL:\n@locale lv-LV\n");
        assert_eq!(problems[0].id, ids::DIRECTIVE_INVALID);
        assert_eq!(problems[0].offset, 5);
        assert_eq!(problems[0].end, 18);

        let (_, problems) = directive("@locale  lv-LV\n");
        assert_eq!(problems[0].id, ids::DIRECTIVE_INVALID);

        let (_, problems) = directive("@locale lv_LV\n");
        assert_eq!(
            (problems[0].id, problems[0].offset),
            (ids::LOCALE_INVALID, 8)
        );
        assert_eq!(problems[0].end, 13);
    }

    #[test]
    fn decision_d9_selects_when_localization_decides() {
        let failed = |id: &'static str| Localization::failed(vec![diagnostic(id, 0, 0, "x")]);
        let plain = b"LCL:\n    VERSION: \"0.2.0\"\n";
        let directed = b"@locale lv-LV\nLCL:\n";
        assert!(!localization_decides(&failed(ids::DETECTION_FAILED), plain));
        assert!(localization_decides(
            &failed(ids::DETECTION_FAILED),
            directed
        ));
        assert!(localization_decides(&failed(ids::MIXED), plain));
        assert!(localization_decides(
            &failed(ids::DETECTION_AMBIGUOUS),
            plain
        ));
        let accepted = Localization {
            diagnostics: Vec::new(),
            ..failed(ids::MIXED)
        };
        assert!(!localization_decides(&accepted, directed));
        assert!(has_locale_directive(b"LCL:\n@locale lv-LV\n"));
        assert!(!has_locale_directive(b"LCL:\n    NAME: \"@locale\"\n"));
    }

    #[test]
    fn strings_hide_words_and_escapes_do_not_close_them() {
        let chars: Vec<char> = "A \"B \\\" C\" D \"\"\"E\"\"\" F".chars().collect();
        let mask = outside_string_mask(&chars);
        let outside: String = chars
            .iter()
            .zip(&mask)
            .map(|(c, o)| if *o { *c } else { ' ' })
            .collect();
        assert_eq!(
            outside.split_whitespace().collect::<Vec<_>>(),
            ["A", "D", "F"]
        );
    }
}
