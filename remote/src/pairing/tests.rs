//! The pairing flow, step by step, against a state directory of its own.

use super::*;

fn pairing(name: &str) -> (Pairing, Registry, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("lcl-remote-pairing-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let paths = Paths::under(&root);
    (Pairing::new(&paths), Registry::new(&paths), root)
}

const PC: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn present(
    pairing: &Pairing,
    registry: &Registry,
    code: &str,
    fingerprint: &str,
    now: u64,
) -> Result<Presented, Refusal> {
    pairing.present(
        code,
        fingerprint,
        "Phone",
        PC,
        registry,
        "lcl.remote/1",
        now,
    )
}

fn pending(presented: Result<Presented, Refusal>) -> Candidate {
    match presented {
        Ok(Presented::Pending(candidate)) => candidate,
        other => panic!("not pending: {other:?}"),
    }
}

/// A11-R10: the exact values the Android app is tested against too.
#[test]
fn verification_codes_match_the_published_vectors() {
    let code = b64::encode(&[7u8; 32]);
    assert_eq!(code, "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc");
    let hash = lcl_spec::sha256::hex_digest(code.as_bytes());
    assert_eq!(
        hash,
        "dc4bf80c77473d130fa0de86ba4018fe98bb214005e6a5891d12ba91446f9e81"
    );
    assert_eq!(
        verification(&"a".repeat(64), &hash, &"b".repeat(64)),
        "edbb-8bd3-ad82"
    );
    let code: Vec<u8> = (0u8..32).collect();
    let code = b64::encode(&code);
    let hash = lcl_spec::sha256::hex_digest(code.as_bytes());
    let pc = lcl_spec::sha256::hex_digest(b"pc");
    assert_eq!(
        verification(&pc, &hash, &lcl_spec::sha256::hex_digest(b"phone")),
        "bd05-957e-fdfc"
    );
    assert_eq!(
        verification(&pc, &hash, &lcl_spec::sha256::hex_digest(b"another phone")),
        "d81e-cb19-d16e"
    );
}

#[test]
fn a_code_only_ever_makes_a_pending_candidate_and_is_never_stored() {
    let (pairing, registry, root) = pairing("pending");
    let (challenge, code) = pairing.create(1_000, 300).unwrap();
    let first = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
    assert_eq!(first.status, Status::Pending);
    assert_eq!(
        first.verification,
        verification(PC, &challenge.hash, &"a".repeat(64))
    );
    // Asked again: the same request, not a second one.
    let again = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_050));
    assert_eq!(again.request, first.request);
    assert_eq!(pairing.candidates(1_050).unwrap().len(), 1);
    assert!(registry.list().unwrap().is_empty());
    // Only a hash is stored.
    let stored = std::fs::read_to_string(root.join("state/lcl/remote/pairing.json")).unwrap();
    assert!(!stored.contains(&code));
    // Unknown and expired codes make nothing.
    assert_eq!(
        present(&pairing, &registry, "not-a-code", &"a".repeat(64), 1_001),
        Err(Refusal::Unknown)
    );
    assert_eq!(
        present(&pairing, &registry, &code, &"b".repeat(64), 1_300),
        Err(Refusal::Expired)
    );
    assert_eq!(
        present(&pairing, &registry, &code, &"a".repeat(64), 1_300),
        Err(Refusal::Expired)
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn approval_trusts_one_certificate_once_and_spends_the_code() {
    let (pairing, registry, root) = pairing("approve");
    let (_, code) = pairing.create(1_000, 300).unwrap();
    let a = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
    let b = pending(present(&pairing, &registry, &code, &"b".repeat(64), 1_002));
    pairing.approve(&a.request, 1_010).unwrap();
    // B was superseded, and cannot be approved or finish.
    assert!(pairing.approve(&b.request, 1_011).is_err());
    assert_eq!(
        present(&pairing, &registry, &code, &"b".repeat(64), 1_012),
        Err(Refusal::Used)
    );
    let Ok(Presented::Paired(device)) = present(&pairing, &registry, &code, &"a".repeat(64), 1_020)
    else {
        panic!("the approved candidate did not pair")
    };
    assert_eq!(device.fingerprint, "a".repeat(64));
    assert_eq!(registry.authorize(&"a".repeat(64)).unwrap().id, device.id);
    // Asked again after it paired: the same device, not another.
    assert_eq!(
        present(&pairing, &registry, &code, &"a".repeat(64), 1_021),
        Ok(Presented::Paired(device.clone()))
    );
    assert_eq!(registry.list().unwrap().len(), 1);
    // A new certificate with the spent code gets nothing.
    assert_eq!(
        present(&pairing, &registry, &code, &"d".repeat(64), 1_022),
        Err(Refusal::Used)
    );
    // Revoked: the old request does not bring it back.
    registry.revoke(&device.id, 1_030).unwrap();
    assert_eq!(
        present(&pairing, &registry, &code, &"a".repeat(64), 1_031),
        Err(Refusal::Used)
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_crash_between_approval_and_trust_is_finished_only_by_the_approved_certificate() {
    let (pairing, registry, root) = pairing("crash");
    let (_, code) = pairing.create(1_000, 300).unwrap();
    let a = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
    pending(present(&pairing, &registry, &code, &"b".repeat(64), 1_002));
    pairing.approve(&a.request, 1_010).unwrap();
    // Step 1 happened and the process died: the id is reserved, nothing
    // is trusted yet. Even after the code's expiry only A can finish.
    pairing
        .update(1_011, |state| {
            state.candidates[0].device = Some("0011223344556677".into());
            Ok::<_, String>(())
        })
        .unwrap();
    assert!(registry.list().unwrap().is_empty());
    assert_eq!(
        present(&pairing, &registry, &code, &"b".repeat(64), 1_400),
        Err(Refusal::Used)
    );
    assert!(pairing.approve(&a.request, 1_012).is_err());
    // Step 2 happened too: the record exists; finishing does not double it.
    registry
        .enroll(
            "0011223344556677",
            "Phone",
            &"a".repeat(64),
            "lcl.remote/1",
            1_013,
        )
        .unwrap();
    let Ok(Presented::Paired(device)) = present(&pairing, &registry, &code, &"a".repeat(64), 1_400)
    else {
        panic!("the approved candidate could not finish")
    };
    assert_eq!(device.id, "0011223344556677");
    assert_eq!(registry.list().unwrap().len(), 1);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn denial_is_final_for_its_candidate_and_leaves_the_code_to_others() {
    let (pairing, registry, root) = pairing("deny");
    let (_, code) = pairing.create(1_000, 300).unwrap();
    let stranger = pending(present(&pairing, &registry, &code, &"e".repeat(64), 1_001));
    pairing.deny(&stranger.request, 1_002).unwrap();
    assert_eq!(
        present(&pairing, &registry, &code, &"e".repeat(64), 1_003),
        Err(Refusal::Denied),
        "a denied candidate came back"
    );
    assert!(pairing.approve(&stranger.request, 1_004).is_err());
    let phone = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_005));
    pairing.approve(&phone.request, 1_006).unwrap();
    assert!(matches!(
        present(&pairing, &registry, &code, &"a".repeat(64), 1_007),
        Ok(Presented::Paired(_))
    ));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn withdrawing_an_approval_leaves_the_code_to_nobody() {
    let (pairing, registry, root) = pairing("withdraw");
    let (_, code) = pairing.create(1_000, 300).unwrap();
    let a = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
    pairing.approve(&a.request, 1_002).unwrap();
    pairing.deny(&a.request, 1_003).unwrap();
    assert_eq!(
        present(&pairing, &registry, &code, &"a".repeat(64), 1_004),
        Err(Refusal::Denied)
    );
    assert_eq!(
        present(&pairing, &registry, &code, &"b".repeat(64), 1_005),
        Err(Refusal::Used)
    );
    assert!(registry.list().unwrap().is_empty());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn an_expired_code_approves_nothing() {
    let (pairing, registry, root) = pairing("expired");
    let (_, code) = pairing.create(1_000, 300).unwrap();
    let a = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
    assert!(pairing.approve(&a.request, 1_300).is_err());
    let (_, code) = pairing.create(2_000, 300).unwrap();
    let b = pending(present(&pairing, &registry, &code, &"b".repeat(64), 2_001));
    pairing.approve(&b.request, 2_002).unwrap();
    // Approved, but the phone did not come back before the code expired.
    assert_eq!(
        present(&pairing, &registry, &code, &"b".repeat(64), 2_300),
        Err(Refusal::Expired)
    );
    assert!(registry.list().unwrap().is_empty());
    assert!(pairing.candidates(2_300).unwrap().is_empty());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn waiting_candidates_are_bounded() {
    let (pairing, registry, root) = pairing("bounded");
    let fingerprint = |i: usize| format!("{i:064x}");
    let (_, code) = pairing.create(1_000, 300).unwrap();
    for i in 0..MAX_PENDING_PER_CHALLENGE {
        pending(present(&pairing, &registry, &code, &fingerprint(i), 1_001));
    }
    assert_eq!(
        present(&pairing, &registry, &code, &fingerprint(99), 1_002),
        Err(Refusal::Busy)
    );
    // The ones already waiting are still answered, and not doubled.
    pending(present(&pairing, &registry, &code, &fingerprint(0), 1_003));
    assert_eq!(
        pairing.candidates(1_003).unwrap().len(),
        MAX_PENDING_PER_CHALLENGE
    );
    // Across codes, the whole PC is bounded too.
    let mut n = 100;
    let mut refused = false;
    for _ in 0..MAX_PENDING / MAX_PENDING_PER_CHALLENGE + 1 {
        let (_, code) = pairing.create(1_000, 300).unwrap();
        for _ in 0..MAX_PENDING_PER_CHALLENGE {
            n += 1;
            if present(&pairing, &registry, &code, &fingerprint(n), 1_004) == Err(Refusal::Busy) {
                refused = true;
            }
        }
    }
    assert!(refused);
    assert_eq!(pairing.candidates(1_004).unwrap().len(), MAX_PENDING);
    // Once the codes expire, their candidates are pruned on the next write.
    pairing.create(2_000, 300).unwrap();
    let stored = pairing.load().unwrap();
    assert!(stored.candidates.is_empty(), "expired candidates were kept");
    assert!(registry.list().unwrap().is_empty());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn unreadable_pairing_state_refuses_everything() {
    let (pairing, registry, root) = pairing("corrupt");
    let (_, code) = pairing.create(1_000, 300).unwrap();
    let a = pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
    let file = root.join("state/lcl/remote/pairing.json");
    for broken in [
        "{broken".to_string(),
        "{\"version\":3,\"challenges\":[],\"candidates\":[]}".to_string(),
        std::fs::read_to_string(&file)
            .unwrap()
            .replace("\"pending\"", "\"trusted\""),
    ] {
        std::fs::write(&file, broken).unwrap();
        assert!(matches!(
            present(&pairing, &registry, &code, &"a".repeat(64), 1_002),
            Err(Refusal::Unreadable(_))
        ));
        assert!(pairing.approve(&a.request, 1_002).is_err());
        assert!(pairing.candidates(1_002).is_err());
    }
    assert!(registry.list().unwrap().is_empty());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_version_1_state_file_is_still_read() {
    let (pairing, registry, root) = pairing("v1");
    let file = root.join("state/lcl/remote/pairing.json");
    paths::write_private(
        &file,
        b"{\"version\":1,\"challenges\":[{\"id\":\"00\",\"hash\":\"00\",\"created\":1,\
          \"expires\":2,\"consumed_at\":1,\"consumed_by\":\"x\"}]}",
    )
    .unwrap();
    let (_, code) = pairing.create(1_000, 300).unwrap();
    pending(present(&pairing, &registry, &code, &"a".repeat(64), 1_001));
    std::fs::remove_dir_all(&root).unwrap();
}
