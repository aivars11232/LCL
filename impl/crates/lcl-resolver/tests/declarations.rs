//! Declaration identity, uniqueness and the reserved-namespace rule.

mod common;

use common::{assert_stages_clean, ids, resolve};
use lcl_resolver::Outcome;

/// A `kind.task` document with the given body, rooted at `sequence.s`.
///
/// A `kind.task` document requires exactly one `EXECUTE` root, so the fixture
/// supplies one rather than leaving the grammar stage to reject it.
fn task(body: &str) -> String {
    format!(
        concat!(
            "LCL:\n    VERSION: \"0.1.0\"\n\n",
            "SPECIFICATION:\n    ID: test.doc\n    NAME: \"T\"\n",
            "    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n{}\n",
            "EXECUTE:\n    REFERENCE: REF(sequence.s)\n"
        ),
        body
    )
}

fn data(body: &str) -> String {
    format!(
        concat!(
            "LCL:\n    VERSION: \"0.1.0\"\n\n",
            "SPECIFICATION:\n    ID: test.doc\n    NAME: \"T\"\n",
            "    VERSION: \"1.0.0\"\n    KIND: kind.data\n\n{}"
        ),
        body
    )
}

#[test]
fn the_specification_block_is_itself_a_declaration() {
    let source = data("");
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    let decl = resolved
        .declarations()
        .all()
        .iter()
        .find(|d| d.block == "SPECIFICATION")
        .expect("SPECIFICATION declares an ID");
    assert_eq!(decl.id.qualified(), "test.doc");
    assert_eq!(decl.id.namespace_path.len(), 0);
    assert_eq!(resolved.outcome(), Outcome::Resolved);
}

#[test]
fn every_top_level_declaring_block_is_indexed() {
    let source = data("DATA:\n    ID: data.one\n    TYPE: INTEGER\n    VALUE: 1\n\nDATA:\n    ID: data.two\n    TYPE: INTEGER\n    VALUE: 2\n");
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    let mut found: Vec<&str> = resolved
        .declarations()
        .all()
        .iter()
        .map(|d| d.id.local.as_str())
        .collect();
    found.sort_unstable();
    assert_eq!(found, ["data.one", "data.two", "test.doc"]);
    assert_eq!(resolved.outcome(), Outcome::Resolved);
}

#[test]
fn a_define_declaration_records_its_kind() {
    let source = data("DEFINE:\n    ID: type.counter\n    KIND: kind.type\n    BASE: INTEGER\n");
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    let decl = resolved
        .declarations()
        .all()
        .iter()
        .find(|d| d.block == "DEFINE")
        .expect("DEFINE indexed");
    assert_eq!(decl.definition_kind.as_deref(), Some("kind.type"));
}

