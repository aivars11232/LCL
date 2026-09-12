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

use crate::obligations::{Obligations, MAPPING_DIGEST};
use crate::runner::{judge, ExecutedCase, Verdict};
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
        }
    }
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
        }
    }

    /// A claim-capable report bound to the complete approved inventory.
    pub fn for_spec(spec: &lcl_spec::SpecPackage) -> Result<Self, String> {
        let obligations = Obligations::load(spec)?;
        let index = crate::ConformanceIndex::load(spec).map_err(|e| e.to_string())?;
        let mut report = Self::new(
            Implementation::under_test(spec.identity_digest(), spec.formal_version()),
            index.requirement_count(), index.witnesses().iter().map(|w| w.id.clone()),
        );
        report.obligations = Some(obligations);
        Ok(report)
    }

    pub fn obligations(&self) -> Option<&Obligations> { self.obligations.as_ref() }

    /// Record one executed case and the coverage it contributes.
    pub fn record(&mut self, mut case: ExecutedCase, coverage: Coverage) {
        // A supplied verdict cannot overrule the observation and expectation.
        case.verdict = judge(&case.expectation, &case.observed);
        if case.source.is_empty() { case.verdict = Verdict::Failed; }
        self.covered.push(CoveredCase { case, coverage });
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

    fn probe_established(&self, id: &str) -> bool {
        let mut records = self.covered.iter().filter(|r| r.case.id == id);
        matches!(records.next(), Some(record) if record.case.verdict == Verdict::Passed)
            && records.next().is_none()
    }

    /// All required probes of a witness must be present exactly once and pass.
    /// An arbitrary suffix, duplicate, or one passing prefix cannot fill a gap.
    pub fn unsupported_witnesses(&self) -> Vec<&str> {
        self.indexed_witnesses.iter().filter(|id| {
            self.obligations.as_ref().and_then(|inventory| inventory.row(id))
                .map(|row| !row.probes.iter().all(|probe| self.probe_established(probe)))
                .unwrap_or(true)
        }).map(String::as_str).collect()
    }

    /// Required probes absent, failed or duplicated at this exact claim level.
    pub fn missing_probes(&self, level: ClaimLevel) -> Vec<&str> {
        self.obligations.as_ref().map(|inventory| inventory.probes()
            .filter(|(id, required)| *required == level && !self.probe_established(id))
            .map(|(id, _)| id).collect()).unwrap_or_default()
    }

    /// Claims follow complete obligations, never category hits or caller counts.
    /// A semantic failure does not erase independently complete source evidence.
    pub fn claim(&self) -> ClaimLevel {
        if self.obligations.is_none() || !self.missing_probes(ClaimLevel::Source).is_empty()
            || self.covered.iter().any(|r| matches!(r.coverage, Coverage::Lexical | Coverage::Grammar)
                && r.case.verdict == Verdict::Failed) {
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
        for id in self.failed_ids() { out.push(format!("case {id} failed")); }
        for level in [ClaimLevel::Source, ClaimLevel::Semantics] {
            for id in self.missing_probes(level) {
                out.push(format!("required {level} probe {id} missing, failed or duplicated"));
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
        out.push_str("  host capabilities :\n");
        for capability in &self.implementation.host_capabilities {
            out.push_str(&format!("    - {capability}\n"));
        }

        if let Some(inventory) = &self.obligations {
            out.push_str(&format!("  obligation mapping: {MAPPING_DIGEST}\n  required obligations: {}\n  required probes: {}\n",
                inventory.rows().count(), inventory.probes().count()));
            for level in [ClaimLevel::Source, ClaimLevel::Semantics] {
                out.push_str(&format!("  missing {level} probes: {}\n", self.missing_probes(level).len()));
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
}

#[cfg(test)]
mod tests {
    // These are instrument tests using synthetic observations. They validate
    // report logic and are never emitted as language conformance evidence.
    use super::*;
    use crate::runner::{Expectation, Observed, Reached};

    fn case(id: &str, passed: bool) -> ExecutedCase {
        ExecutedCase { id: id.into(), contract: "report-instrument-test".into(),
            source: "synthetic report instrument input".into(), expectation: Expectation::Accepts,
            observed: Observed { reached: Reached::Completion,
                primary: if passed { None } else { Some("error.operation.precondition".into()) },
                ..Observed::default() }, verdict: Verdict::Passed }
    }
    fn report() -> ConformanceReport {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../canonical/LCL_Core_0.1.0");
        ConformanceReport::for_spec(&lcl_spec::SpecPackage::open(root).unwrap()).unwrap()
    }
    fn fill(report: &mut ConformanceReport, level: ClaimLevel, omitted: &[&str]) {
        let ids: Vec<_> = report.obligations().unwrap().probes()
            .filter(|(id, required)| *required <= level && !omitted.contains(id))
            .map(|(id, _)| id.to_string()).collect();
        for id in ids { report.record(case(&id, true), if id.starts_with("source/") { Coverage::Grammar } else { Coverage::Runtime }); }
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
            let mut r = ConformanceReport::new(Implementation::under_test(lcl_spec::APPROVED_PACKAGE.identity_digest, "0.1.0"), 799, ids);
            for c in Coverage::ALL { r.record(case(&format!("{c}"), true), c); }
            assert_eq!(r.claim(), ClaimLevel::None);
            assert!(r.claim_limits()[0].contains("no verified complete obligation inventory"));
        }
    }
    #[test]
    fn source_requires_every_lexical_grammar_block_and_field_probe() {
        for omitted in ["source/fixture/invalid_bom.lcl", "source/grammar/expression/positive",
            "source/block/ACTION/minimum", "source/field/ACTION/TARGET/form/expression"] {
            let mut r = report(); fill(&mut r, ClaimLevel::Semantics, &[omitted]);
            assert_eq!(r.claim(), ClaimLevel::None, "{omitted}");
            assert_eq!(r.missing_probes(ClaimLevel::Source), [omitted]);
        }
    }
    #[test]
    fn full_source_evidence_supports_source_independently() {
        let mut r = report(); fill(&mut r, ClaimLevel::Source, &[]);
        assert_eq!(r.claim(), ClaimLevel::Source);
        r.record(case("unrelated-semantic-failure", false), Coverage::Runtime);
        assert_eq!(r.claim(), ClaimLevel::Source);
    }
    #[test]
    fn semantic_contracts_are_required_in_addition_to_all_witnesses() {
        let mut r = report();
        let omitted = r.obligations().unwrap().probes().find(|(id, _)| id.starts_with("semantic/operation_binding/")).unwrap().0.to_string();
        fill(&mut r, ClaimLevel::Semantics, &[&omitted]);
        assert!(r.unsupported_witnesses().is_empty());
        assert_eq!(r.claim(), ClaimLevel::Source);
        r.record(case(&omitted, true), Coverage::Runtime);
        assert_eq!(r.claim(), ClaimLevel::Semantics);
    }
    #[test]
    fn necessary_subprobes_cannot_be_replaced_by_passing_prefixes() {
        let mut r = report(); fill(&mut r, ClaimLevel::Semantics, &["CLOSURE-059/exhausted"]);
        r.record(case("CLOSURE-059/irrelevant", true), Coverage::Runtime);
        assert_eq!(r.unsupported_witnesses(), ["CLOSURE-059"]);
        assert_eq!(r.claim(), ClaimLevel::Source);
        r.record(case("CLOSURE-059/exhausted", true), Coverage::Runtime);
        assert_eq!(r.claim(), ClaimLevel::Semantics);
    }
    #[test]
    fn duplicate_required_probes_block_their_level() {
        let mut r = report(); fill(&mut r, ClaimLevel::Semantics, &[]);
        r.record(case("CLOSURE-059/recovered", true), Coverage::Runtime);
        assert_eq!(r.claim(), ClaimLevel::Source);
        assert_eq!(r.unsupported_witnesses(), ["CLOSURE-059"]);
        r.record(case("source/block/ACTION/minimum", true), Coverage::Grammar);
        assert_eq!(r.claim(), ClaimLevel::None);
    }
    #[test]
    fn forged_passing_verdict_does_not_hide_a_failed_observation() {
        let mut r = report(); fill(&mut r, ClaimLevel::Semantics, &["CLOSURE-059/recovered"]);
        r.record(case("CLOSURE-059/recovered", false), Coverage::Runtime);
        assert_eq!(r.failed_ids(), ["CLOSURE-059/recovered"]);
        assert_eq!(r.claim(), ClaimLevel::Source);
    }
    #[test]
    fn empty_source_is_not_executed_evidence() {
        let mut r = report(); let mut c = case("empty", true); c.source.clear();
        r.record(c, Coverage::Lexical); assert_eq!(r.failed_ids(), ["empty"]);
    }
    #[test]
    fn descriptive_and_executed_counts_remain_separate() {
        let mut r = report(); r.record(case("one", true), Coverage::Lexical);
        assert_eq!(r.descriptive_count(), 799); assert_eq!(r.executed_count(), 1);
        assert!(r.render().contains("799 entries, all not_executed"));
    }
    #[test]
    fn descriptive_only_rows_retain_their_reasons() {
        let mut r = report(); r.record_descriptive("CLOSURE-059", "missing fixture");
        assert!(r.render().contains("CLOSURE-059 — missing fixture"));
        assert!(r.unsupported_witnesses().contains(&"CLOSURE-059"));
    }
}
