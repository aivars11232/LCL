//! What the execution actually did, as the post-execution checks may see it.
//!
//! ## Why this exists instead of reading the plan
//!
//! `check_selection_contract/selection` draws a line that the candidate graph
//! cannot draw for itself:
//!
//! > Targeted post-execution VERIFY applies only to an actually activated
//! > producer or an observed target of that invocation, not an unselected IF
//! > branch merely present in the candidate graph.
//!
//! The plan holds every node the program *might* run. Only the execution knows
//! which ones it *did*. So targeted `VERIFY` selection reads this observation,
//! built from [`lcl_runtime::Execution`], and never the plan's node list. A
//! `VERIFY` aimed at the arm of an `IF` that was not taken is therefore not
//! selected, not selected-and-skipped: it never enters the set at all.
//!
//! ## Activation is entry, not success
//!
//! A producer that entered and failed is activated. `05_SEMANTICS/10` keeps
//! producer completion and domain outcome apart, and the same separation holds
//! here: an activated producer whose result records `status.failed` is still an
//! observed producer, and a `VERIFY` targeting it still applies. Excluding
//! failed producers would silently drop exactly the checks a failing run most
//! needs.
//!
//! ## Observed targets
//!
//! An observed target is a target the execution actually touched: the resolved
//! `target` of an [`lcl_runtime::ObservedEffect`], plus the declared `TARGET`
//! of an activated producer. `selection` forbids widening beyond that — "never
//! expand ambient resources" — so nothing here enumerates a filesystem, a
//! namespace or a registry to find more targets.

use lcl_runtime::{Execution, InvocationId, IterationPath};
use std::collections::{BTreeMap, BTreeSet};

/// One activation of one declaration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Activation {
    /// The invocation identity, carrying node, iteration path and attempt.
    pub invocation: InvocationId,
    /// The declaring block, e.g. `ACTION`.
    pub block: String,
    /// The producer's terminal lifecycle status at the end of execution.
    pub status: String,
    /// The producer's result record schema, when it produced one.
    pub schema: Option<String>,
}

impl Activation {
    /// The iteration context this activation ran in.
    pub fn iteration(&self) -> &IterationPath {
        &self.invocation.iteration
    }
}

/// What one execution actually activated and observed.
///
/// Construction takes an [`Execution`], which only the runtime produces, so an
/// observation of a program that never ran is not representable.
#[derive(Debug, Clone)]
pub struct Observation {
    activations: BTreeMap<String, Vec<Activation>>,
    observed_targets: BTreeSet<String>,
    effect_targets: BTreeSet<String>,
    invocation_count: usize,
}

impl Observation {
    /// Derive the observation from a finished execution.
    pub fn of(execution: &Execution) -> Observation {
        let mut activations: BTreeMap<String, Vec<Activation>> = BTreeMap::new();
        let mut observed_targets = BTreeSet::new();
        let mut effect_targets = BTreeSet::new();

        for record in execution.invocations() {
            if let Some(declaration) = &record.declaration {
                activations
                    .entry(declaration.clone())
                    .or_default()
                    .push(Activation {
                        invocation: record.id.clone(),
                        block: record.block.clone(),
                        status: record.status().to_string(),
                        schema: record.result.as_ref().map(|r| r.schema.clone()),
                    });
                // An activated producer is itself an observable target: a
                // `VERIFY` whose TARGET names the ACTION that ran applies.
                observed_targets.insert(declaration.clone());
            }
            // "the exact material/address TARGET selected by a graph ACTION."
            // Only effects that were actually recorded count; nothing is
            // inferred from an operation's declared possible effects.
            if let Some(result) = &record.result {
                for effect in &result.observed_effects {
                    if let Some(target) = &effect.target {
                        observed_targets.insert(target.clone());
                        effect_targets.insert(target.clone());
                    }
                }
            }
        }

        // A bound OUTPUT is an observed product of this invocation, so a
        // targeted VERIFY may name it. `05_SEMANTICS/05` read order: "Within a
        // valid instance an output not yet bound yields MISSING" — an unbound
        // output is therefore not observed and is deliberately absent here.
        for ((id, _), _) in execution.bindings().outputs() {
            observed_targets.insert(id.clone());
        }

        Observation {
            activations,
            observed_targets,
            effect_targets,
            invocation_count: execution.invocations().len(),
        }
    }

    /// True when this declaration actually ran at least once.
    pub fn activated(&self, declaration: &str) -> bool {
        self.activations.contains_key(declaration)
    }

    /// Every activation of one declaration, in execution-path order.
    pub fn activations_of(&self, declaration: &str) -> &[Activation] {
        self.activations
            .get(declaration)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Every activated declaration, in identifier order.
    pub fn activated_declarations(&self) -> impl Iterator<Item = &String> {
        self.activations.keys()
    }

    /// True when this identifier is an activated producer, an observed effect
    /// target or a bound output of this invocation.
    ///
    /// This is the whole admissible target universe for a targeted
    /// post-execution check. Anything else is not observed.
    pub fn observed(&self, target: &str) -> bool {
        self.observed_targets.contains(target)
    }

    /// Every observed target, in identifier order.
    pub fn observed_targets(&self) -> impl Iterator<Item = &String> {
        self.observed_targets.iter()
    }

    /// Targets reached through a recorded concrete effect, specifically.
    pub fn effect_targets(&self) -> impl Iterator<Item = &String> {
        self.effect_targets.iter()
    }

    /// How many invocations the execution entered.
    pub fn invocation_count(&self) -> usize {
        self.invocation_count
    }

    /// A canonical, order-stable rendering, for reports and comparison.
    pub fn serialize(&self) -> String {
        let mut out = String::from("OBSERVATION\n");
        for (declaration, activations) in &self.activations {
            for activation in activations {
                out.push_str(&format!(
                    "  activated {declaration} [{}] {} status={}\n",
                    activation.invocation, activation.block, activation.status
                ));
            }
        }
        for target in &self.observed_targets {
            out.push_str(&format!("  observed {target}\n"));
        }
        out
    }
}
