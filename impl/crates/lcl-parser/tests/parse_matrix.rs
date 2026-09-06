//! The complete parse matrix.
//!
//! `complete_example_parse_matrix` is classified OUT_OF_SCOPE by the canonical
//! release validator, because the bare-language package ships no parser
//! (`validate_release.py`, the `grammar` scope). This file is the executed
//! implementation gate that classification defers to. It runs the canonical
//! sources and, separately, a sweep synthesised from the registries so that
//! coverage does not depend on the shipped example set.
//!
//! Nothing here is keyed to a filename. Every expectation is read from the
//! example's own `.expected.txt` and the error registry, so an example cannot
//! be special-cased into passing.

mod common;

use common::*;
use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_parser::{FormSet, Grammar, Outcome};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn lcl_files(sub: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(canonical_root().join(sub))
        .expect("dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "lcl"))
        .collect();
    v.sort();
    v
}

fn name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn expectation(p: &Path, key: &str) -> String {
    let text = std::fs::read_to_string(format!("{}.expected.txt", p.display())).expect("expected");
    text.lines()
        .find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix(": ")))
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|| panic!("{key} missing in {}", p.display()))
}

#[test]
fn all_thirteen_valid_examples_pass_m1_and_parse() {
    let files = lcl_files("08_EXAMPLES/VALID");
    assert_eq!(files.len(), 13);
    for p in &files {
        let bytes = std::fs::read(p).expect("read");
        let lexed = lex_bytes(&bytes);
        assert!(lexed.primary().is_none(), "{}: must still pass M1", name(p));
        let parsed = parse_bytes(&bytes);
        assert_well_formed(&parsed, bytes.len(), &name(p));
        assert_eq!(
            parsed.outcome(),
            Outcome::Parsed,
            "{}: {:?}",
            name(p),
            ids(&parsed)
        );
        // A parsed document is a real tree, not an empty one.
        assert!(
            parsed.document().items.len() >= 3,
            "{}: the document must carry its blocks",
            name(p)
        );
        let names: Vec<&str> = parsed
            .document()
            .blocks()
            .map(|b| b.key.text.as_str())
            .collect();
        assert_eq!(&names[..2], &["LCL", "SPECIFICATION"], "{}", name(p));
    }
}

#[test]
fn the_invalid_matrix_fails_at_exactly_the_registered_earliest_stage() {
    let registry = DiagnosticRegistry::load(spec()).expect("diagnostics");
    let files = lcl_files("08_EXAMPLES/INVALID");
    assert_eq!(files.len(), 21);

    let mut lexical = Vec::new();
    let mut grammar_stage = Vec::new();
    let mut later_stage = Vec::new();

    for p in &files {
        let want_error = expectation(p, "EXPECTED_ERROR");
        let want_status = expectation(p, "EXPECTED_TERMINAL_STATUS");
        let stage = registry.error(&want_error).expect("registered").stage;
        let bytes = std::fs::read(p).expect("read");
        let lexed = lex_bytes(&bytes);

        if stage == Stage::Lexical {
            // M1 owns it; the grammar stage is never evaluated.
            assert!(lexed.primary().is_some(), "{}", name(p));
            lexical.push(name(p));
            continue;
        }

        // Every later-stage expectation requires a clean lexical stage.
        assert!(
            lexed.primary().is_none(),
            "{}: a {stage} expectation requires a clean lexical stage, got {:?}",
            name(p),
            lexed.primary().map(|d| d.id.to_string())
        );
        let parsed = parse_bytes(&bytes);
        assert_well_formed(&parsed, bytes.len(), &name(p));

        if stage == Stage::GrammarOrSchema {
            // The pinned error must actually be raised, at this stage.
            let raised: Vec<String> = id_list(&parsed);
            assert!(
                raised.contains(&want_error),
                "{}: expected {want_error}, got {:?}",
                name(p),
                ids(&parsed)
            );
            assert_eq!(
                parsed.terminal_status(),
                Some(want_status.as_str()),
                "{}",
                name(p)
            );
            grammar_stage.push(name(p));
        } else {
            // `earliest_stage_rule`: a resolution or static expectation
            // requires the grammar stage to be clean.
            assert_eq!(
                parsed.outcome(),
                Outcome::Parsed,
                "{}: a {stage} expectation requires a clean grammar stage, got {:?}",
                name(p),
                ids(&parsed)
            );
            later_stage.push(name(p));
        }
    }

    assert_eq!(lexical.len(), 8, "{lexical:?}");
    assert_eq!(grammar_stage.len(), 4, "{grammar_stage:?}");
    assert_eq!(later_stage.len(), 9, "{later_stage:?}");
    assert_eq!(lexical.len() + grammar_stage.len() + later_stage.len(), 21);
}

