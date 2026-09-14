//! Conformance reporting: what was indexed, what was executed, and what a run
//! is entitled to claim.
//!
//! `09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt` fixes both the shape of a
//! claim and its preconditions:
//!
//! > A source-conforming implementation must pass lexical, grammar, block, and
//! > field cases. A semantics-conforming implementation must additionally pass
//! > type, operator, function, operation, status, error, and execution cases. A
//! > claimed result must state implementation version, host capabilities, exact
//! > failed case IDs, and evidence. Unsupported core behavior is failure, not
//! > silent inference.
//!
//! and
//!
//! > No semantic-conformance claim is permitted while the required
//! > implementation or concrete executable cases are absent.
//!
//! So [`ConformanceReport::claim`] returns [`ClaimLevel::None`] unless executed
//! evidence actually supports more, and one failed case is enough to withdraw a
//! level rather than to footnote it.
//!
//! ## Two columns that never add up
//!
//! [`ConformanceReport::descriptive_count`] and
//! [`ConformanceReport::executed_count`] are separate numbers about separate
//! things, and this module offers no total. An indexed requirement is prose; an
//! executed case is a source that ran. Adding them would produce a number that
//! describes nothing, and would read as coverage that does not exist.

use crate::obligations::Obligations;
use crate::runner::{judge, ExecutedCase, Expectation, Observed, Verdict};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The stage an executed case exercises.
///
/// `LCL_IMPLEMENTATION_ARCHITECTURE_AND_CONTRACTS.md` section 6 requires a
/// conformance runner to distinguish these, so a report can say *what* was
/// covered rather than only how much.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Coverage {
    Lexical,
    Grammar,
    Resolution,
    StaticOrType,
    Preflight,
    Runtime,
    StandardLibrary,
    Completion,
    EndToEnd,
}

impl Coverage {
    pub const ALL: [Coverage; 9] = [
        Coverage::Lexical,
        Coverage::Grammar,
        Coverage::Resolution,
        Coverage::StaticOrType,
        Coverage::Preflight,
        Coverage::Runtime,
        Coverage::StandardLibrary,
        Coverage::Completion,
        Coverage::EndToEnd,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Coverage::Lexical => "lexical",
            Coverage::Grammar => "grammar",
            Coverage::Resolution => "resolution",
            Coverage::StaticOrType => "static_or_type",
            Coverage::Preflight => "preflight",
            Coverage::Runtime => "runtime",
            Coverage::StandardLibrary => "standard_library",
            Coverage::Completion => "completion",
            Coverage::EndToEnd => "end_to_end",
        }
    }

    /// The coverage levels a **source**-conforming claim requires.
    ///
    /// "must pass lexical, grammar, block, and field cases" — block and field
    /// cases are grammar-and-schema cases, which this implementation reports
    /// under [`Coverage::Grammar`].
    pub const SOURCE: [Coverage; 2] = [Coverage::Lexical, Coverage::Grammar];

    /// The additional coverage a **semantics**-conforming claim requires.
    ///
    /// "must additionally pass type, operator, function, operation, status,
    /// error, and execution cases."
    pub const SEMANTICS: [Coverage; 5] = [
        Coverage::StaticOrType,
        Coverage::Preflight,
        Coverage::Runtime,
        Coverage::StandardLibrary,
        Coverage::Completion,
    ];
}

impl fmt::Display for Coverage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One executed case with the coverage it contributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoveredCase {
    pub case: ExecutedCase,
    pub coverage: Coverage,
}

/// A catalog entry deliberately left descriptive, and exactly why.
///
/// Present so a report can never be silently short. A witness that has no
/// executable form is *listed*, with its reason, rather than omitted — because
/// "Unsupported core behavior is failure, not silent inference", and an absent
/// row is the purest form of silent inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptive {
    pub id: String,
    pub reason: String,
}

/// What conformance level executed evidence supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClaimLevel {
    /// Nothing is claimed.
    None,
    /// Lexical, grammar, block and field cases pass.
    Source,
    /// Source, plus type, operator, function, operation, status, error and
    /// execution cases.
    Semantics,
}

impl ClaimLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimLevel::None => "none",
            ClaimLevel::Source => "source_conforming",
            ClaimLevel::Semantics => "semantics_conforming",
        }
    }
}

impl fmt::Display for ClaimLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The accounting state of one required probe.
///
/// Disjoint and total: every required probe is in exactly one state, so at
/// each claim level `required = satisfied + failed + missing + invalid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProbeState {
    /// Exactly one record, with exactly its pinned sub-runs, and it passed.
    Satisfied,
    /// Exactly one record, with exactly its pinned sub-runs, and it failed.
    Failed,
    /// No record.
    Missing,
    /// Evidence that cannot establish the probe: more than one record, or a
    /// record whose judged sub-runs differ from the pinned membership.
    Invalid,
}

impl ProbeState {
    pub const ALL: [ProbeState; 4] = [
        ProbeState::Satisfied,
        ProbeState::Failed,
        ProbeState::Missing,
        ProbeState::Invalid,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ProbeState::Satisfied => "satisfied",
            ProbeState::Failed => "failed",
            ProbeState::Missing => "missing",
            ProbeState::Invalid => "invalid",
        }
    }
}

impl fmt::Display for ProbeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One required probe's exact account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeAccount {
    pub id: String,
    pub state: ProbeState,
    /// How many records carry the probe's identifier.
    pub records: usize,
    /// Pinned sub-run labels that no judged run carries.
    pub missing_subruns: Vec<String>,
    /// Runs whose label is not pinned, runs with no label, and labels with no run.
    pub unexpected_subruns: Vec<String>,
    /// Labels that more than one run carries.
    pub duplicated_subruns: Vec<String>,
    /// Runs that did not meet their own expectation.
    pub failed_subruns: Vec<String>,
}

impl ProbeAccount {
    /// Account for one required probe from every record carrying its
    /// identifier. A single record is checked against the pinned sub-runs.
    fn of(id: &str, records: &[&ExecutedCase], pinned: Option<&BTreeSet<String>>) -> ProbeAccount {
        let mut account = ProbeAccount {
            id: id.to_string(),
            state: ProbeState::Missing,
            records: records.len(),
            missing_subruns: Vec::new(),
            unexpected_subruns: Vec::new(),
            duplicated_subruns: Vec::new(),
            failed_subruns: Vec::new(),
        };
        let [record] = records else {
            if !records.is_empty() {
                account.state = ProbeState::Invalid;
            }
            return account;
        };
        if let Some(pinned) = pinned {
            account.check_subruns(pinned, record);
        }
        account.state = if !account.missing_subruns.is_empty()
            || !account.unexpected_subruns.is_empty()
            || !account.duplicated_subruns.is_empty()
        {
            ProbeState::Invalid
        } else if record.verdict == Verdict::Failed {
            ProbeState::Failed
        } else {
            ProbeState::Satisfied
        };
        account
    }

