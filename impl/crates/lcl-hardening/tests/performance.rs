//! Phase C: how long the engine takes, against the thresholds it declared.
//!
//! These are regression guards, not benchmarks. `LCL_RELEASE_BASELINE.md` 5.2
//! sets each ceiling well above the measured median on the reference machine,
//! so an ordinarily loaded machine does not turn a correctness gate into a coin
//! flip while a real regression of an order of magnitude still fails.
//!
//! Everything here runs in the `dev` profile, because that is the profile the
//! test suite runs in. A release build is faster and the release report records
//! both.

use lcl_hardening::{canonical_root, corpus, empty_provider, engine, unit};
use lcl_protocol::{Engine, Inputs};
use lcl_runtime::MockHost;
use std::time::{Duration, Instant};

/// `LCL_RELEASE_BASELINE.md` 5.2.
const PACKAGE_OPEN_CEILING: Duration = Duration::from_millis(2_000);
const STAGE_CEILING: Duration = Duration::from_millis(1_000);
const RUN_CEILING: Duration = Duration::from_millis(2_000);

fn valid_examples() -> Vec<(String, String)> {
    corpus::canonical_examples(&canonical_root())
        .iter()
        .filter(|(name, _)| !name.contains("invalid"))
        .map(|(name, source)| (name.to_string(), source.to_string()))
        .collect()
}

#[test]
fn opening_the_package_stays_under_its_ceiling() {
    // 176 files verified against the trust anchor, which is the dominant cost
    // of any single measurement and the reason a consumer opens one engine.
    let started = Instant::now();
    let engine = Engine::open(canonical_root()).expect("the package is authoritative");
    let elapsed = started.elapsed();
    assert!(
        elapsed < PACKAGE_OPEN_CEILING,
        "opening the package took {elapsed:?}, past the {PACKAGE_OPEN_CEILING:?} ceiling"
    );
    // The identity is what was verified, so a fast open that verified nothing
    // would not pass this test.
    assert_eq!(
        engine.spec_record().identity_digest,
        lcl_spec::APPROVED_PACKAGE.identity_digest
    );
}

#[test]
fn every_canonical_stage_stays_under_its_ceiling() {
    let engine = engine();
    let mut worst = (String::new(), Duration::ZERO);
    for (name, source) in valid_examples() {
        for command in ["check", "validate", "inspect"] {
            let started = Instant::now();
            let report = match command {
                "check" => engine.check(&unit(&source), &empty_provider()),
                "validate" => engine.validate(&unit(&source), &empty_provider(), &Inputs::new()),
                _ => engine.inspect(&unit(&source), &empty_provider(), &Inputs::new()),
            };
            let elapsed = started.elapsed();
            // The measurement is only meaningful if the work actually happened.
            assert!(
                !report.units.is_empty(),
                "{name} produced no unit under {command}"
            );
            if elapsed > worst.1 {
                worst = (format!("{name} {command}"), elapsed);
            }
            assert!(
                elapsed < STAGE_CEILING,
                "{name} {command} took {elapsed:?}, past the {STAGE_CEILING:?} ceiling"
            );
        }
    }
    eprintln!("slowest stage: {} at {:?}", worst.0, worst.1);
}

#[test]
fn a_full_run_stays_under_its_ceiling() {
    let engine = engine();
    let mut worst = (String::new(), Duration::ZERO);
    for (name, source) in valid_examples() {
        let mut stdlib = engine.stdlib().expect("the standard library assembles");
        let mut host = MockHost::new();
        let started = Instant::now();
        let report = engine.run(
            &unit(&source),
            &empty_provider(),
            &Inputs::new(),
            &mut stdlib,
            &mut host,
        );
        let elapsed = started.elapsed();
        assert!(
            report.primary().is_some() || report.terminal_status().is_some(),
            "{name} ended with neither a diagnostic nor a terminal status"
        );
        if elapsed > worst.1 {
            worst = (name.clone(), elapsed);
        }
        assert!(
            elapsed < RUN_CEILING,
            "running {name} took {elapsed:?}, past the {RUN_CEILING:?} ceiling"
        );
    }
    eprintln!("slowest run: {} at {:?}", worst.0, worst.1);
}

#[test]
fn memory_does_not_grow_across_repeated_work() {
    // An engine reused across many documents must not accumulate. There is no
    // portable allocator counter in std, so the observable proxy is that the
    // hundredth pass costs what the first did: a leak that mattered would show
    // as growth in time or as an allocation failure, not as a constant.
    let engine = engine();
    let examples = valid_examples();
    let mut first = Duration::ZERO;
    let mut last = Duration::ZERO;
    for pass in 0..100 {
        let started = Instant::now();
        for (_, source) in &examples {
            engine.check(&unit(source), &empty_provider());
        }
        let elapsed = started.elapsed();
        match pass {
            0 => first = elapsed,
            99 => last = elapsed,
            _ => {}
        }
    }
    eprintln!("first pass {first:?}, hundredth pass {last:?}");
    assert!(
        last < first * 4 + Duration::from_millis(50),
        "the hundredth pass took {last:?} against a first pass of {first:?}"
    );
}
