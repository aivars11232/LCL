//! Live language intelligence: token spans, diagnostics and navigation, all of
//! it engine truth rather than the frontend's own opinion.

mod common;

use common::{canonical_root, example, send, serve_examples, valid_examples, Running};
use lcl_spec::json::Json;

fn post(running: &Running, path: &str, id: &str, body: &str) -> Json {
    let reply = send(
        running.address,
        "POST",
        &format!("{path}?t={}&id={id}", running.token),
        &[],
        body.as_bytes(),
    );
    assert_eq!(reply.status, 200, "POST {path} failed: {}", reply.body);
    lcl_spec::json::parse(&reply.body).expect("a reply is JSON")
}

fn text(value: &Json) -> &str {
    value.as_str().expect("a string")
}

fn usize_of(value: &Json) -> usize {
    value.as_u64().expect("a number") as usize
}

#[test]
fn token_spans_cover_the_document_and_never_overlap() {
    let (_scratch, running) = serve_examples("tokens");
    for name in valid_examples() {
        let source = example(&name);
        let reply = post(&running, "/api/tokens", &name, &source);
        let tokens = reply.get("tokens").unwrap().as_array().expect("an array");

        let mut previous_end = 0usize;
        for token in tokens {
            let start = usize_of(token.get("start").unwrap());
            let end = usize_of(token.get("end").unwrap());
            assert!(start < end, "{name}: a painted span must have width");
            assert!(
                start >= previous_end,
                "{name}: spans overlap at byte {start}"
            );
            assert!(end <= source.len(), "{name}: a span runs past the document");
            // And the span must land on a character boundary, or the frontend
            // cannot slice the text at it.
            assert!(source.is_char_boundary(start), "{name}: start {start}");
            assert!(source.is_char_boundary(end), "{name}: end {end}");
            previous_end = end;
        }
        assert!(!tokens.is_empty(), "{name}: a document has tokens");
    }
}

#[test]
fn a_block_word_is_classified_from_the_registry_and_not_from_a_word_list() {
    let (_scratch, running) = serve_examples("classes");
    let name = "01_MINIMAL_TASK.lcl";
    let source = example(name);
    let reply = post(&running, "/api/tokens", name, &source);
    let tokens = reply.get("tokens").unwrap().as_array().expect("an array");

    let class_of = |needle: &str| -> Option<String> {
        tokens.iter().find_map(|t| {
            let start = usize_of(t.get("start").unwrap());
            let end = usize_of(t.get("end").unwrap());
            (&source[start..end] == needle).then(|| text(t.get("class").unwrap()).to_string())
        })
    };

    assert_eq!(class_of("TASK").as_deref(), Some("block"));
    assert_eq!(class_of("INTEGER").as_deref(), Some("type"));
    assert_eq!(class_of("REF").as_deref(), Some("keyword"));
    assert_eq!(class_of("4").as_deref(), Some("literal"));
    assert_eq!(
        class_of("\"Double one integer\"").as_deref(),
        Some("string")
    );
    assert_eq!(class_of("input.value").as_deref(), Some("ident"));
}

#[test]
fn lexing_a_half_typed_document_does_not_panic_and_still_returns_spans() {
    // The editor lexes on every keystroke, so every prefix of a real document
    // has to be survivable. `Lexer::lex` is documented total; this is the
    // property the UI actually depends on.
    let (_scratch, running) = serve_examples("prefixes");
    let source = example("01_MINIMAL_TASK.lcl");
    let mut checked = 0;
    for end in (0..source.len()).step_by(7) {
        if !source.is_char_boundary(end) {
            continue;
        }
        let reply = send(
            running.address,
            "POST",
            &format!("/api/tokens?t={}&id=partial.lcl", running.token),
            &[],
            &source.as_bytes()[..end],
        );
        assert_eq!(reply.status, 200, "prefix of {end} bytes was refused");
        checked += 1;
    }
    assert!(checked > 10);
}

#[test]
fn diagnostics_carry_the_registered_identifier_stage_status_and_exact_span() {
    let (_scratch, running) = serve_examples("diagnostics");
    // One real example with one reference redirected at nothing.
    let source = example("01_MINIMAL_TASK.lcl").replace(
        "TARGET: REF(input.value)",
        "TARGET: REF(input.nothing_declares_this)",
    );
    let report = post(&running, "/api/check", "01_MINIMAL_TASK.lcl", &source);

    assert_eq!(text(report.get("protocol").unwrap()), "lcl.engine/1");
    assert_eq!(text(report.get("outcome").unwrap()), "rejected");
    assert_eq!(text(report.get("reached").unwrap()), "resolution");

    let diagnostics = report.get("diagnostics").unwrap().as_array().unwrap();
    let found = diagnostics
        .iter()
        .find(|d| text(d.get("id").unwrap()) == "error.reference.unresolved")
        .expect("the registered identifier");

    assert_eq!(text(found.get("stage").unwrap()), "resolution");
    assert!(!text(found.get("default_status").unwrap()).is_empty());
    assert!(!text(found.get("meaning").unwrap()).is_empty());

    // The span is where the identifier actually is.
    let span = found.get("span").unwrap();
    let start = usize_of(span.get("start").unwrap());
    let end = usize_of(span.get("end").unwrap());
    assert_eq!(&source[start..end], "input.nothing_declares_this");

    // And the derived position describes the same byte, beside it.
    let position = found.get("position").unwrap();
    assert_eq!(usize_of(position.get("offset").unwrap()), start);
}

