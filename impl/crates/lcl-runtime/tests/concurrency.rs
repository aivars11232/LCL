//! `mode.parallel`, and the determinism it must not cost.
//!
//! Authority: `05_SEMANTICS/08` and
//! `block_schemas_v0.1.0.json#/execution_graph_contract/parallel`.
//!
//! ## The two halves of the parallel rule
//!
//! > mode.parallel permits unspecified scheduling but requires declared
//! > independence; conflicting side effects invalidate. Completion semantics
//! > remain deterministic.
//!
//! and
//!
//! > Result collection and diagnostics use declared child order, never finish
//! > order. Independent eligible children may execute in any order.
//!
//! So execution order is free and observable order is fixed. These tests
//! exercise the freedom — running the same program under every admissible
//! interleaving this runtime implements — and assert the fixity: identical
//! serialized output every time.
//!
//! Independence itself is not re-checked here. M5 proves it before effects and
//! refuses to plan a `mode.parallel` container whose children conflict, so a
//! plan that reaches this layer already has it.

mod common;

use lcl_runtime::{Interleaving, MockHost, Runtime};

/// A `kind.task` document whose phase runs independent steps in parallel.
///
/// The three actions are read-only, take distinct targets and bind no output,
/// so they are independent under every clause of the parallel rule: "no
/// conflicting writes, read/write dependency, shared mutable binding, or
/// conflicting output destination".
fn parallel_document(mode: &str) -> String {
    format!(
        r#"LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: example.parallel
    NAME: "Parallel"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.one
    TYPE: INTEGER
    VALUE: 1

INPUT:
    ID: input.two
    TYPE: INTEGER
    VALUE: 2

INPUT:
    ID: input.three
    TYPE: INTEGER
    VALUE: 3

GOAL:
    ID: goal.parallel
    ASSERT: TRUE

ACTION:
    ID: action.one
    OPERATION: core.inspect
    TARGET: REF(input.one)

ACTION:
    ID: action.two
    OPERATION: core.inspect
    TARGET: REF(input.two)

ACTION:
    ID: action.three
    OPERATION: core.inspect
    TARGET: REF(input.three)

SEQUENCE:
    ID: sequence.parallel
    MODE: {mode}
    STEP:
        ID: step.one
        ACTION: REF(action.one)
    STEP:
        ID: step.two
        ACTION: REF(action.two)
    STEP:
        ID: step.three
        ACTION: REF(action.three)

SUCCESS:
    ID: success.parallel
    ALL: [TRUE]

TASK:
    ID: task.parallel
    GOAL: REF(goal.parallel)
    INPUT: [REF(input.one), REF(input.two), REF(input.three)]
    SEQUENCE: REF(sequence.parallel)
    SUCCESS: REF(success.parallel)

EXECUTE:
    REFERENCE: REF(task.parallel)
"#
    )
}

/// Execute one document under one interleaving.
fn run_under(source: &str, interleaving: Interleaving) -> (String, Vec<String>) {
    let fixture = common::fixture(source);
    let mut host = MockHost::new();
    let execution = Runtime::new(common::contracts())
        .with_interleaving(interleaving)
        .execute(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut host,
        )
        .expect("preflight planned it");
    let order: Vec<String> = host
        .requests()
        .iter()
        .map(|r| r.target.as_ref().expect("a target").to_string())
        .collect();
    (execution.serialize(), order)
}

// ---------------------------------------------------------------------------
// The freedom
// ---------------------------------------------------------------------------

#[test]
fn a_parallel_container_admits_more_than_one_execution_order() {
    // "Independent eligible children may execute in any order." If every
    // interleaving produced the same execution order, the determinism tests
    // below would prove nothing.
    let source = parallel_document("mode.parallel");
    let orders: Vec<Vec<String>> = Interleaving::ALL
        .into_iter()
        .map(|i| run_under(&source, i).1)
        .collect();
    let distinct: std::collections::BTreeSet<&Vec<String>> = orders.iter().collect();
    assert!(
        distinct.len() > 1,
        "the interleavings must actually differ: {orders:?}"
    );
    // Every one still runs each child exactly once.
    for order in &orders {
        assert_eq!(order.len(), 3, "{order:?}");
        let unique: std::collections::BTreeSet<&String> = order.iter().collect();
        assert_eq!(unique.len(), 3, "{order:?}");
    }
}

#[test]
fn a_sequential_container_admits_exactly_one_order() {
    // "Sequential lexical order contributes required predecessor edges", so a
    // sequential group is not free to reorder, whatever the queue is asked to
    // do.
    let source = parallel_document("mode.sequential");
    let orders: Vec<Vec<String>> = Interleaving::ALL
        .into_iter()
        .map(|i| run_under(&source, i).1)
        .collect();
    for order in &orders {
        assert_eq!(
            order,
            &vec!["1".to_string(), "2".to_string(), "3".to_string()]
        );
    }
}

// ---------------------------------------------------------------------------
// The fixity
// ---------------------------------------------------------------------------

#[test]
fn every_admissible_interleaving_produces_identical_observable_output() {
    // The acceptance criterion: "Same program/inputs/mock capabilities produce
    // the same observable result independent of scheduler timing."
    let source = parallel_document("mode.parallel");
    let outputs: Vec<String> = Interleaving::ALL
        .into_iter()
        .map(|i| run_under(&source, i).0)
        .collect();
    for output in &outputs[1..] {
        assert_eq!(
            output, &outputs[0],
            "an admissible interleaving changed the observable result"
        );
    }
}

