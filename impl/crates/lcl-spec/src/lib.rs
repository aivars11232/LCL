//! # lcl-spec — specification authority loader
//!
//! Milestone M0 component A.
//!
//! Loads the canonical **LCL Core 0.1.0 bare-language specification** as data
//! and verifies that the bytes on disk are exactly the released bytes.
//!
//! ## Authority model
//!
//! The canonical package is immutable normative input. This crate:
//!
//! * opens the package **read-only** and never writes to it;
//! * verifies every payload file against `MANIFEST.json` and `SHA256SUMS.txt`,
//!   honouring the release's documented acyclic self-exclusions;
//! * **fails closed** on any drift, missing file, or extra file;
//! * pins the formal version, so a future release cannot be loaded silently;
//! * exposes registries as parsed data. No registry table is ever transcribed
//!   into Rust source. Downstream crates read the registries or they have no
//!   data at all.
//!
//! ## Scope boundary
//!
//! This crate performs no lexing, parsing, evaluation, or execution of LCL, and
//! reaching a `Verified` state says nothing about any conformance level. Per
//! `09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt`, parser and runtime evidence
//! is separate evidence that this milestone does not produce.

pub mod anchor;
pub mod json;
pub mod sha256;

pub use anchor::{TrustAnchor, APPROVED_PACKAGE};

use json::Json;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// Formal specification version this build is pinned to.
pub const PINNED_FORMAL_VERSION: &str = "0.1.0";

/// Package-root-relative paths of the three integrity artifacts.
pub const MANIFEST_PATH: &str = "MANIFEST.json";
pub const VALIDATION_REPORT_PATH: &str = "VALIDATION_REPORT.txt";
pub const CHECKSUMS_PATH: &str = "SHA256SUMS.txt";

/// The twelve closed registries, by package-root-relative path.
pub const REGISTRY_FILES: &[(&str, &str)] = &[
    (
        "ambiguous_replacements",
        "10_REGISTRIES/ambiguous_replacements_v0.1.0.json",
    ),
    ("block_schemas", "10_REGISTRIES/block_schemas_v0.1.0.json"),
    (
        "built_in_groups_and_results",
        "10_REGISTRIES/built_in_groups_and_results_v0.1.0.json",
    ),
    (
        "field_signatures",
        "10_REGISTRIES/field_signatures_v0.1.0.json",
    ),
    (
        "formats_encodings_units",
        "10_REGISTRIES/formats_encodings_units_v0.1.0.json",
    ),
    ("keywords", "10_REGISTRIES/keywords_v0.1.0.json"),
    ("operations", "10_REGISTRIES/operations_v0.1.0.json"),
    (
        "operators_and_functions",
        "10_REGISTRIES/operators_and_functions_v0.1.0.json",
    ),
    (
        "semantic_meta_types",
        "10_REGISTRIES/semantic_meta_types_v0.1.0.json",
    ),
    (
        "statuses_and_errors",
        "10_REGISTRIES/statuses_and_errors_v0.1.0.json",
    ),
    ("symbols", "10_REGISTRIES/symbols_v0.1.0.json"),
    ("types", "10_REGISTRIES/types_v0.1.0.json"),
];

