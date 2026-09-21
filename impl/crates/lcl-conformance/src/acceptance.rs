//! The testing-readiness acceptance check over one production verdict.
//!
//! The report command answers "what did the engine do"; it exits 0 whenever it
//! could assemble a report, including when the claim it computed is
//! `source_conforming`. That is the right exit for a *report*, and the wrong
//! one for a *gate*: a pipeline that ran the report and looked only at its
//! status would treat an unfinished semantic level as readiness.
//!
//! This module is the gate. It reads the verdict the production report renders
//! — never a second opinion computed some other way — and it fails closed: an
//! absent, malformed or unrecognized verdict is a refusal, exactly like a
//! verdict that says the claim is below `semantics_conforming`.
//!
//! ## Why the inventory comes from outside the verdict
//!
//! A verdict states its own counts and its own probe lists. Checking those
//! against each other establishes that the document is *self*-consistent, which
//! a document that omitted an obligation, or renamed one, would also be. So the
//! required membership of each level is taken from [`Obligations`], loaded
//! independently from the reviewed mapping under its pinned digest and refused
//! unless the approved package verifies. The verdict is then checked *against*
//! that inventory: the same probe ids, the same count, in the same level.
//!
//! What it requires is the `05_TESTING_READY_CONTRACT` semantic half:
//!
//! * the claim is `semantics_conforming`;
//! * both required levels appear exactly once, under recognized names;
//! * each level's satisfied set is exactly the inventory's probes for it —
//!   nothing substituted, omitted, duplicated or moved between levels;
//! * nothing is failed, missing or invalid, and no problem record remains;
//! * the record accounting is consistent with itself and with the states;
//! * the mapping digest is the inventory's and the package identity is the
//!   approved trust anchor's.

use crate::obligations::Obligations;
use crate::report::ClaimLevel;
use lcl_spec::json::{self, Json};
use std::collections::BTreeSet;

/// What an accepted verdict establishes, kept so a caller can print it rather
/// than recompute it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub claim: String,
    pub mapping_digest: String,
    pub package_identity: String,
    /// `(level, required)` for each level, taken from the inventory.
    pub levels: Vec<(String, usize)>,
}

/// The states a level lists its required obligations under.
const STATES: [&str; 4] = ["satisfied", "failed", "missing", "invalid"];

/// Check one rendered verdict against the readiness contract.
///
/// `Err` holds every reason the verdict is not acceptable, in the order they
/// were found, so one run reports the whole gap rather than the first of it.
pub fn accept(
    verdict: &str,
    inventory: &Obligations,
    expected_package_identity: &str,
) -> Result<Accepted, Vec<String>> {
    let parsed = match json::parse(verdict) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Err(vec![format!(
                "the verdict is not JSON ({error}): {} byte(s) beginning {:?}",
                verdict.len(),
                verdict.chars().take(40).collect::<String>()
            )])
        }
    };
    let mut refusals = Vec::new();
    let string = |field: &str| -> Option<String> {
        parsed.get(field).and_then(Json::as_str).map(str::to_string)
    };

    match string("format").as_deref() {
        Some("lcl.conformance.verdict.v1") => {}
        Some(other) => refusals.push(format!(
            "the verdict declares format {other:?}, which this gate does not recognize"
        )),
        None => refusals.push("the verdict declares no format".to_string()),
    }

    let claim = string("claim").unwrap_or_default();
    if claim != "semantics_conforming" {
        refusals.push(format!(
            "the claim is {claim:?}, and readiness requires \"semantics_conforming\""
        ));
    }

    let mapping_digest = string("mapping_digest").unwrap_or_default();
    if mapping_digest != inventory.digest() {
        refusals.push(format!(
            "the verdict's mapping digest {mapping_digest:?} is not the reviewed inventory's {:?}",
            inventory.digest()
        ));
    }

    let package_identity = parsed
        .get("implementation")
        .and_then(|implementation| implementation.get("package_identity"))
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    if package_identity != expected_package_identity {
        refusals.push(format!(
            "the verdict's package identity {package_identity:?} is not the approved anchor's {expected_package_identity:?}"
        ));
    }

    // The required membership of each level, from the inventory rather than
    // from the document being checked.
    let mut levels = Vec::new();
    let required: Vec<(ClaimLevel, BTreeSet<&str>)> = [ClaimLevel::Source, ClaimLevel::Semantics]
        .into_iter()
        .map(|level| {
            let probes = inventory
                .probes()
                .filter(|(_, at)| *at == level)
                .map(|(id, _)| id)
                .collect();
            (level, probes)
        })
        .collect();

    match parsed.get("levels").and_then(Json::as_array) {
        None => refusals.push("the verdict carries no levels".to_string()),
        Some([]) => refusals.push("the verdict carries an empty level list".to_string()),
        Some(entries) => {
            let mut seen_levels: Vec<&str> = Vec::new();
            for entry in entries {
                let Some(name) = entry.get("level").and_then(Json::as_str) else {
                    refusals.push("a level entry declares no level name".to_string());
                    continue;
                };
                if seen_levels.contains(&name) {
                    refusals.push(format!("level {name} appears more than once"));
                    continue;
                }
                seen_levels.push(name);
                let Some((_, expected)) = required.iter().find(|(level, _)| level.as_str() == name)
                else {
                    refusals.push(format!(
                        "level {name:?} is not one of the required conformance levels"
                    ));
                    continue;
                };
                levels.push((name.to_string(), expected.len()));
                check_level(entry, name, expected, &mut refusals);
            }
            for (level, expected) in &required {
                if !seen_levels.contains(&level.as_str()) {
                    refusals.push(format!(
                        "the verdict omits level {}, whose {} obligations are required",
                        level.as_str(),
                        expected.len()
                    ));
                }
            }
        }
    }

    check_records(&parsed, &mut refusals);

    if refusals.is_empty() {
        Ok(Accepted {
            claim,
            mapping_digest,
            package_identity,
            levels,
        })
    } else {
        Err(refusals)
    }
}

