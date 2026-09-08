//! The deterministic step queue.
//!
//! ## Why a queue and not threads
//!
//! `05_SEMANTICS/08`: "mode.parallel permits unspecified scheduling but
//! requires declared independence; conflicting side effects invalidate.
//! Completion semantics remain deterministic." And
//! `execution_graph_contract/parallel`: "Result collection and diagnostics use
//! declared child order, never finish order. Independent eligible children may
//! execute in any order."
//!
//! So the language grants an implementation freedom in *execution* order and
//! withholds it in *observable* order. This runtime takes the freedom as an
//! explicit interleaving over one queue rather than as OS threads, because that
//! makes "the same program, inputs and mock capabilities produce the same
//! observable result independent of scheduler timing" a structural property:
//! there is no scheduler timing. Nothing here reads a clock, spawns a thread or
//! shares mutable state across one.
//!
//! Independence was already *proved* before effects — M5's preflight rejects a
//! `mode.parallel` container whose children conflict — so the queue does not
//! re-derive it. What it does is exercise a different admissible interleaving
//! when asked, so a test can prove the observable result does not depend on
//! which one ran.
//!
//! ## Ordering
//!
//! The queue is LIFO, and callers push a container's `Leave` before its
//! children's `Enter`s, so a container finishes after everything inside it.
//! [`Queue::extend`] reverses a sequential batch on push so the children pop in
//! declared order.

use crate::state::IterationPath;
use crate::value::Value;

/// One unit of work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Begin one plan node in one iteration context.
    Enter {
        node: usize,
        iteration: IterationPath,
    },
    /// Finish one plan node, after everything inside it.
    Leave {
        node: usize,
        iteration: IterationPath,
    },
    /// Run one `FOR EACH` instance and queue the next.
    Iterate {
        node: usize,
        iteration: IterationPath,
        /// The finite snapshot, taken once at reachability.
        snapshot: Vec<Value>,
        index: usize,
    },
}

/// How a batch of independent children is interleaved.
///
/// Every variant is an *admissible* order under
/// `execution_graph_contract/parallel`; none changes observable output, which
/// the engine collects in declared child order regardless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interleaving {
    /// Declared child order.
    #[default]
    Declared,
    /// The reverse admissible order.
    Reversed,
    /// Odd-indexed children first, then even. A third distinct order, so a
    /// determinism test can compare more than two.
    OddFirst,
}

impl Interleaving {
    pub const ALL: [Interleaving; 3] = [
        Interleaving::Declared,
        Interleaving::Reversed,
        Interleaving::OddFirst,
    ];

    /// Reorder one batch of provably independent children.
    fn apply<T>(self, mut batch: Vec<T>) -> Vec<T> {
        match self {
            Interleaving::Declared => batch,
            Interleaving::Reversed => {
                batch.reverse();
                batch
            }
            Interleaving::OddFirst => {
                let mut odd = Vec::new();
                let mut even = Vec::new();
                for (index, item) in batch.into_iter().enumerate() {
                    if index % 2 == 1 {
                        odd.push(item);
                    } else {
                        even.push(item);
                    }
                }
                odd.extend(even);
                odd
            }
        }
    }
}

/// A LIFO queue of steps.
#[derive(Debug, Clone, Default)]
pub struct Queue {
    steps: Vec<Step>,
    interleaving: Interleaving,
}

impl Queue {
    pub fn new() -> Queue {
        Queue::default()
    }

    /// A queue that interleaves independent parallel batches differently.
    pub fn with_interleaving(interleaving: Interleaving) -> Queue {
        Queue {
            steps: Vec::new(),
            interleaving,
        }
    }

    pub fn interleaving(&self) -> Interleaving {
        self.interleaving
    }

    pub fn push(&mut self, step: Step) {
        self.steps.push(step);
    }

    /// Queue one batch of sibling children.
    ///
    /// `parallel` selects whether the batch may be interleaved. A sequential
    /// batch is pushed reversed so it pops in declared order; a parallel batch
    /// is reordered first, which is exactly the freedom "Independent eligible
    /// children may execute in any order" grants.
    pub fn extend(&mut self, steps: impl IntoIterator<Item = Step>, parallel: bool) {
        let batch: Vec<Step> = steps.into_iter().collect();
        let batch = if parallel {
            self.interleaving.apply(batch)
        } else {
            batch
        };
        for step in batch.into_iter().rev() {
            self.steps.push(step);
        }
    }

    /// Take the next step.
    ///
    /// Deliberately not `Iterator::next`: the queue grows while it is being
    /// drained — executing one step queues the steps inside it — and an
    /// `Iterator` that does that is a trap for every adapter that assumes a
    /// fixed sequence.
    pub fn take(&mut self) -> Option<Step> {
        self.steps.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }
}
