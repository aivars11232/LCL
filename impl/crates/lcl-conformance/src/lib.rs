//! # lcl-conformance — the descriptive index, and the executable case runner
//!
//! Milestone M0 component K, extended at M8 with execution.
//!
//! Two things live here, and the whole design turns on keeping them apart.
//!
//! **The descriptive index.** [`ConformanceIndex`] loads the two catalogs the
//! package ships:
//!
//! * `core_conformance_cases_v0.1.0.json` — a 799-entry **requirements index**;
//! * `language_decision_cases_v0.1.0.json` — 66 **decision witnesses**.
//!
//! **The executable runner.** [`runner::Runner`] carries a concrete source
//! through every implemented stage — lexer, parser, resolver, checker, semantic
//! preflight, runtime over the real Core operation surface, completion — and
//! reports what the engine actually did.
//!
//! ## The rule that keeps them apart
//!
//! `09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt` is explicit:
//!
//! > catalog entries without concrete input and an implementation result are
//! > not executed conformance cases
//!
//! An indexed [`Requirement`] has neither. It is prose from a registry, and it
//! still loads in state [`CaseState::NotExecuted`] — the only state that enum
//! has, at M8 as at M0. Nothing in this crate can move it to another one,
//! because there is no other one.
//!
//! An executed case is a *different type*. A [`runner::ExecutedCase`] cannot be
//! constructed without the exact source that ran and the
//! [`runner::Observed`] result the engine produced, and its verdict is computed
//! by comparing them rather than asserted. So the two populations cannot be
//! summed by accident: they do not share a type, a state or a counter, and
//! [`report::ConformanceReport`] counts them in separate columns.
//!
//! What changed at M8 is not the standard of evidence. It is that an engine now
//! exists to supply it.

pub mod fixtures;
pub mod obligations;
pub mod operation_cases;
pub mod production;
pub mod report;
pub mod result_cases;
pub mod runner;
pub mod semantic_cases;
pub mod source_cases;
pub mod witness_cases;

pub use report::{ConformanceReport, Coverage};
pub use runner::{
    judge, ExecutedCase, Expectation, Observed, Reached, Runner, RunnerError, Verdict,
};

use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::BTreeMap;
use std::fmt;

/// Execution state of a catalog entry.
///
/// There is intentionally no `Pass` or `Fail` variant at M0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseState {
    /// Indexed, never executed. The only state reachable in M0.
    NotExecuted,
}

impl fmt::Display for CaseState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaseState::NotExecuted => f.write_str("not_executed"),
        }
    }
}

/// One entry of the 799-item requirements index.
#[derive(Debug, Clone)]
pub struct Requirement {
    pub id: String,
    pub category: String,
    pub subject: String,
    /// Normative prose describing what an implementation must do.
    pub requirement: String,
    /// Declared expectation, e.g. `accept` or `reject`.
    pub expected: String,
    /// Registry this requirement was indexed from.
    pub source: String,
    pub state: CaseState,
}

/// One of the 66 descriptive decision witnesses.
#[derive(Debug, Clone)]
pub struct Witness {
    pub id: String,
    pub contract: String,
    /// The witnessing construct, as LCL source fragment prose.
    pub witness: String,
    pub expected: String,
    pub state: CaseState,
}

#[derive(Debug)]
pub enum ConformanceError {
    MissingCatalog(&'static str),
    Malformed(String),
    /// Declared `case_count` disagreed with the number of entries.
    CountMismatch {
        catalog: &'static str,
        declared: u64,
        actual: usize,
    },
    /// The witness catalog declared itself executed, which M0 cannot honour.
    UnexpectedExecutedFlag,
}

impl fmt::Display for ConformanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConformanceError::MissingCatalog(c) => write!(f, "catalog {c:?} not loaded"),
            ConformanceError::Malformed(m) => write!(f, "malformed catalog: {m}"),
            ConformanceError::CountMismatch { catalog, declared, actual } => write!(
                f,
                "{catalog}: declared case_count {declared} but found {actual} entries"
            ),
            ConformanceError::UnexpectedExecutedFlag => f.write_str(
                "language_decision_cases declares executed:true, which no M0 build can substantiate",
            ),
        }
    }
}