/// The two descriptive conformance catalogs.
pub const CATALOG_FILES: &[(&str, &str)] = &[
    (
        "core_conformance_cases",
        "09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json",
    ),
    (
        "language_decision_cases",
        "09_CONFORMANCE/CASES/language_decision_cases_v0.1.0.json",
    ),
];

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum SpecError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: json::JsonError,
    },
    /// Package structure was not what the release documents.
    Structure(String),
    /// The loaded package is not the pinned version.
    VersionMismatch {
        found: String,
        expected: &'static str,
    },
    /// Integrity verification failed; the report lists every defect.
    IntegrityFailed(Box<IntegrityReport>),
    /// A path in an integrity record was unsafe or non-package-relative.
    UnsafePath(String),
    /// The package is internally self-consistent but is NOT the approved
    /// package. This is the forgery case: regenerating `MANIFEST.json` and
    /// `SHA256SUMS.txt` satisfies internal verification but cannot satisfy an
    /// external anchor.
    TrustAnchorMismatch {
        label: &'static str,
        expected: &'static str,
        actual: String,
        internally_consistent: bool,
    },
    /// The package file count disagrees with the anchor.
    TrustAnchorFileCount {
        label: &'static str,
        expected: usize,
        actual: usize,
    },
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecError::Io { path, source } => write!(f, "I/O error reading {}: {source}", path.display()),
            SpecError::Json { path, source } => write!(f, "invalid JSON in {}: {source}", path.display()),
            SpecError::Structure(m) => write!(f, "package structure error: {m}"),
            SpecError::VersionMismatch { found, expected } => write!(
                f,
                "specification version mismatch: package declares {found:?}, this build is pinned to {expected:?}"
            ),
            SpecError::IntegrityFailed(r) => {
                write!(f, "package integrity verification FAILED with {} defect(s)", r.defects.len())
            }
            SpecError::UnsafePath(p) => write!(f, "unsafe path in integrity record: {p:?}"),
            SpecError::TrustAnchorMismatch { label, expected, actual, internally_consistent } => {
                writeln!(f, "TRUST ANCHOR MISMATCH: this is not the approved package.")?;
                writeln!(f, "  approved          : {label}")?;
                writeln!(f, "  expected identity : {expected}")?;
                writeln!(f, "  actual identity   : {actual}")?;
                if *internally_consistent {
                    write!(
                        f,
                        "  note              : this package IS internally self-consistent. Its manifest and checksums agree with its bytes, so internal verification alone would have accepted it. Only the external anchor rejects it."
                    )
                } else {
                    write!(f, "  note              : this package is also internally inconsistent.")
                }
            }
            SpecError::TrustAnchorFileCount { label, expected, actual } => write!(
                f,
                "TRUST ANCHOR MISMATCH: approved package {label} has {expected} files, this package has {actual}"
            ),
        }
    }
}

impl std::error::Error for SpecError {}

// ---------------------------------------------------------------------------
// Integrity reporting
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Defect {
    /// Listed in an integrity record but absent on disk.
    MissingFile { path: String, record: &'static str },
    /// Present on disk but in no integrity record and not an excluded artifact.
    UnrecordedFile { path: String },
    /// Content hash differs from the recorded hash.
    HashMismatch {
        path: String,
        record: &'static str,
        expected: String,
        actual: String,
    },
    /// Byte length differs from the manifest record.
    SizeMismatch {
        path: String,
        expected: u64,
        actual: u64,
    },
    /// A declared count does not match reality.
    CountMismatch {
        subject: String,
        declared: u64,
        actual: u64,
    },
}

impl fmt::Display for Defect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Defect::MissingFile { path, record } => write!(f, "MISSING          {path}  (listed in {record})"),
            Defect::UnrecordedFile { path } => write!(f, "UNRECORDED       {path}"),
            Defect::HashMismatch { path, record, expected, actual } => write!(
                f,
                "HASH_MISMATCH    {path}  ({record})\n                   expected {expected}\n                   actual   {actual}"
            ),
            Defect::SizeMismatch { path, expected, actual } => {
                write!(f, "SIZE_MISMATCH    {path}  expected {expected} bytes, actual {actual}")
            }
            Defect::CountMismatch { subject, declared, actual } => {
                write!(f, "COUNT_MISMATCH   {subject}  declared {declared}, actual {actual}")
            }
        }
    }
}

/// How much trust a loaded package carries.
///
/// The distinction is deliberately not a boolean flag buried in a struct: an
/// `Unverified` package must never be mistaken for the approved release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// Internally consistent AND matches the external trust anchor. This is the
    /// only state in which the package is the approved specification.
    Authoritative,
    /// Loaded without anchor checking. NON-AUTHORITATIVE: internal consistency
    /// says only that the package agrees with its own metadata, which a forger
    /// can arrange. Use for drift reporting and diagnosis, never as normative
    /// input.
    Unverified,
}

impl Authority {
    pub fn is_authoritative(self) -> bool {
        matches!(self, Authority::Authoritative)
    }
}

impl fmt::Display for Authority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Authority::Authoritative => f.write_str("AUTHORITATIVE"),
            Authority::Unverified => f.write_str("UNVERIFIED (non-authoritative)"),
        }
    }
}

