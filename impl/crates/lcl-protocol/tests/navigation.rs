//! The navigation projection: what the resolver bound, as an editor sees it.
//!
//! Every assertion here is about *copying*. A test that passed because this
//! crate searched for an identifier would be testing a second resolver, so each
//! one checks that a record agrees with `lcl-resolver`'s own decision rather
//! than that it found a plausible answer.

mod common;

use common::{engine, example, example_provider, unit, valid_examples};
use lcl_protocol::Inputs;
use lcl_resolver::MemoryProvider;

/// Inspect one canonical example and take its navigation record.
fn navigation_of(name: &str) -> lcl_protocol::NavigationRecord {
    let source = example(name);
    let report = engine().inspect(&unit(name, &source), &example_provider(), &Inputs::new());
    report
        .navigation
        .unwrap_or_else(|| panic!("{name} inspects to a navigation record"))
}

#[test]
fn inspect_carries_navigation_and_the_other_commands_do_not() {
    let source = example("01_MINIMAL_TASK.lcl");
    let doc = unit("01_MINIMAL_TASK.lcl", &source);
    let provider = MemoryProvider::new();

    assert!(engine().check(&doc, &provider).navigation.is_none());
    assert!(engine()
        .validate(&doc, &provider, &Inputs::new())
        .navigation
        .is_none());
    assert!(engine()
        .inspect(&doc, &provider, &Inputs::new())
        .navigation
        .is_some());
}

#[test]
fn every_valid_example_projects_every_declaration_the_resolver_indexed() {
    for name in valid_examples() {
        let navigation = navigation_of(&name);
        let structure = {
            let source = example(&name);
            engine()
                .inspect(&unit(&name, &source), &example_provider(), &Inputs::new())
                .structure
                .expect("inspect produces a structure record")
        };

        // The structure record already lists every declaration the resolver
        // indexed. Navigation must list exactly those, in the same order:
        // a different count would mean one of the two filtered something.
        assert_eq!(
            navigation.declarations.len(),
            structure.declarations.len(),
            "{name}: navigation and structure disagree on the declaration count"
        );
        for (nav, (id, block)) in navigation.declarations.iter().zip(&structure.declarations) {
            assert_eq!(&nav.id, id, "{name}: declaration identity differs");
            assert_eq!(&nav.block, block, "{name}: declaring block differs");
        }
    }
}

#[test]
fn a_declaration_index_is_its_own_position() {
    for name in valid_examples() {
        let navigation = navigation_of(&name);
        for (position, declaration) in navigation.declarations.iter().enumerate() {
            assert_eq!(
                declaration.index, position,
                "{name}: a reference's declaration index must address this list"
            );
        }
    }
}

#[test]
fn every_id_span_actually_contains_the_declared_identifier() {
    // The spans are the resolver's. This proves the projection did not shift,
    // swap or truncate one on the way through.
    for name in valid_examples() {
        let source = example(&name);
        let navigation = navigation_of(&name);
        for declaration in &navigation.declarations {
            if declaration.source != name {
                continue; // declared in an imported unit
            }
            let span = declaration.id_span;
            let text = &source[span.start..span.end];
            assert!(
                declaration.id.ends_with(text),
                "{name}: id_span covers {text:?}, which is not the tail of {:?}",
                declaration.id
            );
        }
    }
}

#[test]
fn every_reference_span_contains_the_identifier_as_written() {
    for name in valid_examples() {
        let source = example(&name);
        let navigation = navigation_of(&name);
        for reference in &navigation.references {
            if reference.source != name {
                continue;
            }
            assert_eq!(
                &source[reference.span.start..reference.span.end],
                reference.text,
                "{name}: a reference's span and its text disagree"
            );
        }
    }
}

#[test]
fn a_resolved_reference_points_at_a_declaration_that_carries_its_identity() {
    // This is "go to definition", executed. Following `declaration` must land
    // on a declaration whose qualified id is the one the resolver recorded.
    let mut followed = 0usize;
    for name in valid_examples() {
        let navigation = navigation_of(&name);
        for reference in &navigation.references {
            let Some(index) = reference.declaration else {
                continue;
            };
            let target = navigation
                .declarations
                .get(index)
                .unwrap_or_else(|| panic!("{name}: reference points outside the declaration list"));
            assert_eq!(reference.target, "declaration");
            assert_eq!(
                reference.resolved_id.as_deref(),
                Some(target.id.as_str()),
                "{name}: a reference and its target disagree on identity"
            );
            followed += 1;
        }
    }
    assert!(
        followed > 0,
        "the canonical examples must contain at least one resolved reference"
    );
}