impl std::error::Error for ConformanceError {}

/// Indexed, non-executed view of the descriptive conformance catalogs.
pub struct ConformanceIndex {
    requirements: Vec<Requirement>,
    witnesses: Vec<Witness>,
    by_id: BTreeMap<String, usize>,
    declared_category_counts: BTreeMap<String, u64>,
}

impl ConformanceIndex {
    pub fn load(spec: &SpecPackage) -> Result<Self, ConformanceError> {
        let core = spec
            .catalog("core_conformance_cases")
            .ok_or(ConformanceError::MissingCatalog("core_conformance_cases"))?;
        let decisions = spec
            .catalog("language_decision_cases")
            .ok_or(ConformanceError::MissingCatalog("language_decision_cases"))?;

        // The witness catalog must continue to declare itself unexecuted.
        if decisions.get("executed").and_then(|v| v.as_bool()) != Some(false) {
            return Err(ConformanceError::UnexpectedExecutedFlag);
        }

        let cases = core
            .get("cases")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                ConformanceError::Malformed("core catalog has no 'cases' array".into())
            })?;

        let mut requirements = Vec::with_capacity(cases.len());
        for c in cases {
            requirements.push(Requirement {
                id: field(c, "id")?,
                category: field(c, "category")?,
                subject: field(c, "subject")?,
                requirement: field(c, "requirement")?,
                expected: field(c, "expected")?,
                source: field(c, "source")?,
                state: CaseState::NotExecuted,
            });
        }

        if let Some(declared) = core.get("case_count").and_then(|v| v.as_u64()) {
            if declared as usize != requirements.len() {
                return Err(ConformanceError::CountMismatch {
                    catalog: "core_conformance_cases",
                    declared,
                    actual: requirements.len(),
                });
            }
        }

        let wcases = decisions
            .get("cases")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                ConformanceError::Malformed("decision catalog has no 'cases' array".into())
            })?;
        let mut witnesses = Vec::with_capacity(wcases.len());
        for c in wcases {
            witnesses.push(Witness {
                id: field(c, "id")?,
                contract: field(c, "contract")?,
                witness: field(c, "witness")?,
                expected: field(c, "expected")?,
                state: CaseState::NotExecuted,
            });
        }
        if let Some(declared) = decisions.get("case_count").and_then(|v| v.as_u64()) {
            if declared as usize != witnesses.len() {
                return Err(ConformanceError::CountMismatch {
                    catalog: "language_decision_cases",
                    declared,
                    actual: witnesses.len(),
                });
            }
        }

        let mut by_id = BTreeMap::new();
        for (i, r) in requirements.iter().enumerate() {
            if by_id.insert(r.id.clone(), i).is_some() {
                return Err(ConformanceError::Malformed(format!(
                    "duplicate requirement id {}",
                    r.id
                )));
            }
        }

        let mut declared_category_counts = BTreeMap::new();
        if let Some(Json::Object(members)) = core.get("category_counts") {
            for (k, v) in members {
                if let Some(n) = v.as_u64() {
                    declared_category_counts.insert(k.clone(), n);
                }
            }
        }

        Ok(Self {
            requirements,
            witnesses,
            by_id,
            declared_category_counts,
        })
    }

    pub fn requirements(&self) -> &[Requirement] {
        &self.requirements
    }

    pub fn witnesses(&self) -> &[Witness] {
        &self.witnesses
    }

    pub fn requirement(&self, id: &str) -> Option<&Requirement> {
        self.by_id.get(id).map(|i| &self.requirements[*i])
    }

    pub fn requirement_count(&self) -> usize {
        self.requirements.len()
    }

    pub fn witness_count(&self) -> usize {
        self.witnesses.len()
    }

    /// Observed requirement counts per category.
    pub fn observed_category_counts(&self) -> BTreeMap<String, u64> {
        let mut out: BTreeMap<String, u64> = BTreeMap::new();
        for r in &self.requirements {
            *out.entry(r.category.clone()).or_insert(0) += 1;
        }
        out
    }

    /// Categories declared by the catalog.
    pub fn declared_category_counts(&self) -> &BTreeMap<String, u64> {
        &self.declared_category_counts
    }

    /// Categories where declared and observed counts disagree.
    pub fn category_count_defects(&self) -> Vec<(String, u64, u64)> {
        let observed = self.observed_category_counts();
        let mut out = Vec::new();
        for (cat, declared) in &self.declared_category_counts {
            let actual = observed.get(cat).copied().unwrap_or(0);
            if actual != *declared {
                out.push((cat.clone(), *declared, actual));
            }
        }
        for (cat, actual) in &observed {
            if !self.declared_category_counts.contains_key(cat) {
                out.push((cat.clone(), 0, *actual));
            }
        }
        out
    }

    /// Requirements whose `source` names the given registry file.
    pub fn by_source(&self, source: &str) -> Vec<&Requirement> {
        self.requirements
            .iter()
            .filter(|r| r.source == source)
            .collect()
    }

    pub fn by_category(&self, category: &str) -> Vec<&Requirement> {
        self.requirements
            .iter()
            .filter(|r| r.category == category)
            .collect()
    }

    /// Distinct `expected` values across the index.
    pub fn expectation_vocabulary(&self) -> BTreeMap<String, u64> {
        let mut out: BTreeMap<String, u64> = BTreeMap::new();
        for r in &self.requirements {
            *out.entry(r.expected.clone()).or_insert(0) += 1;
        }
        out
    }

    /// Every indexed entry is unexecuted. Structural, not aspirational: there is
    /// no code path in this crate that can set any other state.
    pub fn all_unexecuted(&self) -> bool {
        self.requirements
            .iter()
            .all(|r| r.state == CaseState::NotExecuted)
            && self
                .witnesses
                .iter()
                .all(|w| w.state == CaseState::NotExecuted)
    }

    /// Why no conformance level may be claimed from this crate.
    ///
    /// Returns the reason unconditionally. There is no argument that makes it
    /// return `None`, by design.
    pub fn claim_blocked_reason(&self) -> &'static str {
        "No conformance level may be claimed from this index. The catalogs are descriptive \
         requirement indexes: per 09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt, catalog \
         entries without concrete input and an implementation result are not executed \
         conformance cases, and every entry here has neither. A claim requires executed \
         evidence from `runner::Runner`, and must state implementation version, host \
         capabilities, exact failed case IDs and evidence, which `report::ConformanceReport` \
         carries."
    }
}