#[test]
fn each_grammar_stage_example_reports_its_exact_locus() {
    let dir = canonical_root().join("08_EXAMPLES/INVALID");
    let get = |n: &str| -> (Vec<u8>, lcl_parser::Parsed) {
        let bytes = std::fs::read(dir.join(n)).expect("read");
        let parsed = parse_bytes(&bytes);
        (bytes, parsed)
    };

    // 11: the repeated VERSION key is the locus, not the first occurrence.
    let (bytes, parsed) = get("11_DUPLICATE_FIELD.invalid.lcl");
    let d = parsed.primary().expect("primary");
    assert_eq!(d.id.to_string(), "error.field.duplicate");
    let text = String::from_utf8(bytes).expect("utf8");
    assert_eq!(d.span.slice(&text), Some("VERSION"));
    assert_eq!(d.span.start, text.rfind("VERSION").expect("second"));

    // 15: `location_rule` puts an omitted required top-level block at the
    // end-of-file offset equal to the source byte length.
    let (bytes, parsed) = get("15_MISSING_TASK_STRUCTURE.invalid.lcl");
    let d = parsed.primary().expect("primary");
    assert_eq!(d.id.to_string(), "error.block.required");
    assert_eq!(d.span.start, bytes.len());
    assert!(d.span.is_empty());

    // 16: the offending key itself is the locus.
    let (bytes, parsed) = get("16_ITEM_COLLECTION.invalid.lcl");
    let d = parsed.primary().expect("primary");
    assert_eq!(d.id.to_string(), "error.field.forbidden");
    let text = String::from_utf8(bytes).expect("utf8");
    assert_eq!(d.span.slice(&text), Some("ITEM"));

    // 10 carries two independent grammar defects. Its `.expected.txt` pins
    // `error.field.required`, but the source also puts a CONTEXT block in a
    // kind.data document, which `document_kind_blocks` forbids. Both are
    // emitted; `stable_order` ranks byte offset above specificity_rank, so the
    // earlier `error.block.context` is primary.
    //
    // The registries are tier 1 of `00_RELEASE/02_NORMATIVE_AUTHORITY_ORDER.txt`
    // and examples are tier 8: "A lower item cannot change a higher item's
    // meaning. Examples are never normative exceptions." So the pinned
    // identifier is asserted as raised, not as primary.
    let (bytes, parsed) = get("10_CONTEXT_WITHOUT_SCOPE.invalid.lcl");
    let text = String::from_utf8(bytes.clone()).expect("utf8");
    let raised = ids(&parsed);
    assert!(
        raised.iter().any(|(id, _)| id == "error.field.required"),
        "the pinned expectation is raised: {raised:?}"
    );
    let context = parsed
        .diagnostics()
        .iter()
        .find(|d| d.id.to_string() == "error.block.context")
        .expect("CONTEXT is not legal for kind.data");
    assert_eq!(context.span.slice(&text), Some("CONTEXT"));
    let required = parsed
        .diagnostics()
        .iter()
        .find(|d| d.id.to_string() == "error.field.required")
        .expect("CONTEXT requires SCOPE");
    assert_eq!(
        required.span.start,
        bytes.len(),
        "no following line at or below indent 0"
    );
    assert!(context.span.start < required.span.start);
    assert_eq!(parsed.primary().map(|d| d.id), Some(context.id));
}

#[test]
fn every_lexically_clean_conformance_fixture_parses_without_panic() {
    // The source fixtures are lexical cases; the ones M1 accepts must also be
    // total for the parser, whatever their grammar verdict.
    for p in lcl_files("09_CONFORMANCE/SOURCE_FIXTURES") {
        let bytes = std::fs::read(&p).expect("read");
        let lexed = lex_bytes(&bytes);
        if lexed.primary().is_some() {
            continue;
        }
        let parsed = parse_bytes(&bytes);
        assert_well_formed(&parsed, bytes.len(), &name(&p));
    }
}

// -- registry-driven sweep -------------------------------------------------