    /// Compare one record's labelled runs with the pinned membership.
    ///
    /// Only an exact ordered group judges each run against its own
    /// expectation, so a record of any other shape carries no pinned sub-run.
    fn check_subruns(&mut self, pinned: &BTreeSet<String>, record: &ExecutedCase) {
        let observed = &record.observed;
        let expected = match &record.expectation {
            Expectation::Runs(runs) if runs.len() == observed.runs.len() => runs,
            _ => {
                self.missing_subruns = pinned.iter().cloned().collect();
                return;
            }
        };
        let mut carried = BTreeSet::new();
        let mut unexpected = BTreeSet::new();
        let mut duplicated = BTreeSet::new();
        let mut failed = BTreeSet::new();
        for (index, (run, expectation)) in observed.runs.iter().zip(expected).enumerate() {
            let name = match observed.run_labels.get(index) {
                Some(label) => {
                    if !carried.insert(label.clone()) {
                        duplicated.insert(label.clone());
                    } else if !pinned.contains(label) {
                        unexpected.insert(label.clone());
                    }
                    label.clone()
                }
                None => {
                    let name = format!("run[{index}] (unlabelled)");
                    unexpected.insert(name.clone());
                    name
                }
            };
            if run.input_evidence.is_empty() || judge(expectation, run) == Verdict::Failed {
                failed.insert(name);
            }
        }
        for label in observed.run_labels.iter().skip(observed.runs.len()) {
            unexpected.insert(format!("{label} (no run)"));
        }
        self.missing_subruns = pinned.difference(&carried).cloned().collect();
        self.unexpected_subruns = unexpected.into_iter().collect();
        self.duplicated_subruns = duplicated.into_iter().collect();
        self.failed_subruns = failed.into_iter().collect();
    }

    /// The identifier and state, with every exact evidence problem.
    pub fn describe(&self) -> String {
        let mut out = format!("{} {}", self.id, self.state);
        if self.records > 1 {
            out.push_str(&format!(" [records: {}]", self.records));
        }
        for (name, labels) in [
            ("missing sub-runs", &self.missing_subruns),
            ("unexpected sub-runs", &self.unexpected_subruns),
            ("duplicated sub-runs", &self.duplicated_subruns),
            ("failed sub-runs", &self.failed_subruns),
        ] {
            if !labels.is_empty() {
                out.push_str(&format!(" [{name}: {}]", labels.join(", ")));
            }
        }
        out
    }
}

/// The implementation a claim is about.
///
/// "A claimed result must state implementation version, host capabilities..."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Implementation {
    /// This workspace's version.
    pub version: String,
    /// The canonical package identity the engine verified against.
    pub package_identity: String,
    /// The canonical language version.
    pub language_version: String,
    /// Exactly what the host under test can do. Stated, never implied.
    pub host_capabilities: Vec<String>,
    /// The implementation source snapshot this build recorded from
    /// `LCL_SOURCE_SNAPSHOT`, or `unrecorded`. Stated, never implied.
    pub source_snapshot: String,
}

impl Implementation {
    /// The implementation as the conformance runner assembles it.
    pub fn under_test(
        package_identity: impl Into<String>,
        language_version: impl Into<String>,
    ) -> Implementation {
        Implementation {
            version: env!("CARGO_PKG_VERSION").to_string(),
            package_identity: package_identity.into(),
            language_version: language_version.into(),
            // The runner installs the full Core operation surface and every
            // registered implementation profile, over deterministic in-memory
            // adapters. No real machine is behind any of it, and the report
            // says so rather than letting a reader assume one.
            host_capabilities: vec![
                "lcl-stdlib: the complete registered Core operation surface".to_string(),
                "every registered implementation profile installed".to_string(),
                "deterministic in-memory filesystem, process and transport fixtures".to_string(),
                "lcl-runtime MockHost where a case installs no adapter".to_string(),
                "no real filesystem, process or network adapter".to_string(),
                "no model or provider adapter".to_string(),
            ],
            source_snapshot: option_env!("LCL_SOURCE_SNAPSHOT")
                .unwrap_or("unrecorded")
                .to_string(),
        }
    }
}

/// Everything a record must share with a report to count as evidence about
/// the implementation that report claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunIdentity {
    pub implementation_version: String,
    pub language_version: String,
    pub package_identity: String,
    pub mapping_digest: String,
    pub host_capabilities: Vec<String>,
    pub source_snapshot: String,
}

impl RunIdentity {
    /// The names of the fields in which two identities differ.
    fn differences(&self, other: &RunIdentity) -> Vec<&'static str> {
        [
            (
                "implementation_version",
                self.implementation_version == other.implementation_version,
            ),
            (
                "language_version",
                self.language_version == other.language_version,
            ),
            (
                "package_identity",
                self.package_identity == other.package_identity,
            ),
            (
                "mapping_digest",
                self.mapping_digest == other.mapping_digest,
            ),
            (
                "host_capabilities",
                self.host_capabilities == other.host_capabilities,
            ),
            (
                "source_snapshot",
                self.source_snapshot == other.source_snapshot,
            ),
        ]
        .into_iter()
        .filter(|(_, same)| !same)
        .map(|(field, _)| field)
        .collect()
    }
}

/// A record kept out of the evidence, and exactly why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedRecord {
    pub id: String,
    pub reason: String,
}

/// One conformance run's complete evidence.
#[derive(Debug, Clone)]
pub struct ConformanceReport {
    implementation: Implementation,
    /// The 799-entry requirements index size. Descriptive, never executed.
    indexed_requirements: usize,
    /// Every decision witness the canonical catalog indexes, by identifier.
    ///
    /// The identifiers rather than a count, because completeness is a question
    /// about *which* ones ran. A report holding a count could say that sixty
    /// cases passed while saying nothing about the six that never ran.
    indexed_witnesses: BTreeSet<String>,
    obligations: Option<Obligations>,
    covered: Vec<CoveredCase>,
    descriptive_only: Vec<Descriptive>,
    /// Records from another run identity. Listed, never evidence.
    excluded: Vec<ExcludedRecord>,
}

impl ConformanceReport {
    /// Legacy descriptive report. Caller-selected indexes cannot certify a version.
    pub fn new(
        implementation: Implementation,
        indexed_requirements: usize,
        indexed_witnesses: impl IntoIterator<Item = String>,
    ) -> ConformanceReport {
        ConformanceReport {
            implementation,
            indexed_requirements,
            indexed_witnesses: indexed_witnesses.into_iter().collect(),
            obligations: None,
            covered: Vec::new(),
            descriptive_only: Vec::new(),
            excluded: Vec::new(),
        }
    }

    /// A claim-capable report bound to the complete approved inventory.
    pub fn for_spec(spec: &lcl_spec::SpecPackage) -> Result<Self, String> {
        let obligations = Obligations::load(spec)?;
        let index = crate::ConformanceIndex::load(spec).map_err(|e| e.to_string())?;
        let mut report = Self::new(
            Implementation::under_test(spec.identity_digest(), spec.formal_version()),
            index.requirement_count(),
            index.witnesses().iter().map(|w| w.id.clone()),
        );
        report.obligations = Some(obligations);
        Ok(report)
    }

    pub fn obligations(&self) -> Option<&Obligations> {
        self.obligations.as_ref()
    }

    /// This report's own run identity: the implementation it claims, the
    /// canonical package and reviewed mapping it is bound to, the host
    /// configuration, and the recorded source snapshot.
    pub fn run_identity(&self) -> RunIdentity {
        RunIdentity {
            implementation_version: self.implementation.version.clone(),
            language_version: self.implementation.language_version.clone(),
            package_identity: self.implementation.package_identity.clone(),
            mapping_digest: match &self.obligations {
                Some(inventory) => inventory.digest().to_string(),
                None => "unverified".to_string(),
            },
            host_capabilities: self.implementation.host_capabilities.clone(),
            source_snapshot: self.implementation.source_snapshot.clone(),
        }
    }

