//! Behavior sub-runs of the `error_contract` groups.
//!
//! The component runs beside these compare the registry's metadata with each
//! implementing component's mirror. A behavior run executes a concrete source
//! that must actually raise the identifier, so the contract is evidenced by
//! the engine's behavior and not only by its tables.

use crate::{attempt_field, ExecutedCase, Expectation, Runner};
use lcl_capabilities::{Determinism, Profile, Role, TargetClass};
use lcl_runtime::{CapabilityOutcome, MockHost};
use lcl_spec::SpecPackage;

/// A `kind.task` document with declarations and one subject ACTION.
fn document(declarations: &str, action: &str) -> String {
    crate::fixtures::task_document(&format!(
        "{declarations}\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    {action}\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n"
    ))
}

fn parameter(name: &str, ty: &str, required: &str, value: &str) -> String {
    format!("\n    PARAMETER:\n        NAME: {name}\n        TYPE: {ty}\n        REQUIRED: {required}\n        VALUE: {value}")
}

fn data(id: &str, ty: &str, value: &str) -> String {
    format!("\nDATA:\n    ID: {id}\n    TYPE: {ty}\n    VALUE: {value}\n")
}

/// One calculation of an expression fragment.
fn calculate(expression: &str) -> String {
    document(
        "",
        &format!(
            "OPERATION: core.calculate{}",
            parameter("expression", "STRING", "TRUE", expression)
        ),
    )
}

fn raises(runner: &Runner, label: &str, clause: &str, source: &str, error: &str) -> ExecutedCase {
    runner.execute(label, clause, source, Expectation::Diagnostic(error.into()))
}

fn raises_on(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    error: &str,
    mut host: MockHost,
) -> ExecutedCase {
    let mut case = runner.execute_on(
        label,
        clause,
        source,
        Expectation::Diagnostic(error.into()),
        &mut host,
    );
    case.observed.input_evidence.push(format!(
        "host: lcl-runtime MockHost; requests={:?}",
        host.requests()
    ));
    case
}

/// The shipped profiles with one row's profiles replaced by `profiles`.
fn catalog(spec: &SpecPackage, operation: &str, profiles: Vec<Profile>) -> Runner {
    Runner::with_profiles(
        spec,
        Runner::shipped_profiles()
            .into_iter()
            .filter(|p| p.operation_id != operation)
            .chain(profiles)
            .collect(),
    )
    .expect("the engine assembles with the scenario catalog")
}

fn verification_profile(implementation: &str, category: Determinism, complete: bool) -> Profile {
    let mut profile = Profile::builder(
        "core.verify",
        Role::new("verification"),
        implementation,
        "1",
    )
    .serving(TargetClass::Any)
    .determinism(
        category,
        "conformance fixture: the declared assertion fixes the result",
    )
    .axes(lcl_capabilities::profile::axes(&[], &[]))
    .resolving("evaluate the declared assertion over the resolved target");
    if !complete {
        profile.implementation_version.clear();
    }
    profile
}