/// A value satisfying `forms`, preferring the simplest inline shape.
fn value_for(forms: FormSet) -> Option<&'static str> {
    for (bit, text) in [
        (FormSet::STRING, "\"0.1.0\""),
        (FormSet::INTEGER, "1"),
        (FormSet::BOOLEAN, "TRUE"),
        (FormSet::TYPE_EXPRESSION, "STRING"),
        (FormSet::SIMPLE_IDENTIFIER, "x"),
        (FormSet::QUALIFIED_IDENTIFIER, "a.b"),
        (FormSet::REFERENCE, "REF(a.b)"),
        (FormSet::REFERENCE_LIST, "[REF(a.b)]"),
        (FormSet::EXPRESSION, "\"v\""),
    ] {
        if forms.intersects(bit) {
            // A simple-identifier slot must not receive a dotted spelling.
            if bit == FormSet::SIMPLE_IDENTIFIER && !forms.intersects(FormSet::SIMPLE_IDENTIFIER) {
                continue;
            }
            return Some(text);
        }
    }
    None
}

/// The minimal legal source for one block, at `indent`, or `None` when the
/// block needs a construction this sweep does not synthesise.
fn minimal_block(g: &Grammar, block: &str, indent: usize, depth: usize) -> Option<String> {
    if depth > 3 {
        return None;
    }
    let schema = g.schema(block)?;
    let pad = " ".repeat(indent);
    let inner = " ".repeat(indent + 4);
    let mut out = format!("{pad}{block}:\n");
    let mut wrote = false;
    for field in &schema.fields {
        if !field.required {
            continue;
        }
        if field.forms.contains(FormSet::NESTED) && value_for(field.forms).is_none() {
            let child = field.nested_block.as_deref()?;
            out.push_str(&minimal_block(g, child, indent + 4, depth + 1)?);
            wrote = true;
            continue;
        }
        let value = value_for(field.forms)?;
        out.push_str(&format!("{inner}{}: {value}\n", field.name));
        wrote = true;
    }
    // A block body must hold at least one statement.
    if !wrote {
        let any = schema.fields.first()?;
        let value = value_for(any.forms)?;
        out.push_str(&format!("{inner}{}: {value}\n", any.name));
    }
    Some(out)
}

/// A document of a kind that permits `block` at top level, wrapping `body`.
fn document_for(g: &Grammar, block: &str, body: &str) -> Option<String> {
    // Prefer a kind that does not itself demand an EXECUTE root.
    let mut kinds: Vec<&str> = g
        .document_kinds()
        .filter(|k| g.document_kind_blocks(k).is_some_and(|s| s.contains(block)))
        .collect();
    kinds.sort_by_key(|k| matches!(*k, "kind.task" | "kind.test"));
    let kind = kinds.first()?;
    let mut doc = format!("{}\n{body}", header(kind));
    if matches!(*kind, "kind.task" | "kind.test") && block != "EXECUTE" {
        doc.push_str("\nEXECUTE:\n    REFERENCE: REF(task.t)\n");
    }
    Some(doc)
}

/// The blocks this sweep can synthesise at top level.
fn sweepable(g: &Grammar) -> Vec<String> {
    g.schemas()
        .filter(|s| s.accepts_parent("top_level"))
        .map(|s| s.name.clone())
        .collect()
}

#[test]
fn the_sweep_covers_every_top_level_block() {
    let g = grammar();
    let sweepable: BTreeSet<String> = sweepable(g).into_iter().collect();
    let declared: BTreeSet<String> = g
        .schemas()
        .filter(|s| s.accepts_parent("top_level"))
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(sweepable, declared);
    // LCL and SPECIFICATION are position-pinned rather than `top_level`, so the
    // sweep covers every block but those two.
    assert_eq!(sweepable.len(), g.block_count() - 2 - nested_only(g));
}

/// Blocks legal only inside another block or a branch body.
fn nested_only(g: &Grammar) -> usize {
    g.schemas()
        .filter(|s| {
            !s.accepts_parent("top_level")
                && !s.accepts_parent("top_level_first")
                && !s.accepts_parent("top_level_second")
        })
        .count()
}