/// Outcome of verifying the package against its own integrity records.
///
/// Note the scope: this reports **internal** consistency only. It is necessary
/// but not sufficient for authority; see [`Authority`].
#[derive(Debug, Clone)]
pub struct IntegrityReport {
    pub package_root: PathBuf,
    pub formal_version: String,
    pub status: String,
    pub release_ready: bool,
    /// Files present on disk, package-root-relative.
    pub files_on_disk: usize,
    /// Records in `MANIFEST.json#/files`.
    pub manifest_records: usize,
    /// Records in `SHA256SUMS.txt`.
    pub checksum_records: usize,
    /// Files whose bytes were hashed and matched a manifest record.
    pub manifest_verified: usize,
    /// Files whose bytes were hashed and matched a checksum record.
    pub checksum_verified: usize,
    /// Domain-separated digest over the complete file inventory.
    pub identity_digest: String,
    pub defects: Vec<Defect>,
}

impl IntegrityReport {
    pub fn is_verified(&self) -> bool {
        self.defects.is_empty()
    }

    pub fn summary(&self) -> String {
        // Deliberately not the word "VERIFIED": this report covers internal
        // consistency only, which is necessary but not sufficient for
        // authority. See `Authority`.
        let verdict = if self.is_verified() {
            "INTERNALLY CONSISTENT"
        } else {
            "FAILED"
        };
        format!(
            "{verdict}: {} files on disk, {}/{} manifest records verified, {}/{} checksum records verified, {} defect(s)",
            self.files_on_disk,
            self.manifest_verified,
            self.manifest_records,
            self.checksum_verified,
            self.checksum_records,
            self.defects.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Package
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

/// A verified, read-only handle on the canonical specification package.
///
/// Construction via [`SpecPackage::open`] performs full integrity verification
/// and fails closed. [`SpecPackage::open_unverified`] exists only so that
/// tooling can *report* on a drifted package; it is never the normal path.
pub struct SpecPackage {
    root: PathBuf,
    manifest: Json,
    file_records: BTreeMap<String, FileRecord>,
    checksums: BTreeMap<String, String>,
    registries: BTreeMap<String, Json>,
    catalogs: BTreeMap<String, Json>,
    integrity: IntegrityReport,
    authority: Authority,
}

/// Concise on purpose: a derived `Debug` would dump the entire specification
/// into every failing assertion message.
impl fmt::Debug for SpecPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpecPackage")
            .field("root", &self.root)
            .field("formal_version", &self.integrity.formal_version)
            .field("registries", &self.registries.len())
            .field("catalogs", &self.catalogs.len())
            .field("authority", &self.authority)
            .field("internally_consistent", &self.integrity.is_verified())
            .field("defects", &self.integrity.defects.len())
            .finish()
    }
}

impl SpecPackage {
    /// Open the approved package at `root` and establish full authority.
    ///
    /// Three independent gates, all of which must pass:
    ///
    /// 1. **Version pin** — the package declares [`PINNED_FORMAL_VERSION`].
    /// 2. **Internal integrity** — every byte agrees with `MANIFEST.json` and
    ///    `SHA256SUMS.txt`, with no missing, extra or miscounted file.
    /// 3. **External trust anchor** — the package identity digest equals
    ///    [`APPROVED_PACKAGE`], which is compiled into this crate and therefore
    ///    outside the reach of the package's own metadata.
    ///
    /// Gate 3 is what makes gates 1 and 2 meaningful. Without it, an altered
    /// package whose manifest and checksums were regenerated to match would
    /// pass, because it would be perfectly self-consistent.
    ///
    /// Fails closed on any gate.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, SpecError> {
        Self::open_with_anchor(root, &APPROVED_PACKAGE)
    }

    /// As [`SpecPackage::open`], against an explicitly supplied anchor.
    ///
    /// Exists so that a future approved release can be pinned without weakening
    /// the default, and so tests can exercise anchor behaviour. It does not
    /// accept an anchor read from the package.
    pub fn open_with_anchor(
        root: impl AsRef<Path>,
        anchor: &'static TrustAnchor,
    ) -> Result<Self, SpecError> {
        let mut pkg = Self::open_unverified(root)?;

        if pkg.integrity.formal_version != anchor.formal_version {
            return Err(SpecError::VersionMismatch {
                found: pkg.integrity.formal_version.clone(),
                expected: anchor.formal_version,
            });
        }
        if !pkg.integrity.is_verified() {
            return Err(SpecError::IntegrityFailed(Box::new(pkg.integrity)));
        }
        if pkg.integrity.files_on_disk != anchor.package_file_count {
            return Err(SpecError::TrustAnchorFileCount {
                label: anchor.label,
                expected: anchor.package_file_count,
                actual: pkg.integrity.files_on_disk,
            });
        }
        if pkg.integrity.identity_digest != anchor.identity_digest {
            return Err(SpecError::TrustAnchorMismatch {
                label: anchor.label,
                expected: anchor.identity_digest,
                actual: pkg.integrity.identity_digest.clone(),
                // Reaching here means internal verification already passed.
                internally_consistent: true,
            });
        }

        pkg.authority = Authority::Authoritative;
        Ok(pkg)
    }

