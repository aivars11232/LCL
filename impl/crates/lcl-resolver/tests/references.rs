//! REF binding: identity, kind, scope and cycles.

mod common;

use common::{assert_stages_clean, canonical_root, ids, resolve, resolve_with};
use lcl_resolver::{BindingTarget, Outcome};

fn data(body: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: test.doc\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n\n{body}"
    )
}

fn task(body: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: test.doc\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n{body}"
    )
}

#[test]
fn every_canonical_valid_example_resolves_cleanly() {
    let dir = canonical_root().join("08_EXAMPLES/VALID");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("examples present")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".lcl"))
        .collect();
    entries.sort();
    assert_eq!(entries.len(), 13);

    // Every example, keyed by its own filename, so the one import in the set
    // resolves as a sibling exactly as it is written.
    let bodies: Vec<(String, String)> = entries
        .iter()
        .map(|name| {
            (
                name.clone(),
                std::fs::read_to_string(dir.join(name)).expect("readable"),
            )
        })
        .collect();
    let units: Vec<(&str, &str)> = bodies
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    for (name, body) in &bodies {
        let mut provider = lcl_resolver::MemoryProvider::new();
        for (key, unit_body) in &units {
            provider.insert(*key, unit_body.as_bytes());
        }
        let resolved = common::resolver()
            .resolve(
                &lcl_resolver::SourceUnit::new(
                    lcl_resolver::SourceId::new(name.clone()),
                    body.as_bytes(),
                ),
                &provider,
            )
            .expect("earlier stages pass");
        assert_eq!(
            ids(&resolved),
            Vec::<String>::new(),
            "{name} must resolve cleanly"
        );
        assert_eq!(resolved.outcome(), Outcome::Resolved, "{name}");
    }
    let _ = resolve_with;
}

#[test]
fn an_unresolved_reference_is_rejected() {
    // 08_EXAMPLES/INVALID/06_UNRESOLVED_REFERENCE.invalid.lcl
    let source = std::fs::read_to_string(
        canonical_root().join("08_EXAMPLES/INVALID/06_UNRESOLVED_REFERENCE.invalid.lcl"),
    )
    .expect("example present");
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), ["error.reference.unresolved"]);
    assert_eq!(resolved.terminal_status(), Some("status.invalid"));
}

