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
//! What it requires is the `05_TESTING_READY_CONTRACT` semantic half:
//!
//! * the claim is `semantics_conforming`;
//! * every required obligation of both levels is satisfied — none failed,
//!   missing, invalid, or counted in two states at once;
//! * the mapping digest is the reviewed inventory's;
//! * the package identity is the approved trust anchor's.

use lcl_spec::json::{self, Json};

/// What an accepted verdict establishes, kept so a caller can print it rather
/// than recompute it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub claim: String,
    pub mapping_digest: String,
    pub package_identity: String,
    /// `(level, required)` for each level the verdict carries.
    pub levels: Vec<(String, u64)>,
}

/// Check one rendered verdict against the readiness contract.
///
/// `Err` holds every reason the verdict is not acceptable, in the order they
/// were found, so one run reports the whole gap rather than the first of it.
pub fn accept(
    verdict: &str,
    expected_mapping_digest: &str,
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
            "the claim is {:?}, and readiness requires \"semantics_conforming\"",
            claim
        ));
    }

    let mapping_digest = string("mapping_digest").unwrap_or_default();
    if mapping_digest != expected_mapping_digest {
        refusals.push(format!(
            "the verdict's mapping digest {mapping_digest:?} is not the reviewed inventory's {expected_mapping_digest:?}"
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

    let mut levels = Vec::new();
    match parsed.get("levels").and_then(Json::as_array) {
        None => refusals.push("the verdict carries no levels".to_string()),
        Some([]) => refusals.push("the verdict carries an empty level list".to_string()),
        Some(entries) => {
            for entry in entries {
                let level = entry
                    .get("level")
                    .and_then(Json::as_str)
                    .unwrap_or("<unnamed>")
                    .to_string();
                let required = entry.get("required").and_then(Json::as_u64);
                let Some(required) = required else {
                    refusals.push(format!("level {level} declares no required count"));
                    continue;
                };
                levels.push((level.clone(), required));
                // Every state but `satisfied` is a gap, and a probe counted in
                // two states at once is an accounting defect of the same kind.
                let mut seen: Vec<&str> = Vec::new();
                let mut total = 0usize;
                for state in ["satisfied", "failed", "missing", "invalid"] {
                    let Some(ids) = entry.get(state).and_then(Json::as_array) else {
                        refusals.push(format!("level {level} carries no {state} list"));
                        continue;
                    };
                    total += ids.len();
                    for id in ids.iter().filter_map(Json::as_str) {
                        if seen.contains(&id) {
                            refusals
                                .push(format!("level {level} counts {id} in more than one state"));
                        }
                        seen.push(id);
                    }
                    if state != "satisfied" && !ids.is_empty() {
                        let named: Vec<&str> = ids.iter().filter_map(Json::as_str).collect();
                        refusals.push(format!(
                            "level {level} has {} {state} obligation(s): {named:?}",
                            ids.len()
                        ));
                    }
                }
                if total != required as usize {
                    refusals.push(format!(
                        "level {level} accounts for {total} obligations and requires {required}"
                    ));
                }
            }
        }
    }

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
