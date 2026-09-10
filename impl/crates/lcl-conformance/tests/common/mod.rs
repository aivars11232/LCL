//! Shared helpers. Every case runs against the approved package only.

#![allow(dead_code)]

use lcl_conformance::{ConformanceIndex, Runner};
use lcl_spec::SpecPackage;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

pub fn spec() -> &'static SpecPackage {
    static SPEC: OnceLock<SpecPackage> = OnceLock::new();
    SPEC.get_or_init(|| SpecPackage::open(canonical_root()).expect("approved package opens"))
}

pub fn index() -> &'static ConformanceIndex {
    static INDEX: OnceLock<ConformanceIndex> = OnceLock::new();
    INDEX.get_or_init(|| ConformanceIndex::load(spec()).expect("the catalogs load"))
}

/// The whole engine, assembled for one test.
///
/// `Runner` holds a `RefCell`, so it is deliberately not `Sync` and cannot live
/// in a `OnceLock` static. Assembly reads the already-verified package and is
/// cheap next to the cases each test then runs.
pub fn runner() -> Runner {
    Runner::new(spec()).expect("the engine assembles from the approved package")
}

/// A minimal `kind.task` document with the given blocks appended.
pub fn task_document(blocks: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.case\n    \
         NAME: \"Conformance case\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n{blocks}"
    )
}

/// A `kind.data` document holding one `DATA` declaration per entry.
///
/// `(id, TYPE, VALUE)`. The smallest document shape that carries an arbitrary
/// expression through every stage below execution.
pub fn data_document(entries: &[(&str, &str, &str)]) -> String {
    let mut out = String::from(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.case\n    \
         NAME: \"Conformance case\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n",
    );
    for (id, ty, value) in entries {
        out.push_str(&format!(
            "\nDATA:\n    ID: {id}\n    TYPE: {ty}\n    VALUE: {value}\n"
        ));
    }
    out
}

/// A runnable `kind.task` whose completion asserts one expression.
///
/// The witness under test becomes a `VERIFY` assertion, so a case exercises the
/// whole pipeline — parse, resolve, check, preflight, execute, complete — and
/// the recorded check outcome is the observation.
pub fn assertion_task(declarations: &str, assertion: &str) -> String {
    task_document(&format!(
        "{declarations}
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
    ))
}