#[test]
fn results_are_collected_in_declared_child_order_never_finish_order() {
    // "Result collection and diagnostics use declared child order, never finish
    // order."
    let source = parallel_document("mode.parallel");
    for interleaving in Interleaving::ALL {
        let fixture = common::fixture(&source);
        let mut host = MockHost::new();
        let execution = Runtime::new(common::contracts())
            .with_interleaving(interleaving)
            .execute(
                &fixture.planned,
                &fixture.checked,
                &fixture.resolved,
                &mut host,
            )
            .expect("planned");
        let declared: Vec<&str> = execution
            .invocations()
            .iter()
            .filter_map(|r| r.declaration.as_deref())
            .filter(|id| id.starts_with("step."))
            .collect();
        assert_eq!(
            declared,
            vec!["step.one", "step.two", "step.three"],
            "under {interleaving:?}"
        );
    }
}

#[test]
fn diagnostics_are_ordered_by_declared_path_under_every_interleaving() {
    // A failing parallel group: whichever child the queue ran first, the
    // diagnostics come back in declared execution-path order.
    let source = parallel_document("mode.parallel");
    let mut rendered: Vec<Vec<String>> = Vec::new();
    for interleaving in Interleaving::ALL {
        let fixture = common::fixture(&source);
        let mut host = MockHost::new().unavailable("core.inspect", "no capability");
        let execution = Runtime::new(common::contracts())
            .with_interleaving(interleaving)
            .execute(
                &fixture.planned,
                &fixture.checked,
                &fixture.resolved,
                &mut host,
            )
            .expect("planned");
        rendered.push(
            execution
                .diagnostics()
                .iter()
                .map(|d| format!("{}@{}", d.id, d.producer_path.unwrap_or_default()))
                .collect(),
        );
    }
    for list in &rendered[1..] {
        assert_eq!(list, &rendered[0]);
    }
    assert!(!rendered[0].is_empty(), "the run produced diagnostics");
}

// ---------------------------------------------------------------------------
// The strategy, checked structurally
// ---------------------------------------------------------------------------

#[test]
fn the_runtime_contains_no_thread_and_no_hash_container() {
    // The owner-approved strategy is a deterministic explicit step queue, not
    // OS threads: "no `std::thread`; no scheduler-timing dependence". And
    // `05_SEMANTICS/11` forbids observable meaning depending on "hash-map
    // iteration order", which is guaranteed here by not having one.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut stack = vec![root];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            for entry in std::fs::read_dir(&path).expect("readable") {
                stack.push(entry.expect("readable").path());
            }
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("readable");
        for needle in ["std::thread", "HashMap", "HashSet", "SystemTime", "Instant"] {
            // The doc comments explain *why* these are absent, so only real
            // code occurrences count.
            for line in text.lines() {
                let code = line.trim_start();
                if code.starts_with("//") || code.starts_with("*") {
                    continue;
                }
                if code.contains(needle) {
                    offenders.push(format!("{}: {code}", path.display()));
                }
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
}

#[test]
fn the_queue_reorders_only_a_parallel_batch() {
    use lcl_runtime::{IterationPath, Queue, Step};
    let steps = |queue: &mut Queue| {
        let mut out = Vec::new();
        while let Some(step) = queue.take() {
            if let Step::Enter { node, .. } = step {
                out.push(node);
            }
        }
        out
    };
    let batch = || {
        (1..=3).map(|node| Step::Enter {
            node,
            iteration: IterationPath::root(),
        })
    };

    // A sequential batch pops in declared order under every interleaving.
    for interleaving in Interleaving::ALL {
        let mut queue = Queue::with_interleaving(interleaving);
        queue.extend(batch(), false);
        assert_eq!(steps(&mut queue), vec![1, 2, 3], "{interleaving:?}");
    }

    // A parallel batch may be reordered.
    let mut declared = Queue::with_interleaving(Interleaving::Declared);
    declared.extend(batch(), true);
    assert_eq!(steps(&mut declared), vec![1, 2, 3]);

    let mut reversed = Queue::with_interleaving(Interleaving::Reversed);
    reversed.extend(batch(), true);
    assert_eq!(steps(&mut reversed), vec![3, 2, 1]);

    let mut odd_first = Queue::with_interleaving(Interleaving::OddFirst);
    odd_first.extend(batch(), true);
    assert_eq!(steps(&mut odd_first), vec![2, 1, 3]);
}

#[test]
fn a_canonical_example_is_identical_under_every_interleaving() {
    for name in common::canonical_example_names() {
        let mut outputs = Vec::new();
        for interleaving in Interleaving::ALL {
            let fixture = common::example_fixture(&name);
            let mut host = MockHost::new();
            let execution = Runtime::new(common::contracts())
                .with_interleaving(interleaving)
                .execute(
                    &fixture.planned,
                    &fixture.checked,
                    &fixture.resolved,
                    &mut host,
                )
                .expect("planned");
            outputs.push(execution.serialize());
        }
        for output in &outputs[1..] {
            assert_eq!(output, &outputs[0], "{name} varied with the interleaving");
        }
    }
}
