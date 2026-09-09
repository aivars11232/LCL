//! The seam is plumbing, and plumbing may not change meaning.
//!
//! `Runtime::execute` and `Runtime::execute_with` must agree whenever both
//! defer every operation. That is what makes an implemented operation's effect
//! on a result attributable to *that operation* rather than to the machinery
//! that routes it — so these tests deliberately compare two deferring runs,
//! not the standard library against nothing.

mod common;

use lcl_runtime::operations::DeferAll;
use lcl_runtime::{MockHost, Runtime};

/// The same document, run through `execute` and through `execute_with`.
fn both_ways(name: &str) -> (Vec<String>, Vec<String>) {
    let fixture = common::example_fixture(name);
    let runtime = Runtime::new(common::contracts());

    let mut host = MockHost::new();
    let before = runtime
        .execute(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut host,
        )
        .expect("the example planned");

    let mut deferring = DeferAll;
    let mut host = MockHost::new();
    let after = runtime
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut deferring,
            &mut host,
        )
        .expect("the example planned");

    (common::summary(&before), common::summary(&after))
}

#[test]
fn the_seam_itself_changes_no_canonical_example() {
    let names = common::canonical_example_names();
    assert_eq!(
        names.len(),
        13,
        "the canonical corpus has 13 valid examples"
    );
    for name in names {
        let (before, after) = both_ways(&name);
        assert_eq!(
            before, after,
            "{name} executed differently through execute_with than through execute"
        );
    }
}

#[test]
fn a_deferring_dispatcher_still_reaches_the_host() {
    // The seam must not silently swallow a request. Until an operation is
    // implemented, its request crosses the boundary exactly as before, and the
    // host sees it.
    let source = common::task(
        &common::data("data.subject", "STRING", "\"alpha\""),
        &["ID: action.read\nOPERATION: core.read\nTARGET: REF(data.subject)"],
    );
    let fixture = common::fixture(&source);

    let mut stdlib = common::stdlib();
    let mut host = MockHost::new();
    let execution = Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("the document planned");

    assert_eq!(
        host.count("core.read"),
        1,
        "the deferred request reached the host exactly once"
    );
    assert_eq!(
        execution
            .invocations()
            .iter()
            .filter(|r| r.declaration.as_deref() == Some("action.read"))
            .count(),
        1
    );
}

#[test]
fn an_empty_catalog_installs_no_profiles() {
    // The catalog carries the registry's rows whether or not any implementation
    // is installed. An engine with no installed profiles is a valid state, and
    // it must not pretend to have one.
    let stdlib = common::stdlib();
    assert!(stdlib.catalog().profiles().is_empty());
    assert_eq!(stdlib.catalog().rows().count(), 39);
}
