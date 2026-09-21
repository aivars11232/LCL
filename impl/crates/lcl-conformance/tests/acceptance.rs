//! The readiness gate, driven over deliberately nonconforming verdicts.
//!
//! The gate's whole value is that it refuses: a report command that exits 0 on
//! a `source_conforming` verdict is exactly the case a pipeline would otherwise
//! read as readiness. Each case below is a verdict that must not be accepted,
//! and the last one is the real production verdict this build renders.

mod common;

use lcl_conformance::acceptance::accept;
use lcl_conformance::obligations::MAPPING_DIGEST;
use lcl_conformance::production;

const DIGEST: &str = "9b32a28b79d9c3872cb3f810365a3275583ce5731a74c98a8d2013d07633a8ad";
const IDENTITY: &str = "00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed";

/// One synthetic verdict, with each part replaceable.
fn verdict(claim: &str, satisfied: &[&str], state: &str, ids: &[&str], required: usize) -> String {
    let list = |ids: &[&str]| {
        ids.iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let other = |name: &str| {
        if name == state {
            list(ids)
        } else {
            String::new()
        }
    };
    format!(
        "{{\"format\":\"lcl.conformance.verdict.v1\",\"claim\":\"{claim}\",\
         \"implementation\":{{\"package_identity\":\"{IDENTITY}\"}},\
         \"mapping_digest\":\"{DIGEST}\",\
         \"levels\":[{{\"level\":\"semantics_conforming\",\"required\":{required},\
         \"satisfied\":[{}],\"failed\":[{}],\"missing\":[{}],\"invalid\":[{}]}}]}}",
        list(satisfied),
        other("failed"),
        other("missing"),
        other("invalid"),
    )
}

fn refusal(verdict: &str) -> String {
    match accept(verdict, DIGEST, IDENTITY) {
        Ok(accepted) => panic!("the gate accepted a verdict it must refuse: {accepted:?}"),
        Err(refusals) => refusals.join(" | "),
    }
}

#[test]
fn a_complete_semantic_verdict_is_accepted() {
    let accepted = accept(
        &verdict("semantics_conforming", &["a", "b"], "invalid", &[], 2),
        DIGEST,
        IDENTITY,
    )
    .expect("a complete verdict is accepted");
    assert_eq!(accepted.claim, "semantics_conforming");
    assert_eq!(
        accepted.levels,
        vec![("semantics_conforming".to_string(), 2)]
    );
}

#[test]
fn a_source_conforming_verdict_is_refused_even_though_its_command_exits_zero() {
    let refusal = refusal(&verdict(
        "source_conforming",
        &["a", "b"],
        "invalid",
        &[],
        2,
    ));
    assert!(
        refusal.contains("source_conforming") && refusal.contains("semantics_conforming"),
        "{refusal}"
    );
}

#[test]
fn any_unsatisfied_obligation_is_refused_by_state() {
    for state in ["failed", "missing", "invalid"] {
        let refusal = refusal(&verdict(
            "semantics_conforming",
            &["a"],
            state,
            &["semantic/operation_errors/core.group"],
            2,
        ));
        assert!(
            refusal.contains(state) && refusal.contains("core.group"),
            "{state}: {refusal}"
        );
    }
}

#[test]
fn an_obligation_counted_in_two_states_is_refused() {
    let doubled = format!(
        "{{\"format\":\"lcl.conformance.verdict.v1\",\"claim\":\"semantics_conforming\",\
         \"implementation\":{{\"package_identity\":\"{IDENTITY}\"}},\
         \"mapping_digest\":\"{DIGEST}\",\
         \"levels\":[{{\"level\":\"semantics_conforming\",\"required\":1,\
         \"satisfied\":[\"a\"],\"failed\":[\"a\"],\"missing\":[],\"invalid\":[]}}]}}"
    );
    let refusal = refusal(&doubled);
    assert!(refusal.contains("more than one state"), "{refusal}");
}

#[test]
fn a_count_that_does_not_account_for_every_obligation_is_refused() {
    let refusal = refusal(&verdict("semantics_conforming", &["a"], "invalid", &[], 7));
    assert!(refusal.contains("requires 7"), "{refusal}");
}

#[test]
fn a_mapping_or_identity_mismatch_is_refused() {
    let verdict = verdict("semantics_conforming", &["a"], "invalid", &[], 1);
    let refusal = match accept(&verdict, "0000", IDENTITY) {
        Ok(_) => panic!("a mapping mismatch must be refused"),
        Err(refusals) => refusals.join(" | "),
    };
    assert!(refusal.contains("mapping digest"), "{refusal}");
    let refusal = match accept(&verdict, DIGEST, "0000") {
        Ok(_) => panic!("an identity mismatch must be refused"),
        Err(refusals) => refusals.join(" | "),
    };
    assert!(refusal.contains("package identity"), "{refusal}");
}

#[test]
fn an_absent_malformed_or_unrecognized_verdict_is_refused() {
    assert!(refusal("").contains("not JSON"));
    assert!(refusal("{\"claim\":").contains("not JSON"));
    assert!(refusal("{}").contains("declares no format"));
    let wrong_format = verdict("semantics_conforming", &["a"], "invalid", &[], 1)
        .replace("lcl.conformance.verdict.v1", "something.else.v9");
    assert!(refusal(&wrong_format).contains("does not recognize"));
    let no_levels = format!(
        "{{\"format\":\"lcl.conformance.verdict.v1\",\"claim\":\"semantics_conforming\",\
         \"implementation\":{{\"package_identity\":\"{IDENTITY}\"}},\
         \"mapping_digest\":\"{DIGEST}\"}}"
    );
    assert!(refusal(&no_levels).contains("no levels"));
}

/// The gate over the verdict this build actually renders.
///
/// The assertion is not that the verdict passes — it is that the gate's answer
/// is the report's own state, whichever that is, and that an accepted verdict
/// could only be a complete one.
#[test]
fn the_gate_answers_the_real_production_verdict() {
    let report = production::report(common::spec()).expect("the production report assembles");
    let rendered = report.render_verdict_json();
    let claim = report.claim().as_str().to_string();
    match accept(
        &rendered,
        MAPPING_DIGEST,
        lcl_spec::APPROVED_PACKAGE.identity_digest,
    ) {
        Ok(accepted) => {
            assert_eq!(accepted.claim, "semantics_conforming");
            assert_eq!(claim, "semantics_conforming");
        }
        Err(refusals) => {
            assert_ne!(
                claim, "semantics_conforming",
                "a complete claim was refused: {refusals:?}"
            );
            assert!(
                refusals.iter().any(|r| r.contains("claim")),
                "an incomplete verdict must be refused for its claim: {refusals:?}"
            );
        }
    }
}