/// One level's states, against the obligations that level requires.
fn check_level(entry: &Json, name: &str, expected: &BTreeSet<&str>, refusals: &mut Vec<String>) {
    match entry.get("required").and_then(Json::as_u64) {
        Some(required) if required as usize == expected.len() => {}
        Some(required) => refusals.push(format!(
            "level {name} declares {required} required obligations; the reviewed inventory has {}",
            expected.len()
        )),
        None => refusals.push(format!(
            "level {name} declares no required count, or one that is not a whole number"
        )),
    }

    let mut accounted: BTreeSet<&str> = BTreeSet::new();
    let mut satisfied: BTreeSet<&str> = BTreeSet::new();
    for state in STATES {
        let Some(ids) = entry.get(state).and_then(Json::as_array) else {
            refusals.push(format!("level {name} carries no {state} list"));
            continue;
        };
        for id in ids {
            // A non-string member is a malformed list, not something to skip.
            let Some(id) = id.as_str() else {
                refusals.push(format!(
                    "level {name} lists a non-string obligation in {state}"
                ));
                continue;
            };
            if !accounted.insert(id) {
                refusals.push(format!("level {name} counts {id} in more than one state"));
            }
            if state == "satisfied" {
                satisfied.insert(id);
            }
        }
        if state != "satisfied" && !ids.is_empty() {
            let named: Vec<&str> = ids.iter().filter_map(Json::as_str).collect();
            refusals.push(format!(
                "level {name} has {} {state} obligation(s): {named:?}",
                ids.len()
            ));
        }
    }

    // Membership, both ways: nothing required may be absent, and nothing may be
    // present that the inventory does not require.
    let missing: Vec<&&str> = expected
        .iter()
        .filter(|id| !satisfied.contains(**id))
        .collect();
    if !missing.is_empty() {
        refusals.push(format!(
            "level {name} does not establish {} required obligation(s): {:?}",
            missing.len(),
            missing.iter().take(8).collect::<Vec<_>>()
        ));
    }
    let unknown: Vec<&&str> = satisfied
        .iter()
        .filter(|id| !expected.contains(**id))
        .collect();
    if !unknown.is_empty() {
        refusals.push(format!(
            "level {name} counts {} obligation(s) the reviewed inventory does not require: {:?}",
            unknown.len(),
            unknown.iter().take(8).collect::<Vec<_>>()
        ));
    }
    if let Some(problems) = entry.get("problems").and_then(Json::as_array) {
        if !problems.is_empty() {
            refusals.push(format!(
                "level {name} retains {} problem record(s)",
                problems.len()
            ));
        }
    }
}

/// The executed-record accounting, which must agree with itself and with the
/// failed-case list.
fn check_records(parsed: &Json, refusals: &mut Vec<String>) {
    let Some(records) = parsed.get("records") else {
        refusals.push("the verdict carries no record accounting".to_string());
        return;
    };
    let count = |field: &str| records.get(field).and_then(Json::as_u64);
    match (count("executed"), count("passed"), count("failed")) {
        (Some(executed), Some(passed), Some(failed)) => {
            if passed + failed != executed {
                refusals.push(format!(
                    "the verdict records {executed} executed cases but {passed} passed and {failed} failed"
                ));
            }
            if failed != 0 {
                refusals.push(format!("the verdict records {failed} failed case(s)"));
            }
        }
        _ => refusals.push(
            "the verdict's record accounting is missing executed, passed or failed".to_string(),
        ),
    }
    match parsed.get("failed_case_ids").and_then(Json::as_array) {
        None => refusals.push("the verdict carries no failed case list".to_string()),
        Some(ids) if !ids.is_empty() => {
            let named: Vec<&str> = ids.iter().filter_map(Json::as_str).collect();
            refusals.push(format!("the verdict names failed cases: {named:?}"));
        }
        Some(_) => {}
    }
}