#[test]
fn nested_declarations_are_indexed_with_their_parent() {
    // A STEP inside a SEQUENCE, and an ACTION inside that STEP, are
    // declarations in their own right: both blocks carry an ID field.
    let source = task(concat!(
        "SEQUENCE:\n    ID: sequence.s\n",
        "    STEP:\n        ID: step.one\n",
        "        ACTION:\n            ID: action.one\n            OPERATION: core.inspect\n",
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    let all = resolved.declarations().all();
    let index = |local: &str| {
        all.iter()
            .position(|d| d.id.local == local)
            .unwrap_or_else(|| panic!("{local} indexed"))
    };
    let sequence = index("sequence.s");
    let step = index("step.one");
    let action = index("action.one");
    assert_eq!(all[step].parent, Some(sequence));
    assert_eq!(all[action].parent, Some(step));
    assert_eq!(all[sequence].parent, None);
    assert_eq!(all[action].block, "ACTION");
}

#[test]
fn declarations_inside_a_loop_body_are_indexed() {
    // 08_EXAMPLES/VALID/05 writes its STEP and ACTION inside a FOR EACH body.
    let source = task(concat!(
        "INPUT:\n    ID: input.files\n    TYPE: LIST[PATH]\n    VALUE: [PATH(\"/tmp/a\")]\n\n",
        "SEQUENCE:\n    ID: sequence.s\n",
        "    FOR EACH file IN REF(input.files):\n",
        "        STEP:\n            ID: step.one\n",
        "            ACTION:\n                ID: action.one\n                OPERATION: core.inspect\n                TARGET: REF(file)\n",
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    let locals: Vec<&str> = resolved
        .declarations()
        .all()
        .iter()
        .map(|d| d.id.local.as_str())
        .collect();
    assert!(locals.contains(&"step.one"), "got {locals:?}");
    assert!(locals.contains(&"action.one"), "got {locals:?}");
}

#[test]
fn declarations_in_both_branches_of_a_conditional_are_indexed() {
    // 05_SEMANTICS/01: "Include both IF branches ... as source templates for
    // static checking." Neither condition is evaluated to decide membership.
    let source = task(concat!(
        "SEQUENCE:\n    ID: sequence.s\n",
        "    IF (TRUE) THEN:\n",
        "        STEP:\n            ID: step.then\n",
        "            ACTION:\n                ID: action.then\n                OPERATION: core.inspect\n",
        "    ELSE:\n",
        "        STEP:\n            ID: step.else\n",
        "            ACTION:\n                ID: action.else\n                OPERATION: core.inspect\n",
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    let locals: Vec<&str> = resolved
        .declarations()
        .all()
        .iter()
        .map(|d| d.id.local.as_str())
        .collect();
    for expected in ["step.then", "action.then", "step.else", "action.else"] {
        assert!(
            locals.contains(&expected),
            "{expected} missing from {locals:?}"
        );
    }
}

#[test]
fn a_duplicate_id_is_rejected() {
    // 08_EXAMPLES/INVALID/05_DUPLICATE_ID.invalid.lcl, in essence.
    let source = data("DATA:\n    ID: data.same\n    TYPE: INTEGER\n    VALUE: 1\n\nDATA:\n    ID: data.same\n    TYPE: INTEGER\n    VALUE: 2\n");
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), ["error.id.duplicate"]);
    assert_eq!(resolved.terminal_status(), Some("status.invalid"));
    assert_eq!(resolved.outcome(), Outcome::Rejected);
    // The locus is the *second* declaration, not the first.
    let primary = resolved.primary().expect("primary");
    let first_offset = source.find("data.same").expect("first");
    assert!(primary.span.start > first_offset);
}

#[test]
fn three_declarations_of_one_id_report_each_repeat() {
    // multiplicity_rule: "Emit every independent applicable diagnostic ...
    // Distinct source loci ... are independent even when their identifiers
    // match."
    let source = data(concat!(
        "DATA:\n    ID: data.same\n    TYPE: INTEGER\n    VALUE: 1\n\n",
        "DATA:\n    ID: data.same\n    TYPE: INTEGER\n    VALUE: 2\n\n",
        "DATA:\n    ID: data.same\n    TYPE: INTEGER\n    VALUE: 3\n",
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), ["error.id.duplicate", "error.id.duplicate"]);
}

#[test]
fn a_reserved_prefix_declaration_id_is_rejected() {
    // 02_LEXICAL/03: "User identifiers cannot begin with those namespaces."
    let source = data("DATA:\n    ID: status.mine\n    TYPE: INTEGER\n    VALUE: 1\n");
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), ["error.namespace.invalid"]);
    assert_eq!(resolved.terminal_status(), Some("status.invalid"));
}

#[test]
fn every_reserved_namespace_is_refused_as_a_first_segment() {
    for reserved in [
        "core", "encoding", "error", "event", "format", "kind", "mode", "status", "unit",
    ] {
        let source = data(&format!(
            "DATA:\n    ID: {reserved}.mine\n    TYPE: INTEGER\n    VALUE: 1\n"
        ));
        let resolved = resolve(&source);
        assert!(
            ids(&resolved).contains(&"error.namespace.invalid".to_string()),
            "{reserved} must be refused, got {:?}",
            ids(&resolved)
        );
    }
}

#[test]
fn a_lowercase_keyword_spelling_remains_a_legal_id() {
    // 02_LEXICAL/03: "count, status, and true do not become COUNT, STATUS, or
    // TRUE." Only the reserved *namespace* rule applies, and `counter` is not
    // one of the nine.
    let source = data("DATA:\n    ID: counter.value\n    TYPE: INTEGER\n    VALUE: 1\n");
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
}

#[test]
fn a_reserved_word_as_a_later_segment_is_legal() {
    // Only the *first* segment is governed: "User identifiers cannot begin
    // with those namespaces."
    let source = data("DATA:\n    ID: my.status\n    TYPE: INTEGER\n    VALUE: 1\n");
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
}
