//! The resolution vocabulary is the registry's, re-derived independently here.
//!
//! Every assertion recomputes its expectation from the canonical JSON rather
//! than restating a number this crate also hard-codes, so a registry change
//! fails the test instead of being mirrored by it.

mod common;

use common::{grammar, rules, spec};
use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_resolver::rules::{RefTarget, Rules};
use lcl_resolver::{ResolutionError, DEFERRED};
use lcl_spec::json::Json;
use std::collections::BTreeSet;

/// The registry object at a pointer, as a slice of key/value pairs.
fn object<'a>(root: &'a Json, path: &[&str]) -> &'a [(String, Json)] {
    let mut node = root;
    for key in path {
        node = node.get(key).unwrap_or_else(|| panic!("missing {key}"));
    }
    node.as_object().expect("object")
}

fn array_strings(root: &Json, path: &[&str]) -> BTreeSet<String> {
    let mut node = root;
    for key in path {
        node = node.get(key).unwrap_or_else(|| panic!("missing {key}"));
    }
    node.as_array()
        .expect("array")
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_string)
        .collect()
}

#[test]
fn mirrored_error_set_equals_the_registry_resolution_stage() {
    let registry = DiagnosticRegistry::load(spec()).expect("diagnostics load");
    let registered: BTreeSet<String> = registry
        .errors_by_stage(Stage::Resolution)
        .into_iter()
        .map(|e| e.id.clone())
        .collect();
    let mirrored: BTreeSet<String> = ResolutionError::ALL
        .iter()
        .map(|e| e.as_registry_str().to_string())
        .collect();
    assert_eq!(
        mirrored, registered,
        "the mirrored enum must be exactly the registry's resolution-stage set"
    );
    assert_eq!(registered.len(), 14);
}