/// Every behavior run of one `error_contract` group.
pub(super) fn behaviors(spec: &SpecPackage, runner: &Runner, error: &str) -> Vec<ExecutedCase> {
    let verify = |assertion: &str| {
        document(
            &data("data.subject", "INTEGER", "3"),
            &format!(
                "OPERATION: core.verify\n    TARGET: REF(data.subject){}",
                parameter("assertion", "BOOLEAN", "TRUE", assertion)
            ),
        )
    };
    let write = |target: &str, content: &str| {
        document(
            "",
            &format!(
                "OPERATION: core.write\n    TARGET: {target}{}",
                parameter("content", "STRING", "TRUE", content)
            ),
        )
    };
    let mut out = Vec::new();
    match error {
        "error.numeric.division_by_zero" => {
            out.push(raises(
                runner,
                "behavior/scalar-denominator",
                "a demanded division with a zero scalar denominator",
                &calculate("\"1 / 0\""),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/measure-denominator",
                "a demanded division with a zero MEASURE denominator",
                &calculate("\"MEASURE(1, unit.meter) / MEASURE(0, unit.meter)\""),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/round-quotient",
                "a demanded ROUND over a zero-denominator quotient",
                &calculate("\"ROUND(1 / 0, 2)\""),
                error,
            ));
        }
        "error.numeric.non_terminating" => {
            out.push(raises(
                runner,
                "behavior/quotient-outside-round",
                "an exact division with no finite base-10 result outside a direct ROUND context",
                &calculate("\"1 / 3\""),
                error,
            ));
        }
        "error.numeric.unit_mismatch" => {
            out.push(raises(
                runner,
                "behavior/exact-unit-mismatch-same-category",
                "MEASURE operands of the same category but different exact units",
                &calculate("\"MEASURE(1, unit.meter) + MEASURE(1, unit.kilometer)\""),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/unit-outside-required-category",
                "a unit outside the category the constructor requires",
                &calculate("\"DURATION(1, unit.meter) + DURATION(1, unit.second)\""),
                error,
            ));
        }
        "error.pattern.resource_limit" => {
            out.push(raises(
                runner,
                "behavior/pattern-resource-exhaustion",
                "matching a demanded pattern exhausts its declared finite resource limit",
                &calculate("\"\\\"a\\\" MATCHES REGEX(\\\"a{200000}\\\")\""),
                error,
            ));
        }
        "error.permission.denied" => {
            out.push(raises(
                runner,
                "behavior/prohibited-effect",
                "FORBID blocks the matching required ACTION even though an ACTION requires it",
                &write("PATH(\"/case/out.txt\")", "\"x\"").replacen(
                    "\nACTION:\n    ID: action.subject",
                    "\nFORBID:\n    ID: forbid.write\n    OPERATION: core.write\n    TARGET: PATH(\"/case/out.txt\")\n\nACTION:\n    ID: action.subject",
                    1,
                ),
                error,
            ));
            out.push(raises_on(
                runner,
                "behavior/unauthorized-access",
                "the host refuses the resolved request",
                &document(
                    "",
                    "OPERATION: core.read\n    TARGET: PATH(\"/case/data.txt\")",
                ),
                error,
                MockHost::new().deny("core.read", "conformance: the host grants no access"),
            ));
        }
        "error.determinism.mismatch" => {
            // 05_SEMANTICS/11: "Validation emits error.determinism.mismatch
            // exactly when DETERMINISTIC TRUE is declared and that resolved
            // contract is nondeterministic", and "DETERMINISTIC FALSE ... never
            // triggers this error". core.validate is the row that checks a
            // referenced contract, and records detected failures under their
            // registered identifiers rather than raising them.
            let definition = |asserted: &str| {
                format!(
                    "\nDEFINE:\n    ID: op.inferred\n    KIND: kind.operation\n    MEANING: \"Infer a summary.\"\n    SIDE_EFFECT: FALSE\n    DETERMINISTIC: {asserted}\n    DEPENDENCY: [model]\n    PARAMETER:\n        NAME: subject\n        TYPE: STRING\n        REQUIRED: TRUE\n    RESULT:\n        TYPE: STRING\n"
                )
            };
            let validating = |asserted: &str| {
                document(
                    &definition(asserted),
                    "OPERATION: core.validate\n    TARGET: REF(op.inferred)",
                )
            };
            out.push(runner.execute(
                "behavior/deterministic-true-resolves-nondeterministic",
                "a kind.operation asserting DETERMINISTIC TRUE whose declared dependency admits permitted variation is a determinism mismatch",
                &validating("TRUE"),
                Expectation::All(vec![
                    attempt_field("valid", "FALSE"),
                    attempt_field("errors", "[error.determinism.mismatch]"),
                ]),
            ));
            out.push(runner.execute(
                "behavior/deterministic-false-never-mismatches",
                "the same declared contract under DETERMINISTIC FALSE never triggers the error",
                &validating("FALSE"),
                Expectation::All(vec![
                    attempt_field("valid", "TRUE"),
                    attempt_field("errors", "[]"),
                    Expectation::NoDiagnostic(error.into()),
                ]),
            ));
        }
        "error.reference.cycle" => {
            let unit = |body: &str, root: &str| {
                crate::fixtures::task_document(&format!(
                    "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n{body}\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    {root}\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n"
                ))
            };
            out.push(raises(
                runner,
                "behavior/sequence-cycle",
                "a SEQUENCE whose STEP references the enclosing SEQUENCE",
                &unit("\nSEQUENCE:\n    ID: sequence.a\n    STEP:\n        ID: step.one\n        SEQUENCE: REF(sequence.a)\n", "SEQUENCE: REF(sequence.a)"),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/phase-cycle",
                "a PHASE reachable from its own SEQUENCE",
                &unit(
                    "\nPHASE:\n    ID: phase.a\n    SEQUENCE: REF(sequence.a)\n\nSEQUENCE:\n    ID: sequence.a\n    STEP:\n        ID: step.one\n        PHASE: REF(phase.a)\n",
                    "PHASE: REF(phase.a)",
                ),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/task-cycle",
                "a TASK reachable from a STEP of its own SEQUENCE",
                &unit("\nSEQUENCE:\n    ID: sequence.a\n    STEP:\n        ID: step.one\n        TASK: REF(task.case)\n", "SEQUENCE: REF(sequence.a)"),
                error,
            ));
            // The graph-target half of the identifier.
            // `05_SEMANTICS/11`: "For a referenced TASK, PHASE, SEQUENCE,
            // ACTION, or TEST, a prohibited reference cycle emits
            // error.reference.cycle and fails before axis resolution."
            let delegating = |operation: &str, extra: &str| {
                unit(
                    &format!(
                        "\nDATA:\n    ID: data.subject\n    TYPE: INTEGER\n    VALUE: 3\n\nACTION:\n    ID: action.one\n    OPERATION: {operation}\n    TARGET: REF(action.two){extra}\n\nACTION:\n    ID: action.two\n    OPERATION: {operation}\n    TARGET: REF(action.one){extra}\n"
                    ),
                    "ACTION: [REF(action.one), REF(action.two)]",
                )
            };
            out.push(raises(
                runner,
                "behavior/action-cycle",
                "two actions whose core.execute graph targets name each other",
                &delegating("core.execute", ""),
                error,
            ));
            let comparison = concat!(
                "\n    PARAMETER:\n        NAME: expected\n        TYPE: INTEGER\n        REQUIRED: TRUE\n        VALUE: 3",
                "\n    PARAMETER:\n        NAME: actual\n        TYPE: INTEGER\n        REQUIRED: TRUE\n        VALUE: REF(data.subject)",
            );
            out.push(raises(
                runner,
                "behavior/test-cycle",
                "two core.test actions whose graph targets name each other",
                &delegating("core.test", comparison),
                error,
            ));
            out.push(runner.execute(
                "behavior/before-graph-axis-resolution",
                "the cycle fails before axis resolution: no attempt is made and no request crosses",
                &delegating("core.execute", ""),
                Expectation::All(vec![
                    Expectation::Rejects(error.into()),
                    Expectation::Attempts {
                        declaration: "action.one".into(),
                        statuses: Vec::new(),
                    },
                    Expectation::Attempts {
                        declaration: "action.two".into(),
                        statuses: Vec::new(),
                    },
                ]),
            ));
        }
        "error.type.mismatch" => {
            out.push(raises(
                runner,
                "behavior/declared-type-incompatible",
                "a declared value incompatible with its declared type",
                &document(
                    &data("data.subject", "INTEGER", "\"three\""),
                    "OPERATION: core.return\n    TARGET: REF(data.subject)",
                ),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/set-for-each-without-total-order",
                "a direct FOR EACH over a SET whose actual members fail the registered pairwise order check",
                &crate::fixtures::task_document(
                    "\nDEFINE:\n    ID: type.tagged\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: tag\n        TYPE: STRING\n        REQUIRED: TRUE\n\nDATA:\n    ID: data.one\n    TYPE: OBJECT[REF(type.tagged)]\n    VALUE:\n        tag: \"a\"\n\nDATA:\n    ID: data.two\n    TYPE: OBJECT[REF(type.tagged)]\n    VALUE:\n        tag: \"b\"\n\nDATA:\n    ID: data.members\n    TYPE: SET[OBJECT[REF(type.tagged)]]\n    VALUE: [REF(data.one), REF(data.two)]\n\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nSEQUENCE:\n    ID: sequence.case\n    FOR EACH item IN REF(data.members):\n        STEP:\n            ID: step.one\n            ACTION:\n                ID: action.subject\n                OPERATION: core.return\n                TARGET: REF(item)\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    SEQUENCE: REF(sequence.case)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
                ),
                error,
            ));
        }
        "error.operation.parameter" => {
            let read = "OPERATION: core.read\n    TARGET: PATH(\"/case/data.txt\")";
            out.push(raises(
                runner,
                "behavior/action-target-omitted",
                "an ACTION omitting a TARGET the selected operation marks required",
                &document("", "OPERATION: core.read"),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/required-named-parameter-omitted",
                "an invocation omitting a required named parameter",
                &document(
                    "",
                    "OPERATION: core.write\n    TARGET: PATH(\"/case/out.txt\")",
                ),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/unregistered-named-parameter",
                "an invocation supplying an unregistered named parameter",
                &document(
                    "",
                    &format!(
                        "{read}{}",
                        parameter("shell", "STRING", "FALSE", "\"bash\"")
                    ),
                ),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/duplicate-named-parameter",
                "an invocation duplicating a named parameter",
                &document(
                    "",
                    &format!(
                        "{read}{}{}",
                        parameter("format", "STRING", "FALSE", "\"format.plain_text\""),
                        parameter("format", "STRING", "FALSE", "\"format.plain_text\"")
                    ),
                ),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/positional-argument",
                "no positional parameters exist: NAME is required by the higher-authority field registry, so an unnamed argument stops at its earliest registered stage",
                &document("", &format!("{read}\n    PARAMETER:\n        TYPE: STRING\n        REQUIRED: FALSE\n        VALUE: \"x\"")),
                "error.field.required",
            ));
            out.push(raises(
                runner,
                "behavior/handler-target-omitted",
                "a HANDLER omitting a required TARGET no handler-context binding supplies",
                &crate::fixtures::task_document(
                    "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nHANDLER:\n    ID: handler.case\n    EVENT: event.host_constraint\n    OPERATION: core.write\n    PARAMETER:\n        NAME: content\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"x\"\n\nACTION:\n    ID: action.subject\n    OPERATION: core.read\n    TARGET: PATH(\"/case/data.txt\")\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    HANDLER: REF(handler.case)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
                ),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/fallback-target-omitted",
                "a FALLBACK operation identifier whose required TARGET no handler-context binding supplies",
                &crate::fixtures::task_document(
                    "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nHANDLER:\n    ID: handler.case\n    EVENT: event.host_constraint\n    OPERATION: core.stop\n    FALLBACK: core.delete\n\nACTION:\n    ID: action.subject\n    OPERATION: core.read\n    TARGET: PATH(\"/case/data.txt\")\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    HANDLER: REF(handler.case)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
                ),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/fallback-operation-requires-named-parameter",
                "a FALLBACK operation identifier registering a required named parameter",
                &crate::fixtures::task_document(
                    "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nHANDLER:\n    ID: handler.case\n    EVENT: event.host_constraint\n    OPERATION: core.stop\n    FALLBACK: core.move\n\nACTION:\n    ID: action.subject\n    OPERATION: core.read\n    TARGET: PATH(\"/case/data.txt\")\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    HANDLER: REF(handler.case)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
                ),
                error,
            ));
            let sort = |name: &str, ty: &str, value: &str| {
                document(
                    &data("data.members", "LIST[INTEGER]", "[3, 1, 2]"),
                    &format!(
                        "OPERATION: core.sort\n    TARGET: REF(data.members){}",
                        parameter(name, ty, "FALSE", value)
                    ),
                )
            };
            out.push(raises(
                runner,
                "behavior/sort-comparator-forbidden",
                "core.sort registers no comparator parameter",
                &sort("comparator", "STRING", "\"numeric\""),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/sort-stable-forbidden",
                "core.sort registers no stable parameter",
                &sort("stable", "BOOLEAN", "TRUE"),
                error,
            ));
            out.push(raises(
                runner,
                "behavior/constraint-rejection-keeps-its-own-error",
                "a declared parameter value rejected by its own bound keeps that error and is not remapped",
                &document("", &format!("OPERATION: core.inspect\n    TARGET: PATH(\"/case/data.txt\"){}", parameter("depth", "INTEGER", "FALSE", "101"))),
                "error.value.out_of_range",
            ));
        }
        "error.operation.precondition" => {
            let assertion = "REF(data.subject) == 3";
            out.push(raises_on(
                &catalog(spec, "core.verify", Vec::new()),
                "behavior/profile-missing",
                "a required profile role that resolves no profile",
                &verify(assertion),
                error,
                MockHost::new(),
            ));
            out.push(raises_on(
                &catalog(
                    spec,
                    "core.verify",
                    vec![
                        verification_profile("one", Determinism::Deterministic, true),
                        verification_profile("two", Determinism::Deterministic, true),
                    ],
                ),
                "behavior/profile-ambiguous",
                "a required profile role that resolves more than one profile",
                &verify(assertion),
                error,
                MockHost::new(),
            ));
            out.push(raises_on(
                &catalog(
                    spec,
                    "core.verify",
                    vec![verification_profile(
                        "one",
                        Determinism::Deterministic,
                        false,
                    )],
                ),
                "behavior/profile-incomplete",
                "a required profile missing one of the registered profile properties",
                &verify(assertion),
                error,
                MockHost::new(),
            ));
            out.push(raises_on(
                &catalog(
                    spec,
                    "core.verify",
                    vec![
                        Profile::builder("core.verify", Role::new("verification"), "wide", "1")
                            .serving(TargetClass::Any)
                            .determinism(Determinism::Deterministic, "conformance fixture")
                            .axes(lcl_capabilities::profile::axes(
                                &[],
                                &[lcl_capabilities::Effect::Filesystem],
                            ))
                            .resolving("a profile claiming an effect its read_only row forbids"),
                    ],
                ),
                "behavior/profile-out-of-bounds",
                "a profile claiming a class outside its row's maximum",
                &verify(assertion),
                error,
                MockHost::new(),
            ));
            out.push(raises_on(
                &catalog(
                    spec,
                    "core.write",
                    vec![
                        Profile::builder("core.write", Role::new("write"), "varying", "1")
                            .serving(TargetClass::Any)
                            .determinism(
                                Determinism::Nondeterministic,
                                "conformance fixture: the implementation varies",
                            )
                            .axes(lcl_capabilities::profile::axes(
                                &[lcl_capabilities::Dependency::Host],
                                &[lcl_capabilities::Effect::Filesystem],
                            ))
                            .resolving("a nondeterministic profile under a deterministic base row"),
                    ],
                ),
                "behavior/determinism-incompatible-profile",
                "a deterministic base row accepts only a deterministic profile",
                &write("PATH(\"/case/out.txt\")", "\"x\""),
                error,
                MockHost::new(),
            ));
            out.push(raises_on(
                runner,
                "behavior/false-precondition",
                "a registered row precondition that is false",
                &write("PATH(\"/case/absent.txt\")", "\"x\""),
                error,
                MockHost::new().script(
                    "core.write",
                    vec![CapabilityOutcome::Refused {
                        observation: lcl_runtime::capability::Observation::none(),
                        error: lcl_runtime::RuntimeError::OperationPrecondition,
                        cause: "target".into(),
                        detail: "conformance: the target does not exist".into(),
                    }],
                ),
            ));
            out.push(raises(
                runner,
                "behavior/missing-precondition",
                "a registered row precondition whose required input is absent",
                &document(
                    "",
                    "OPERATION: core.create\n    TARGET: PATH(\"/case/new.txt\")",
                ),
                error,
            ));
            out.push(raises_on(
                runner,
                "behavior/unknown-precondition",
                "a registered row precondition the host cannot establish",
                &write("PATH(\"/case/out.txt\")", "\"x\""),
                error,
                MockHost::new().script(
                    "core.write",
                    vec![CapabilityOutcome::Refused {
                        observation: lcl_runtime::capability::Observation::none(),
                        error: lcl_runtime::RuntimeError::OperationPrecondition,
                        cause: "target".into(),
                        detail: "conformance: the target state cannot be established".into(),
                    }],
                ),
            ));
            let custom = |side_effect: &str, target: &str| {
                document(
                    &format!(
                        "{}\nDEFINE:\n    ID: custom.publish\n    KIND: kind.operation\n    MEANING: \"Publish the declared value.\"\n    SIDE_EFFECT: {side_effect}\n    DETERMINISTIC: TRUE\n    PARAMETER:\n        NAME: value\n        TYPE: STRING\n        REQUIRED: TRUE\n    RESULT:\n        TYPE: BOOLEAN\n",
                        data("data.subject", "STRING", "\"x\"")
                    ),
                    &format!("OPERATION: custom.publish\n    TARGET: {target}{}", parameter("value", "STRING", "TRUE", "\"x\"")),
                )
            };
            out.push(raises_on(runner, "behavior/custom-operation-effect-outside-maximum", "a custom kind.operation invocation resolving an effect class outside its declared maximum", &custom("[state]", "PATH(\"/case/out.txt\")"), error, MockHost::new()));
            out.push(raises_on(
                runner,
                "behavior/custom-operation-no-concrete-effect",
                "a custom kind.operation declaring effects whose invocation resolves none",
                &custom("[state]", "REF(data.subject)"),
                error,
                MockHost::new(),
            ));
        }
        _ => {}
    }
    let _ = attempt_field;
    out
}