    /// Record one case with the identity of the run that executed it.
    ///
    /// Closed policy: a record from any other identity is excluded and listed
    /// with the fields that differ. It never becomes evidence.
    pub fn record_run(&mut self, case: ExecutedCase, coverage: Coverage, identity: &RunIdentity) {
        let differences = self.run_identity().differences(identity);
        if differences.is_empty() {
            self.record(case, coverage);
        } else {
            self.excluded.push(ExcludedRecord {
                id: case.id,
                reason: format!("run identity differs in {}", differences.join(", ")),
            });
        }
    }

    /// Every excluded record, in identifier order.
    pub fn excluded_records(&self) -> Vec<&ExcludedRecord> {
        let mut rows: Vec<&ExcludedRecord> = self.excluded.iter().collect();
        rows.sort_by(|a, b| (&a.id, &a.reason).cmp(&(&b.id, &b.reason)));
        rows
    }

    /// Record one case this process executed under this report's own run
    /// identity, and the coverage it contributes.
    pub fn record(&mut self, mut case: ExecutedCase, coverage: Coverage) {
        // A supplied verdict cannot overrule the observation and expectation.
        case.verdict = judge(&case.expectation, &case.observed);
        // Closed policy: a record without concrete input is failed evidence,
        // never a success.
        if case.source.is_empty() || !Self::carries_input(&case.observed) {
            case.verdict = Verdict::Failed;
        }
        self.covered.push(CoveredCase { case, coverage });
    }

    /// Whether an observation retains the concrete input it ran on: its own
    /// input evidence, or, for an ordered group, every run's.
    fn carries_input(observed: &Observed) -> bool {
        if observed.runs.is_empty() {
            !observed.input_evidence.is_empty()
        } else {
            observed.runs.iter().all(Self::carries_input)
        }
    }

    /// Record one catalog entry that stays descriptive, with its exact reason.
    pub fn record_descriptive(&mut self, id: impl Into<String>, reason: impl Into<String>) {
        self.descriptive_only.push(Descriptive {
            id: id.into(),
            reason: reason.into(),
        });
    }

    pub fn implementation(&self) -> &Implementation {
        &self.implementation
    }

    /// How many requirements the catalog indexes. Prose, not evidence.
    pub fn descriptive_count(&self) -> usize {
        self.indexed_requirements
    }

    pub fn indexed_witnesses(&self) -> usize {
        self.indexed_witnesses.len()
    }

    /// How many cases actually ran.
    pub fn executed_count(&self) -> usize {
        self.covered.len()
    }

    pub fn executed(&self) -> impl Iterator<Item = &CoveredCase> {
        self.covered.iter()
    }

    pub fn passed_count(&self) -> usize {
        self.covered
            .iter()
            .filter(|c| c.case.verdict == Verdict::Passed)
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.covered
            .iter()
            .filter(|c| c.case.verdict == Verdict::Failed)
            .count()
    }

