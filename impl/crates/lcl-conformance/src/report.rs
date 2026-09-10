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

use crate::runner::{ExecutedCase, Verdict};
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
    /// The 66-entry witness catalog size.
    indexed_witnesses: usize,
    covered: Vec<CoveredCase>,
    descriptive_only: Vec<Descriptive>,
}

impl ConformanceReport {
    pub fn new(
        implementation: Implementation,
        indexed_requirements: usize,
        indexed_witnesses: usize,
    ) -> ConformanceReport {
        ConformanceReport {
            implementation,
            indexed_requirements,
            indexed_witnesses,
            covered: Vec::new(),
            descriptive_only: Vec::new(),
        }
    }

    /// Record one executed case and the coverage it contributes.
    pub fn record(&mut self, case: ExecutedCase, coverage: Coverage) {
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
        self.indexed_witnesses
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

    /// Coverage levels with at least one passing case and no failing one.
    fn clean_coverage(&self) -> BTreeSet<Coverage> {
        self.by_coverage()
            .into_iter()
            .filter(|(_, (passed, failed))| *passed > 0 && *failed == 0)
            .map(|(coverage, _)| coverage)
            .collect()
    }

    /// The level this run's executed evidence supports.
    ///
    /// One failed case withdraws the level rather than qualifying it:
    /// "Unsupported core behavior is failure, not silent inference."
    pub fn claim(&self) -> ClaimLevel {
        if self.covered.is_empty() || self.failed_count() > 0 {
            return ClaimLevel::None;
        }
        let clean = self.clean_coverage();
        if !Coverage::SOURCE.iter().all(|c| clean.contains(c)) {
            return ClaimLevel::None;
        }
        if Coverage::SEMANTICS.iter().all(|c| clean.contains(c)) {
            return ClaimLevel::Semantics;
        }
        ClaimLevel::Source
    }

    /// Why the run does not support a higher level, in plain terms.
    pub fn claim_limits(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.covered.is_empty() {
            out.push("no case was executed".to_string());
            return out;
        }
        for id in self.failed_ids() {
            out.push(format!("case {id} failed"));
        }
        let clean = self.clean_coverage();
        for coverage in Coverage::SOURCE.iter().chain(Coverage::SEMANTICS.iter()) {
            if !clean.contains(coverage) {
                out.push(format!("no clean executed coverage at {coverage}"));
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

        out.push_str("\nDESCRIPTIVE CATALOGS (indexed, never executed)\n");
        out.push_str(&format!(
            "  requirements index : {} entries, all not_executed\n",
            self.indexed_requirements
        ));
        out.push_str(&format!(
            "  decision witnesses : {} entries\n",
            self.indexed_witnesses
        ));

        out.push_str("\nEXECUTED CASES (concrete input, observed result)\n");
        out.push_str(&format!(
            "  executed : {}\n  passed   : {}\n  failed   : {}\n",
            self.executed_count(),
            self.passed_count(),
            self.failed_count()
        ));
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
    use super::*;
    use crate::runner::{Expectation, Observed, Reached};

    fn case(id: &str, verdict: Verdict) -> ExecutedCase {
        ExecutedCase {
            id: id.to_string(),
            contract: "test".to_string(),
            source: "LCL:\n".to_string(),
            expectation: Expectation::Accepts,
            observed: Observed {
                reached: Reached::Completion,
                primary: None,
                primary_stage: None,
                diagnostics: Vec::new(),
                terminal_status: Some("status.succeeded".to_string()),
                checks: Vec::new(),
                outputs: Vec::new(),
            },
            verdict,
        }
    }

    fn implementation() -> Implementation {
        Implementation::under_test("digest", "0.1.0")
    }

    #[test]
    fn an_empty_run_claims_nothing() {
        let report = ConformanceReport::new(implementation(), 799, 66);
        assert_eq!(report.claim(), ClaimLevel::None);
        assert_eq!(report.executed_count(), 0);
        assert_eq!(report.descriptive_count(), 799);
    }

    #[test]
    fn one_failed_case_withdraws_the_claim_entirely() {
        let mut report = ConformanceReport::new(implementation(), 799, 66);
        for coverage in Coverage::ALL {
            report.record(case(&format!("ok-{coverage}"), Verdict::Passed), coverage);
        }
        assert_eq!(report.claim(), ClaimLevel::Semantics);

        report.record(case("bad", Verdict::Failed), Coverage::Runtime);
        assert_eq!(
            report.claim(),
            ClaimLevel::None,
            "unsupported behavior is failure, not a footnote"
        );
        assert_eq!(report.failed_ids(), vec!["bad".to_string()]);
    }

    #[test]
    fn source_coverage_alone_claims_source_only() {
        let mut report = ConformanceReport::new(implementation(), 799, 66);
        report.record(case("lex", Verdict::Passed), Coverage::Lexical);
        report.record(case("gram", Verdict::Passed), Coverage::Grammar);
        assert_eq!(report.claim(), ClaimLevel::Source);
        assert!(report
            .claim_limits()
            .iter()
            .any(|l| l.contains("no clean executed coverage at runtime")));
    }

    #[test]
    fn descriptive_and_executed_are_never_summed() {
        let mut report = ConformanceReport::new(implementation(), 799, 66);
        report.record(case("one", Verdict::Passed), Coverage::Lexical);
        assert_eq!(report.descriptive_count(), 799);
        assert_eq!(report.executed_count(), 1);
        let rendered = report.render();
        assert!(rendered.contains("799 entries, all not_executed"));
        assert!(rendered.contains("executed : 1"));
        // There is no combined figure to print, and none is printed.
        assert!(!rendered.contains("800"));
    }

    #[test]
    fn a_descriptive_only_entry_is_listed_with_its_reason() {
        let mut report = ConformanceReport::new(implementation(), 799, 66);
        report.record_descriptive("CLOSURE-999", "needs a real network adapter");
        let rendered = report.render();
        assert!(rendered.contains("CLOSURE-999 — needs a real network adapter"));
    }
}