    /// Load a package **without** anchor checking.
    ///
    /// The result is [`Authority::Unverified`] and is NOT the approved
    /// specification, whatever its internal integrity report says. Internal
    /// consistency proves only that the package agrees with metadata it
    /// carries itself, which anyone altering the package can regenerate.
    ///
    /// Intended for drift diagnosis and for reporting on a package that fails
    /// [`SpecPackage::open`]. Never use the result as normative input.
    pub fn open_unverified(root: impl AsRef<Path>) -> Result<Self, SpecError> {
        let root = root.as_ref().to_path_buf();
        if !root.is_dir() {
            return Err(SpecError::Structure(format!(
                "package root {} is not a directory",
                root.display()
            )));
        }

        let manifest = read_json(&root.join(MANIFEST_PATH))?;
        let file_records = parse_manifest_files(&manifest)?;
        let checksums = parse_checksums(&read_text(&root.join(CHECKSUMS_PATH))?)?;

        let mut registries = BTreeMap::new();
        for (name, rel) in REGISTRY_FILES {
            registries.insert((*name).to_string(), read_json(&root.join(rel))?);
        }
        let mut catalogs = BTreeMap::new();
        for (name, rel) in CATALOG_FILES {
            catalogs.insert((*name).to_string(), read_json(&root.join(rel))?);
        }

        let integrity = verify(&root, &manifest, &file_records, &checksums)?;

        Ok(Self {
            root,
            manifest,
            file_records,
            checksums,
            registries,
            catalogs,
            integrity,
            authority: Authority::Unverified,
        })
    }

    /// Trust level of this handle.
    pub fn authority(&self) -> Authority {
        self.authority
    }

    /// True only when the external trust anchor matched.
    pub fn is_authoritative(&self) -> bool {
        self.authority.is_authoritative()
    }

