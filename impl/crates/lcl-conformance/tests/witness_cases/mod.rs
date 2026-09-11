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

use lcl_conformance::report::Coverage;
use lcl_conformance::Expectation;

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
}

impl Probe {
    pub fn new(source: String, expectation: Expectation) -> Probe {
        Probe {
            label: "",
            source,
            expectation,
            needs_filesystem: false,
        }
    }

    pub fn labelled(label: &'static str, source: String, expectation: Expectation) -> Probe {
        Probe {
            label,
            source,
            expectation,
            needs_filesystem: false,
        }
    }

    pub fn on_filesystem(mut self) -> Probe {
        self.needs_filesystem = true;
        self
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
            plan: Plan::Descriptive {
                reason: "The witness reads REF(output.copy).TARGET, a declaration-metadata \
                         projection over an OUTPUT declaration. The canonical text names the \
                         field but supplies no declaration for it, and OUTPUT.TARGET is a \
                         destination whose concrete value this build's mock host never \
                         assigns, so no source exhibits the read without inventing one.",
            },
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
            plan: Plan::Descriptive {
                reason: "The witness needs a REFERENCE-typed binding that is read by equality \
                         without a second dereference. A bare `TYPE: REFERENCE` field is \
                         refused at grammar stage and `TYPE: REFERENCE[INTEGER]` with a \
                         `REF(...)` value is refused at static checking, so no declaration \
                         form in this build produces the binding the witness describes, and \
                         choosing one would be inventing the missing declaration syntax \
                         rather than executing the witness.",
            },
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
            plan: Plan::Descriptive {
                reason: "The witness groups a LIST whose members are OBJECTs, and Core 0.1.0 \
                         supplies no way to write one. `04_GRAMMAR/10` gives \
                         MULTILINE_COLLECTION and COLLECTION_LITERAL members as EXPRESSION, \
                         and `04_GRAMMAR/12` puts an object in an indented VALUE body, which \
                         is not an expression; no Core operation returns a LIST of objects \
                         either. The object-resolution gap this once cited was closed by \
                         LCL-TASK-0020, and the remaining obstacle is the collection member \
                         form, not the object value.",
            },
        },
        // -- retry ---------------------------------------------------------
        WitnessCase {
            id: "CLOSURE-022",
            coverage: Coverage::Runtime,
            plan: Plan::Descriptive {
                reason: "The witness needs an action that fails once and then succeeds. The \
                         canonical text supplies no such host behaviour, and this build's \
                         in-memory fixtures answer a given request the same way every time, \
                         so the attempt sequence cannot be exhibited without inventing a \
                         failure schedule the witness does not state.",
            },
        },
        WitnessCase {
            id: "CLOSURE-023",
            coverage: Coverage::Runtime,
            plan: Plan::Descriptive {
                reason: "Same missing test data as CLOSURE-022: the witness begins from an \
                         initial failure, and no canonical fixture supplies one.",
            },
        },
        WitnessCase {
            id: "CLOSURE-024",
            coverage: Coverage::Runtime,
            plan: Plan::Descriptive {
                reason: "Same missing test data as CLOSURE-022: exhaustion requires three \
                         actual unsuccessful attempts, which needs a host scripted to fail.",
            },
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
            plan: Plan::Descriptive {
                reason: "The witness needs a raised diagnostic to select a handler that then \
                         invokes core.continue successfully. Raising one requires a scripted \
                         failure, the same missing test data as CLOSURE-022.",
            },
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
            plan: Plan::Descriptive {
                reason: "The witness needs an explicit BEFORE ordering request that reverses a \
                         sequential sibling edge. Core 0.1.0 declares no ordering field on \
                         STEP or ACTION that this build admits, so the request cannot be \
                         written without inventing the syntax that carries it.",
            },
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
            plan: Plan::Descriptive {
                reason: "The witness reads a loop-produced OUTPUT from outside its FOR EACH \
                         body. Encoding it needs a FOR EACH whose body binds an OUTPUT and a \
                         reader outside the loop; the canonical text names neither the \
                         collection nor the producing action, so the source would be invented \
                         rather than derived.",
            },
        },
        WitnessCase {
            id: "CLOSURE-059",
            coverage: Coverage::Runtime,
            plan: Plan::Descriptive {
                reason: "The witness begins from a failed attempt that produced permitted \
                         partial output. That needs a scripted partial failure, the same \
                         missing test data as CLOSURE-022.",
            },
        },
        // -- check_selection and root completion ----------------------------
        WitnessCase {
            id: "CLOSURE-060",
            coverage: Coverage::Completion,
            plan: Plan::Descriptive {
                reason: "The witness needs a second imported document holding an unrelated \
                         targetless VERIFY. The runner resolves one source unit through an \
                         empty provider, and the import target the witness assumes is not \
                         named in the canonical text, so the imported unit would be invented.",
            },
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