#[test]
fn deferred_identifiers_are_named_with_their_owner() {
    assert_eq!(DEFERRED.len(), 2);
    let deferred: BTreeSet<&str> = DEFERRED.iter().map(|(e, _)| e.as_registry_str()).collect();
    assert_eq!(
        deferred,
        ["error.conflict.hard", "error.override.invalid"]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
    for (_, owner) in DEFERRED {
        assert!(
            !owner.is_empty(),
            "a deferred identifier must name its owner"
        );
    }
    // The emitted set is the complement, and nothing is in both.
    let emitted: BTreeSet<&str> = ResolutionError::emitted()
        .map(|e| e.as_registry_str())
        .collect();
    assert_eq!(emitted.len(), 12);
    assert!(emitted.is_disjoint(&deferred));
}

#[test]
fn error_metadata_is_copied_verbatim_from_the_registry() {
    let registry = DiagnosticRegistry::load(spec()).expect("diagnostics load");
    for id in ResolutionError::ALL {
        let def = registry
            .error(id.as_registry_str())
            .expect("registered identifier");
        let ours = rules().error(id);
        assert_eq!(ours.meaning, def.meaning, "{id} meaning");
        assert_eq!(
            ours.default_status, def.default_status,
            "{id} default_status"
        );
        assert_eq!(def.stage, Stage::Resolution, "{id} stage");
    }
}

#[test]
fn specificity_ranks_match_the_selection_contract() {
    let errors = spec().registry("statuses_and_errors").expect("registry");
    let default = errors
        .get("diagnostic_selection")
        .and_then(|d| d.get("specificity_rank"))
        .and_then(|s| s.get("default_for_every_error"))
        .and_then(Json::as_u64)
        .expect("default rank");
    let overrides = object(
        errors,
        &["diagnostic_selection", "specificity_rank", "overrides"],
    );
    for id in ResolutionError::ALL {
        let expected = overrides
            .iter()
            .find(|(k, _)| k == id.as_registry_str())
            .and_then(|(_, v)| v.as_u64())
            .unwrap_or(default);
        assert_eq!(rules().error(id).specificity_rank, expected, "{id}");
    }
}

#[test]
fn reserved_namespaces_come_from_the_registry() {
    let groups = spec()
        .registry("built_in_groups_and_results")
        .expect("registry");
    let expected = array_strings(groups, &["reserved_namespaces"]);
    let ours: BTreeSet<String> = rules().reserved_namespaces().map(str::to_string).collect();
    assert_eq!(ours, expected);
    // The nine of 02_LEXICAL/03, named so a silent registry change is visible.
    assert_eq!(ours.len(), 9);
    for name in [
        "core", "encoding", "error", "event", "format", "kind", "mode", "status", "unit",
    ] {
        assert!(rules().is_reserved_namespace(name), "{name} is reserved");
    }
    assert!(!rules().is_reserved_namespace("library"));
}

#[test]
fn core_operation_ids_come_from_the_registry() {
    let groups = spec()
        .registry("built_in_groups_and_results")
        .expect("registry");
    let expected = array_strings(groups, &["core_operation_ids"]);
    assert_eq!(rules().core_operation_count(), expected.len());
    for id in &expected {
        assert!(rules().is_core_operation(id), "{id}");
    }
    assert!(!rules().is_core_operation("image.generate"));
}

#[test]
fn the_exact_lcl_version_is_the_block_schema_argument() {
    // `field_signatures#/blocks/LCL/fields/VERSION/value_kind` is
    // `exact_string("0.1.0")`. That argument, not a constant in this crate, is
    // what `error.version.unsupported` compares against.
    let fs = spec().registry("field_signatures").expect("registry");
    let value_kind = fs
        .get("blocks")
        .and_then(|b| b.get("LCL"))
        .and_then(|b| b.get("fields"))
        .and_then(|f| f.get("VERSION"))
        .and_then(|v| v.get("value_kind"))
        .and_then(Json::as_str)
        .expect("LCL.VERSION value kind");
    let expected = value_kind
        .trim_start_matches("exact_string(\"")
        .trim_end_matches("\")");
    assert_eq!(rules().lcl_version(), expected);
    assert_eq!(rules().lcl_version(), spec().formal_version());
}

#[test]
fn reference_slot_targets_are_the_registry_union() {
    // EXECUTE.REFERENCE is `reference(TASK|PHASE|SEQUENCE|ACTION|TEST)`.
    let slot = rules()
        .reference_slot("EXECUTE", "REFERENCE")
        .expect("EXECUTE.REFERENCE is a reference slot");
    let expected: BTreeSet<RefTarget> = ["TASK", "PHASE", "SEQUENCE", "ACTION", "TEST"]
        .into_iter()
        .map(|b| RefTarget::Block(b.to_string()))
        .collect();
    assert_eq!(slot.targets, expected);
    assert!(slot.accepts("TASK", None));
    assert!(!slot.accepts("DATA", None));
}

#[test]
fn reference_domains_expand_to_their_registered_members() {
    let meta = spec().registry("semantic_meta_types").expect("registry");
    for domain in ["execution_unit", "rule_clause"] {
        let members = array_strings(meta, &["reference_domains", domain, "members"]);
        assert_eq!(
            rules()
                .reference_domain(domain)
                .cloned()
                .unwrap_or_default(),
            members,
            "{domain}"
        );
    }
    // PHASE.BEFORE is `reference_or_list(execution_unit)`, so its targets are
    // the domain's exact members.
    let slot = rules().reference_slot("PHASE", "BEFORE").expect("slot");
    let expected: BTreeSet<RefTarget> =
        array_strings(meta, &["reference_domains", "execution_unit", "members"])
            .into_iter()
            .map(RefTarget::Block)
            .collect();
    assert_eq!(slot.targets, expected);

    // OVERRIDE.WINNER is `reference(rule_clause)`.
    let slot = rules().reference_slot("OVERRIDE", "WINNER").expect("slot");
    let expected: BTreeSet<RefTarget> =
        array_strings(meta, &["reference_domains", "rule_clause", "members"])
            .into_iter()
            .map(RefTarget::Block)
            .collect();
    assert_eq!(slot.targets, expected);
}

#[test]
fn every_reference_value_kind_yields_a_slot() {
    // Independently re-derive the set of reference-receiving fields from the
    // registry and require the loaded rules to cover exactly it.
    let fs = spec().registry("field_signatures").expect("registry");
    let mut expected = 0usize;
    for (block, body) in object(fs, &["blocks"]) {
        for (field, sig) in object(body, &["fields"]) {
            let kind = sig
                .get("value_kind")
                .and_then(Json::as_str)
                .expect("value_kind");
            let receives_reference = kind.starts_with("reference")
                || matches!(
                    kind,
                    "type_expression"
                        | "schema_reference_or_nested_schema"
                        | "string_uri_or_evidence_reference"
                        | "operation_identifier_or_handler_reference"
                );
            if receives_reference {
                expected += 1;
                assert!(
                    rules().reference_slot(block, field).is_some(),
                    "{block}.{field} ({kind}) must be a reference slot"
                );
            }
        }
    }
    assert_eq!(rules().reference_slot_count(), expected);
}

#[test]
fn defined_type_slots_require_a_kind_type_definition() {
    let slot = rules().reference_slot("OUTPUT", "TYPE").expect("slot");
    assert!(slot.accepts("DEFINE", Some("kind.type")));
    assert!(!slot.accepts("DEFINE", Some("kind.constant")));
    assert!(!slot.accepts("DATA", None));
}

#[test]
fn the_bare_reference_kind_accepts_any_declaration() {
    // DEPENDENCY.REFERENCE is the bare `reference` kind: "REF(identifier)
    // resolving exactly once", with no target restriction.
    let slot = rules()
        .reference_slot("DEPENDENCY", "REFERENCE")
        .expect("slot");
    assert!(slot.accepts_any());
    assert!(slot.accepts("DATA", None));
    assert!(slot.accepts("ACTION", None));
}

#[test]
fn operation_slots_are_every_operation_identifier_field() {
    let fs = spec().registry("field_signatures").expect("registry");
    for (block, body) in object(fs, &["blocks"]) {
        for (field, sig) in object(body, &["fields"]) {
            let kind = sig
                .get("value_kind")
                .and_then(Json::as_str)
                .expect("value_kind");
            assert_eq!(
                rules().is_operation_slot(block, field),
                kind.starts_with("operation_identifier"),
                "{block}.{field} ({kind})"
            );
        }
    }
}

#[test]
fn declaring_blocks_are_exactly_the_blocks_with_an_id() {
    let expected: BTreeSet<String> = grammar()
        .schemas()
        .filter(|s| s.field("ID").is_some())
        .map(|s| s.name.clone())
        .collect();
    let ours: BTreeSet<String> = rules().declaring_blocks().map(str::to_string).collect();
    assert_eq!(ours, expected);
    assert_eq!(ours.len(), 34);
    assert!(!rules().is_declaring_block("LCL"));
    assert!(rules().is_declaring_block("ACTION"));
}

#[test]
fn extension_blocks_are_the_document_kind_set() {
    let expected = grammar()
        .document_kind_blocks("kind.extension")
        .cloned()
        .expect("kind.extension");
    let ours: BTreeSet<String> = rules().extension_blocks().map(str::to_string).collect();
    assert_eq!(ours, expected);
    // 07_VERSIONING_AND_EXTENSIONS/03: "An extension is kind.extension and may
    // contain IMPORT, DEFINE, DATA, COMMENT, and EXAMPLE only."
    for block in ["IMPORT", "DEFINE", "DATA", "COMMENT", "EXAMPLE"] {
        assert!(rules().extension_permits(block), "{block}");
    }
    for block in ["ACTION", "TASK", "EXECUTE", "SCOPE"] {
        assert!(!rules().extension_permits(block), "{block}");
    }
}

#[test]
fn alias_domains_come_from_the_registry_enum_groups() {
    let groups = spec()
        .registry("built_in_groups_and_results")
        .expect("registry");
    for (kind, group) in [
        ("kind.error", "errors"),
        ("kind.event", "events"),
        ("kind.status", "statuses"),
        ("kind.format", "formats"),
    ] {
        let expected = array_strings(groups, &["enum_groups", group]);
        assert_eq!(
            rules().alias_domain(kind).cloned().unwrap_or_default(),
            expected,
            "{kind}"
        );
    }
}

#[test]
fn an_unverified_package_is_refused() {
    let unverified =
        lcl_spec::SpecPackage::open_unverified(common::canonical_root()).expect("opens");
    assert!(!unverified.is_authoritative());
    let grammar_result = lcl_parser::Grammar::load(&unverified);
    assert!(grammar_result.is_err(), "the grammar refuses it too");
    // Rules cannot even be attempted without a grammar, so refuse on the same
    // ground the other layers do: build a grammar from the approved package and
    // pair it with the unverified one.
    let err = Rules::load(&unverified, grammar()).expect_err("must refuse");
    assert!(
        format!("{err}").contains("only the approved release is normative input"),
        "unexpected: {err}"
    );
}
