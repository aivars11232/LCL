//! The readiness gate, driven over deliberately nonconforming verdicts.
//!
//! The gate's whole value is that it refuses: a report command that exits 0 on
//! a `source_conforming` verdict is exactly the case a pipeline would otherwise
//! read as readiness. Every verdict below is **synthetic** — a structural
//! fixture for the gate, never executed language-conformance evidence — and is
//! built from the *real* reviewed inventory, so "accepted" can only mean that
//! every obligation that inventory requires was established.

mod common;

use lcl_conformance::acceptance::accept;
use lcl_conformance::obligations::Obligations;
use lcl_conformance::production;
use lcl_conformance::report::ClaimLevel;

const IDENTITY: &str = "00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed";

fn inventory() -> Obligations {
    Obligations::load(common::spec()).expect("the reviewed inventory loads")
}

fn probes(inventory: &Obligations, level: ClaimLevel) -> Vec<String> {
    inventory
        .probes()
        .filter(|(_, at)| *at == level)
        .map(|(id, _)| id.to_string())
        .collect()
}

fn list(ids: &[String]) -> String {
    ids.iter()
        .map(|id| format!("\"{id}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One level entry, with every state given explicitly.
fn level(name: &str, required: usize, satisfied: &[String], other: (&str, &[String])) -> String {
    let (state, ids) = other;
    let entry = |which: &str| {
        if which == state {
            list(ids)
        } else if which == "satisfied" {
            list(satisfied)
        } else {
            String::new()
        }
    };
    format!(
        "{{\"level\":\"{name}\",\"required\":{required},\"satisfied\":[{}],\"failed\":[{}],\
         \"missing\":[{}],\"invalid\":[{}],\"problems\":[]}}",
        entry("satisfied"),
        entry("failed"),
        entry("missing"),
        entry("invalid"),
    )
}

/// A complete verdict: every obligation of both levels satisfied.
fn complete(inventory: &Obligations) -> String {
    let source = probes(inventory, ClaimLevel::Source);
    let semantics = probes(inventory, ClaimLevel::Semantics);
    verdict(
        inventory,
        "semantics_conforming",
        &[
            level("source_conforming", source.len(), &source, ("failed", &[])),
            level(
                "semantics_conforming",
                semantics.len(),
                &semantics,
                ("failed", &[]),
            ),
        ],
    )
}

fn verdict(inventory: &Obligations, claim: &str, levels: &[String]) -> String {
    format!(
        "{{\"format\":\"lcl.conformance.verdict.v1\",\"claim\":\"{claim}\",\
         \"implementation\":{{\"package_identity\":\"{IDENTITY}\"}},\
         \"mapping_digest\":\"{}\",\"levels\":[{}],\
         \"records\":{{\"executed\":10,\"passed\":10,\"failed\":0}},\"failed_case_ids\":[]}}",
        inventory.digest(),
        levels.join(",")
    )
}

fn refusal(inventory: &Obligations, verdict: &str) -> String {
    match accept(verdict, inventory, IDENTITY) {
        Ok(accepted) => panic!("the gate accepted a verdict it must refuse: {accepted:?}"),
        Err(refusals) => refusals.join(" | "),
    }
}

#[test]
fn a_verdict_establishing_every_required_obligation_is_accepted() {
    let inventory = inventory();
    let accepted = accept(&complete(&inventory), &inventory, IDENTITY)
        .expect("a complete verdict is accepted");
    assert_eq!(accepted.claim, "semantics_conforming");
    assert_eq!(
        accepted.levels,
        vec![
            (
                "source_conforming".to_string(),
                probes(&inventory, ClaimLevel::Source).len()
            ),
            (
                "semantics_conforming".to_string(),
                probes(&inventory, ClaimLevel::Semantics).len()
            ),
        ]
    );
}

#[test]
fn a_source_conforming_verdict_is_refused_even_though_its_command_exits_zero() {
    let inventory = inventory();
    let refusal = refusal(
        &inventory,
        &complete(&inventory).replace(
            "\"claim\":\"semantics_conforming\"",
            "\"claim\":\"source_conforming\"",
        ),
    );
    assert!(
        refusal.contains("source_conforming") && refusal.contains("semantics_conforming"),
        "{refusal}"
    );
}

#[test]
fn arbitrary_ids_under_a_copied_valid_digest_are_refused() {
    // The case the previous fixture missed: a well-formed verdict carrying the
    // real mapping digest and two invented obligations.
    let inventory = inventory();
    let invented: Vec<String> = vec!["a".to_string(), "b".to_string()];
    let refusal = refusal(
        &inventory,
        &verdict(
            &inventory,
            "semantics_conforming",
            &[
                level("source_conforming", 2, &invented, ("failed", &[])),
                level("semantics_conforming", 2, &invented, ("failed", &[])),
            ],
        ),
    );
    assert!(refusal.contains("does not require"), "{refusal}");
    assert!(refusal.contains("does not establish"), "{refusal}");
    assert!(refusal.contains("reviewed inventory has"), "{refusal}");
}

#[test]
fn required_ids_replaced_by_the_same_number_of_others_are_refused() {
    let inventory = inventory();
    let mut semantics = probes(&inventory, ClaimLevel::Semantics);
    let source = probes(&inventory, ClaimLevel::Source);
    // Same count, one obligation swapped for a name the inventory never lists.
    let dropped = semantics.pop().expect("the semantic level is not empty");
    semantics.push(format!("{dropped}.substituted"));
    let refusal = refusal(
        &inventory,
        &verdict(
            &inventory,
            "semantics_conforming",
            &[
                level("source_conforming", source.len(), &source, ("failed", &[])),
                level(
                    "semantics_conforming",
                    semantics.len(),
                    &semantics,
                    ("failed", &[]),
                ),
            ],
        ),
    );
    assert!(refusal.contains(&dropped), "{refusal}");
    assert!(refusal.contains("substituted"), "{refusal}");
}

#[test]
fn an_obligation_in_a_level_it_does_not_belong_to_is_refused() {
    let inventory = inventory();
    let source = probes(&inventory, ClaimLevel::Source);
    let semantics = probes(&inventory, ClaimLevel::Semantics);
    // Every semantic obligation, listed under the source level.
    let refusal = refusal(
        &inventory,
        &verdict(
            &inventory,
            "semantics_conforming",
            &[
                level(
                    "source_conforming",
                    source.len(),
                    &semantics,
                    ("failed", &[]),
                ),
                level(
                    "semantics_conforming",
                    semantics.len(),
                    &semantics,
                    ("failed", &[]),
                ),
            ],
        ),
    );
    assert!(refusal.contains("does not require"), "{refusal}");
}

#[test]
fn a_missing_duplicate_unnamed_or_unknown_level_is_refused() {
    let inventory = inventory();
    let semantics = probes(&inventory, ClaimLevel::Semantics);
    let source = probes(&inventory, ClaimLevel::Source);
    let semantic_entry = level(
        "semantics_conforming",
        semantics.len(),
        &semantics,
        ("failed", &[]),
    );
    let source_entry = level("source_conforming", source.len(), &source, ("failed", &[]));

    let missing = refusal(
        &inventory,
        &verdict(
            &inventory,
            "semantics_conforming",
            std::slice::from_ref(&semantic_entry),
        ),
    );
    assert!(
        missing.contains("omits level source_conforming"),
        "{missing}"
    );

    let duplicated = refusal(
        &inventory,
        &verdict(
            &inventory,
            "semantics_conforming",
            &[
                source_entry.clone(),
                semantic_entry.clone(),
                semantic_entry.clone(),
            ],
        ),
    );
    assert!(
        duplicated.contains("appears more than once"),
        "{duplicated}"
    );

    let unnamed = refusal(
        &inventory,
        &verdict(
            &inventory,
            "semantics_conforming",
            &[
                source_entry.clone(),
                semantic_entry.clone(),
                "{\"required\":0,\"satisfied\":[],\"failed\":[],\"missing\":[],\"invalid\":[]}"
                    .to_string(),
            ],
        ),
    );
    assert!(unnamed.contains("declares no level name"), "{unnamed}");

    let unknown = refusal(
        &inventory,
        &verdict(
            &inventory,
            "semantics_conforming",
            &[
                source_entry,
                semantic_entry,
                level("deployment_conforming", 0, &[], ("failed", &[])),
            ],
        ),
    );
    assert!(
        unknown.contains("not one of the required conformance levels"),
        "{unknown}"
    );
}

#[test]
fn any_unsatisfied_obligation_is_refused_by_state() {
    let inventory = inventory();
    let source = probes(&inventory, ClaimLevel::Source);
    let mut semantics = probes(&inventory, ClaimLevel::Semantics);
    let moved = semantics.pop().expect("the semantic level is not empty");
    for state in ["failed", "missing", "invalid"] {
        let refusal = refusal(
            &inventory,
            &verdict(
                &inventory,
                "semantics_conforming",
                &[
                    level("source_conforming", source.len(), &source, ("failed", &[])),
                    level(
                        "semantics_conforming",
                        semantics.len() + 1,
                        &semantics,
                        (state, std::slice::from_ref(&moved)),
                    ),
                ],
            ),
        );
        assert!(
            refusal.contains(state) && refusal.contains(&moved),
            "{state}: {refusal}"
        );
    }
}

#[test]
fn a_non_string_member_or_a_duplicate_id_is_refused() {
    let inventory = inventory();
    let complete = complete(&inventory);
    let first = probes(&inventory, ClaimLevel::Semantics)
        .first()
        .expect("the semantic level is not empty")
        .clone();

    // A number where an obligation id belongs is malformed, not skippable.
    let typed = complete.replacen(&format!("\"{first}\""), "7", 1);
    let mistyped = refusal(&inventory, &typed);
    assert!(mistyped.contains("non-string obligation"), "{mistyped}");

    // The same id twice, in two states of the level that lists it. The first
    // `"failed":[]` in the rendered verdict belongs to the source level, so the
    // id duplicated there is a source obligation.
    let source_first = probes(&inventory, ClaimLevel::Source)
        .first()
        .expect("the source level is not empty")
        .clone();
    let duplicated = complete.replacen(
        "\"failed\":[]",
        &format!("\"failed\":[\"{source_first}\"]"),
        1,
    );
    let twice = refusal(&inventory, &duplicated);
    assert!(twice.contains("more than one state"), "{twice}");
}

#[test]
fn contradictory_counters_problems_or_failed_cases_are_refused() {
    let inventory = inventory();
    let complete = complete(&inventory);

    let counts = complete.replacen(
        "\"records\":{\"executed\":10,\"passed\":10,\"failed\":0}",
        "\"records\":{\"executed\":10,\"passed\":4,\"failed\":0}",
        1,
    );
    assert!(
        refusal(&inventory, &counts).contains("executed cases but"),
        "{}",
        refusal(&inventory, &counts)
    );

    let failed = complete.replacen(
        "\"records\":{\"executed\":10,\"passed\":10,\"failed\":0}",
        "\"records\":{\"executed\":10,\"passed\":9,\"failed\":1}",
        1,
    );
    assert!(refusal(&inventory, &failed).contains("1 failed case(s)"));

    let named = complete.replacen("\"failed_case_ids\":[]", "\"failed_case_ids\":[\"x\"]", 1);
    assert!(refusal(&inventory, &named).contains("names failed cases"));

    let problems = complete.replacen("\"problems\":[]", "\"problems\":[{\"id\":\"x\"}]", 1);
    assert!(refusal(&inventory, &problems).contains("problem record"));
}

#[test]
fn a_mapping_or_identity_mismatch_is_refused() {
    let inventory = inventory();
    let complete = complete(&inventory);
    let digest = complete.replacen(inventory.digest(), &"0".repeat(64), 1);
    assert!(refusal(&inventory, &digest).contains("mapping digest"));
    let identity = match accept(&complete, &inventory, "0000") {
        Ok(_) => panic!("an identity mismatch must be refused"),
        Err(refusals) => refusals.join(" | "),
    };
    assert!(identity.contains("package identity"), "{identity}");
}

#[test]
fn an_absent_malformed_or_unrecognized_verdict_is_refused() {
    let inventory = inventory();
    assert!(refusal(&inventory, "").contains("not JSON"));
    assert!(refusal(&inventory, "{\"claim\":").contains("not JSON"));
    assert!(refusal(&inventory, "{}").contains("declares no format"));
    let complete = complete(&inventory);
    let wrong_format = complete.replacen("lcl.conformance.verdict.v1", "something.else.v9", 1);
    assert!(refusal(&inventory, &wrong_format).contains("does not recognize"));
    let no_levels = complete.replacen(
        &format!(
            "\"levels\":[{}]",
            complete
                .split("\"levels\":[")
                .nth(1)
                .unwrap()
                .rsplit_once("],\"records\"")
                .unwrap()
                .0
        ),
        "\"levels\":[]",
        1,
    );
    assert!(refusal(&inventory, &no_levels).contains("level"));
    let no_records = complete.replacen(
        "\"records\":{\"executed\":10,\"passed\":10,\"failed\":0},",
        "",
        1,
    );
    assert!(refusal(&inventory, &no_records).contains("record accounting"));
}

/// The gate over the verdict this build actually renders.
///
/// The assertion is not that the verdict passes — it is that the gate's answer
/// is the report's own state, whichever that is, and that an accepted verdict
/// could only be one that established every obligation the reviewed inventory
/// requires.
#[test]
fn the_gate_answers_the_real_production_verdict() {
    let report = production::report(common::spec()).expect("the production report assembles");
    let rendered = report.render_verdict_json();
    let claim = report.claim().as_str().to_string();
    let inventory = inventory();
    match accept(
        &rendered,
        &inventory,
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