    /// Domain-separated digest over the complete file inventory.
    pub fn identity_digest(&self) -> &str {
        &self.integrity.identity_digest
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn integrity(&self) -> &IntegrityReport {
        &self.integrity
    }

    pub fn manifest(&self) -> &Json {
        &self.manifest
    }

    pub fn formal_version(&self) -> &str {
        &self.integrity.formal_version
    }

    pub fn file_records(&self) -> &BTreeMap<String, FileRecord> {
        &self.file_records
    }

    pub fn checksums(&self) -> &BTreeMap<String, String> {
        &self.checksums
    }

    /// A closed registry, by short name (see [`REGISTRY_FILES`]).
    pub fn registry(&self, name: &str) -> Option<&Json> {
        self.registries.get(name)
    }

    /// A descriptive conformance catalog, by short name (see [`CATALOG_FILES`]).
    pub fn catalog(&self, name: &str) -> Option<&Json> {
        self.catalogs.get(name)
    }

    pub fn registry_names(&self) -> Vec<&str> {
        self.registries.keys().map(|s| s.as_str()).collect()
    }

    /// Counts the package declares in `MANIFEST.json#/component_counts`.
    pub fn declared_component_counts(&self) -> BTreeMap<String, u64> {
        let mut out = BTreeMap::new();
        if let Some(Json::Object(members)) = self.manifest.get("component_counts") {
            for (k, v) in members {
                if let Some(n) = v.as_u64() {
                    out.insert(k.clone(), n);
                }
            }
        }
        out
    }

    /// Cardinalities actually observed in the loaded registries, keyed to match
    /// `component_counts`. This is the cross-check that proves the loader reads
    /// the same specification the release describes.
    pub fn observed_component_counts(&self) -> BTreeMap<String, u64> {
        let mut out = BTreeMap::new();
        let mut put = |k: &str, v: Option<usize>| {
            if let Some(v) = v {
                out.insert(k.to_string(), v as u64);
            }
        };

        let count =
            |reg: Option<&Json>, key: &str| -> Option<usize> { reg?.get(key)?.cardinality() };

        put("keywords", count(self.registry("keywords"), "keywords"));
        put(
            "adopted_symbols",
            count(self.registry("symbols"), "adopted"),
        );
        put(
            "excluded_exact_lexemes",
            count(self.registry("symbols"), "excluded_exact_lexemes"),
        );
        put("types", count(self.registry("types"), "types"));
        put("blocks", count(self.registry("block_schemas"), "schemas"));
        put(
            "field_signature_blocks",
            count(self.registry("field_signatures"), "blocks"),
        );
        put(
            "operators",
            count(self.registry("operators_and_functions"), "operators"),
        );
        put(
            "functions",
            count(self.registry("operators_and_functions"), "functions"),
        );
        put(
            "operations",
            count(self.registry("operations"), "contracts"),
        );
        put(
            "statuses",
            count(self.registry("statuses_and_errors"), "statuses"),
        );
        put(
            "errors",
            count(self.registry("statuses_and_errors"), "errors"),
        );
        put(
            "conformance_requirements",
            count(self.catalog("core_conformance_cases"), "cases"),
        );

        // field_signatures: total field signatures across all blocks. Each
        // block carries its signatures under a `fields` member alongside
        // `legal_parents`, `block_occurrence` and friends, so the count is the
        // sum of `fields` cardinalities, not of the blocks' own keys.
        if let Some(Json::Object(blocks)) = self
            .registry("field_signatures")
            .and_then(|r| r.get("blocks"))
        {
            let mut total = 0usize;
            for (_, block) in blocks {
                if let Some(n) = block.get("fields").and_then(|f| f.cardinality()) {
                    total += n;
                }
            }
            out.insert("field_signatures".to_string(), total as u64);
        }

        out
    }

    /// Compare declared and observed counts. An empty result means agreement on
    /// every subject that can be checked from the loaded registries.
    pub fn component_count_defects(&self) -> Vec<Defect> {
        let declared = self.declared_component_counts();
        let observed = self.observed_component_counts();
        let mut defects = Vec::new();
        for (subject, declared_n) in &declared {
            if let Some(actual) = observed.get(subject) {
                if actual != declared_n {
                    defects.push(Defect::CountMismatch {
                        subject: subject.clone(),
                        declared: *declared_n,
                        actual: *actual,
                    });
                }
            }
        }
        defects
    }
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

fn verify(
    root: &Path,
    manifest: &Json,
    file_records: &BTreeMap<String, FileRecord>,
    checksums: &BTreeMap<String, String>,
) -> Result<IntegrityReport, SpecError> {
    let on_disk = walk(root)?;
    let mut defects = Vec::new();
    let mut manifest_verified = 0usize;
    let mut checksum_verified = 0usize;

    // Hash every file on disk exactly once.
    let mut actual: BTreeMap<String, (String, u64)> = BTreeMap::new();
    for rel in &on_disk {
        let bytes = std::fs::read(root.join(rel)).map_err(|e| SpecError::Io {
            path: root.join(rel),
            source: e,
        })?;
        actual.insert(
            rel.clone(),
            (sha256::hex_digest(&bytes), bytes.len() as u64),
        );
    }

    // Manifest records: every listed file must exist and match hash and size.
    for (path, rec) in file_records {
        match actual.get(path) {
            None => defects.push(Defect::MissingFile {
                path: path.clone(),
                record: "MANIFEST.json",
            }),
            Some((hash, size)) => {
                let mut ok = true;
                if *hash != rec.sha256 {
                    ok = false;
                    defects.push(Defect::HashMismatch {
                        path: path.clone(),
                        record: "MANIFEST.json",
                        expected: rec.sha256.clone(),
                        actual: hash.clone(),
                    });
                }
                if *size != rec.bytes {
                    ok = false;
                    defects.push(Defect::SizeMismatch {
                        path: path.clone(),
                        expected: rec.bytes,
                        actual: *size,
                    });
                }
                if ok {
                    manifest_verified += 1;
                }
            }
        }
    }

    // Checksum records: every listed file must exist and match.
    for (path, expected) in checksums {
        match actual.get(path) {
            None => defects.push(Defect::MissingFile {
                path: path.clone(),
                record: "SHA256SUMS.txt",
            }),
            Some((hash, _)) => {
                if hash == expected {
                    checksum_verified += 1;
                } else {
                    defects.push(Defect::HashMismatch {
                        path: path.clone(),
                        record: "SHA256SUMS.txt",
                        expected: expected.clone(),
                        actual: hash.clone(),
                    });
                }
            }
        }
    }

    // Every file on disk must be accounted for. The only permitted unlisted
    // file is SHA256SUMS.txt, which the release documents as a checksum
    // self-exclusion; it is still covered by the manifest exclusion rules.
    for path in actual.keys() {
        let in_manifest = file_records.contains_key(path);
        let in_checksums = checksums.contains_key(path);
        if !in_manifest && !in_checksums && path != CHECKSUMS_PATH {
            defects.push(Defect::UnrecordedFile { path: path.clone() });
        }
    }

    // Declared package counts must match reality.
    let declared_files = manifest.get("package_file_count").and_then(|v| v.as_u64());
    if let Some(n) = declared_files {
        if n != actual.len() as u64 {
            defects.push(Defect::CountMismatch {
                subject: "package_file_count".into(),
                declared: n,
                actual: actual.len() as u64,
            });
        }
    }
    if let Some(n) = manifest
        .get("manifest_record_count")
        .and_then(|v| v.as_u64())
    {
        if n != file_records.len() as u64 {
            defects.push(Defect::CountMismatch {
                subject: "manifest_record_count".into(),
                declared: n,
                actual: file_records.len() as u64,
            });
        }
    }
    if let Some(n) = manifest
        .get("checksum_record_count")
        .and_then(|v| v.as_u64())
    {
        if n != checksums.len() as u64 {
            defects.push(Defect::CountMismatch {
                subject: "checksum_record_count".into(),
                declared: n,
                actual: checksums.len() as u64,
            });
        }
    }

    // Identity digest covers EVERY file, including the three integrity
    // artifacts. Nothing is self-excluded, because this digest is not stored
    // in the package. BTreeMap iteration is ascending by path, which is the
    // order the digest definition requires.
    let mut ident = anchor::IdentityBuilder::new();
    for (path, (hash, _)) in &actual {
        ident.push(path, hash);
    }
    debug_assert!(!ident.is_out_of_order());
    let identity_digest = ident.finish();

    Ok(IntegrityReport {
        package_root: root.to_path_buf(),
        formal_version: manifest
            .get("formal_version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        status: manifest
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        release_ready: manifest
            .get("release_ready")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        files_on_disk: actual.len(),
        manifest_records: file_records.len(),
        checksum_records: checksums.len(),
        manifest_verified,
        checksum_verified,
        identity_digest,
        defects,
    })
}

/// Compute a package's identity digest without loading or verifying it.
///
/// Used to mint an anchor value for a newly approved package, and by tooling
/// that needs to report what a package *is* before deciding whether to trust it.
pub fn compute_identity_digest(root: impl AsRef<Path>) -> Result<(String, usize), SpecError> {
    let root = root.as_ref();
    let files = walk(root)?;
    let mut b = anchor::IdentityBuilder::new();
    for rel in &files {
        let bytes = std::fs::read(root.join(rel)).map_err(|e| SpecError::Io {
            path: root.join(rel),
            source: e,
        })?;
        b.push(rel, &sha256::hex_digest(&bytes));
    }
    if b.is_out_of_order() {
        return Err(SpecError::Structure(
            "inventory was not in ascending path order".into(),
        ));
    }
    let n = b.file_count();
    Ok((b.finish(), n))
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

fn parse_manifest_files(manifest: &Json) -> Result<BTreeMap<String, FileRecord>, SpecError> {
    let files = manifest
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or_else(|| SpecError::Structure("MANIFEST.json has no 'files' array".into()))?;

    let mut out = BTreeMap::new();
    for entry in files {
        let path = entry
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SpecError::Structure("manifest file record missing 'path'".into()))?;
        check_relative_path(path)?;
        let bytes = entry.get("bytes").and_then(|v| v.as_u64()).ok_or_else(|| {
            SpecError::Structure(format!("manifest record {path} missing 'bytes'"))
        })?;
        let sha256 = entry
            .get("sha256")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SpecError::Structure(format!("manifest record {path} missing 'sha256'"))
            })?;
        check_hex64(sha256, path)?;
        if out
            .insert(
                path.to_string(),
                FileRecord {
                    path: path.to_string(),
                    bytes,
                    sha256: sha256.to_string(),
                },
            )
            .is_some()
        {
            return Err(SpecError::Structure(format!(
                "duplicate manifest record for {path}"
            )));
        }
    }
    Ok(out)
}

fn parse_checksums(text: &str) -> Result<BTreeMap<String, String>, SpecError> {
    let mut out = BTreeMap::new();
    for (lineno, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        // coreutils format: "<64 hex><two spaces><path>"
        let (hash, path) = line.split_once("  ").ok_or_else(|| {
            SpecError::Structure(format!(
                "SHA256SUMS.txt line {}: expected '<hash>  <path>'",
                lineno + 1
            ))
        })?;
        check_hex64(hash, path)?;
        check_relative_path(path)?;
        if out.insert(path.to_string(), hash.to_string()).is_some() {
            return Err(SpecError::Structure(format!(
                "duplicate checksum record for {path}"
            )));
        }
    }
    Ok(out)
}

/// Reject absolute paths, parent traversal, and anything that could escape the
/// package root. An integrity record is untrusted input until checked.
fn check_relative_path(path: &str) -> Result<(), SpecError> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return Err(SpecError::UnsafePath(path.to_string()));
    }
    for c in Path::new(path).components() {
        match c {
            Component::Normal(_) => {}
            _ => return Err(SpecError::UnsafePath(path.to_string())),
        }
    }
    Ok(())
}