#[test]
fn a_reference_binds_to_its_exact_declaration() {
    let source = data(concat!(
        "DATA:\n    ID: data.one\n    TYPE: INTEGER\n    VALUE: 1\n\n",
        "DATA:\n    ID: data.two\n    TYPE: INTEGER\n    VALUE: REF(data.one)\n",
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    let binding = resolved
        .bindings()
        .iter()
        .find(|b| b.text == "data.one")
        .expect("bound");
    assert!(matches!(binding.target, BindingTarget::Declaration(_)));
    assert_eq!(
        binding.resolved_id.as_ref().map(|i| i.qualified()),
        Some("data.one".to_string())
    );
}

#[test]
fn a_reference_of_the_wrong_kind_is_rejected() {
    // EXECUTE.REFERENCE is reference(TASK|PHASE|SEQUENCE|ACTION|TEST); a DATA
    // declaration is none of them.
    let source = task(concat!(
        "DATA:\n    ID: data.one\n    TYPE: INTEGER\n    VALUE: 1\n\n",
        "EXECUTE:\n    REFERENCE: REF(data.one)\n",
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), ["error.reference.kind"]);
    let primary = resolved.primary().expect("primary");
    assert!(primary.detail.as_ref().expect("detail").contains("DATA"));
}

#[test]
fn a_reference_inside_an_operator_is_not_kind_checked_by_the_slot() {
    // context_boundary: a receiving identity context "does not change the
    // evaluation rules inside another operator, function call, property
    // access, or index access". The reference must still bind.
    let source = task(concat!(
        "INPUT:\n    ID: input.n\n    TYPE: INTEGER\n    VALUE: 2\n\n",
        "GOAL:\n    ID: goal.g\n    ASSERT: COUNT(REF(input.n)) == 2\n\n",
        "ACTION:\n    ID: action.a\n    OPERATION: core.inspect\n    TARGET: REF(input.n)\n\n",
        "VERIFY:\n    ID: verify.v\n    ASSERT: TRUE\n\n",
        "SUCCESS:\n    ID: success.s\n    ALL: [REF(verify.v)]\n\n",
        "TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n    ACTION: REF(action.a)\n    SUCCESS: REF(success.s)\n\n",
        "EXECUTE:\n    REFERENCE: REF(task.t)\n",
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    assert!(resolved.bindings().iter().any(|b| b.text == "input.n"));
}

#[test]
fn a_loop_local_binding_resolves_inside_its_body_only() {
    // 02_LEXICAL/03: "Loop-local identifiers exist only in their FOR EACH body."
    let inside = task(concat!(
        "INPUT:\n    ID: input.files\n    TYPE: LIST[PATH]\n    VALUE: [PATH(\"/tmp/a\")]\n\n",
        "SEQUENCE:\n    ID: sequence.s\n",
        "    FOR EACH file IN REF(input.files):\n",
        "        STEP:\n            ID: step.one\n",
        "            ACTION:\n                ID: action.one\n                OPERATION: core.inspect\n                TARGET: REF(file)\n\n",
        "EXECUTE:\n    REFERENCE: REF(sequence.s)\n",
    ));
    assert_stages_clean(&inside);
    let resolved = resolve(&inside);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    let binding = resolved
        .bindings()
        .iter()
        .find(|b| b.text == "file")
        .expect("the loop local is bound");
    assert!(matches!(binding.target, BindingTarget::LoopLocal { .. }));
}

#[test]
fn a_loop_local_does_not_resolve_outside_its_body() {
    let outside = task(concat!(
        "INPUT:\n    ID: input.files\n    TYPE: LIST[PATH]\n    VALUE: [PATH(\"/tmp/a\")]\n\n",
        "SEQUENCE:\n    ID: sequence.s\n",
        "    FOR EACH file IN REF(input.files):\n",
        "        STEP:\n            ID: step.one\n",
        "            ACTION:\n                ID: action.one\n                OPERATION: core.inspect\n                TARGET: REF(input.files)\n\n",
        "ACTION:\n    ID: action.outside\n    OPERATION: core.inspect\n    TARGET: REF(file)\n\n",
        "EXECUTE:\n    REFERENCE: REF(sequence.s)\n",
    ));
    assert_stages_clean(&outside);
    let resolved = resolve(&outside);
    assert_eq!(ids(&resolved), ["error.reference.unresolved"]);
}

#[test]
fn an_imported_declaration_is_referenced_through_its_prefix() {
    let lib = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: lib.doc\n    NAME: \"L\"\n    VERSION: \"1.0.0\"\n    KIND: kind.library\n\nDATA:\n    ID: data.value\n    TYPE: INTEGER\n    VALUE: 1\n";
    let root = data(concat!(
        "IMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n\n",
        "DATA:\n    ID: data.copy\n    TYPE: INTEGER\n    VALUE: REF(lib.data.value)\n",
    ));
    let resolved = resolve_with(&root, &[("lib.lcl", lib)]);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    let binding = resolved
        .bindings()
        .iter()
        .find(|b| b.text == "lib.data.value")
        .expect("bound");
    assert_eq!(
        binding.resolved_id.as_ref().map(|i| i.qualified()),
        Some("lib.data.value".to_string())
    );
}

#[test]
fn no_unqualified_reference_searches_an_imported_namespace() {
    // 07/02: "No unqualified reference searches imported namespaces."
    let lib = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: lib.doc\n    NAME: \"L\"\n    VERSION: \"1.0.0\"\n    KIND: kind.library\n\nDATA:\n    ID: data.value\n    TYPE: INTEGER\n    VALUE: 1\n";
    let root = data(concat!(
        "IMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n\n",
        "DATA:\n    ID: data.copy\n    TYPE: INTEGER\n    VALUE: REF(data.value)\n",
    ));
    let resolved = resolve_with(&root, &[("lib.lcl", lib)]);
    assert_eq!(ids(&resolved), ["error.reference.unresolved"]);
}

#[test]
fn a_reference_into_a_failed_import_is_not_reported_twice() {
    // multiplicity_rule emits every *independent* diagnostic. A reference that
    // is unresolvable only because its source never loaded is not independent
    // of the import failure that caused it.
    let root = data(concat!(
        "IMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"gone.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n\n",
        "DATA:\n    ID: data.copy\n    TYPE: INTEGER\n    VALUE: REF(lib.data.value)\n",
    ));
    let resolved = resolve(&root);
    assert_eq!(ids(&resolved), ["error.import.not_found"]);
}

#[test]
fn a_core_operation_binds_and_an_unknown_one_does_not() {
    let good = data("DEFINE:\n    ID: def.one\n    KIND: kind.term\n    MEANING: \"x\"\n");
    assert_stages_clean(&good);

    let source = task(concat!(
        "ACTION:\n    ID: action.a\n    OPERATION: core.inspect\n\n",
        "GOAL:\n    ID: goal.g\n    ASSERT: TRUE\n\n",
        "VERIFY:\n    ID: verify.v\n    ASSERT: TRUE\n\n",
        "SUCCESS:\n    ID: success.s\n    ALL: [REF(verify.v)]\n\n",
        "TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n    ACTION: REF(action.a)\n    SUCCESS: REF(success.s)\n\n",
        "EXECUTE:\n    REFERENCE: REF(task.t)\n",
    ));
    assert_stages_clean(&source);
    assert_eq!(ids(&resolve(&source)), Vec::<String>::new());

    let unknown = source.replace("core.inspect", "core.teleport");
    let resolved = resolve(&unknown);
    assert_eq!(ids(&resolved), ["error.operation.undefined"]);
}

#[test]
fn a_defined_operation_binds() {
    // 08_EXAMPLES/VALID/07 names a local DEFINE kind.operation from an ACTION.
    let source = std::fs::read_to_string(
        canonical_root().join("08_EXAMPLES/VALID/07_DOMAIN_EXTENSION_OPERATION.lcl"),
    )
    .expect("example present");
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
}

#[test]
fn an_alias_base_resolves_to_a_core_identifier() {
    let source = data(
        "DEFINE:\n    ID: my.status\n    KIND: kind.status\n    MEANING: \"Mine.\"\n    BASE: status.failed\n",
    );
    assert_stages_clean(&source);
    assert_eq!(ids(&resolve(&source)), Vec::<String>::new());
}

#[test]
fn an_alias_chain_resolves_transitively() {
    let source = data(concat!(
        "DEFINE:\n    ID: my.first\n    KIND: kind.status\n    MEANING: \"One.\"\n    BASE: status.failed\n\n",
        "DEFINE:\n    ID: my.second\n    KIND: kind.status\n    MEANING: \"Two.\"\n    BASE: my.first\n",
    ));
    assert_stages_clean(&source);
    assert_eq!(ids(&resolve(&source)), Vec::<String>::new());
}

#[test]
fn an_unresolved_alias_base_is_rejected() {
    let source = data(
        "DEFINE:\n    ID: my.status\n    KIND: kind.status\n    MEANING: \"Mine.\"\n    BASE: status.nonexistent\n",
    );
    assert_stages_clean(&source);
    assert_eq!(ids(&resolve(&source)), ["error.reference.unresolved"]);
}

#[test]
fn an_alias_base_in_the_wrong_domain_is_rejected() {
    // 07/03: "A BASE that resolves to the wrong domain or DEFINE KIND uses
    // error.reference.kind."
    let source = data(concat!(
        "DEFINE:\n    ID: my.err\n    KIND: kind.error\n    MEANING: \"E.\"\n    BASE: error.value.unknown\n\n",
        "DEFINE:\n    ID: my.status\n    KIND: kind.status\n    MEANING: \"S.\"\n    BASE: my.err\n",
    ));
    assert_stages_clean(&source);
    assert_eq!(ids(&resolve(&source)), ["error.reference.kind"]);
}

#[test]
fn an_alias_cycle_is_rejected() {
    // "Resolve the BASE chain transitively and acyclically."
    let source = data(concat!(
        "DEFINE:\n    ID: my.first\n    KIND: kind.status\n    MEANING: \"One.\"\n    BASE: my.second\n\n",
        "DEFINE:\n    ID: my.second\n    KIND: kind.status\n    MEANING: \"Two.\"\n    BASE: my.first\n",
    ));
    assert_stages_clean(&source);
    let got = ids(&resolve(&source));
    assert!(
        got.iter().all(|id| id == "error.reference.cycle"),
        "got {got:?}"
    );
    assert!(!got.is_empty());
}

#[test]
fn a_handler_fallback_cycle_is_rejected() {
    // Decision witness CLOSURE-028.
    let source = task(concat!(
        "HANDLER:\n    ID: handler.a\n    EVENT: event.failure\n    OPERATION: core.stop\n    FALLBACK: REF(handler.b)\n\n",
        "HANDLER:\n    ID: handler.b\n    EVENT: event.failure\n    OPERATION: core.stop\n    FALLBACK: REF(handler.a)\n\n",
        "GOAL:\n    ID: goal.g\n    ASSERT: TRUE\n\n",
        "ACTION:\n    ID: action.a\n    OPERATION: core.inspect\n\n",
        "VERIFY:\n    ID: verify.v\n    ASSERT: TRUE\n\n",
        "SUCCESS:\n    ID: success.s\n    ALL: [REF(verify.v)]\n\n",
        "TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n    ACTION: REF(action.a)\n    SUCCESS: REF(success.s)\n    HANDLER: [REF(handler.a), REF(handler.b)]\n\n",
        "EXECUTE:\n    REFERENCE: REF(task.t)\n",
    ));
    assert_stages_clean(&source);
    let got = ids(&resolve(&source));
    assert!(
        got.contains(&"error.reference.cycle".to_string()),
        "got {got:?}"
    );
}

#[test]
fn a_defined_type_reference_must_name_a_kind_type() {
    // #/source_type_contract: "a defined type is REF(identifier) resolving to
    // DEFINE kind.type, never a bare identifier."
    let good = data(concat!(
        "DEFINE:\n    ID: type.counter\n    KIND: kind.type\n    BASE: INTEGER\n\n",
        "DATA:\n    ID: data.n\n    TYPE: REF(type.counter)\n    VALUE: 1\n",
    ));
    assert_stages_clean(&good);
    assert_eq!(ids(&resolve(&good)), Vec::<String>::new());

    let bad = data(concat!(
        "DEFINE:\n    ID: term.counter\n    KIND: kind.term\n    MEANING: \"x\"\n\n",
        "DATA:\n    ID: data.n\n    TYPE: REF(term.counter)\n    VALUE: 1\n",
    ));
    assert_stages_clean(&bad);
    assert_eq!(ids(&resolve(&bad)), ["error.reference.kind"]);
}

/// One `core.calculate` document whose fragment is `expression`.
fn calculating(expression: &str, extra_parameters: &str) -> String {
    task(&format!(
        concat!(
            "DATA:\n    ID: data.number\n    TYPE: INTEGER\n    VALUE: 3\n\n",
            "GOAL:\n    ID: goal.g\n    ASSERT: TRUE\n\n",
            "ACTION:\n    ID: action.a\n    OPERATION: core.calculate\n",
            "    PARAMETER:\n        NAME: expression\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: {}\n{}",
            "\nSUCCESS:\n    ID: success.s\n    ALL: TRUE\n\n",
            "TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n    ACTION: REF(action.a)\n    SUCCESS: REF(success.s)\n\n",
            "EXECUTE:\n    REFERENCE: REF(task.t)\n",
        ),
        expression, extra_parameters
    ))
}

/// Names inside an expression fragment resolve where every other reference
/// does.
///
/// `operations_v0.1.0.json#/expression_fragment_contract/environment`: "REF
/// references resolve in the enclosing document … Bare names in fragments
/// denote only declared local bindings, reserved target or item where provided,
/// or contextual enum/qualified-identifier data. They never implicitly read a
/// document declaration; use REF for that value read. Bindings are immutable
/// snapshots; an unknown binding name produces error.reference.unresolved."
///
/// `#/evaluation`: "Static checks cover the complete fragment", and the
/// identifier's registered stage is `resolution`.
#[test]
fn an_unknown_bare_name_inside_a_fragment_is_unresolved() {
    let source = calculating("\"ghost + 1\"", "");
    assert_stages_clean(&source);
    assert_eq!(ids(&resolve(&source)), ["error.reference.unresolved"]);
}

#[test]
fn an_unresolved_ref_written_inside_a_fragment_is_unresolved() {
    let source = calculating("\"REF(data.absent) + 1\"", "");
    assert_stages_clean(&source);
    assert_eq!(ids(&resolve(&source)), ["error.reference.unresolved"]);
}

#[test]
fn a_fragment_reading_declared_and_bound_names_resolves() {
    // The controls: a REF to a declaration that exists, a declared local
    // binding, the reserved `target`, and contextual qualified-identifier data.
    let bindings = concat!(
        "    PARAMETER:\n        NAME: bindings\n        TYPE: OBJECT\n        REQUIRED: FALSE\n",
        "        VALUE:\n            amount: 2\n",
    );
    for expression in [
        "\"REF(data.number) + 1\"",
        "\"amount + 1\"",
        "\"target\"",
        "\"format.plain_text\"",
    ] {
        let source = calculating(expression, bindings);
        assert_stages_clean(&source);
        assert_eq!(
            ids(&resolve(&source)),
            Vec::<String>::new(),
            "{expression} must resolve"
        );
    }
}

#[test]
fn a_predicate_fragment_binds_item_and_refuses_an_unknown_name() {
    let filtering = |predicate: &str| {
        task(&format!(
            concat!(
                "DATA:\n    ID: data.members\n    TYPE: LIST[INTEGER]\n    VALUE: [1, 2]\n\n",
                "GOAL:\n    ID: goal.g\n    ASSERT: TRUE\n\n",
                "ACTION:\n    ID: action.a\n    OPERATION: core.filter\n    TARGET: REF(data.members)\n",
                "    PARAMETER:\n        NAME: predicate\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: {predicate}\n\n",
                "SUCCESS:\n    ID: success.s\n    ALL: TRUE\n\n",
                "TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n    ACTION: REF(action.a)\n    SUCCESS: REF(success.s)\n\n",
                "EXECUTE:\n    REFERENCE: REF(task.t)\n",
            ),
            predicate = predicate
        ))
    };
    let good = filtering("\"item > 1\"");
    assert_stages_clean(&good);
    assert_eq!(ids(&resolve(&good)), Vec::<String>::new());

    let bad = filtering("\"other > 1\"");
    assert_stages_clean(&bad);
    assert_eq!(ids(&resolve(&bad)), ["error.reference.unresolved"]);
}