fn field(v: &Json, key: &str) -> Result<String, ConformanceError> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::to_string)
        .ok_or_else(|| ConformanceError::Malformed(format!("catalog entry missing field {key:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obligation_inventory_requires_the_approved_package() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../canonical/LCL_Core_0.1.0");
        let spec = SpecPackage::open(&root).unwrap();
        let inventory = obligations::Obligations::load(&spec).unwrap();
        assert_eq!(inventory.rows().count(), 980);
        assert_eq!(inventory.probes().count(), 2413);
        assert_eq!(
            inventory
                .rows()
                .filter(|r| r.id.starts_with("source/field/"))
                .count(),
            334
        );
        assert_eq!(
            inventory
                .rows()
                .filter(|r| r.id.starts_with("CLOSURE-"))
                .count(),
            66
        );
        let unverified = SpecPackage::open_unverified(&root).unwrap();
        assert!(obligations::Obligations::load(&unverified).is_err());
    }

    #[test]
    fn case_state_has_no_pass_variant() {
        // Guards the invariant by construction: if a Pass/Fail variant is ever
        // added, this exhaustive match stops compiling and forces a review of
        // the false-claim risk.
        let s = CaseState::NotExecuted;
        match s {
            CaseState::NotExecuted => {}
        }
        assert_eq!(s.to_string(), "not_executed");
    }
}
