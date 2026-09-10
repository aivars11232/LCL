//! Human rendering of an engine record.
//!
//! ## Rendering is not semantics
//!
//! The implementation contract is explicit: "Every displayed diagnostic/result
//! must be reproducible through the engine." So this module reads a
//! [`Report`] and formats it. It computes no verdict, resolves no alias,
//! reclassifies no stage, and has no access to a document beyond the record.
//! Everything a reader sees here is also in `--machine` output, in the same
//! words.
//!
//! ## What a human rendering adds
//!
//! Ordering and emphasis, nothing else. A diagnostic leads with
//! `source:line:column`, which is the shape a terminal, an editor and every
//! other tool already know how to jump to — while the byte span it was derived
//! from stays in the machine record, where an exact consumer reads it.

use lcl_protocol::{Outcome, Reached, Report};

/// The complete human rendering of one report.
pub fn report(report: &Report) -> String {
    let mut out = String::new();

    for record in &report.inputs {
        if let Some(reason) = &record.reason {
            out.push_str(&format!("input {}: {reason}\n", record.id));
        }
    }
    if report.outcome == Outcome::Refused {
        out.push_str("\nThe request was not usable, so the document was not judged.\n");
        return out;
    }

    for diagnostic in &report.diagnostics {
        out.push_str(&format!(
            "{}{}\n",
            if diagnostic.primary { "" } else { "  " },
            diagnostic.render()
        ));
        if !diagnostic.meaning.is_empty() {
            out.push_str(&format!("    {}\n", diagnostic.meaning));
        }
    }

    if !report.diagnostics.is_empty() {
        out.push('\n');
    }

    match report.outcome {
        Outcome::Refused => {}
        Outcome::Rejected => {
            out.push_str(&format!(
                "{} was rejected at the {} stage.\n",
                report.root().unwrap_or("the document"),
                report.reached
            ));
        }
        Outcome::Accepted => match report.reached {
            Reached::Completion => {
                out.push_str(&completion(report));
            }
            reached => {
                out.push_str(&format!(
                    "{} passed every stage through {}.\n",
                    report.root().unwrap_or("the document"),
                    reached
                ));
                // Saying what was *not* evaluated matters as much as saying
                // what passed. `Tokenized`, `Parsed` and the rest mean only
                // that the stage succeeded, never that the document is
                // finished, and the same discipline applies to a command.
                out.push_str(&format!(
                    "This is a statement about {}, and not about execution.\n",
                    match reached {
                        Reached::StaticChecking => "canonical steps 1 to 5",
                        Reached::Preflight => "canonical steps 1 to 9, before any effect",
                        _ => "the stages that ran",
                    }
                ));
            }
        },
    }

    if let Some(structure) = &report.structure {
        out.push('\n');
        out.push_str(&format!(
            "{} unit(s), {} import(s), {} declaration(s), {} candidate node(s), \
             {} plan node(s)\n",
            report.units.len(),
            structure.imports.len(),
            structure.declarations.len(),
            structure.candidates,
            structure.plan.len()
        ));
        for import in &structure.imports {
            out.push_str(&format!(
                "  {} {} as {} -> {}\n",
                import.kind,
                import.reference.as_deref().unwrap_or("-"),
                import.namespace,
                import.outcome
            ));
        }
        let mut ordered: Vec<_> = structure
            .plan
            .iter()
            .filter_map(|node| node.order.map(|order| (order, node)))
            .collect();
        ordered.sort_by_key(|(order, _)| *order);
        if !ordered.is_empty() {
            out.push_str("  execution order:\n");
            for (order, node) in ordered {
                out.push_str(&format!(
                    "    {order:>3}. {} {}{}\n",
                    node.block,
                    node.id.as_deref().unwrap_or("-"),
                    match &node.operation {
                        Some(operation) => format!(" [{operation}]"),
                        None => String::new(),
                    }
                ));
            }
        }
        for unused in &structure.unused_inputs {
            out.push_str(&format!(
                "  supplied {unused} matches no declaration and was not read\n"
            ));
        }
    }

    out
}

/// The part of a run a reader most wants: what it decided, and why.
fn completion(report: &Report) -> String {
    let Some(completion) = &report.completion else {
        return String::new();
    };
    let mut out = String::new();
    out.push_str(&format!("{}\n", completion.terminal_status));

    for check in &completion.checks {
        match (&check.outcome, &check.skipped) {
            (Some(outcome), _) => out.push_str(&format!(
                "  {} {} = {outcome}{}\n",
                check.kind,
                check.id,
                if check.required { " (required)" } else { "" }
            )),
            (None, Some(reason)) => {
                out.push_str(&format!("  {} {} skipped: {reason}\n", check.kind, check.id))
            }
            (None, None) => {}
        }
    }

    if let (Some(success), Some(quantifier), Some(value)) = (
        &completion.verdict.success,
        &completion.verdict.quantifier,
        &completion.verdict.success_value,
    ) {
        out.push_str(&format!("  SUCCESS {success} {quantifier} = {value}\n"));
    }
    if let Some(failure) = &completion.verdict.failure {
        out.push_str(&format!(
            "  FAILURE {failure} -> {}\n",
            completion
                .verdict
                .failure_status
                .as_deref()
                .unwrap_or("(no status)")
        ));
    }

    for evidence in &completion.evidence {
        out.push_str(&format!(
            "  EVIDENCE {} {} {}\n",
            evidence.id,
            if evidence.satisfied {
                "satisfied"
            } else {
                "unsatisfied"
            },
            evidence.detail
        ));
    }

    for output in &completion.outputs {
        out.push_str(&format!(
            "  OUTPUT {} {}{}\n",
            output.id,
            output.publication,
            match &output.value {
                Some(value) => format!(" = {value}"),
                None => String::new(),
            }
        ));
    }

    out.push_str(&format!("  because: {}\n", completion.reason));
    if let Some(execution) = &report.execution {
        out.push_str(&format!(
            "  {} invocation(s), {} event(s), {} step(s)\n",
            execution.invocations.len(),
            execution.events.len(),
            execution.steps
        ));
    }
    out
}
