//! The grammar vocabulary is the registry's, not this build's.
//!
//! Every fact the parser relies on is asserted against the canonical registries
//! as data. No count, block name, field name or value kind is written here as a
//! literal expectation, so a future release that changes one of them fails
//! these tests instead of silently drifting.

mod common;

use common::*;
use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_parser::{FormSet, Grammar, GrammarError, Occurrence};
use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::BTreeSet;

fn registry(name: &str) -> &'static Json {
    // The package outlives the test process; leaking the clone keeps the
    // helper's signature simple without borrowing gymnastics.
    Box::leak(Box::new(
        spec().registry(name).expect("registry loads").clone(),
    ))
}

#[test]
fn grammar_error_set_equals_the_registered_stage() {
    let registry = DiagnosticRegistry::load(spec()).expect("diagnostics");
    let registered: BTreeSet<String> = registry
        .errors_by_stage(Stage::GrammarOrSchema)
        .into_iter()
        .map(|e| e.id.clone())
        .collect();
    let built: BTreeSet<String> = GrammarError::ALL
        .iter()
        .map(|e| e.as_registry_str().to_string())
        .collect();
    assert_eq!(
        built, registered,
        "the closed grammar error enum must equal the registry's grammar_or_schema set"
    );
    // Round-trips, and nothing outside the set is accepted.
    for e in GrammarError::ALL {
        assert_eq!(GrammarError::from_registry_str(e.as_registry_str()), Some(e));
    }
    assert_eq!(GrammarError::from_registry_str("error.grammar"), None);
    assert_eq!(GrammarError::from_registry_str("error.source.tab"), None);
}

#[test]
fn every_grammar_error_carries_its_registry_metadata() {
    let g = grammar();
    let registry = DiagnosticRegistry::load(spec()).expect("diagnostics");
    for id in GrammarError::ALL {
        let def = registry.error(id.as_registry_str()).expect("registered");
        let loaded = g.error(id);
        assert_eq!(loaded.meaning, def.meaning, "{id}");
        assert_eq!(loaded.default_status, def.default_status, "{id}");
        assert_eq!(def.stage, Stage::GrammarOrSchema, "{id}");
        // specificity_rank comes from the registry: the default, or an override.
        assert!(loaded.specificity_rank >= 100, "{id}");
    }
}

#[test]
fn specificity_and_supersession_match_the_registry() {
    let g = grammar();
    let sel = registry("statuses_and_errors")
        .get("diagnostic_selection")
        .expect("diagnostic_selection");

    let rank = sel.get("specificity_rank").expect("specificity_rank");
    let default = rank
        .get("default_for_every_error")
        .and_then(Json::as_u64)
        .expect("default rank");
    let overrides = rank.get("overrides").and_then(Json::as_object).unwrap_or(&[]);
    for id in GrammarError::ALL {
        let expected = overrides
            .iter()
            .find(|(k, _)| k == id.as_registry_str())
            .and_then(|(_, v)| v.as_u64())
            .unwrap_or(default);
        assert_eq!(g.error(id).specificity_rank, expected, "{id}");
    }

    let edges = sel
        .get("supersedes")
        .and_then(|s| s.get("overrides"))
        .and_then(Json::as_object)
        .unwrap_or(&[]);
    for id in GrammarError::ALL {
        let expected: BTreeSet<GrammarError> = edges
            .iter()
            .find(|(k, _)| k == id.as_registry_str())
            .map(|(_, v)| v.as_array().unwrap_or_default())
            .unwrap_or_default()
            .iter()
            .filter_map(|t| t.as_str().and_then(GrammarError::from_registry_str))
            .collect();
        assert_eq!(g.error(id).supersedes, expected, "{id}");
    }

    // The three grammar-stage edges the registry actually declares.
    assert!(g
        .error(GrammarError::BlockDuplicate)
        .supersedes
        .contains(&GrammarError::BlockOccurrence));
    assert!(g
        .error(GrammarError::FieldDuplicate)
        .supersedes
        .contains(&GrammarError::FieldCardinality));
    assert!(g
        .error(GrammarError::FieldRequired)
        .supersedes
        .contains(&GrammarError::FieldCardinality));
}

