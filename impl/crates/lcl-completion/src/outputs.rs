//! Step 13a: the declared outputs an invocation publishes.
//!
//! Authority: `05_SEMANTICS/05_INPUT_DATA_OUTPUT_RESULT_AND_FORMAT.txt`,
//! `OUTPUT OWNERSHIP AND INVOCATION BINDING`, and
//! `failure_lifecycle#/output_binding_rule`.
//!
//! ## What this layer is, and is not
//!
//! Step 10 already bound each `OUTPUT` to its producing `ACTION`'s projection.
//! Producer ownership is checked before effects — "Two distinct ACTION owners
//! use error.execution.order before effects" — and M5 owns that. Nothing here
//! re-decides either.
//!
//! What is left is the *root's* view: an `EXECUTE` or `TASK` `OUTPUT` list
//! "select[s] requirements or exports; they do not produce or reassign values."
//! So this module reads the root's declared output list and reports, for each
//! entry, whether the invocation actually bound it.
//!
//! ## Why a loop-local output cannot be exported
//!
//! > Root OUTPUT requirements and exports cannot select a loop-local output.
//!
//! An `ACTION` replicated by `FOR EACH` binds one output per full enclosing
//! iteration path, and "No implicit last-value selection or collection
//! aggregation exists." A root export naming such an output has no unique
//! instance to publish, so it is reported [`Publication::Ambiguous`] rather
//! than silently resolved to some instance. Picking one would be inventing an
//! aggregation the language explicitly does not have.
//!
//! ## Unbound is MISSING, not absent
//!
//! > Within a valid instance an output not yet bound yields MISSING.
//!
//! A required root output that is unbound therefore prevents
//! `status.succeeded`, which `05_SEMANTICS/10` states directly: root success
//! needs "all required outputs are fully bound and valid".

use crate::engine::Engine;
use crate::syntax;
use lcl_runtime::{IterationPath, Value};

/// What happened to one declared root output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Publication {
    /// Bound at the root instance and published.
    Published(Value),
    /// Selected but never bound. Reads as MISSING.
    Unbound,
    /// Bound only inside a loop instance, which a root export cannot select.
    Ambiguous(usize),
}

impl Publication {
    pub fn published(&self) -> bool {
        matches!(self, Publication::Published(_))
    }
}

/// One declared root output and its publication state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputRecord {
    pub id: String,
    /// The declared `TYPE`, verbatim.
    pub declared_type: Option<String>,
    pub required: bool,
    pub publication: Publication,
}

impl OutputRecord {
    /// True when an unsatisfied required output blocks success.
    pub fn blocks(&self) -> bool {
        self.required && !self.publication.published()
    }

    pub fn serialize(&self) -> String {
        match &self.publication {
            Publication::Published(value) => {
                format!("{} required={} = {value}", self.id, self.required)
            }
            Publication::Unbound => format!("{} required={} unbound", self.id, self.required),
            Publication::Ambiguous(n) => format!(
                "{} required={} ambiguous across {n} loop instances",
                self.id, self.required
            ),
        }
    }
}

/// The declared outputs of one invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outputs {
    records: Vec<OutputRecord>,
}

impl Outputs {
    pub fn records(&self) -> &[OutputRecord] {
        &self.records
    }

    pub fn get(&self, id: &str) -> Option<&OutputRecord> {
        self.records.iter().find(|r| r.id == id)
    }

    /// Every required output that is not fully bound.
    pub fn unsatisfied(&self) -> impl Iterator<Item = &OutputRecord> {
        self.records.iter().filter(|r| r.blocks())
    }

    /// True when every required declared output is fully bound.
    pub fn complete(&self) -> bool {
        self.records.iter().all(|r| !r.blocks())
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn serialize(&self) -> String {
        let mut out = String::from("OUTPUTS\n");
        for record in &self.records {
            out.push_str(&format!("  {}\n", record.serialize()));
        }
        out
    }
}

/// Collect the root's declared outputs and their publication state.
pub(crate) fn run(engine: &Engine) -> Outputs {
    let mut ids: Vec<String> = Vec::new();

    // The EXECUTE block's own OUTPUT export list, when it declares one, then
    // the root declaration's. Both "select requirements or exports".
    if let Some(root) = engine.root_declaration() {
        if let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, root) {
            if let Some(field) = block.field("OUTPUT") {
                for (id, _) in syntax::reference_list(&field.body) {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
    }

    let mut records = Vec::new();
    for id in ids {
        let Some(declaration) = engine
            .resolved
            .declarations()
            .all()
            .iter()
            .position(|d| d.id.qualified() == id && d.block == "OUTPUT")
        else {
            continue;
        };
        let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, declaration)
        else {
            continue;
        };
        let declared_type = lcl_runtime::syntax::field_text(&block, "TYPE");
        let required = match lcl_runtime::syntax::field_text(&block, "REQUIRED").as_deref() {
            Some("TRUE") => true,
            Some("FALSE") => false,
            _ => engine
                .contracts
                .runtime()
                .preflight()
                .field_default_boolean("OUTPUT", "REQUIRED")
                .unwrap_or(true),
        };

        let root_path = IterationPath::root();
        let publication = match engine.bindings.output(&id, &root_path) {
            Some(value) => {
                // A binding visible from the root path is either bound at the
                // root itself or bound in exactly one instance of it. Either
                // way it names a unique value, so it publishes. Several loop
                // instances do not, because "No implicit last-value selection
                // or collection aggregation exists."
                let instances = engine
                    .bindings
                    .outputs()
                    .filter(|((bound, _), _)| bound == &id)
                    .count();
                let at_root = engine
                    .bindings
                    .outputs()
                    .any(|((bound, path), _)| bound == &id && path.is_root());
                if at_root || instances == 1 {
                    Publication::Published(value.clone())
                } else {
                    Publication::Ambiguous(instances)
                }
            }
            None => {
                let instances = engine
                    .bindings
                    .outputs()
                    .filter(|((bound, _), _)| bound == &id)
                    .count();
                if instances > 0 {
                    // Bound only in loop instances the root cannot select.
                    Publication::Ambiguous(instances)
                } else {
                    Publication::Unbound
                }
            }
        };

        records.push(OutputRecord {
            id,
            declared_type,
            required,
            publication,
        });
    }

    Outputs { records }
}