    /// "exact failed case IDs" — in identifier order, never elided.
    pub fn failed_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .covered
            .iter()
            .filter(|c| c.case.verdict == Verdict::Failed)
            .map(|c| c.case.id.clone())
            .collect();
        ids.sort();
        ids
    }

    /// Every entry that stayed descriptive, in identifier order.
    pub fn descriptive_only(&self) -> Vec<&Descriptive> {
        let mut rows: Vec<&Descriptive> = self.descriptive_only.iter().collect();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        rows
    }

    /// Executed case counts per coverage level.
    pub fn by_coverage(&self) -> BTreeMap<Coverage, (usize, usize)> {
        let mut out: BTreeMap<Coverage, (usize, usize)> = BTreeMap::new();
        for covered in &self.covered {
            let entry = out.entry(covered.coverage).or_insert((0, 0));
            match covered.case.verdict {
                Verdict::Passed => entry.0 += 1,
                Verdict::Failed => entry.1 += 1,
            }
        }
        out
    }

    /// Every record, grouped by identifier in recording order.
    fn record_index(&self) -> BTreeMap<&str, Vec<&ExecutedCase>> {
        let mut index: BTreeMap<&str, Vec<&ExecutedCase>> = BTreeMap::new();
        for covered in &self.covered {
            index
                .entry(covered.case.id.as_str())
                .or_default()
                .push(&covered.case);
        }
        index
    }

    fn account(&self, index: &BTreeMap<&str, Vec<&ExecutedCase>>, id: &str) -> ProbeAccount {
        ProbeAccount::of(
            id,
            index.get(id).map_or(&[][..], Vec::as_slice),
            self.obligations
                .as_ref()
                .and_then(|inventory| inventory.subruns(id)),
        )
    }

    fn probe_established(&self, index: &BTreeMap<&str, Vec<&ExecutedCase>>, id: &str) -> bool {
        self.account(index, id).state == ProbeState::Satisfied
    }

    /// All required probes of a witness must be satisfied.
    /// An arbitrary suffix, duplicate, or one passing prefix cannot fill a gap.
    pub fn unsupported_witnesses(&self) -> Vec<&str> {
        let index = self.record_index();
        self.indexed_witnesses
            .iter()
            .filter(|id| {
                self.obligations
                    .as_ref()
                    .and_then(|inventory| inventory.row(id))
                    .map(|row| {
                        !row.probes
                            .iter()
                            .all(|probe| self.probe_established(&index, probe))
                    })
                    .unwrap_or(true)
            })
            .map(String::as_str)
            .collect()
    }

    /// Every required probe at exactly this claim level, in identifier order,
    /// each in exactly one [`ProbeState`].
    pub fn probe_accounts(&self, level: ClaimLevel) -> Vec<ProbeAccount> {
        let index = self.record_index();
        self.obligations
            .as_ref()
            .map(|inventory| {
                inventory
                    .probes()
                    .filter(|(_, required)| *required == level)
                    .map(|(id, _)| self.account(&index, id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Required probes at this exact claim level that are not satisfied:
    /// absent, failed, duplicated, or with inexact sub-run membership.
    pub fn missing_probes(&self, level: ClaimLevel) -> Vec<&str> {
        let index = self.record_index();
        self.obligations
            .as_ref()
            .map(|inventory| {
                inventory
                    .probes()
                    .filter(|(id, required)| {
                        *required == level && !self.probe_established(&index, id)
                    })
                    .map(|(id, _)| id)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Identifiers of records that no required probe names, in identifier
    /// order. Closed policy: such a record is listed and satisfies no
    /// obligation, and a failed one is still a failed case.
    pub fn unrequired_records(&self) -> Vec<&str> {
        let required: BTreeSet<&str> = self
            .obligations
            .as_ref()
            .map(|inventory| inventory.probes().map(|(id, _)| id).collect())
            .unwrap_or_default();
        self.record_index()
            .into_keys()
            .filter(|id| !required.contains(id))
            .collect()
    }

    /// Claims follow complete obligations, never category hits or caller counts.
    /// A semantic failure does not erase independently complete source evidence.
    pub fn claim(&self) -> ClaimLevel {
        if self.obligations.is_none()
            || !self.missing_probes(ClaimLevel::Source).is_empty()
            || self.covered.iter().any(|r| {
                matches!(r.coverage, Coverage::Lexical | Coverage::Grammar)
                    && r.case.verdict == Verdict::Failed
            })
        {
            return ClaimLevel::None;
        }
        if !self.missing_probes(ClaimLevel::Semantics).is_empty() || self.failed_count() > 0 {
            return ClaimLevel::Source;
        }
        ClaimLevel::Semantics
    }

    pub fn claim_limits(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.obligations.is_none() {
            out.push("no verified complete obligation inventory; caller-selected indexes cannot certify a version".into());
        }
        for id in self.failed_ids() {
            out.push(format!("case {id} failed"));
        }
        for level in [ClaimLevel::Source, ClaimLevel::Semantics] {
            for account in self.probe_accounts(level) {
                if account.state != ProbeState::Satisfied {
                    out.push(format!("required {level} probe {}", account.describe()));
                }
            }
        }
        out
    }

    /// The full report, order-stable and free of timing or addresses.
    pub fn render(&self) -> String {
        let mut out = String::from("LCL CONFORMANCE REPORT\n\n");
        out.push_str("IMPLEMENTATION\n");
        out.push_str(&format!(
            "  version           : {}\n",
            self.implementation.version
        ));
        out.push_str(&format!(
            "  language version  : {}\n",
            self.implementation.language_version
        ));
        out.push_str(&format!(
            "  package identity  : {}\n",
            self.implementation.package_identity
        ));
        out.push_str(&format!(
            "  source snapshot   : {}\n",
            self.implementation.source_snapshot
        ));
        out.push_str("  host capabilities :\n");
        for capability in &self.implementation.host_capabilities {
            out.push_str(&format!("    - {capability}\n"));
        }

        if let Some(inventory) = &self.obligations {
            out.push_str(&format!(
                "  obligation mapping: {}\n  required obligations: {}\n  required probes: {}\n",
                inventory.digest(),
                inventory.rows().count(),
                inventory.probes().count()
            ));
            for level in [ClaimLevel::Source, ClaimLevel::Semantics] {
                let accounts = self.probe_accounts(level);
                out.push_str(&format!("  {level} probes: {} required", accounts.len()));
                for state in ProbeState::ALL {
                    let count = accounts.iter().filter(|a| a.state == state).count();
                    out.push_str(&format!(", {count} {state}"));
                }
                out.push('\n');
            }
        } else {
            out.push_str("  obligation mapping: UNVERIFIED caller-selected index\n");
        }

        out.push_str("\nDESCRIPTIVE CATALOGS (indexed, never executed)\n");
        out.push_str(&format!(
            "  requirements index : {} entries, all not_executed\n",
            self.indexed_requirements
        ));
        out.push_str(&format!(
            "  decision witnesses : {} entries\n",
            self.indexed_witnesses.len()
        ));

        out.push_str("\nEXECUTED CASES (concrete input, observed result)\n");
        out.push_str(&format!(
            "  executed : {}\n  passed   : {}\n  failed   : {}\n",
            self.executed_count(),
            self.passed_count(),
            self.failed_count()
        ));

        // Three populations, never added together. A probe is one execution; a
        // witness is one indexed behaviour and may take several probes; an
        // indexed requirement is prose. Reading an executed-probe count as
        // witness coverage is the specific misreading this section prevents.
        let supported = self.indexed_witnesses.len() - self.unsupported_witnesses().len();
        out.push_str(&format!(
            "\n  witnesses established : {} of {}\n",
            supported,
            self.indexed_witnesses.len()
        ));
        let unsupported = self.unsupported_witnesses();
        if !unsupported.is_empty() {
            out.push_str("  witnesses not established, by identifier:\n");
            for id in unsupported {
                out.push_str(&format!("    {id}\n"));
            }
        }
        out.push_str("\n  by coverage:\n");
        for (coverage, (passed, failed)) in self.by_coverage() {
            out.push_str(&format!(
                "    {coverage:<17} {passed} passed, {failed} failed\n"
            ));
        }

        let failed = self.failed_ids();
        out.push_str("\n  failed case IDs: ");
        if failed.is_empty() {
            out.push_str("none\n");
        } else {
            out.push('\n');
            for id in &failed {
                out.push_str(&format!("    {id}\n"));
            }
        }

        let unrequired = self.unrequired_records();
        out.push_str(
            "\n  unrequired records (no required probe names them; they satisfy no obligation): ",
        );
        if unrequired.is_empty() {
            out.push_str("none\n");
        } else {
            out.push('\n');
            for id in unrequired {
                out.push_str(&format!("    {id}\n"));
            }
        }

        let excluded = self.excluded_records();
        out.push_str(
            "\n  excluded records (another run identity; never evidence for this claim): ",
        );
        if excluded.is_empty() {
            out.push_str("none\n");
        } else {
            out.push('\n');
            for row in excluded {
                out.push_str(&format!("    {} — {}\n", row.id, row.reason));
            }
        }

        out.push_str("\n  evidence:\n");
        let mut rows: Vec<&CoveredCase> = self.covered.iter().collect();
        rows.sort_by(|a, b| a.case.id.cmp(&b.case.id));
        for covered in rows {
            out.push_str(&format!(
                "    [{}] {}\n",
                covered.coverage,
                covered.case.serialize()
            ));
        }

        let descriptive = self.descriptive_only();
        out.push_str(&format!(
            "\nDESCRIPTIVE-ONLY ENTRIES ({})\n",
            descriptive.len()
        ));
        for row in descriptive {
            out.push_str(&format!("  {} — {}\n", row.id, row.reason));
        }

        out.push_str(&format!("\nCLAIM: {}\n", self.claim()));
        for limit in self.claim_limits() {
            out.push_str(&format!("  limited by: {limit}\n"));
        }
        out
    }

    /// The verdict as one deterministic JSON document, for independent
    /// reconciliation.
    ///
    /// At each claim level the four probe-state lists are disjoint and together
    /// hold exactly the required probes: `required = satisfied + failed +
    /// missing + invalid`. Every probe that is not satisfied is also listed in
    /// `problems`, with its exact evidence detail. Record order changes nothing
    /// in this document.
    pub fn render_verdict_json(&self) -> String {
        let implementation = &self.implementation;
        let levels = [ClaimLevel::Source, ClaimLevel::Semantics].map(|level| {
            let accounts = self.probe_accounts(level);
            let mut fields = vec![
                ("level", json_string(level.as_str())),
                ("required", accounts.len().to_string()),
            ];
            for state in ProbeState::ALL {
                let ids = accounts
                    .iter()
                    .filter(|account| account.state == state)
                    .map(|account| account.id.as_str());
                fields.push((state.as_str(), json_strings(ids)));
            }
            let problems = accounts
                .iter()
                .filter(|account| account.state != ProbeState::Satisfied)
                .map(|account| {
                    json_object(&[
                        ("id", json_string(&account.id)),
                        ("state", json_string(account.state.as_str())),
                        ("records", account.records.to_string()),
                        (
                            "missing_subruns",
                            json_strings(account.missing_subruns.iter().map(String::as_str)),
                        ),
                        (
                            "unexpected_subruns",
                            json_strings(account.unexpected_subruns.iter().map(String::as_str)),
                        ),
                        (
                            "duplicated_subruns",
                            json_strings(account.duplicated_subruns.iter().map(String::as_str)),
                        ),
                        (
                            "failed_subruns",
                            json_strings(account.failed_subruns.iter().map(String::as_str)),
                        ),
                    ])
                });
            fields.push(("problems", json_array(problems)));
            json_object(&fields)
        });
        let mut descriptive = self.descriptive_only();
        descriptive.sort_by(|a, b| (&a.id, &a.reason).cmp(&(&b.id, &b.reason)));
        let fields = [
            ("format", json_string("lcl.conformance.verdict.v1")),
            ("claim", json_string(self.claim().as_str())),
            (
                "implementation",
                json_object(&[
                    ("version", json_string(&implementation.version)),
                    (
                        "language_version",
                        json_string(&implementation.language_version),
                    ),
                    (
                        "package_identity",
                        json_string(&implementation.package_identity),
                    ),
                    (
                        "source_snapshot",
                        json_string(&implementation.source_snapshot),
                    ),
                    (
                        "host_capabilities",
                        json_strings(implementation.host_capabilities.iter().map(String::as_str)),
                    ),
                ]),
            ),
            (
                "mapping_digest",
                json_string(&self.run_identity().mapping_digest),
            ),
            ("levels", json_array(levels)),
            (
                "records",
                json_object(&[
                    ("executed", self.executed_count().to_string()),
                    ("passed", self.passed_count().to_string()),
                    ("failed", self.failed_count().to_string()),
                ]),
            ),
            (
                "failed_case_ids",
                json_strings(self.failed_ids().iter().map(String::as_str)),
            ),
            (
                "unrequired_records",
                json_strings(self.unrequired_records()),
            ),
            (
                "excluded_records",
                json_array(self.excluded_records().iter().map(|row| {
                    json_object(&[
                        ("id", json_string(&row.id)),
                        ("reason", json_string(&row.reason)),
                    ])
                })),
            ),
            (
                "indexed",
                json_object(&[
                    ("requirements", self.indexed_requirements.to_string()),
                    ("witnesses", self.indexed_witnesses.len().to_string()),
                    (
                        "witnesses_not_established",
                        json_strings(self.unsupported_witnesses()),
                    ),
                ]),
            ),
            (
                "descriptive_only",
                json_array(descriptive.iter().map(|row| {
                    json_object(&[
                        ("id", json_string(&row.id)),
                        ("reason", json_string(&row.reason)),
                    ])
                })),
            ),
            (
                "claim_limits",
                json_strings(self.claim_limits().iter().map(String::as_str)),
            ),
        ];
        let members: Vec<String> = fields
            .iter()
            .map(|(key, value)| format!("  {}: {value}", json_string(key)))
            .collect();
        format!("{{\n{}\n}}\n", members.join(",\n"))
    }
}

/// One JSON string literal, escaping everything JSON requires.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_strings<'a>(values: impl IntoIterator<Item = &'a str>) -> String {
    json_array(values.into_iter().map(json_string))
}

fn json_array(values: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", values.into_iter().collect::<Vec<_>>().join(","))
}

fn json_object(fields: &[(&str, String)]) -> String {
    let members: Vec<String> = fields
        .iter()
        .map(|(key, value)| format!("{}:{value}", json_string(key)))
        .collect();
    format!("{{{}}}", members.join(","))
}

#[cfg(test)]
mod tests {
    // These are instrument tests using synthetic observations. They validate
    // report logic and are never emitted as language conformance evidence.
    use super::*;
    use crate::runner::{Expectation, Observed, Reached};

    fn case(id: &str, passed: bool) -> ExecutedCase {
        ExecutedCase {
            id: id.into(),
            contract: "report-instrument-test".into(),
            source: "synthetic report instrument input".into(),
            expectation: Expectation::Accepts,
            observed: Observed {
                reached: Reached::Completion,
                primary: if passed {
                    None
                } else {
                    Some("error.operation.precondition".into())
                },
                input_evidence: vec!["synthetic report instrument input".into()],
                ..Observed::default()
            },
            verdict: Verdict::Passed,
        }
    }
    const PINNED: &str = "semantic/failure_lifecycle/core.failure_lifecycle";

    fn spec() -> lcl_spec::SpecPackage {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../canonical/LCL_Core_0.1.0");
        lcl_spec::SpecPackage::open(root).unwrap()
    }
    fn report() -> ConformanceReport {
        ConformanceReport::for_spec(&spec()).unwrap()
    }
    /// A claim-capable report whose inventory pins `labels` as the sub-runs of
    /// `PINNED`, in whichever mapping revision is embedded.
    fn pinned_report(labels: &[&str]) -> ConformanceReport {
        let spec = spec();
        let mapping = Obligations::embedded_mapping_with_subruns(PINNED, labels);
        let mut report = ConformanceReport::for_spec(&spec).unwrap();
        report.obligations = Some(Obligations::load_mapping_text(&spec, &mapping).unwrap());
        report
    }
    /// One synthetic ordered group of `runs` runs, labelled by `labels` in order.
    /// Runs at the `failing` indices observe a primary diagnostic.
    fn group(id: &str, labels: &[&str], runs: usize, failing: &[usize]) -> ExecutedCase {
        let run = |index: usize| Observed {
            reached: Reached::Completion,
            primary: failing
                .contains(&index)
                .then(|| "error.operation.precondition".to_string()),
            input_evidence: vec![format!("synthetic sub-run {index}")],
            ..Observed::default()
        };
        ExecutedCase {
            id: id.into(),
            contract: "report-instrument-test".into(),
            source: "synthetic report instrument input".into(),
            expectation: Expectation::Runs(vec![Expectation::Accepts; runs]),
            observed: Observed {
                runs: (0..runs).map(run).collect(),
                run_labels: labels.iter().map(|label| label.to_string()).collect(),
                ..Observed::default()
            },
            verdict: Verdict::Passed,
        }
    }
    fn pinned_account(report: &ConformanceReport) -> ProbeAccount {
        report
            .probe_accounts(ClaimLevel::Semantics)
            .into_iter()
            .find(|account| account.id == PINNED)
            .unwrap()
    }
    /// A record that satisfies `id` under whichever mapping is loaded: a group
    /// carrying exactly the probe's pinned sub-runs, or one passing case.
    fn satisfying(report: &ConformanceReport, id: &str) -> ExecutedCase {
        match report
            .obligations()
            .and_then(|inventory| inventory.subruns(id))
        {
            Some(pinned) => {
                let labels: Vec<&str> = pinned.iter().map(String::as_str).collect();
                group(id, &labels, labels.len(), &[])
            }
            None => case(id, true),
        }
    }
    fn fill(report: &mut ConformanceReport, level: ClaimLevel, omitted: &[&str]) {
        let records: Vec<(ExecutedCase, Coverage)> = report
            .obligations()
            .unwrap()
            .probes()
            .filter(|(id, required)| *required <= level && !omitted.contains(id))
            .map(|(id, _)| {
                let coverage = if id.starts_with("source/") {
                    Coverage::Grammar
                } else {
                    Coverage::Runtime
                };
                (satisfying(report, id), coverage)
            })
            .collect();
        for (record, coverage) in records {
            report.record(record, coverage);
        }
    }

    #[test]
    fn an_empty_run_claims_nothing() {
        let report = report();
        assert_eq!(report.claim(), ClaimLevel::None);
        assert_eq!(report.executed_count(), 0);
        assert_eq!(report.descriptive_count(), 799);
    }
    #[test]
    fn caller_selected_sparse_or_empty_inventories_cannot_certify() {
        for ids in [vec![], vec!["CLOSURE-001".to_string()]] {
            let mut r = ConformanceReport::new(
                Implementation::under_test(lcl_spec::APPROVED_PACKAGE.identity_digest, "0.1.0"),
                799,
                ids,
            );
            for c in Coverage::ALL {
                r.record(case(&format!("{c}"), true), c);
            }
            assert_eq!(r.claim(), ClaimLevel::None);
            assert!(r.claim_limits()[0].contains("no verified complete obligation inventory"));
        }
    }
    #[test]
    fn source_requires_every_lexical_grammar_block_and_field_probe() {
        for omitted in [
            "source/fixture/invalid_bom.lcl",
            "source/grammar/expression/positive",
            "source/block/ACTION/minimum",
            "source/field/ACTION/TARGET/form/expression",
        ] {
            let mut r = report();
            fill(&mut r, ClaimLevel::Semantics, &[omitted]);
            assert_eq!(r.claim(), ClaimLevel::None, "{omitted}");
            assert_eq!(r.missing_probes(ClaimLevel::Source), [omitted]);
        }
    }
    #[test]
    fn full_source_evidence_supports_source_independently() {
        let mut r = report();
        fill(&mut r, ClaimLevel::Source, &[]);
        assert_eq!(r.claim(), ClaimLevel::Source);
        r.record(case("unrelated-semantic-failure", false), Coverage::Runtime);
        assert_eq!(r.claim(), ClaimLevel::Source);
    }
    #[test]
    fn semantic_contracts_are_required_in_addition_to_all_witnesses() {
        let mut r = report();
        let omitted = r
            .obligations()
            .unwrap()
            .probes()
            .find(|(id, _)| id.starts_with("semantic/operation_binding/"))
            .unwrap()
            .0
            .to_string();
        fill(&mut r, ClaimLevel::Semantics, &[&omitted]);
        assert!(r.unsupported_witnesses().is_empty());
        assert_eq!(r.claim(), ClaimLevel::Source);
        let record = satisfying(&r, &omitted);
        r.record(record, Coverage::Runtime);
        assert_eq!(r.claim(), ClaimLevel::Semantics);
    }
    #[test]
    fn necessary_subprobes_cannot_be_replaced_by_passing_prefixes() {
        let mut r = report();
        fill(&mut r, ClaimLevel::Semantics, &["CLOSURE-059/exhausted"]);
        r.record(case("CLOSURE-059/irrelevant", true), Coverage::Runtime);
        assert_eq!(r.unsupported_witnesses(), ["CLOSURE-059"]);
        assert_eq!(r.claim(), ClaimLevel::Source);
        r.record(case("CLOSURE-059/exhausted", true), Coverage::Runtime);
        assert_eq!(r.claim(), ClaimLevel::Semantics);
    }
    #[test]
    fn duplicate_required_probes_block_their_level() {
        let mut r = report();
        fill(&mut r, ClaimLevel::Semantics, &[]);
        r.record(case("CLOSURE-059/recovered", true), Coverage::Runtime);
        assert_eq!(r.claim(), ClaimLevel::Source);
        assert_eq!(r.unsupported_witnesses(), ["CLOSURE-059"]);
        r.record(case("source/block/ACTION/minimum", true), Coverage::Grammar);
        assert_eq!(r.claim(), ClaimLevel::None);
    }
    #[test]
    fn forged_passing_verdict_does_not_hide_a_failed_observation() {
        let mut r = report();
        fill(&mut r, ClaimLevel::Semantics, &["CLOSURE-059/recovered"]);
        r.record(case("CLOSURE-059/recovered", false), Coverage::Runtime);
        assert_eq!(r.failed_ids(), ["CLOSURE-059/recovered"]);
        assert_eq!(r.claim(), ClaimLevel::Source);
    }
    #[test]
    fn empty_source_is_not_executed_evidence() {
        let mut r = report();
        let mut c = case("empty", true);
        c.source.clear();
        r.record(c, Coverage::Lexical);
        assert_eq!(r.failed_ids(), ["empty"]);
    }
    #[test]
    fn descriptive_and_executed_counts_remain_separate() {
        let mut r = report();
        r.record(case("one", true), Coverage::Lexical);
        assert_eq!(r.descriptive_count(), 799);
        assert_eq!(r.executed_count(), 1);
        assert!(r.render().contains("799 entries, all not_executed"));
    }
    #[test]
    fn descriptive_only_rows_retain_their_reasons() {
        let mut r = report();
        r.record_descriptive("CLOSURE-059", "missing fixture");
        assert!(r.render().contains("CLOSURE-059 — missing fixture"));
        assert!(r.unsupported_witnesses().contains(&"CLOSURE-059"));
    }
    #[test]
    fn omitting_one_pinned_subrun_denies_completeness() {
        let mut r = pinned_report(&["a", "b"]);
        fill(&mut r, ClaimLevel::Semantics, &[PINNED]);
        r.record(group(PINNED, &["a"], 1, &[]), Coverage::Runtime);
        let account = pinned_account(&r);
        assert_eq!(account.state, ProbeState::Invalid, "{}", account.describe());
        assert_eq!(account.missing_subruns, ["b"]);
        assert_eq!(r.missing_probes(ClaimLevel::Semantics), [PINNED]);
        assert_eq!(r.claim(), ClaimLevel::Source);
        assert!(r.claim_limits().contains(&format!(
            "required semantics_conforming probe {PINNED} invalid [missing sub-runs: b]"
        )));
        // Control: the exact pinned membership establishes the same probe.
        let mut r = pinned_report(&["a", "b"]);
        fill(&mut r, ClaimLevel::Semantics, &[PINNED]);
        r.record(group(PINNED, &["a", "b"], 2, &[]), Coverage::Runtime);
        assert_eq!(pinned_account(&r).state, ProbeState::Satisfied);
        assert_eq!(r.claim(), ClaimLevel::Semantics);
    }
    /// A malformed group record, then the exact missing, unexpected and
    /// duplicated sub-run labels its account must list.
    type SubrunCase = (
        ExecutedCase,
        &'static [&'static str],
        &'static [&'static str],
        &'static [&'static str],
    );
    /// One run-identity field and an alteration that changes only that field.
    type IdentityAlteration = (&'static str, fn(&mut RunIdentity));

    #[test]
    fn unexpected_duplicated_unlabelled_or_ungrouped_subruns_are_invalid() {
        let mut ungrouped = group(PINNED, &["a", "b"], 2, &[]);
        ungrouped.expectation = Expectation::All(vec![Expectation::Accepts]);
        let cases: [SubrunCase; 5] = [
            (group(PINNED, &["a", "b", "c"], 3, &[]), &[], &["c"], &[]),
            (group(PINNED, &["a", "b", "b"], 3, &[]), &[], &[], &["b"]),
            (
                group(PINNED, &["a"], 2, &[]),
                &["b"],
                &["run[1] (unlabelled)"],
                &[],
            ),
            (
                group(PINNED, &["a", "b"], 1, &[]),
                &["b"],
                &["b (no run)"],
                &[],
            ),
            (ungrouped, &["a", "b"], &[], &[]),
        ];
        for (record, missing, unexpected, duplicated) in cases {
            let mut r = pinned_report(&["a", "b"]);
            fill(&mut r, ClaimLevel::Semantics, &[PINNED]);
            r.record(record, Coverage::Runtime);
            let account = pinned_account(&r);
            let described = account.describe();
            assert_eq!(account.state, ProbeState::Invalid, "{described}");
            assert_eq!(account.missing_subruns, missing, "{described}");
            assert_eq!(account.unexpected_subruns, unexpected, "{described}");
            assert_eq!(account.duplicated_subruns, duplicated, "{described}");
            assert_eq!(r.claim(), ClaimLevel::Source, "{described}");
        }
    }
    #[test]
    fn a_failed_pinned_subrun_fails_its_probe_and_is_named() {
        let mut r = pinned_report(&["a", "b"]);
        fill(&mut r, ClaimLevel::Semantics, &[PINNED]);
        r.record(group(PINNED, &["a", "b"], 2, &[1]), Coverage::Runtime);
        let account = pinned_account(&r);
        assert_eq!(account.state, ProbeState::Failed, "{}", account.describe());
        assert_eq!(account.failed_subruns, ["b"]);
        assert_eq!(r.failed_ids(), [PINNED]);
        assert_eq!(r.claim(), ClaimLevel::Source);
    }
    #[test]
    fn probe_states_partition_every_required_probe_exactly_once() {
        let mut r = report();
        let first: Vec<String> = r
            .obligations()
            .unwrap()
            .probes()
            .filter(|(_, level)| *level == ClaimLevel::Semantics)
            .map(|(id, _)| id.to_string())
            .take(3)
            .collect();
        let [missing, failed, duplicated] = &first[..] else {
            unreachable!("the inventory has semantic probes")
        };
        fill(
            &mut r,
            ClaimLevel::Semantics,
            &[missing.as_str(), failed.as_str()],
        );
        r.record(case(failed, false), Coverage::Runtime);
        r.record(case(duplicated, true), Coverage::Runtime);
        for level in [ClaimLevel::Source, ClaimLevel::Semantics] {
            let required: Vec<&str> = r
                .obligations()
                .unwrap()
                .probes()
                .filter(|(_, required)| *required == level)
                .map(|(id, _)| id)
                .collect();
            let accounts = r.probe_accounts(level);
            let accounted: Vec<&str> = accounts.iter().map(|a| a.id.as_str()).collect();
            assert_eq!(accounted, required);
        }
        let accounts = r.probe_accounts(ClaimLevel::Semantics);
        let state = |id: &str| accounts.iter().find(|a| a.id == id).unwrap().state;
        assert_eq!(state(missing), ProbeState::Missing);
        assert_eq!(state(failed), ProbeState::Failed);
        assert_eq!(state(duplicated), ProbeState::Invalid);
        let total = accounts.len();
        assert_eq!(
            ProbeState::ALL.map(|s| accounts.iter().filter(|a| a.state == s).count()),
            [total - 3, 1, 1, 1]
        );
        assert_eq!(
            r.missing_probes(ClaimLevel::Semantics),
            [missing.as_str(), failed.as_str(), duplicated.as_str()]
        );
        assert!(r.claim_limits().contains(&format!(
            "required semantics_conforming probe {duplicated} invalid [records: 2]"
        )));
        assert!(r.render().contains(&format!(
            "semantics_conforming probes: {total} required, {} satisfied, 1 failed, 1 missing, 1 invalid",
            total - 3
        )));
    }
    #[test]
    fn inputless_success_is_failed_evidence() {
        let mut r = report();
        fill(&mut r, ClaimLevel::Semantics, &["CLOSURE-059/recovered"]);
        let mut inputless = case("CLOSURE-059/recovered", true);
        inputless.observed.input_evidence.clear();
        r.record(inputless, Coverage::Runtime);
        assert_eq!(r.failed_ids(), ["CLOSURE-059/recovered"]);
        assert_eq!(r.claim(), ClaimLevel::Source);
        // A group carries input only when every run does, whatever its own
        // top-level evidence says. `Accepts` alone does not look at runs.
        for (run_input, verdict) in [(false, Verdict::Failed), (true, Verdict::Passed)] {
            let mut r = report();
            let mut grouped = case("grouped", true);
            grouped.observed.runs = vec![Observed {
                reached: Reached::Completion,
                input_evidence: if run_input {
                    vec!["synthetic sub-run input".into()]
                } else {
                    Vec::new()
                },
                ..Observed::default()
            }];
            r.record(grouped, Coverage::Runtime);
            assert_eq!(r.executed().next().unwrap().case.verdict, verdict);
        }
    }
    #[test]
    fn unrequired_records_are_listed_and_neither_fill_a_gap_nor_change_the_verdict() {
        let mut r = report();
        fill(&mut r, ClaimLevel::Semantics, &["CLOSURE-059/exhausted"]);
        r.record(case("CLOSURE-059/exhausted/extra", true), Coverage::Runtime);
        r.record(case("unknown-probe", true), Coverage::Runtime);
        assert_eq!(r.claim(), ClaimLevel::Source);
        assert_eq!(
            r.missing_probes(ClaimLevel::Semantics),
            ["CLOSURE-059/exhausted"]
        );
        assert_eq!(
            r.unrequired_records(),
            ["CLOSURE-059/exhausted/extra", "unknown-probe"]
        );
        assert!(r.render().contains(
            "they satisfy no obligation): \n    CLOSURE-059/exhausted/extra\n    unknown-probe\n"
        ));
        // Control: a complete population lists none, and an irrelevant success
        // added to it changes nothing but the list.
        let mut r = report();
        fill(&mut r, ClaimLevel::Semantics, &[]);
        assert!(r.unrequired_records().is_empty());
        assert!(r.render().contains("they satisfy no obligation): none\n"));
        r.record(case("unknown-probe", true), Coverage::Runtime);
        assert_eq!(r.claim(), ClaimLevel::Semantics);
        assert_eq!(r.unrequired_records(), ["unknown-probe"]);
    }
    #[test]
    fn records_from_another_run_identity_are_excluded_and_listed() {
        let alterations: [IdentityAlteration; 6] = [
            ("implementation_version", |i| {
                i.implementation_version.push_str("-other")
            }),
            ("language_version", |i| i.language_version = "0.2.0".into()),
            ("package_identity", |i| i.package_identity = "0".repeat(64)),
            ("mapping_digest", |i| i.mapping_digest = "0".repeat(64)),
            ("host_capabilities", |i| {
                i.host_capabilities.push("a real network adapter".into())
            }),
            ("source_snapshot", |i| i.source_snapshot.push_str("-other")),
        ];
        for (field, alter) in alterations {
            let mut r = report();
            fill(&mut r, ClaimLevel::Semantics, &["CLOSURE-059/exhausted"]);
            let executed = r.executed_count();
            let own = r.run_identity();
            let mut other = own.clone();
            alter(&mut other);
            r.record_run(
                case("CLOSURE-059/exhausted", true),
                Coverage::Runtime,
                &other,
            );
            assert_eq!(r.executed_count(), executed, "{field}");
            assert_eq!(r.claim(), ClaimLevel::Source, "{field}");
            assert_eq!(
                r.missing_probes(ClaimLevel::Semantics),
                ["CLOSURE-059/exhausted"],
                "{field}"
            );
            let reason = format!("run identity differs in {field}");
            assert_eq!(
                r.excluded_records(),
                [&ExcludedRecord {
                    id: "CLOSURE-059/exhausted".into(),
                    reason: reason.clone(),
                }],
                "{field}"
            );
            assert!(r.render().contains(&format!(
                "never evidence for this claim): \n    CLOSURE-059/exhausted — {reason}\n"
            )));
            // Control: the same record under the report's own identity is evidence.
            r.record_run(case("CLOSURE-059/exhausted", true), Coverage::Runtime, &own);
            assert_eq!(r.claim(), ClaimLevel::Semantics, "{field}");
        }
        // Every differing field is named.
        let mut r = report();
        let mut other = r.run_identity();
        other.host_capabilities.clear();
        other.source_snapshot.push_str("-other");
        r.record_run(case("one", true), Coverage::Runtime, &other);
        assert_eq!(
            r.excluded_records()[0].reason,
            "run identity differs in host_capabilities, source_snapshot"
        );
        // A report states its snapshot, and lists no exclusion it did not make.
        let r = report();
        assert!(r.excluded_records().is_empty());
        let rendered = r.render();
        assert!(rendered.contains(&format!(
            "  source snapshot   : {}\n",
            option_env!("LCL_SOURCE_SNAPSHOT").unwrap_or("unrecorded")
        )));
        assert!(rendered.contains("never evidence for this claim): none\n"));
    }
    #[test]
    fn verdict_json_partitions_every_required_probe_and_reconciles_independently() {
        use lcl_spec::json::{self, Json};
        let mut r = pinned_report(&["a", "b"]);
        let semantic: Vec<String> = r
            .obligations()
            .unwrap()
            .probes()
            .filter(|(id, level)| *level == ClaimLevel::Semantics && *id != PINNED)
            .map(|(id, _)| id.to_string())
            .take(3)
            .collect();
        let [missing, failed, duplicated] = &semantic[..] else {
            unreachable!("the inventory has semantic probes")
        };
        fill(
            &mut r,
            ClaimLevel::Semantics,
            &[missing.as_str(), failed.as_str(), PINNED],
        );
        r.record(case(failed, false), Coverage::Runtime);
        r.record(case(duplicated, true), Coverage::Runtime);
        r.record(group(PINNED, &["a"], 1, &[]), Coverage::Runtime);
        r.record(case("unknown-probe", true), Coverage::Runtime);
        let mut other = r.run_identity();
        other.source_snapshot.push_str("-other");
        let odd = "odd \"id\" \\ with\na control \u{1} character";
        r.record_run(case(odd, true), Coverage::Runtime, &other);
        r.record_descriptive("CLOSURE-059", "a reason with \"quotes\"");

        let parsed = json::parse(&r.render_verdict_json()).expect("the verdict is JSON");
        let strings = |value: Option<&Json>| -> Vec<String> {
            value
                .and_then(Json::as_array)
                .unwrap()
                .iter()
                .map(|item| item.as_str().unwrap().to_string())
                .collect()
        };
        let levels = parsed.get("levels").and_then(Json::as_array).unwrap();
        assert_eq!(levels.len(), 2);
        for (level, entry) in [ClaimLevel::Source, ClaimLevel::Semantics]
            .into_iter()
            .zip(levels)
        {
            assert_eq!(
                entry.get("level").and_then(Json::as_str),
                Some(level.as_str())
            );
            let inventory: BTreeSet<String> = r
                .obligations()
                .unwrap()
                .probes()
                .filter(|(_, required)| *required == level)
                .map(|(id, _)| id.to_string())
                .collect();
            let required = entry.get("required").and_then(Json::as_u64).unwrap();
            let mut union = BTreeSet::new();
            let mut total = 0;
            for state in ProbeState::ALL {
                let ids = strings(entry.get(state.as_str()));
                total += ids.len();
                for id in ids {
                    assert!(union.insert(id.clone()), "{id} is in two states");
                }
            }
            assert_eq!(required, inventory.len() as u64);
            assert_eq!(total, inventory.len());
            assert_eq!(union, inventory);
            let problems: Vec<String> = entry
                .get("problems")
                .and_then(Json::as_array)
                .unwrap()
                .iter()
                .map(|problem| {
                    problem
                        .get("id")
                        .and_then(Json::as_str)
                        .unwrap()
                        .to_string()
                })
                .collect();
            assert_eq!(problems, r.missing_probes(level));
        }
        let semantics = &levels[1];
        assert_eq!(strings(semantics.get("missing")), [missing.as_str()]);
        assert_eq!(strings(semantics.get("failed")), [failed.as_str()]);
        let mut invalid = vec![duplicated.as_str(), PINNED];
        invalid.sort();
        assert_eq!(strings(semantics.get("invalid")), invalid);
        let pinned = semantics
            .get("problems")
            .and_then(Json::as_array)
            .unwrap()
            .iter()
            .find(|problem| problem.get("id").and_then(Json::as_str) == Some(PINNED))
            .unwrap();
        assert_eq!(pinned.get("state").and_then(Json::as_str), Some("invalid"));
        assert_eq!(pinned.get("records").and_then(Json::as_u64), Some(1));
        assert_eq!(strings(pinned.get("missing_subruns")), ["b"]);

        assert_eq!(
            parsed.get("claim").and_then(Json::as_str),
            Some("source_conforming")
        );
        // The report states the digest of the mapping it actually loaded.
        assert_eq!(
            parsed.get("mapping_digest").and_then(Json::as_str),
            Some(r.obligations().unwrap().digest())
        );
        assert_ne!(
            r.obligations().unwrap().digest(),
            crate::obligations::MAPPING_DIGEST
        );
        assert_eq!(
            parsed
                .get("implementation")
                .and_then(|i| i.get("source_snapshot"))
                .and_then(Json::as_str),
            Some(r.implementation().source_snapshot.as_str())
        );
        assert_eq!(
            parsed
                .get("records")
                .and_then(|records| records.get("executed"))
                .and_then(Json::as_u64),
            Some(r.executed_count() as u64)
        );
        assert_eq!(strings(parsed.get("failed_case_ids")), [failed.as_str()]);
        assert_eq!(strings(parsed.get("unrequired_records")), ["unknown-probe"]);
        let excluded = parsed
            .get("excluded_records")
            .and_then(Json::as_array)
            .unwrap();
        assert_eq!(excluded.len(), 1);
        assert_eq!(excluded[0].get("id").and_then(Json::as_str), Some(odd));
        assert_eq!(
            excluded[0].get("reason").and_then(Json::as_str),
            Some("run identity differs in source_snapshot")
        );
        let descriptive = parsed
            .get("descriptive_only")
            .and_then(Json::as_array)
            .unwrap();
        assert_eq!(
            descriptive[0].get("reason").and_then(Json::as_str),
            Some("a reason with \"quotes\"")
        );
        assert_eq!(strings(parsed.get("claim_limits")), r.claim_limits());
    }
    #[test]
    fn reordering_records_changes_neither_the_verdict_nor_its_json() {
        let base = report();
        let mut records: Vec<(ExecutedCase, Coverage)> = base
            .obligations()
            .unwrap()
            .probes()
            .filter(|(id, _)| *id != "CLOSURE-059/exhausted")
            .map(|(id, level)| {
                let coverage = if level == ClaimLevel::Source {
                    Coverage::Grammar
                } else {
                    Coverage::Runtime
                };
                (satisfying(&base, id), coverage)
            })
            .collect();
        // A duplicate success, an irrelevant success and an unrequired failure.
        records.push((case("CLOSURE-059/recovered", true), Coverage::Runtime));
        records.push((case("CLOSURE-059/irrelevant", true), Coverage::Runtime));
        records.push((case("unrelated-semantic-failure", false), Coverage::Runtime));
        let replay = |order: Vec<&(ExecutedCase, Coverage)>| {
            let mut r = report();
            for (case, coverage) in order {
                r.record(case.clone(), *coverage);
            }
            r
        };
        let forward = replay(records.iter().collect());
        let reversed = replay(records.iter().rev().collect());
        let interleaved = replay(
            records
                .iter()
                .skip(1)
                .step_by(2)
                .chain(records.iter().step_by(2))
                .collect(),
        );
        assert_eq!(forward.claim(), ClaimLevel::Source);
        assert_eq!(
            forward.missing_probes(ClaimLevel::Semantics),
            ["CLOSURE-059/exhausted", "CLOSURE-059/recovered"]
        );
        for other in [&reversed, &interleaved] {
            assert_eq!(other.claim(), forward.claim());
            assert_eq!(other.claim_limits(), forward.claim_limits());
            assert_eq!(other.render_verdict_json(), forward.render_verdict_json());
        }
    }
}