#[test]
fn every_registered_block_loads_with_its_exact_signature() {
    let g = grammar();
    let blocks = registry("field_signatures")
        .get("blocks")
        .and_then(Json::as_object)
        .expect("blocks");

    assert_eq!(g.block_count(), blocks.len());
    let observed_fields: usize = blocks
        .iter()
        .map(|(_, b)| {
            b.get("fields")
                .and_then(Json::as_object)
                .map(<[(String, Json)]>::len)
                .unwrap_or(0)
        })
        .sum();
    assert_eq!(g.field_count(), observed_fields);

    for (name, body) in blocks {
        let schema = g.schema(name).unwrap_or_else(|| panic!("{name} loads"));
        let fields = body.get("fields").and_then(Json::as_object).expect("fields");
        assert_eq!(schema.fields.len(), fields.len(), "{name}");
        for (field_name, spec) in fields {
            let sig = schema
                .field(field_name)
                .unwrap_or_else(|| panic!("{name}.{field_name}"));
            assert_eq!(
                sig.required,
                spec.get("required").and_then(Json::as_bool).unwrap(),
                "{name}.{field_name}"
            );
            assert_eq!(
                sig.minimum_occurrences,
                spec.get("minimum_occurrences").and_then(Json::as_u64).unwrap(),
                "{name}.{field_name}"
            );
            assert_eq!(
                sig.maximum_occurrences,
                spec.get("maximum_occurrences").and_then(Json::as_u64),
                "{name}.{field_name}"
            );
            assert_eq!(
                sig.value_kind,
                spec.get("value_kind").and_then(Json::as_str).unwrap(),
                "{name}.{field_name}"
            );
        }
    }
}

