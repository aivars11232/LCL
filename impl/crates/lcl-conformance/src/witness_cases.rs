//! The 66 canonical decision witnesses, as concrete executable sources.
//!
//! `CASES/language_decision_cases_v0.1.0.json` states each witness in prose and
//! declares itself `executed: false`. This module is the other half: for each
//! witness, the exact LCL source that exhibits it and the exact engine result
//! its `expected` sentence names.
//!
//! ## Three populations, and why the third is not a loophole
//!
//! * [`Plan::Executable`] — the source runs and the observation must match.
//! * [`Plan::NotImplemented`] — the source runs, the engine's answer is wrong,
//!   and the exact missing behavior and the milestone that owns it are recorded
//!   here in the file. The suite asserts that the set of failing witnesses is
//!   *exactly* this set, so a gap can neither be quietly added nor quietly
//!   fixed: closing one fails the suite until this list is updated.
//! * [`Plan::Descriptive`] — the witness cannot be expressed as a concrete
//!   source without inventing test data the canonical text does not supply. The
//!   reason is recorded and the witness is never counted as executed.
//!
//! Nothing may move between these populations silently. That is the whole
//! design: `09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt` says "Unsupported
//! core behavior is failure, not silent inference", and an unlisted gap or an
//! unlisted pass is exactly that inference.

#![allow(dead_code)]

use crate::report::Coverage;
use crate::Expectation;

/// What this build does with one witness.
pub enum Plan {
    /// Run these probes; all must match.
    Executable(Vec<Probe>),
    /// Run these probes; they currently do not match. The gap is named.
    NotImplemented {
        probes: Vec<Probe>,
        /// The behavior that is absent.
        missing: &'static str,
        /// The milestone or component that owns it.
        owner: &'static str,
    },
    /// Not expressible as a concrete source from the witness text alone.
    Descriptive { reason: &'static str },
}

/// One concrete source and the result the witness's `expected` sentence names.
pub struct Probe {
    /// Suffix distinguishing probes of one witness, e.g. `"a"`. Empty when the
    /// witness has exactly one.
    pub label: &'static str,
    pub source: String,
    pub expectation: Expectation,
    /// Whether this probe needs the in-memory filesystem fixture.
    pub needs_filesystem: bool,
    pub imports: Vec<(String, String)>,
    pub invocation: lcl_semantics::Invocation,
    /// Exact initial failure count for the bounded read fixture, when used.
    pub read_failures: Option<usize>,
    pub command_retry: Option<CommandRetry>,
}

impl Probe {
    pub fn new(source: String, expectation: Expectation) -> Probe {
        Probe {
            label: "",
            source,
            expectation,
            needs_filesystem: false,
            imports: Vec::new(),
            invocation: lcl_semantics::Invocation::new(),
            read_failures: None,
            command_retry: None,
        }
    }

    pub fn labelled(label: &'static str, source: String, expectation: Expectation) -> Probe {
        let mut probe = Probe::new(source, expectation);
        probe.label = label;
        probe
    }

    pub fn on_filesystem(mut self) -> Probe {
        self.needs_filesystem = true;
        self
    }

    pub fn with_read_failures(mut self, count: usize) -> Probe {
        self.read_failures = Some(count);
        self
    }

    pub fn with_import(mut self, path: &str, source: String) -> Probe {
        self.imports.push((path.to_string(), source));
        self
    }

    pub fn execute(
        &self,
        runner: &crate::Runner,
        witness: &str,
        contract: &str,
    ) -> crate::ExecutedCase {
        let id = if self.label.is_empty() {
            witness.to_string()
        } else {
            format!("{witness}/{}", self.label)
        };
        let mut provider = lcl_resolver::MemoryProvider::new();
        for (path, source) in &self.imports {
            provider.insert(path, source.as_bytes());
        }
        let mut host: Box<dyn lcl_runtime::Host> = if let Some(failures) = self.read_failures {
            Box::new(ScheduledRead { failures, calls: 0 })
        } else if let Some(mode) = self.command_retry {
            Box::new(PartialCommand { mode, calls: 0 })
        } else if self.needs_filesystem {
            let fs = lcl_stdlib::MemoryFileSystem::new()
                .with_read_scope("/case")
                .with_scope("/case")
                .with_file("/case/a.txt", *b"abcd")
                .with_file("/case/lines.txt", *b"a\nb");
            let grants = fs.grants().clone();
            Box::new(lcl_stdlib::HostAdapter::new(grants).with_filesystem(fs))
        } else {
            Box::new(lcl_runtime::MockHost::new())
        };
        let observed = runner.run_input(
            &lcl_resolver::SourceUnit::new(
                lcl_resolver::SourceId::new("case.lcl"),
                self.source.as_bytes(),
            ),
            &provider,
            &self.invocation,
            &mut *host,
        );
        let verdict = crate::judge(&self.expectation, &observed);
        crate::ExecutedCase {
            id,
            contract: contract.to_string(),
            source: self.source.clone(),
            expectation: self.expectation.clone(),
            observed,
            verdict,
        }
    }
}

/// Test data: a single immutable read target and a finite failure schedule.
/// No language rule or standard-library operation is replaced by this host.
struct ScheduledRead {
    failures: usize,
    calls: usize,
}

impl lcl_runtime::Host for ScheduledRead {
    fn permits(&mut self, request: &lcl_runtime::CapabilityRequest) -> lcl_runtime::Permission {
        if request.operation == "core.read"
            && matches!(&request.target,
            Some(lcl_runtime::Value::Constructed { constructor, text }) if constructor == "PATH" && text == "/case/retry.txt")
        {
            lcl_runtime::Permission::Granted
        } else {
            lcl_runtime::Permission::Denied("outside the read fixture's exact target".into())
        }
    }

