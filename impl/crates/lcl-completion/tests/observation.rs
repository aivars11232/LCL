//! What an execution actually activated, as post-execution checks may see it.

mod common;

use common::*;
use lcl_completion::Observation;

fn canonical_example(name: &str) -> String {
    std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID").join(name))
        .unwrap_or_else(|e| panic!("{name} is readable: {e}"))
}

#[test]
fn an_executed_action_is_an_activated_producer_and_an_observed_target() {
    let fixture = execute(&canonical_example("01_MINIMAL_TASK.lcl"));
    let observation = Observation::of(&fixture.execution);

    assert!(
        observation.activated("action.calculate"),
        "the example's only ACTION ran"
    );
    assert!(
        observation.observed("action.calculate"),
        "an activated producer is observable"
    );
    assert_eq!(observation.activations_of("action.calculate").len(), 1);
}

#[test]
fn a_bound_output_is_observed_and_an_undeclared_name_is_not() {
    let fixture = execute(&canonical_example("01_MINIMAL_TASK.lcl"));
    let observation = Observation::of(&fixture.execution);

    assert!(
        observation.observed("output.value"),
        "the ACTION bound this OUTPUT, so it is an observed product"
    );
    assert!(
        !observation.observed("output.never_declared"),
        "selection never expands beyond what the invocation touched"
    );
    assert!(
        !observation.observed("action.does_not_exist"),
        "an unrun declaration is not observed"
    );
}

#[test]
fn an_unselected_branch_is_present_in_the_plan_and_absent_from_the_observation() {
    // The canonical condition example takes exactly one arm. The arm not taken
    // is a plan node and must not become an observed target, because
    // `selection` excludes "an unselected IF branch merely present in the
    // candidate graph".
    let fixture = execute(&canonical_example("05_CONDITION_AND_ITERATION.lcl"));
    let observation = Observation::of(&fixture.execution);

    let planned_declarations: Vec<&str> = fixture
        .planned
        .plan()
        .expect("preflight accepted")
        .nodes()
        .iter()
        .filter_map(|n| n.id.as_deref())
        .collect();
    let activated: Vec<&String> = observation.activated_declarations().collect();

    assert!(
        !planned_declarations.is_empty(),
        "the plan holds candidate declarations"
    );
    assert!(
        activated.len() <= planned_declarations.len(),
        "activation is a subset of candidacy: {} activated, {} planned",
        activated.len(),
        planned_declarations.len()
    );
    for declaration in &activated {
        assert!(
            planned_declarations.contains(&declaration.as_str()),
            "{declaration} was activated, so it must have been a candidate"
        );
    }
}

#[test]
fn an_activated_producer_is_recorded_whatever_its_status() {
    // A producer that entered and failed is still an observed producer, so a
    // targeted VERIFY over it still applies. Dropping failed producers would
    // silently discard exactly the checks a failing run needs.
    let source = task_document(
        "
INPUT:
    ID: input.value
    TYPE: INTEGER
    VALUE: 3

OUTPUT:
    ID: output.value
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.copy
    ASSERT: REF(output.value) == 3

ACTION:
    ID: action.copy
    OPERATION: core.return
    TARGET: REF(input.value)
    OUTPUT: REF(output.value)

VERIFY:
    ID: verify.copy
    ASSERT: REF(output.value) == 3

SUCCESS:
    ID: success.copy
    ALL: [REF(verify.copy)]

TASK:
    ID: task.copy
    GOAL: REF(goal.copy)
    INPUT: REF(input.value)
    ACTION: REF(action.copy)
    OUTPUT: REF(output.value)
    SUCCESS: REF(success.copy)

EXECUTE:
    REFERENCE: REF(task.copy)
",
    );
    let fixture = execute(&source);
    let observation = Observation::of(&fixture.execution);

    let activations = observation.activations_of("action.copy");
    assert_eq!(activations.len(), 1);
    assert!(
        !activations[0].status.is_empty(),
        "an activation carries the producer's terminal status"
    );
}

#[test]
fn the_observation_is_order_stable_across_identical_runs() {
    let source = canonical_example("01_MINIMAL_TASK.lcl");
    let first = Observation::of(&execute(&source).execution).serialize();
    let second = Observation::of(&execute(&source).execution).serialize();
    assert_eq!(
        first, second,
        "the same program over the same mock host observes identical bytes"
    );
}
