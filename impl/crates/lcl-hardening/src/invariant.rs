//! What must hold of every report, whatever the input was.
//!
//! These are not assertions about LCL. They are assertions about the engine's
//! own contract, taken from the execution contract and from the registry:
//!
//! * **Source identity and spans.** "Source byte offsets remain authoritative."
//!   A span that points outside the unit it names is not a locus.
//! * **Registry closure.** Every identifier a stage emits is registered, and
//!   the stage it is classified at is the registry's or the one
//!   `expression_demand_resolution` resolved.
//! * **Stage monotonicity.** "A later-stage artifact can exist only after
//!   earlier required stages have succeeded." A report cannot carry an
//!   execution record while saying it stopped at the lexical stage.
//! * **No false success.** An accepted outcome and a terminal status are
//!   different claims, and a report may not make the second without having run.
//!
//! Each violation names the input that produced it, so a failure is a
//! reproduction rather than a report.

use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_protocol::{Reached, Report};
use std::collections::BTreeMap;

/// One invariant a report broke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub invariant: &'static str,
    pub detail: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.invariant, self.detail)
    }
}

/// The stage a `reached` value corresponds to, for ordering.
fn stage_of(reached: Reached) -> Stage {
    match reached {
        Reached::Lexical => Stage::Lexical,
        Reached::Grammar => Stage::GrammarOrSchema,
        Reached::Resolution => Stage::Resolution,
        Reached::StaticChecking => Stage::StaticOrExpression,
        Reached::Preflight => Stage::Validation,
        Reached::Execution => Stage::Execution,
        Reached::Completion => Stage::VerificationOrCompletion,
    }
}

fn index(stage: Stage) -> usize {
    Stage::ORDER
        .iter()
        .position(|s| *s == stage)
        .expect("every stage is in the normative order")
}

/// Check one report against every invariant, given the sources that produced it.
///
/// `sources` maps each unit identity to the exact bytes the engine was given,
/// which is what a span is checked against.
pub fn check_report(
    report: &Report,
    sources: &BTreeMap<String, String>,
    registry: &DiagnosticRegistry,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for unit in &report.units {
        if let Some(source) = sources.get(&unit.id) {
            if unit.bytes != source.len() {
                violations.push(Violation {
                    invariant: "source identity",
                    detail: format!(
                        "unit {} reports {} bytes for a source of {}",
                        unit.id,
                        unit.bytes,
                        source.len()
                    ),
                });
            }
        }
    }

    for diagnostic in &report.diagnostics {
        // Span containment, against the exact bytes of the unit it names. A
        // diagnostic about a unit the caller did not supply is either an import
        // the provider answered or a defect; the suites that use an empty
        // provider assert there are none.
        if let Some(source) = sources.get(&diagnostic.source) {
            {
                let length = source.len();
                if diagnostic.span.start > length || diagnostic.span.end > length {
                    violations.push(Violation {
                        invariant: "span containment",
                        detail: format!(
                            "{} spans {}..{} of a {length}-byte unit {}",
                            diagnostic.id,
                            diagnostic.span.start,
                            diagnostic.span.end,
                            diagnostic.source
                        ),
                    });
                }
                if diagnostic.span.start > diagnostic.span.end {
                    violations.push(Violation {
                        invariant: "span containment",
                        detail: format!(
                            "{} spans {}..{}, which is backwards",
                            diagnostic.id, diagnostic.span.start, diagnostic.span.end
                        ),
                    });
                }
            }
        }

        // Registry closure.
        if registry.error(&diagnostic.id).is_none() {
            violations.push(Violation {
                invariant: "registry closure",
                detail: format!("{} is not a registered identifier", diagnostic.id),
            });
        }

        // No diagnostic may be classified later than the report says it got.
        if index(diagnostic.stage) > index(stage_of(report.reached)) {
            violations.push(Violation {
                invariant: "stage monotonicity",
                detail: format!(
                    "{} is classified {} but the report reached {}",
                    diagnostic.id,
                    diagnostic.stage.as_registry_str(),
                    report.reached
                ),
            });
        }
    }

    // Stage monotonicity for the artifacts themselves.
    if report.execution.is_some() && index(stage_of(report.reached)) < index(Stage::Execution) {
        violations.push(Violation {
            invariant: "stage monotonicity",
            detail: format!(
                "an execution record exists while the report reached {}",
                report.reached
            ),
        });
    }
    if report.completion.is_some()
        && index(stage_of(report.reached)) < index(Stage::VerificationOrCompletion)
    {
        violations.push(Violation {
            invariant: "stage monotonicity",
            detail: format!(
                "a completion record exists while the report reached {}",
                report.reached
            ),
        });
    }

    // "No false success claims": a terminal status is a claim that execution
    // finished, and only completion can make it.
    if report.terminal_status().is_some() && report.reached != Reached::Completion {
        violations.push(Violation {
            invariant: "no false success",
            detail: format!(
                "a terminal status is reported while the report reached {}",
                report.reached
            ),
        });
    }

    violations
}