fn check_hex64(hash: &str, path: &str) -> Result<(), SpecError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(SpecError::Structure(format!(
            "record for {path} has a malformed SHA-256 digest {hash:?}"
        )));
    }
    Ok(())
}

/// Recursively list regular files under `root`, as sorted package-root-relative
/// POSIX paths. Symlinks are refused: the release's own hygiene gate records a
/// symlink count of zero, so one appearing here is drift.
fn walk(root: &Path) -> Result<Vec<String>, SpecError> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| SpecError::Io {
            path: dir.clone(),
            source: e,
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| SpecError::Io {
                path: dir.clone(),
                source: e,
            })?;
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path).map_err(|e| SpecError::Io {
                path: path.clone(),
                source: e,
            })?;
            if meta.file_type().is_symlink() {
                return Err(SpecError::Structure(format!(
                    "symlink present in package: {}",
                    path.display()
                )));
            }
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|_| SpecError::Structure("path escaped package root".into()))?;
                let rel = rel
                    .to_str()
                    .ok_or_else(|| SpecError::Structure("non-UTF-8 path in package".into()))?;
                out.push(rel.replace('\\', "/"));
            }
        }
    }
    out.sort();
    Ok(out)
}

fn read_text(path: &Path) -> Result<String, SpecError> {
    let bytes = std::fs::read(path).map_err(|e| SpecError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    String::from_utf8(bytes)
        .map_err(|_| SpecError::Structure(format!("{} is not valid UTF-8", path.display())))
}

fn read_json(path: &Path) -> Result<Json, SpecError> {
    let text = read_text(path)?;
    json::parse(&text).map_err(|e| SpecError::Json {
        path: path.to_path_buf(),
        source: e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_paths() {
        for bad in ["/etc/passwd", "../escape", "a/../../b", "", "a\\b"] {
            assert!(check_relative_path(bad).is_err(), "should reject {bad:?}");
        }
        assert!(check_relative_path("10_REGISTRIES/types_v0.1.0.json").is_ok());
    }

    #[test]
    fn rejects_malformed_digests() {
        assert!(check_hex64("abc", "x").is_err());
        assert!(
            check_hex64(&"A".repeat(64), "x").is_err(),
            "uppercase must be rejected"
        );
        assert!(check_hex64(&"g".repeat(64), "x").is_err());
        assert!(check_hex64(&"a".repeat(64), "x").is_ok());
    }

    #[test]
    fn parses_checksum_lines() {
        let text = format!(
            "{}  00_RELEASE/x.txt\n{}  y.txt\n",
            "a".repeat(64),
            "b".repeat(64)
        );
        let m = parse_checksums(&text).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m["y.txt"], "b".repeat(64));
    }

    #[test]
    fn rejects_duplicate_checksum_records() {
        let text = format!("{}  x.txt\n{}  x.txt\n", "a".repeat(64), "b".repeat(64));
        assert!(parse_checksums(&text).is_err());
    }
}