#[test]
fn find_references_returns_exactly_the_references_that_bound_to_that_declaration() {
    for name in valid_examples() {
        let navigation = navigation_of(&name);
        for declaration in &navigation.declarations {
            let found: Vec<_> = navigation.references_to(declaration.index).collect();
            let expected: Vec<_> = navigation
                .references
                .iter()
                .filter(|r| r.declaration == Some(declaration.index))
                .collect();
            assert_eq!(
                found.len(),
                expected.len(),
                "{name}: find-references drifted"
            );
        }
    }
}

#[test]
fn a_cursor_on_a_reference_resolves_and_a_cursor_off_one_does_not() {
    let name = "01_MINIMAL_TASK.lcl";
    let navigation = navigation_of(name);
    let reference = navigation
        .references
        .first()
        .expect("the minimal task contains a reference")
        .clone();

    // Every byte of the identifier resolves to the same occurrence.
    for offset in reference.span.start..reference.span.end {
        let found = navigation
            .reference_at(name, offset)
            .expect("a cursor inside a reference resolves");
        assert_eq!(found.span, reference.span);
    }
    // The byte after it does not belong to it.
    if let Some(found) = navigation.reference_at(name, reference.span.end) {
        assert_ne!(found.span, reference.span);
    }
    // Nor does any offset in a unit that was never loaded.
    assert!(navigation
        .reference_at("not-a-loaded-unit.lcl", reference.span.start)
        .is_none());
}

#[test]
fn a_cursor_on_a_declared_id_resolves_to_that_declaration() {
    let name = "01_MINIMAL_TASK.lcl";
    let navigation = navigation_of(name);
    for declaration in navigation.declarations.clone() {
        if declaration.source != name {
            continue;
        }
        let found = navigation
            .declaration_at(name, declaration.id_span.start)
            .expect("a cursor on a declared ID resolves");
        assert_eq!(found.index, declaration.index);
        assert_eq!(found.id, declaration.id);
    }
}

#[test]
fn a_derived_position_agrees_with_the_span_it_was_derived_from() {
    // The byte offset is normative and the line/column is carried beside it.
    // They must describe the same byte.
    for name in valid_examples() {
        let navigation = navigation_of(&name);
        for declaration in &navigation.declarations {
            assert_eq!(declaration.id_position.offset, declaration.id_span.start);
            assert!(declaration.id_position.line >= 1);
            assert!(declaration.id_position.column >= 1);
        }
        for reference in &navigation.references {
            assert_eq!(reference.position.offset, reference.span.start);
        }
    }
}

#[test]
fn an_unresolved_reference_is_reported_rather_than_dropped() {
    // A canonical example with exactly one reference redirected at an
    // identifier nothing declares. Built from real canonical bytes rather than
    // hand-written, so the only thing wrong with it is the thing under test.
    let source = example("01_MINIMAL_TASK.lcl").replace(
        "TARGET: REF(input.value)",
        "TARGET: REF(input.nothing_declares_this)",
    );
    let report = engine().inspect(
        &unit("unresolved.lcl", &source),
        &MemoryProvider::new(),
        &Inputs::new(),
    );

    // The resolver rejected it, and said so with the registered identifier.
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.id == "error.reference.unresolved"));

    // And the occurrence is still navigable. This is the case an editor is in
    // most of the time: a document mid-edit, with one broken reference and
    // everything else still worth jumping around.
    let navigation = report
        .navigation
        .expect("navigation survives a rejected resolution");
    let broken = navigation
        .references
        .iter()
        .find(|r| r.text == "input.nothing_declares_this")
        .expect("the unresolved occurrence is reported, not dropped");
    assert_eq!(broken.target, "unresolved");
    assert!(broken.declaration.is_none());
    assert!(broken.resolved_id.is_none());

    // Its neighbours in the same document still resolve, so one bad reference
    // does not cost the editor every other binding.
    let resolved_count = navigation
        .references
        .iter()
        .filter(|r| r.declaration.is_some())
        .count();
    assert!(
        resolved_count > 0,
        "one unresolved reference must not discard the bindings that did resolve"
    );
}

#[test]
fn the_json_projection_carries_navigation_only_for_inspect() {
    let source = example("01_MINIMAL_TASK.lcl");
    let doc = unit("01_MINIMAL_TASK.lcl", &source);

    let inspected = engine()
        .inspect(&doc, &MemoryProvider::new(), &Inputs::new())
        .to_json()
        .pretty();
    assert!(inspected.contains("\"navigation\""));
    assert!(inspected.contains("\"declarations\""));
    assert!(inspected.contains("\"references\""));

    let checked = engine()
        .check(&doc, &MemoryProvider::new())
        .to_json()
        .pretty();
    assert!(!checked.contains("\"navigation\""));
}