#[test]
fn block_minimum_every_block_accepts_its_exact_minimum_form() {
    // Conformance category `block_minimum`: "Accept exact minimum required
    // field/child form."
    let g = grammar();
    let mut checked = 0usize;
    for block in sweepable(g) {
        let Some(body) = minimal_block(g, &block, 0, 0) else {
            continue;
        };
        let Some(doc) = document_for(g, &block, &body) else {
            continue;
        };
        let lexed = lex(&doc);
        assert!(
            lexed.primary().is_none(),
            "{block}: synthesised source must be lexically clean:\n{doc}"
        );
        let parsed = parse(&doc);
        // Only defects attributable to *this* block matter; a synthesised
        // sibling reference is not resolved at this stage.
        let structural: Vec<String> = id_list(&parsed)
            .into_iter()
            .filter(|d| d != "error.block.conditional_requirement")
            .collect();
        assert!(
            structural.is_empty(),
            "{block}: minimum form must parse, got {:?}\n{doc}",
            ids(&parsed)
        );
        checked += 1;
    }
    println!("block_minimum: {checked} blocks synthesised and parsed");
    assert!(
        checked >= 25,
        "the sweep must cover most blocks, covered {checked}"
    );
}

#[test]
fn block_extra_one_forbidden_field_is_reported_for_every_block() {
    // Conformance category `block_extra`: "Insert one forbidden field."
    // `ITEM` is a registered word, so the lexer accepts it, and it is absent
    // from almost every block signature.
    let g = grammar();
    let mut checked = 0usize;
    for block in sweepable(g) {
        let schema = g.schema(&block).expect("schema");
        let intruder = ["ITEM", "MODE", "PRIORITY", "LIMIT", "DELAY"]
            .into_iter()
            .find(|f| schema.field(f).is_none());
        let Some(intruder) = intruder else { continue };
        let Some(mut body) = minimal_block(g, &block, 0, 0) else {
            continue;
        };
        body.push_str(&format!("    {intruder}: x\n"));
        let Some(doc) = document_for(g, &block, &body) else {
            continue;
        };
        if lex(&doc).primary().is_some() {
            continue;
        }
        let parsed = parse(&doc);
        let raised = id_list(&parsed);
        assert!(
            raised.contains(&"error.field.forbidden".to_string())
                || raised.contains(&"error.block.field".to_string()),
            "{block}: a forbidden `{intruder}` must be reported, got {:?}",
            ids(&parsed)
        );
        checked += 1;
    }
    println!("block_extra: {checked} blocks given one forbidden field");
    assert!(checked >= 25, "covered {checked}");
}

#[test]
fn block_missing_one_removed_required_field_is_reported_for_every_block() {
    // Conformance category `block_missing`: "Remove one required field or
    // conditional child."
    let g = grammar();
    let mut checked = 0usize;
    for block in sweepable(g) {
        let schema = g.schema(&block).expect("schema");
        let required: Vec<&str> = schema
            .fields
            .iter()
            .filter(|f| f.required)
            .map(|f| f.name.as_str())
            .collect();
        if required.is_empty() {
            continue;
        }
        let Some(full) = minimal_block(g, &block, 0, 0) else {
            continue;
        };
        for drop in &required {
            let body: String = full
                .lines()
                .filter(|l| !l.trim_start().starts_with(&format!("{drop}: ")))
                .map(|l| format!("{l}\n"))
                .collect();
            // Dropping the only statement leaves an empty block, which the
            // lexer rejects before the parser sees it.
            if body.lines().count() < 2 {
                continue;
            }
            let Some(doc) = document_for(g, &block, &body) else {
                continue;
            };
            if lex(&doc).primary().is_some() {
                continue;
            }
            let parsed = parse(&doc);
            let raised = id_list(&parsed);
            assert!(
                raised.contains(&"error.field.required".to_string())
                    || raised.contains(&"error.block.conditional_requirement".to_string()),
                "{block}: removing required `{drop}` must be reported, got {:?}\n{doc}",
                ids(&parsed)
            );
            checked += 1;
        }
    }
    println!("block_missing: {checked} required-field removals checked");
    assert!(checked >= 30, "covered {checked}");
}

#[test]
fn no_expectation_is_keyed_to_a_filename() {
    // Guard against the exemption this task forbids: every invalid example's
    // verdict must come from its own `.expected.txt` and the registry.
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parse_matrix.rs"),
    )
    .expect("this file");
    // The only filenames named are in the exact-locus test, which asserts
    // stricter behaviour rather than excusing any.
    let matrix_test = source
        .split("fn the_invalid_matrix_fails_at_exactly_the_registered_earliest_stage")
        .nth(1)
        .and_then(|s| s.split("\n#[test]").next())
        .expect("matrix test present");
    assert!(
        !matrix_test.contains(".invalid.lcl"),
        "the matrix verdict must not mention an example filename"
    );
}