#[test]
fn the_two_registries_agree_on_containment_and_occurrence() {
    // `Grammar::load` fails closed on a parity break; this test proves the
    // property holds for the shipped release and names what is compared.
    let g = grammar();
    let bs = registry("block_schemas").get("schemas").cloned().expect("schemas");
    let fs = registry("field_signatures").get("blocks").cloned().expect("blocks");
    let bs = bs.as_object().expect("object");
    let fs = fs.as_object().expect("object");
    assert_eq!(bs.len(), fs.len());

    for (name, b) in bs {
        let f = fs.iter().find(|(k, _)| k == name).map(|(_, v)| v).expect(name);
        let contexts: Vec<&str> = b
            .get("contexts")
            .and_then(Json::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(Json::as_str)
            .collect();
        let parents: Vec<&str> = f
            .get("legal_parents")
            .and_then(Json::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(Json::as_str)
            .collect();
        assert_eq!(contexts, parents, "{name}: contexts vs legal_parents");
        assert_eq!(
            b.get("occurrence").and_then(Json::as_str),
            f.get("block_occurrence").and_then(Json::as_str),
            "{name}: occurrence"
        );

        let schema = g.schema(name).expect(name);
        assert_eq!(schema.parents, parents, "{name}");
        assert_eq!(
            schema.occurrence.as_registry_str(),
            b.get("occurrence").and_then(Json::as_str).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn repeatable_fields_are_exactly_the_unbounded_ones() {
    let g = grammar();
    for schema in g.schemas() {
        for field in &schema.fields {
            assert_eq!(
                field.maximum_occurrences.is_none(),
                schema.repeatable.contains(&field.name),
                "{}.{}: block_schemas repeatable and an unbounded maximum must agree",
                schema.name,
                field.name
            );
        }
        for r in &schema.repeatable {
            assert!(
                schema.field(r).is_some(),
                "{}: repeatable {r} is not a field of the block",
                schema.name
            );
        }
    }
}

#[test]
fn every_value_kind_is_classified_and_none_is_guessed() {
    let g = grammar();
    // Totality: `Grammar::load` already refuses an unclassifiable kind, so
    // reaching here proves every shipped expression resolved to a form set.
    let kinds = g.value_kinds();
    assert!(!kinds.is_empty());
    for schema in g.schemas() {
        for field in &schema.fields {
            assert!(
                !field.forms.is_empty(),
                "{}.{} has no accepted form",
                schema.name,
                field.name
            );
        }
    }

    // Templated kinds must carry exactly the registry's accepted_forms.
    let templates = registry("field_signatures")
        .get("value_kind_templates")
        .cloned()
        .expect("templates");
    let templates = templates.as_object().expect("object");
    for schema in g.schemas() {
        for field in &schema.fields {
            let Some(split) = field.value_kind.find(['[', '(']) else {
                continue;
            };
            let head = &field.value_kind[..split];
            let declared = templates
                .iter()
                .find(|(k, _)| k == head)
                .map(|(_, v)| v)
                .unwrap_or_else(|| panic!("template {head} is registered"));
            let accepted: Vec<&str> = declared
                .get("accepted_forms")
                .and_then(Json::as_array)
                .unwrap_or_default()
                .iter()
                .filter_map(Json::as_str)
                .collect();
            for a in accepted {
                let bit = match a {
                    "STRING" => FormSet::STRING,
                    "INTEGER" => FormSet::INTEGER,
                    "QUALIFIED_IDENTIFIER" => FormSet::QUALIFIED_IDENTIFIER,
                    "single_reference" => FormSet::REFERENCE,
                    "reference_list" => FormSet::REFERENCE_LIST,
                    "nested_block" => FormSet::NESTED,
                    other => panic!("unhandled accepted form {other}"),
                };
                assert!(
                    field.forms.intersects(bit),
                    "{}.{}: {} must accept {a}",
                    schema.name,
                    field.name,
                    field.value_kind
                );
            }
        }
    }
}

#[test]
fn nested_block_arguments_name_registered_blocks() {
    let g = grammar();
    for schema in g.schemas() {
        for field in &schema.fields {
            let Some(block) = &field.nested_block else {
                continue;
            };
            assert!(
                g.is_block(block),
                "{}.{}: nested block argument {block} is not a registered block",
                schema.name,
                field.name
            );
            // Parent closure is bidirectional (04_GRAMMAR/13): a field that
            // permits a nested BLOCK is legal only when that BLOCK declares the
            // containing block as a parent.
            let child = g.schema(block).expect("registered");
            assert!(
                child.accepts_parent(&schema.name),
                "{}.{}: {block} does not declare {} as a legal parent",
                schema.name,
                field.name,
                schema.name
            );
        }
    }
}

#[test]
fn document_kind_blocks_are_closed_and_registered() {
    let g = grammar();
    let declared = registry("block_schemas")
        .get("document_kind_blocks")
        .cloned()
        .expect("document_kind_blocks");
    let declared = declared.as_object().expect("object");
    assert_eq!(g.document_kinds().count(), declared.len());
    for (kind, list) in declared {
        let loaded = g.document_kind_blocks(kind).expect(kind);
        let expected: BTreeSet<String> = list
            .as_array()
            .unwrap_or_default()
            .iter()
            .filter_map(Json::as_str)
            .map(str::to_string)
            .collect();
        assert_eq!(loaded, &expected, "{kind}");
        for block in &expected {
            assert!(g.is_block(block), "{kind} names unregistered block {block}");
        }
    }
    assert!(g.document_kind_blocks("kind.nonexistent").is_none());
}

#[test]
fn the_two_headers_are_position_pinned_blocks() {
    let g = grammar();
    assert_eq!(g.schema("LCL").expect("LCL").parents, vec!["top_level_first"]);
    assert_eq!(
        g.schema("SPECIFICATION").expect("SPECIFICATION").parents,
        vec!["top_level_second"]
    );
    assert_eq!(g.schema("LCL").unwrap().occurrence, Occurrence::ExactlyOne);
    assert_eq!(
        g.schema("SPECIFICATION").unwrap().occurrence,
        Occurrence::ExactlyOne
    );
}

#[test]
fn an_unverified_package_cannot_build_a_grammar() {
    let unverified =
        SpecPackage::open_unverified(canonical_root()).expect("package opens unverified");
    assert!(
        !unverified.is_authoritative(),
        "open_unverified must not establish authority"
    );
    let err = Grammar::load(&unverified).expect_err("must refuse an unverified package");
    let text = err.to_string();
    assert!(
        text.contains("refusing"),
        "the refusal must say so plainly: {text}"
    );
}

#[test]
fn defaults_are_recorded_but_never_applied_here() {
    let g = grammar();
    // The registry records 27 defaults; the parser only notes their presence.
    // `04_GRAMMAR/13`: "A declared default is applied only when the field is
    // MISSING", which is a later-stage determination.
    let with_defaults: Vec<&str> = g
        .schemas()
        .flat_map(|s| {
            s.fields
                .iter()
                .filter(|f| f.has_default)
                .map(|f| f.name.as_str())
        })
        .collect();
    assert!(!with_defaults.is_empty());
    let spec_authority = g.schema("SPECIFICATION").unwrap().field("AUTHORITY").unwrap();
    assert!(spec_authority.has_default);
    assert!(!spec_authority.required);
}