#[test]
fn every_valid_example_checks_clean_through_the_workspace() {
    let (_scratch, running) = serve_examples("clean");
    for name in valid_examples() {
        let report = post(&running, "/api/check", &name, &example(&name));
        assert_eq!(
            text(report.get("outcome").unwrap()),
            "accepted",
            "{name} must check clean"
        );
        assert_eq!(text(report.get("reached").unwrap()), "static_checking");
    }
}

#[test]
fn navigation_comes_from_the_resolver_and_points_at_real_declarations() {
    let (_scratch, running) = serve_examples("navigate");
    let name = "01_MINIMAL_TASK.lcl";
    let source = example(name);
    let report = post(&running, "/api/inspect", name, &source);

    let navigation = report
        .get("navigation")
        .expect("inspect carries navigation");
    let declarations = navigation.get("declarations").unwrap().as_array().unwrap();
    let references = navigation.get("references").unwrap().as_array().unwrap();
    assert!(!declarations.is_empty());
    assert!(!references.is_empty());

    // Go to definition, executed: follow a reference to its declaration and
    // land on bytes that spell the identifier.
    let resolved = references
        .iter()
        .find(|r| text(r.get("target").unwrap()) == "declaration")
        .expect("at least one reference resolves");
    let index = usize_of(resolved.get("declaration").unwrap());
    let target = &declarations[index];

    let id_span = target.get("id_span").unwrap();
    let start = usize_of(id_span.get("start").unwrap());
    let end = usize_of(id_span.get("end").unwrap());
    let declared = &source[start..end];
    assert_eq!(
        text(target.get("id").unwrap()),
        declared,
        "the definition's span must cover its own identifier"
    );
    assert_eq!(text(resolved.get("resolved_id").unwrap()), declared);
}

#[test]
fn the_buffer_is_judged_rather_than_the_file_on_disk() {
    // The editor shows unsaved text. If the engine judged the file instead,
    // every diagnostic would be one save behind, which is the single most
    // important thing to get right in a live editor.
    let (scratch, running) = serve_examples("buffer");
    let name = "01_MINIMAL_TASK.lcl";

    let broken = example(name).replace("TASK:", "TSAK:");
    let report = post(&running, "/api/check", name, &broken);
    assert_eq!(text(report.get("outcome").unwrap()), "rejected");

    // The file on disk is untouched and still checks clean.
    let on_disk = std::fs::read_to_string(scratch.join(name)).unwrap();
    assert_eq!(on_disk, example(name));
    let clean = post(&running, "/api/check", name, &on_disk);
    assert_eq!(text(clean.get("outcome").unwrap()), "accepted");
}

#[test]
fn an_import_still_resolves_from_disk_while_the_root_is_an_unsaved_buffer() {
    let (_scratch, running) = serve_examples("imports");
    let name = "03_IMPORTING_TASK.lcl";
    let report = post(&running, "/api/inspect", name, &example(name));
    assert_eq!(text(report.get("outcome").unwrap()), "accepted");

    let units = report.get("units").unwrap().as_array().unwrap();
    let ids: Vec<&str> = units.iter().map(|u| text(u.get("id").unwrap())).collect();
    assert!(ids.contains(&"03_IMPORTING_TASK.lcl"));
    assert!(
        ids.len() > 1,
        "the imported library must load from disk: {ids:?}"
    );
}

#[test]
fn the_workspace_report_is_byte_for_byte_what_the_engine_produces() {
    // The equivalence that matters: the route adds nothing and reshapes
    // nothing. Phase F extends this to the CLI binary.
    let (scratch, running) = serve_examples("identical");
    let name = "01_MINIMAL_TASK.lcl";
    let source = example(name);

    let reply = send(
        running.address,
        "POST",
        &format!("/api/check?t={}&id={name}", running.token),
        &[],
        source.as_bytes(),
    );

    let engine = lcl_protocol::Engine::open(canonical_root()).expect("engine");
    let provider = lcl_project::FileProvider::new(&scratch.path).expect("provider");
    let unit = lcl_workspace::intelligence::unit_of(name, &source);
    let direct = engine.check(&unit, &provider).to_json().pretty();

    assert_eq!(reply.body, direct, "the route must not reshape a report");
}