    fn invoke(
        &mut self,
        request: &lcl_runtime::CapabilityRequest,
    ) -> lcl_runtime::CapabilityOutcome {
        // The schedule cannot silently supply a fourth attempt or an unrelated
        // capability. The recorded request attempt must agree with call order.
        if self.calls >= 3 || request.invocation.attempt != self.calls {
            return lcl_runtime::CapabilityOutcome::Denied("fixture attempt bound exceeded".into());
        }
        let failed = self.calls < self.failures;
        self.calls += 1;
        if failed {
            lcl_runtime::CapabilityOutcome::Unavailable(format!(
                "scripted read limitation {}",
                self.calls
            ))
        } else {
            lcl_runtime::CapabilityOutcome::Completed(
                lcl_runtime::Observation::none()
                    .with("value", lcl_runtime::Value::Text("complete".into()))
                    .with("evidence", lcl_runtime::Value::List(Vec::new())),
            )
        }
    }
}

/// One witness and how this build treats it.
pub struct WitnessCase {
    pub id: &'static str,
    pub coverage: Coverage,
    pub plan: Plan,
}

// ---------------------------------------------------------------------------
// Document builders
// ---------------------------------------------------------------------------

const HEADER: &str = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.case\n    \
                      NAME: \"Conformance case\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n";

const DATA_HEADER: &str =
    "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.case\n    \
                           NAME: \"Conformance case\"\n    VERSION: \"1.0.0\"\n    \
                           KIND: kind.data\n";

/// A `kind.data` document holding the supplied declarations.
pub fn data_doc(declarations: &str) -> String {
    format!("{DATA_HEADER}{declarations}")
}

/// A runnable task whose `VERIFY` asserts one expression.
///
/// The expression under test becomes a post-execution check, so a case
/// exercises parse, resolve, check, preflight, execute and complete, and the
/// recorded check outcome is the observation.
pub fn assertion(declarations: &str, assertion: &str) -> String {
    format!(
        "{HEADER}{declarations}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.seed
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: REF(output.seed) == 1

ACTION:
    ID: action.seed
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.seed)

VERIFY:
    ID: verify.case
    ASSERT: {assertion}

SUCCESS:
    ID: success.case
    ALL: [REF(verify.case)]

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.seed)
    ACTION: REF(action.seed)
    OUTPUT: REF(output.seed)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// The expectation that an assertion held.
pub fn holds() -> Expectation {
    Expectation::Check {
        id: "verify.case".to_string(),
        outcome: "TRUE".to_string(),
    }
}

/// A runnable task built from arbitrary declarations plus a graph root.
pub fn task(declarations: &str, root: &str, execute: &str) -> String {
    format!("{HEADER}{declarations}\n{root}\nEXECUTE:\n    REFERENCE: REF({execute})\n")
}

/// One `ACTION` invoking an operation on a target, with named parameters.
pub fn action(id: &str, operation: &str, target: &str, parameters: &str, output: &str) -> String {
    let mut out = format!("\nACTION:\n    ID: {id}\n    OPERATION: {operation}\n");
    if !target.is_empty() {
        out.push_str(&format!("    TARGET: {target}\n"));
    }
    out.push_str(parameters);
    if !output.is_empty() {
        out.push_str(&format!("    OUTPUT: {output}\n"));
    }
    out
}

/// A `TASK` root with one `ACTION`, a `GOAL` and a `SUCCESS`.
pub fn single_action_task(
    declarations: &str,
    action_block: &str,
    action_id: &str,
    output_id: &str,
) -> String {
    let output_field = if output_id.is_empty() {
        String::new()
    } else {
        format!("    OUTPUT: REF({output_id})\n")
    };
    let goal = if output_id.is_empty() {
        "    ASSERT: TRUE\n".to_string()
    } else {
        format!("    ASSERT: EXISTS(REF({output_id}))\n")
    };
    format!(
        "{HEADER}{declarations}{action_block}
GOAL:
    ID: goal.case
{goal}
SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    ACTION: REF({action_id})
{output_field}    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// A `STRING`-typed `OUTPUT` declaration.
pub fn string_output(id: &str) -> String {
    format!("\nOUTPUT:\n    ID: {id}\n    TYPE: STRING\n    FORMAT: format.plain_text\n")
}

// ---------------------------------------------------------------------------
// The catalog
// ---------------------------------------------------------------------------

/// Every witness in `language_decision_cases_v0.1.0.json`, in catalog order.
pub fn cases() -> Vec<WitnessCase> {
    vec![
        // -- source_type ---------------------------------------------------
        WitnessCase {
            id: "CLOSURE-001",
            coverage: Coverage::StaticOrType,
            plan: Plan::Executable(vec![
                Probe::labelled(
                    "a",
                    assertion(
                        "
DEFINE:
    ID: type.counter
    KIND: kind.type
    BASE: INTEGER
\nDATA:
    ID: data.count
    TYPE: REF(type.counter)
    VALUE: 5
",
                        "REF(data.count) == 5",
                    ),
                    holds(),
                ),
                // "bare type.counter is not a type."
                Probe::labelled(
                    "b",
                    data_doc(
                        "
DEFINE:
    ID: type.counter
    KIND: kind.type
    BASE: INTEGER

DATA:
    ID: data.count
    TYPE: type.counter
    VALUE: 5
",
                    ),
                    Expectation::Rejects("error.field.type".to_string()),
                ),
            ]),
        },
        WitnessCase {
            id: "CLOSURE-002",
            coverage: Coverage::StaticOrType,
            plan: Plan::Executable(vec![Probe::new(
                data_doc("\nDATA:\n    ID: data.nothing\n    TYPE: NULL\n    VALUE: NULL\n"),
                Expectation::Accepts,
            )]),
        },
        WitnessCase {
            id: "CLOSURE-003",
            coverage: Coverage::StaticOrType,
            plan: Plan::Executable(vec![
                Probe::labelled(
                    "a",
                    assertion(
                        "
DEFINE:
    ID: type.state
    KIND: kind.type
    BASE: ENUM
    ITEM: ready
    ITEM: done

DEFINE:
    ID: type.first
    KIND: kind.type
    BASE: REF(type.state)

DEFINE:
    ID: type.second
    KIND: kind.type
    BASE: REF(type.first)

DATA:
    ID: data.one
    TYPE: REF(type.first)
    VALUE: ready

DATA:
    ID: data.two
    TYPE: REF(type.second)
    VALUE: ready
",
                        "REF(data.one) == REF(data.two)",
                    ),
                    holds(),
                ),
                // "an unrelated ready domain is incompatible."
                Probe::labelled(
                    "b",
                    data_doc(
                        "
DEFINE:
    ID: type.state
    KIND: kind.type
    BASE: ENUM
    ITEM: ready
    ITEM: done

DEFINE:
    ID: type.other
    KIND: kind.type
    BASE: ENUM
    ITEM: ready

DATA:
    ID: data.one
    TYPE: REF(type.state)
    VALUE: ready

DATA:
    ID: data.two
    TYPE: REF(type.other)
    VALUE: REF(data.one)
",
                    ),
                    Expectation::Rejects("error.type.mismatch".to_string()),
                ),
            ]),
        },
        // -- reference -----------------------------------------------------
        WitnessCase {
            id: "CLOSURE-004",
            coverage: Coverage::StaticOrType,
            plan: Plan::Executable(vec![Probe::new(
                assertion(
                    "\nOUTPUT:\n    ID: output.copy\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n    TARGET: PATH(\"/case/copy.txt\")\n",
                    "REF(output.copy).TARGET == PATH(\"/case/copy.txt\") AND REF(output.copy) == MISSING",
                ),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-005",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion(
                    "
DEFINE:
    ID: type.record
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: name
        TYPE: STRING
        REQUIRED: TRUE

DATA:
    ID: data.record
    TYPE: OBJECT[REF(type.record)]
    VALUE:
        name: \"a\"
",
                    "REF(data.record).name == \"a\"",
                ),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-006",
            coverage: Coverage::StaticOrType,
            plan: Plan::Executable(vec![Probe::new(
                assertion(
                    "\nDEFINE:\n    ID: constant.left\n    KIND: kind.constant\n    TYPE: INTEGER\n    VALUE: 2\n\nDEFINE:\n    ID: constant.right\n    KIND: kind.constant\n    TYPE: INTEGER\n    VALUE: 2\n\nDEFINE:\n    ID: constant.first\n    KIND: kind.constant\n    TYPE: REFERENCE[REF(constant.left)]\n    VALUE: REF(constant.left)\n\nDEFINE:\n    ID: constant.same\n    KIND: kind.constant\n    TYPE: REFERENCE[REF(constant.left)]\n    VALUE: REF(constant.left)\n\nDEFINE:\n    ID: constant.other\n    KIND: kind.constant\n    TYPE: REFERENCE[REF(constant.right)]\n    VALUE: REF(constant.right)\n",
                    "REF(constant.first) == REF(constant.same) AND REF(constant.first) != REF(constant.other) AND REF(constant.left) == REF(constant.right)",
                ),
                holds(),
            )]),
        },
        // -- index ---------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-007",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![
                Probe::labelled("a", assertion("", "[10, 20][0] == 10"), holds()),
                Probe::labelled("b", assertion("", "[10, 20][1] == 20"), holds()),
                Probe::labelled("c", assertion("", "[10, 20][2] == MISSING"), holds()),
                Probe::labelled("d", assertion("", "[10, 20][-1] == MISSING"), holds()),
            ]),
        },
        // -- call ----------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-008",
            coverage: Coverage::Grammar,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "COUNT(value: \"abc\") == 3"),
                Expectation::Rejects("error.grammar.invalid".to_string()),
            )]),
        },
        // -- keyword_case --------------------------------------------------
        WitnessCase {
            id: "CLOSURE-009",
            coverage: Coverage::StaticOrType,
            plan: Plan::Executable(vec![Probe::new(
                data_doc("\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: true\n"),
                Expectation::Rejects("error.type.mismatch".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-010",
            coverage: Coverage::Lexical,
            plan: Plan::Executable(vec![Probe::new(
                "LCL:\n    version: \"0.1.0\"\n".to_string(),
                Expectation::Rejects("error.keyword.case".to_string()),
            )]),
        },
        // -- evaluation ----------------------------------------------------
        WitnessCase {
            id: "CLOSURE-011",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion(
                    "\nDATA:\n    ID: data.zero\n    TYPE: INTEGER\n    VALUE: 0\n",
                    "(FALSE AND (1 / REF(data.zero) == 0)) == FALSE",
                ),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-012",
            coverage: Coverage::StaticOrType,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "TRUE OR (1 + \"x\" == 0)"),
                Expectation::Rejects("error.operator.operand".to_string()),
            )]),
        },
        // -- quantifier ----------------------------------------------------
        WitnessCase {
            id: "CLOSURE-013",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![
                Probe::labelled("a", assertion("", "ALL([]) == TRUE"), holds()),
                Probe::labelled("b", assertion("", "ANY([]) == FALSE"), holds()),
                Probe::labelled("c", assertion("", "NONE([]) == TRUE"), holds()),
            ]),
        },
        WitnessCase {
            id: "CLOSURE-014",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "ALL([TRUE, UNKNOWN]) == UNKNOWN"),
                holds(),
            )]),
        },
        // -- reduction / collection ----------------------------------------
        WitnessCase {
            id: "CLOSURE-015",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion(
                    "\nDATA:\n    ID: data.empty\n    TYPE: LIST[INTEGER]\n    VALUE: []\n",
                    "SUM(REF(data.empty)) == 0",
                ),
                Expectation::Rejects("error.operator.operand".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-016",
            coverage: Coverage::StaticOrType,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "SUM([]) == 0"),
                Expectation::Rejects("error.type.mismatch".to_string()),
            )]),
        },
        // -- count ---------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-017",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![
                Probe::labelled("a", assertion("", "COUNT(BYTES(8)) == 8"), holds()),
                Probe::labelled("b", assertion("", "EMPTY(BYTES(0)) == TRUE"), holds()),
                Probe::labelled("c", assertion("", "COUNT(\"A\u{1F600}\") == 2"), holds()),
            ]),
        },
        // -- fragment ------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-018",
            coverage: Coverage::StandardLibrary,
            plan: Plan::Executable(vec![Probe::new(
                calculate("\"REF(target) + REF(data.increment)\""),
                Expectation::Terminal("status.succeeded".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-019",
            coverage: Coverage::StandardLibrary,
            plan: Plan::Executable(vec![Probe::new(
                calculate("\"1; 2\""),
                Expectation::Rejects("error.operation.parameter".to_string()),
            )]),
        },
        // -- compare / group -----------------------------------------------
        WitnessCase {
            id: "CLOSURE-020",
            coverage: Coverage::StandardLibrary,
            plan: Plan::Executable(vec![Probe::new(
                compare_case(),
                Expectation::Terminal("status.succeeded".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-021",
            coverage: Coverage::StandardLibrary,
            plan: Plan::Executable(vec![Probe::new(group_objects(), holds())]),
        },
        // -- retry ---------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-022",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                retry_read(None),
                Expectation::All(vec![
                    Expectation::Accepts,
                    Expectation::Terminal("status.succeeded".into()),
                    Expectation::Attempts { declaration: "action.read".into(), statuses: vec!["status.blocked".into(), "status.succeeded".into()] },
                    Expectation::NoDiagnostic("error.retry.exhausted".into()),
                    Expectation::Output { id: "output.payload".into(), value: "\"complete\"".into() },
                ]),
            ).with_read_failures(1)]),
        },
        WitnessCase {
            id: "CLOSURE-023",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                retry_read(Some("FALSE")),
                Expectation::All(vec![
                    Expectation::Rejects("error.host.constraint".into()),
                    Expectation::Terminal("status.blocked".into()),
                    Expectation::Attempts { declaration: "action.read".into(), statuses: vec!["status.blocked".into()] },
                    Expectation::NoDiagnostic("error.retry.exhausted".into()),
                    Expectation::Output { id: "output.payload".into(), value: "UNBOUND".into() },
                ]),
            ).with_read_failures(1)]),
        },
        WitnessCase {
            id: "CLOSURE-024",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                retry_read(None),
                Expectation::All(vec![
                    Expectation::Attempts { declaration: "action.read".into(), statuses: vec!["status.blocked".into(); 3] },
                    Expectation::Diagnostic("error.retry.exhausted".into()),
                    Expectation::Output { id: "output.payload".into(), value: "UNBOUND".into() },
                ]),
            ).with_read_failures(3)]),
        },
        WitnessCase {
            id: "CLOSURE-025",
            coverage: Coverage::Grammar,
            plan: Plan::Executable(vec![Probe::new(
                step_with_inline_retry(),
                Expectation::Rejects("error.block.field".to_string()),
            )]),
        },
        // -- alias ---------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-026",
            coverage: Coverage::StaticOrType,
            plan: Plan::Executable(vec![Probe::new(
                data_doc(
                    "
DEFINE:
    ID: fault.absent
    KIND: kind.error
    BASE: error.required.missing
    MEANING: \"Missing required datum\"
",
                ),
                Expectation::Accepts,
            )]),
        },
        // -- continue / fallback -------------------------------------------
        WitnessCase {
            id: "CLOSURE-027",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![
                Probe::labelled("advance", continue_read(true), Expectation::All(vec![
                    Expectation::Accepts,
                    Expectation::Terminal("status.succeeded".into()),
                    Expectation::Recovered("action.read".into()),
                    Expectation::Diagnostic("error.host.constraint".into()),
                    Expectation::Attempts { declaration: "action.read".into(), statuses: vec!["status.blocked".into()] },
                    Expectation::Attempts { declaration: "action.next".into(), statuses: vec!["status.succeeded".into()] },
                    Expectation::Output { id: "output.payload".into(), value: "9".into() },
                ])).with_read_failures(1),
                Probe::labelled("no-successor", continue_read(false), Expectation::All(vec![
                    Expectation::Rejects("error.execution.order".into()),
                    Expectation::Diagnostic("error.host.constraint".into()),
                    Expectation::Attempts { declaration: "action.next".into(), statuses: vec![] },
                ])).with_read_failures(1),
            ]),
        },
        WitnessCase {
            id: "CLOSURE-028",
            coverage: Coverage::Resolution,
            plan: Plan::Executable(vec![Probe::new(
                handler_fallback_cycle(),
                Expectation::Rejects("error.reference.cycle".to_string()),
            )]),
        },
        // -- status --------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-029",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                skipped_action(),
                Expectation::Terminal("status.succeeded".to_string()),
            )]),
        },
        // -- pattern -------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-030",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "\"ABC\" MATCHES REGEX(\"[a-z]+\", \"i\")"),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-031",
            coverage: Coverage::Lexical,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "\"a\" MATCHES REGEX(\"(?=a)\")"),
                Expectation::Rejects("error.literal.invalid".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-032",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "\"src/test1.lcl\" MATCHES GLOB(\"src/**/test?.lcl\")"),
                holds(),
            )]),
        },
        // -- literal -------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-033",
            coverage: Coverage::Lexical,
            plan: Plan::Executable(vec![Probe::new(
                assertion(
                    "\nDATA:\n    ID: data.list\n    TYPE: LIST[INTEGER]\n    VALUE: [\n        1,\n        2\n    ]\n",
                    "COUNT(REF(data.list)) == 2",
                ),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-034",
            coverage: Coverage::Lexical,
            plan: Plan::Executable(vec![Probe::new(
                assertion(
                    "\nDATA:\n    ID: data.text\n    TYPE: STRING\n    VALUE: \"\"\"\n        a\n    \"\"\"\n",
                    "REF(data.text) == \"a\\n\"",
                ),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-035",
            coverage: Coverage::Lexical,
            plan: Plan::Executable(vec![Probe::new(
                assertion(
                    "\nDATA:\n    ID: data.text\n    TYPE: STRING\n    VALUE: \"\"\"\n         a\n    \"\"\"\n",
                    "REF(data.text) == \" a\\n\"",
                ),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-036",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "COUNT(\"\\uD83D\\uDE00\") == 1"),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-037",
            coverage: Coverage::Lexical,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "COUNT(\"\\uD800\") == 1"),
                Expectation::Rejects("error.literal.escape".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-038",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "COUNT(\"\\n\") == 1"),
                holds(),
            )]),
        },
        // -- equality ------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-039",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "(REGEX(\"a\") == REGEX(\"[a]\")) == FALSE"),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-040",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion(
                    "",
                    "(URI(\"https://EXAMPLE.invalid/\") == URI(\"https://example.invalid/\")) == FALSE",
                ),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-041",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "(PATH(\"/a/../b\") == PATH(\"/b\")) == FALSE"),
                holds(),
            )]),
        },
        // -- object_type ---------------------------------------------------
        WitnessCase {
            id: "CLOSURE-042",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion(
                    "
DEFINE:
    ID: type.one
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: name
        TYPE: STRING
        REQUIRED: TRUE
    FIELD:
        NAME: count
        TYPE: INTEGER
        REQUIRED: TRUE

DEFINE:
    ID: type.two
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: count
        TYPE: INTEGER
        REQUIRED: TRUE
    FIELD:
        NAME: name
        TYPE: STRING
        REQUIRED: TRUE

DATA:
    ID: data.one
    TYPE: OBJECT[REF(type.one)]
    VALUE:
        name: \"a\"
        count: 2

DATA:
    ID: data.two
    TYPE: OBJECT[REF(type.two)]
    VALUE:
        count: 2
        name: \"a\"
",
                    "REF(data.one) == REF(data.two)",
                ),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-043",
            coverage: Coverage::StaticOrType,
            plan: Plan::Executable(vec![Probe::new(
                schema_conflict(),
                Expectation::Rejects("error.object.schema".to_string()),
            )]),
        },
        // -- temporal ------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-044",
            coverage: Coverage::Lexical,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "TIME(\"23:59:60Z\") == TIME(\"00:00:00Z\")"),
                Expectation::Rejects("error.literal.invalid".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-045",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![Probe::new(
                assertion("", "(TIME(\"00:00:00+01:00\") == TIME(\"23:00:00Z\")) == FALSE"),
                holds(),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-046",
            coverage: Coverage::Lexical,
            plan: Plan::Executable(vec![
                Probe::labelled(
                    "a",
                    assertion(
                        "",
                        "DATETIME(\"2024-02-29T00:00:00Z\") == DATETIME(\"2024-02-29T00:00:00Z\")",
                    ),
                    holds(),
                ),
                Probe::labelled(
                    "b",
                    assertion(
                        "",
                        "DATETIME(\"2023-02-29T00:00:00Z\") == DATETIME(\"2023-02-29T00:00:00Z\")",
                    ),
                    Expectation::Rejects("error.literal.invalid".to_string()),
                ),
            ]),
        },
        // -- unit_error ----------------------------------------------------
        WitnessCase {
            id: "CLOSURE-047",
            coverage: Coverage::StaticOrType,
            plan: Plan::Executable(vec![
                Probe::labelled(
                    "a",
                    assertion(
                        "",
                        "MEASURE(1, unit.pixel) + MEASURE(1, unit.second) == MEASURE(2, unit.pixel)",
                    ),
                    Expectation::Rejects("error.numeric.unit_mismatch".to_string()),
                ),
                Probe::labelled(
                    "b",
                    assertion("", "(MEASURE(1, unit.pixel) == MEASURE(1, unit.second)) == FALSE"),
                    holds(),
                ),
            ]),
        },
        // -- read_range ----------------------------------------------------
        WitnessCase {
            id: "CLOSURE-048",
            coverage: Coverage::StandardLibrary,
            plan: Plan::Executable(vec![Probe::new(
                read_range("PATH(\"/case/a.txt\")", "scalar", 1, 3, "bc"),
                Expectation::Terminal("status.succeeded".to_string()),
            )
            .on_filesystem()]),
        },
        WitnessCase {
            id: "CLOSURE-049",
            coverage: Coverage::StandardLibrary,
            plan: Plan::Executable(vec![Probe::new(
                read_range("PATH(\"/case/lines.txt\")", "line", 0, 1, "a\\n"),
                Expectation::Terminal("status.succeeded".to_string()),
            )
            .on_filesystem()]),
        },
        WitnessCase {
            id: "CLOSURE-050",
            coverage: Coverage::StandardLibrary,
            plan: Plan::Executable(vec![Probe::new(
                read_range("PATH(\"/case/a.txt\")", "scalar", 2, 1, "never read"),
                Expectation::Rejects("error.value.out_of_range".to_string()),
            )
            .on_filesystem()]),
        },
        // -- validate_schema / append_content -------------------------------
        WitnessCase {
            id: "CLOSURE-051",
            coverage: Coverage::StandardLibrary,
            plan: Plan::Executable(vec![Probe::new(
                validate_object_schema(),
                Expectation::Rejects("error.operation.parameter".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-052",
            coverage: Coverage::StandardLibrary,
            plan: Plan::Executable(vec![Probe::new(
                append_bytes(),
                Expectation::Rejects("error.operation.parameter".to_string()),
            )
            .on_filesystem()]),
        },
        WitnessCase {
            id: "CLOSURE-053",
            coverage: Coverage::Preflight,
            plan: Plan::Executable(vec![Probe::new(
                action_then_phase(),
                Expectation::Terminal("status.succeeded".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-054",
            coverage: Coverage::Preflight,
            plan: Plan::Executable(vec![Probe::new(
                two_paths_one_action(),
                Expectation::Rejects("error.execution.order".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-055",
            coverage: Coverage::Preflight,
            plan: Plan::Executable(vec![
                Probe::labelled(
                    "reversed",
                    sequential_order("BEFORE"),
                    Expectation::Rejects("error.execution.order".to_string()),
                ),
                Probe::labelled(
                    "legal",
                    sequential_order("AFTER"),
                    Expectation::Terminal("status.succeeded".to_string()),
                ),
            ]),
        },
        WitnessCase {
            id: "CLOSURE-056",
            coverage: Coverage::Preflight,
            plan: Plan::Executable(vec![Probe::new(
                two_actions_one_output(),
                Expectation::Rejects("error.execution.order".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-057",
            coverage: Coverage::Grammar,
            plan: Plan::Executable(vec![Probe::new(
                action_output_list(),
                Expectation::Rejects("error.field.type".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-058",
            coverage: Coverage::Resolution,
            plan: Plan::Executable(vec![
                Probe::labelled(
                    "outside",
                    loop_output(true),
                    Expectation::Rejects("error.reference.unresolved".to_string()),
                ),
                Probe::labelled(
                    "inside",
                    loop_output(false),
                    Expectation::Terminal("status.succeeded".to_string()),
                ),
            ]),
        },
        WitnessCase {
            id: "CLOSURE-059",
            coverage: Coverage::Runtime,
            plan: Plan::Executable(vec![
                retry_command(CommandRetry::Complete),
                retry_command(CommandRetry::NoProof),
                retry_command(CommandRetry::FailAgain),
            ]),
        },
        // -- check_selection and root completion ----------------------------
        WitnessCase {
            id: "CLOSURE-060",
            coverage: Coverage::Completion,
            plan: Plan::Executable(vec![
                imported_check(false),
                imported_check(true),
            ]),
        },
        WitnessCase {
            id: "CLOSURE-061",
            coverage: Coverage::Completion,
            plan: Plan::Executable(vec![Probe::new(
                failing_test_root(),
                Expectation::Rejects("error.verification.failed".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-062",
            coverage: Coverage::Completion,
            plan: Plan::Executable(vec![Probe::new(
                action_root(),
                Expectation::Terminal("status.succeeded".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-063",
            coverage: Coverage::Preflight,
            plan: Plan::Executable(vec![Probe::new(
                two_actions_one_output(),
                Expectation::Rejects("error.execution.order".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-064",
            coverage: Coverage::Preflight,
            plan: Plan::Executable(vec![Probe::new(
                optional_validate_prerequisite(),
                Expectation::Terminal("status.succeeded".to_string()),
            )]),
        },
        WitnessCase {
            id: "CLOSURE-065",
            coverage: Coverage::Completion,
            plan: Plan::Executable(vec![Probe::new(
                material_target_verify(),
                Expectation::Check {
                    id: "verify.material".to_string(),
                    outcome: "TRUE".to_string(),
                },
            )
            .on_filesystem()]),
        },
        WitnessCase {
            id: "CLOSURE-066",
            coverage: Coverage::Completion,
            plan: Plan::Executable(vec![Probe::new(
                unselected_branch_verify(),
                Expectation::Terminal("status.succeeded".to_string()),
            )]),
        },
    ]
}

/// A real imported targetless check is selected only by an explicit prerequisite.
pub fn imported_check(selected: bool) -> Probe {
    let library = assertion("", "FALSE").replace("verify.case", "verify.unrelated");
    let expression = if selected {
        "REF(lib.verify.unrelated)"
    } else {
        "TRUE"
    };
    let source = assertion(
        "\nIMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"checks.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n",
        expression,
    );
    let expectation = if selected {
        Expectation::All(vec![
            Expectation::Rejects("error.verification.failed".into()),
            Expectation::Check {
                id: "lib.verify.unrelated".into(),
                outcome: "FALSE".into(),
            },
            Expectation::Check {
                id: "verify.case".into(),
                outcome: "FALSE".into(),
            },
        ])
    } else {
        Expectation::All(vec![
            Expectation::Accepts,
            holds(),
            Expectation::NoDiagnostic("error.verification.failed".into()),
        ])
    };
    Probe::labelled(
        if selected {
            "prerequisite"
        } else {
            "import-only"
        },
        source,
        expectation,
    )
    .with_import("checks.lcl", library)
}

/// Recovery and declared successor activation must commit together.
pub fn continue_read(successor: bool) -> String {
    let successor = if successor {
        "    STEP:\n        ID: step.next\n        ACTION:\n            ID: action.next\n            OPERATION: core.return\n            TARGET: 9\n            OUTPUT: REF(output.payload)\n"
    } else {
        ""
    };
    task(
        "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nOUTPUT:\n    ID: output.payload\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n\nHANDLER:\n    ID: handler.continue\n    EVENT: event.host_constraint\n    OPERATION: core.continue\n",
        &format!(
            "SEQUENCE:\n    ID: sequence.case\n    STEP:\n        ID: step.read\n        ACTION:\n            ID: action.read\n            OPERATION: core.read\n            TARGET: PATH(\"/case/retry.txt\")\n{successor}\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    SEQUENCE: REF(sequence.case)\n    HANDLER: REF(handler.continue)\n    OUTPUT: REF(output.payload)\n    SUCCESS: REF(success.case)\n"
        ),
        "task.case",
    )
}

/// The source fixes retry policy; only host observations come from a schedule.
pub fn retry_read(when: Option<&str>) -> String {
    let when = when
        .map(|w| format!("        WHEN: {w}\n"))
        .unwrap_or_default();
    task(
        "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nOUTPUT:\n    ID: output.payload\n    TYPE: STRING\n    FORMAT: format.plain_text\n\nHANDLER:\n    ID: handler.retry\n    EVENT: event.host_constraint\n    OPERATION: core.retry\n    LIMIT: 2\n",
        &format!(
            "ACTION:\n    ID: action.read\n    OPERATION: core.read\n    TARGET: PATH(\"/case/retry.txt\")\n    OUTPUT: REF(output.payload)\n    RETRY:\n        LIMIT: 2\n{when}        HANDLER: REF(handler.retry)\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.read)\n    HANDLER: REF(handler.retry)\n    OUTPUT: REF(output.payload)\n    SUCCESS: REF(success.case)\n"
        ),
        "task.case",
    )
}

/// An iteration-local producer and an optionally out-of-scope value reader.
pub fn loop_output(outside: bool) -> String {
    let outside = if outside {
        "\nVERIFY:\n    ID: verify.outside\n    ASSERT: REF(output.item) == 1\n"
    } else {
        ""
    };
    task(
        &format!(
            "\nINPUT:\n    ID: input.members\n    TYPE: LIST[INTEGER]\n    VALUE: [1, 2]\n\nOUTPUT:\n    ID: output.item\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n{outside}"
        ),
        "SEQUENCE:\n    ID: sequence.case\n    FOR EACH item IN REF(input.members):\n        STEP:\n            ID: step.producer\n            ACTION:\n                ID: action.producer\n                OPERATION: core.return\n                TARGET: REF(item)\n                OUTPUT: REF(output.item)\n        STEP:\n            ID: step.consumer\n            ACTION:\n                ID: action.consumer\n                OPERATION: core.inspect\n                TARGET: REF(output.item)\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    INPUT: REF(input.members)\n    SEQUENCE: REF(sequence.case)\n    SUCCESS: REF(success.case)\n",
        "task.case",
    )
}

/// An explicit edge either agrees with or reverses sequential sibling order.
pub fn sequential_order(order: &str) -> String {
    task(
        "\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n",
        &format!(
            "SEQUENCE:\n    ID: sequence.case\n    MODE: mode.sequential\n    STEP:\n        ID: step.first\n        ACTION:\n            ID: action.first\n            OPERATION: core.inspect\n            TARGET: REF(data.subject)\n    STEP:\n        ID: step.second\n        {order}: [REF(step.first)]\n        ACTION:\n            ID: action.second\n            OPERATION: core.inspect\n            TARGET: REF(data.subject)\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    SEQUENCE: REF(sequence.case)\n    SUCCESS: REF(success.case)\n"
        ),
        "task.case",
    )
}

/// Closed typed OBJECTs can be collection members through ordinary value REF.
pub fn group_objects() -> String {
    format!(
        r#"{HEADER}
DEFINE:
    ID: type.tagged
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: tag
        TYPE: STRING
        REQUIRED: TRUE

DEFINE:
    ID: type.grouped
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: key
        TYPE: STRING
        REQUIRED: TRUE
    FIELD:
        NAME: items
        TYPE: LIST[OBJECT[REF(type.tagged)]]
        REQUIRED: TRUE

DATA:
    ID: data.first
    TYPE: OBJECT[REF(type.tagged)]
    VALUE:
        tag: "a"

DATA:
    ID: data.second
    TYPE: OBJECT[REF(type.tagged)]
    VALUE:
        tag: "b"

DATA:
    ID: data.third
    TYPE: OBJECT[REF(type.tagged)]
    VALUE:
        tag: "a"

INPUT:
    ID: input.members
    TYPE: LIST[OBJECT[REF(type.tagged)]]
    VALUE: [REF(data.first), REF(data.second), REF(data.third)]

OUTPUT:
    ID: output.groups
    TYPE: LIST[OBJECT[REF(type.grouped)]]
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: EXISTS(REF(output.groups))

ACTION:
    ID: action.group
    OPERATION: core.group
    TARGET: REF(input.members)
    PARAMETER:
        NAME: key
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "tag"
    OUTPUT: REF(output.groups)

VERIFY:
    ID: verify.case
    ASSERT: COUNT(REF(output.groups)) == 2 AND REF(output.groups)[0].key == "a" AND REF(output.groups)[1].key == "b" AND REF(output.groups)[0].items == [REF(data.first), REF(data.third)] AND REF(output.groups)[1].items == [REF(data.second)]

SUCCESS:
    ID: success.case
    ALL: [REF(verify.case)]

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.members)
    ACTION: REF(action.group)
    OUTPUT: REF(output.groups)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"#
    )
}

// ---------------------------------------------------------------------------
// Case-specific documents
// ---------------------------------------------------------------------------

/// A task that calculates one expression fragment over a target.
pub fn calculate(expression: &str) -> String {
    format!(
        "{HEADER}
DATA:
    ID: data.increment
    TYPE: INTEGER
    VALUE: 3

INPUT:
    ID: input.base
    TYPE: INTEGER
    VALUE: 2

OUTPUT:
    ID: output.sum
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: EXISTS(REF(output.sum))

ACTION:
    ID: action.calc
    OPERATION: core.calculate
    TARGET: REF(input.base)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: {expression}
    OUTPUT: REF(output.sum)

VERIFY:
    ID: verify.case
    ASSERT: REF(output.sum) == 5

SUCCESS:
    ID: success.case
    ALL: [REF(verify.case)]

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.base)
    ACTION: REF(action.calc)
    OUTPUT: REF(output.sum)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// core.compare of 1 against 2, whose FALSE result is still a completed one.
pub fn compare_case() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.left
    TYPE: INTEGER
    VALUE: 1

DATA:
    ID: data.right
    TYPE: INTEGER
    VALUE: 2

OUTPUT:
    ID: output.same
    TYPE: BOOLEAN
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: EXISTS(REF(output.same))

ACTION:
    ID: action.compare
    OPERATION: core.compare
    TARGET: REF(input.left)
    PARAMETER:
        NAME: against
        TYPE: INTEGER
        REQUIRED: TRUE
        VALUE: REF(data.right)
    OUTPUT: REF(output.same)

VERIFY:
    ID: verify.case
    ASSERT: REF(output.same) == FALSE

SUCCESS:
    ID: success.case
    ALL: [REF(verify.case)]

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.left)
    ACTION: REF(action.compare)
    OUTPUT: REF(output.same)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// A STEP carrying an inline RETRY, which is ACTION-only.
pub fn step_with_inline_retry() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.seed
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: EXISTS(REF(output.seed))

ACTION:
    ID: action.seed
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.seed)

SEQUENCE:
    ID: sequence.one
    STEP:
        ID: step.one
        ACTION: REF(action.seed)
        RETRY:
            LIMIT: 2

SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.seed)
    SEQUENCE: REF(sequence.one)
    OUTPUT: REF(output.seed)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// Two handlers whose FALLBACK references close a cycle.
pub fn handler_fallback_cycle() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.seed
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: EXISTS(REF(output.seed))

ACTION:
    ID: action.seed
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.seed)

HANDLER:
    ID: handler.a
    EVENT: event.missing
    OPERATION: core.continue
    FALLBACK: REF(handler.b)

HANDLER:
    ID: handler.b
    EVENT: event.missing
    OPERATION: core.continue
    FALLBACK: REF(handler.a)

SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.seed)
    ACTION: REF(action.seed)
    OUTPUT: REF(output.seed)
    HANDLER: [REF(handler.a), REF(handler.b)]
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// A non-root ACTION whose WHEN is FALSE, which must take status.skipped.
pub fn skipped_action() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.seed
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: TRUE

ACTION:
    ID: action.seed
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.seed)
    WHEN: FALSE
    REQUIRED: FALSE

SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.seed)
    ACTION: REF(action.seed)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// A TYPE selecting one object schema and a SCHEMA selecting a different one.
pub fn schema_conflict() -> String {
    format!(
        "{DATA_HEADER}
DEFINE:
    ID: type.one
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: a
        TYPE: STRING
        REQUIRED: TRUE

DEFINE:
    ID: type.two
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: b
        TYPE: STRING
        REQUIRED: TRUE

DATA:
    ID: data.record
    TYPE: OBJECT[REF(type.one)]
    SCHEMA: REF(type.two)
    VALUE:
        a: \"x\"
"
    )
}

/// core.read with a declared range over an addressable target.
pub fn read_range(target: &str, unit: &str, start: i64, end: i64, expected: &str) -> String {
    format!(
        "{HEADER}
DATA:
    ID: data.subject
    TYPE: PATH
    VALUE: {target}

OUTPUT:
    ID: output.slice
    TYPE: STRING
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: EXISTS(REF(output.slice))

ACTION:
    ID: action.read
    OPERATION: core.read
    TARGET: REF(data.subject)
    PARAMETER:
        NAME: range
        TYPE: OBJECT
        REQUIRED: TRUE
        VALUE:
            unit: \"{unit}\"
            start: {start}
            end: {end}
    OUTPUT: REF(output.slice)

VERIFY:
    ID: verify.case
    ASSERT: REF(output.slice) == \"{expected}\"

SUCCESS:
    ID: success.case
    ALL: [REF(verify.case)]

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    ACTION: REF(action.read)
    OUTPUT: REF(output.slice)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// core.validate whose schema parameter is an arbitrary OBJECT.
///
/// `#/contracts/core.validate/parameters/schema`: "Material OBJECT schema
/// encodings are not admitted."
pub fn validate_object_schema() -> String {
    format!(
        "{HEADER}
DATA:
    ID: data.subject
    TYPE: STRING
    VALUE: \"x\"

GOAL:
    ID: goal.case
    ASSERT: TRUE

ACTION:
    ID: action.validate
    OPERATION: core.validate
    TARGET: REF(data.subject)
    PARAMETER:
        NAME: schema
        TYPE: OBJECT
        REQUIRED: TRUE
        VALUE:
            name: \"a\"

SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    ACTION: REF(action.validate)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// core.append whose content parameter is a byte count rather than content.
pub fn append_bytes() -> String {
    format!(
        "{HEADER}
DATA:
    ID: data.subject
    TYPE: PATH
    VALUE: PATH(\"/case/a.txt\")

GOAL:
    ID: goal.case
    ASSERT: TRUE

ACTION:
    ID: action.append
    OPERATION: core.append
    TARGET: REF(data.subject)
    PARAMETER:
        NAME: content
        TYPE: BYTES
        REQUIRED: TRUE
        VALUE: BYTES(4)

SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    ACTION: REF(action.append)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// A TASK whose ACTION field precedes its PHASE field, both reference lists.
pub fn action_then_phase() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.first
    TYPE: INTEGER
    FORMAT: format.plain_text

OUTPUT:
    ID: output.second
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: TRUE

ACTION:
    ID: action.first
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.first)

ACTION:
    ID: action.second
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.second)

PHASE:
    ID: phase.one
    STEP:
        ID: step.second
        ACTION: REF(action.second)

SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.seed)
    ACTION: REF(action.first)
    PHASE: REF(phase.one)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// Two STEP activation paths selecting the same ACTION source declaration.
pub fn two_paths_one_action() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.seed
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: TRUE

ACTION:
    ID: action.shared
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.seed)

SEQUENCE:
    ID: sequence.one
    STEP:
        ID: step.one
        ACTION: REF(action.shared)
    STEP:
        ID: step.two
        ACTION: REF(action.shared)

SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.seed)
    SEQUENCE: REF(sequence.one)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// Two ACTION declarations selecting the same OUTPUT.
pub fn two_actions_one_output() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.shared
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: TRUE

ACTION:
    ID: action.one
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.shared)

ACTION:
    ID: action.two
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.shared)

SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.seed)
    ACTION: [REF(action.one), REF(action.two)]
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// An ACTION whose OUTPUT field is a LIST of two OUTPUT references.
pub fn action_output_list() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.one
    TYPE: INTEGER
    FORMAT: format.plain_text

OUTPUT:
    ID: output.two
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: TRUE

ACTION:
    ID: action.seed
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: [REF(output.one), REF(output.two)]

SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.seed)
    ACTION: REF(action.seed)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// A required TEST root whose assertion is FALSE.
pub fn failing_test_root() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

TEST:
    ID: test.case
    REQUIRED: TRUE
    ASSERT: REF(input.seed) == 999

EXECUTE:
    REFERENCE: REF(test.case)
"
    )
}

/// An EXECUTE that selects an ACTION directly, so no TASK SUCCESS exists.
pub fn action_root() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.seed
    TYPE: INTEGER
    FORMAT: format.plain_text

ACTION:
    ID: action.seed
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.seed)

EXECUTE:
    REFERENCE: REF(action.seed)
"
    )
}

/// An optional VALIDATE that the root SUCCESS references as a prerequisite.
pub fn optional_validate_prerequisite() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.seed
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: TRUE

ACTION:
    ID: action.seed
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.seed)

VALIDATE:
    ID: validate.optional
    REQUIRED: FALSE
    ASSERT: REF(input.seed) == 1

SUCCESS:
    ID: success.case
    ALL: [REF(validate.optional)]

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.seed)
    ACTION: REF(action.seed)
    OUTPUT: REF(output.seed)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// An ACTION acting on a PATH value, and a VERIFY observing the same value.
pub fn material_target_verify() -> String {
    format!(
        "{HEADER}
DATA:
    ID: data.subject
    TYPE: PATH
    VALUE: PATH(\"/case/a.txt\")

OUTPUT:
    ID: output.content
    TYPE: STRING
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: TRUE

ACTION:
    ID: action.read
    OPERATION: core.read
    TARGET: PATH(\"/case/a.txt\")
    OUTPUT: REF(output.content)

VERIFY:
    ID: verify.material
    TARGET: PATH(\"/case/a.txt\")
    ASSERT: EXISTS(REF(output.content))

SUCCESS:
    ID: success.case
    ALL: [REF(verify.material)]

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    ACTION: REF(action.read)
    OUTPUT: REF(output.content)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// An IF selecting its first branch, with a VERIFY targeting the other one.
pub fn unselected_branch_verify() -> String {
    format!(
        "{HEADER}
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.first
    TYPE: INTEGER
    FORMAT: format.plain_text

OUTPUT:
    ID: output.second
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: TRUE

ACTION:
    ID: action.first
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.first)

ACTION:
    ID: action.second
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.second)

SEQUENCE:
    ID: sequence.one
    IF (REF(input.seed) == 1) THEN:
        STEP:
            ID: step.first
            ACTION: REF(action.first)
    ELSE:
        STEP:
            ID: step.second
            ACTION: REF(action.second)

VERIFY:
    ID: verify.unselected
    TARGET: REF(action.second)
    ASSERT: FALSE

SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.seed)
    SEQUENCE: REF(sequence.one)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
"
    )
}

/// Finite, exact command fixture: emit fixed bytes to stdout and no other
/// application-visible state. This is not a safety claim about arbitrary code.
#[derive(Clone, Copy)]
pub enum CommandRetry {
    Complete,
    NoProof,
    FailAgain,
}
struct PartialCommand {
    mode: CommandRetry,
    calls: usize,
}
impl lcl_runtime::Host for PartialCommand {
    fn permits(&mut self, request: &lcl_runtime::CapabilityRequest) -> lcl_runtime::Permission {
        if request.operation == "core.execute"
            && request.target
                == Some(lcl_runtime::Value::Constructed {
                    constructor: "PATH".into(),
                    text: "/case/emit-fixed".into(),
                })
            && self.calls < 2
            && request.invocation.attempt == self.calls
        {
            lcl_runtime::Permission::Granted
        } else {
            lcl_runtime::Permission::Denied("outside exact fixed-emitter fixture".into())
        }
    }
    fn invoke(
        &mut self,
        request: &lcl_runtime::CapabilityRequest,
    ) -> lcl_runtime::CapabilityOutcome {
        assert_eq!(request.invocation.attempt, self.calls);
        self.calls += 1;
        let complete = self.calls == 2 && matches!(self.mode, CommandRetry::Complete);
        let stdout = if complete {
            "complete"
        } else if self.calls == 1 {
            "old partial"
        } else {
            "final partial"
        };
        let mut observation = lcl_runtime::Observation::none()
            .with("mode", lcl_runtime::Value::Identifier("non_graph".into()))
            .with("started", lcl_runtime::Value::Boolean(true))
            .with("completed", lcl_runtime::Value::Boolean(complete))
            .with("stdout", lcl_runtime::Value::Text(stdout.into()))
            .with("stderr", lcl_runtime::Value::Text(String::new()))
            .with_effect(lcl_runtime::ObservedEffect {
                class: lcl_runtime::EffectClass::Process,
                state: if complete { lcl_runtime::RecordState::Applied } else { lcl_runtime::RecordState::Partial },
                target: Some("/case/emit-fixed".into()),
                evidence: vec![format!("fixed emitter stopped; captured stdout {stdout:?}; no other application-visible effects")],
            });
        if complete {
            observation = observation.with(
                "exit_code",
                lcl_runtime::Value::Integer(
                    lcl_checker::numeric::Decimal::parse_integer("0").unwrap(),
                ),
            );
            lcl_runtime::CapabilityOutcome::Completed(observation)
        } else {
            observation.host_limited = true;
            lcl_runtime::CapabilityOutcome::Failed {
                detail: "bounded fixed emitter interrupted".into(),
                observation,
            }
        }
    }
    fn retry_evidence(
        &mut self,
        context: &lcl_runtime::capability::RetryContext,
    ) -> lcl_runtime::capability::RetryEvidence {
        use lcl_runtime::capability::{RetryEvidence, RetryMethod, RetryProof};
        assert_eq!(self.calls, 1);
        if matches!(self.mode, CommandRetry::NoProof) {
            return RetryEvidence::Missing;
        }
        RetryEvidence::Established(Box::new(RetryProof {
            context: context.clone(), method: RetryMethod::Repeat,
            effect_state: context.previous.effect_state,
            observed_effects: context.previous.observed_effects.clone(),
            post_state: "fixed emitter stopped, private stdout captured; exact same immutable command and request".into(),
            evidence: vec!["the bounded fixture only emits fixed bytes to a fresh private stdout; repeat cannot duplicate an external mutation or expand target, arguments, environment, authority or scope".into()],
        }))
    }
}

pub fn retry_command(mode: CommandRetry) -> Probe {
    let source = retry_read(None)
        .replace("core.read", "core.execute")
        .replace("/case/retry.txt", "/case/emit-fixed")
        .replace("LIMIT: 2", "LIMIT: 1");
    let field = |attempt, name: &str, value: &str| Expectation::AttemptField {
        declaration: "action.read".into(),
        attempt,
        field: name.into(),
        value: value.into(),
    };
    let mut assertions = vec![
        field(0, "initial_output", "MISSING"),
        field(0, "output_binding", "partial"),
        field(0, "stdout", "\"old partial\""),
        field(0, "effect_state", "partial"),
        Expectation::Diagnostic("error.host.constraint".into()),
    ];
    let (label, final_value, statuses) = match mode {
        CommandRetry::Complete => {
            assertions.extend([
                Expectation::Accepts,
                Expectation::Terminal("status.succeeded".into()),
                Expectation::RetryProofs(1),
                field(1, "initial_output", "MISSING"),
                field(1, "output_binding", "bound"),
                field(1, "stdout", "\"complete\""),
                Expectation::NoDiagnostic("error.retry.exhausted".into()),
            ]);
            (
                "recovered",
                "\"complete\"",
                vec!["status.blocked", "status.succeeded"],
            )
        }
        CommandRetry::NoProof => {
            assertions.extend([
                Expectation::Diagnostic("error.required.missing".into()),
                Expectation::RetryProofs(0),
                Expectation::NoDiagnostic("error.retry.exhausted".into()),
            ]);
            ("safety-blocked", "\"old partial\"", vec!["status.blocked"])
        }
        CommandRetry::FailAgain => {
            assertions.extend([
                Expectation::Diagnostic("error.retry.exhausted".into()),
                Expectation::RetryProofs(1),
                field(1, "initial_output", "MISSING"),
                field(1, "output_binding", "partial"),
                field(1, "stdout", "\"final partial\""),
            ]);
            (
                "exhausted",
                "\"final partial\"",
                vec!["status.blocked", "status.blocked"],
            )
        }
    };
    assertions.push(Expectation::Attempts {
        declaration: "action.read".into(),
        statuses: statuses.into_iter().map(str::to_string).collect(),
    });
    assertions.push(Expectation::Output {
        id: "output.payload".into(),
        value: final_value.into(),
    });
    let mut probe = Probe::labelled(label, source, Expectation::All(assertions));
    probe.command_retry = Some(mode);
    probe
}
