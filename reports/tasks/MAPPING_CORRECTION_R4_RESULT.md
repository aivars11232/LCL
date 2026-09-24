# Approved mapping correction — obligations r4 (2026-09-24)

Applies one explicit owner approval, and nothing beyond it.

> Approved

The approval answers the request recorded at the close of the `a665cfc` audit
response: a mapping correction for `core.execute precondition/profile-out-of-bounds`
on the same basis as the four `core.sort` sub-runs closed by r3, namely that the
condition is not constructible. Revision r3 stated explicitly that this
obligation was *outside* that earlier approval and remained pinned and
unestablished; this correction is the separate decision r3 said would be needed.

## 1. The proof, re-verified at this HEAD

`ProfileFault::OutOfBounds` is raised in exactly one place,
`check_bounds` in `impl/crates/lcl-capabilities/src/profile.rs`, down two
paths. On the `core.execute` row both are dead.

**Path 1 — axis bounds.** The check returns the declared effects and
dependencies lying outside the row's maximum:

    if row.determinism != RowDeterminism::Inherited {
        let effects = profile.axes.effects_outside(&row.maximum);
        ...
        let dependencies = profile.axes.dependencies_outside(&row.maximum);
    }

`core.execute` is `derived`, not `inherited`, so the branch *is* entered and
the bounds are genuinely checked — the claim is not that the check is skipped.
It is that nothing can fall outside. Read from
`canonical/LCL_Core_0.1.0/10_REGISTRIES/operations_v0.1.0.json`:

| axis | registry vocabulary | `core.execute` maximum | omitted |
|---|---|---|---|
| dependencies | 5 | 4 | `declared_state_only` |
| effects | 8 | 7 | `none` |

The two omitted entries are not declarable. In
`impl/crates/lcl-capabilities/src/address.rs` they are `&'static str`
sentinels, not enum variants:

    pub const NONE: &'static str = "declared_state_only";   // Dependency
    pub const NONE: &'static str = "none";                  // Effect

and the declarable vocabularies are `Dependency::ALL` (4 members) and
`Effect::ALL` (7 members) — every one of them permitted by this row. A
`Profile`'s axes are typed by those enums, so `effects_outside` and
`dependencies_outside` return empty for every profile constructible against
`core.execute`.

**Path 2 — determinism.** The remaining path requires
`row.determinism == RowDeterminism::Deterministic`. The registry gives
`core.execute` the determinism category `derived`, so the row is
`RowDeterminism::Derived` and the branch is never entered.

No input reaches the condition. Constructing one would require either widening
`core.execute`'s maximum or adding a declarable axis member — both of which the
standing instruction forbids, and neither of which this correction does.

## 2. Why this is confined to one operation

The label is **not** retired from the vocabulary. It stays pinned, reachable
and established on four other rows, whose maxima are strictly narrower:

| operation | declarable deps permitted (of 4) | declarable effects permitted (of 7) |
|---|---|---|
| `core.analyze` | 3 | 1 |
| `core.verify`  | 2 | 1 |
| `core.report`  | 1 | 1 |
| `core.publish` | 2 | 2 |

`core.execute` is the only row in the mapping whose maximum spans both full
declarable enums, and therefore the only one on which the fault is
unconstructible.

## 3. The change, and its exact extent

| | |
|---|---|
| New mapping | `impl/crates/lcl-conformance/src/obligations_v0.1.0_r4.json`, `"revision": "r4"` |
| Digest | `c592f8d9e0b5feb69785256395c9b67cac08932cec96c492a3786ac6b6cd780e` |
| Pinned in | `obligations.rs` as `MAPPING_DIGEST` |
| Superseded | `obligations_v0.1.0_r3.json` removed, as r2 was when r3 landed |

Measured delta against r3 (read from git at `a665cfc`, not from memory):

    revision        r3 -> r4
    package identity  unchanged (00d648b1...67ed)
    obligation rows   980 -> 980
    rows with subruns 319 -> 319
    probes          2,413 -> 2,413   (2,011 source + 402 semantics)
    sub-run pins    3,721 -> 3,720
    REMOVED  ('semantic/operation_errors/core.execute',
              'precondition/profile-out-of-bounds')
    ADDED    (none)

Exactly one pin removed. No level reclassified, no probe dropped, no row
dropped, no expected output altered to match behaviour, no skip or ignore
added. The row and its probe remain and are still judged on their other pins.

## 4. Result — conformance and the real readiness gate

    required probes: 2413
    source_conforming    2011 required, 2011 satisfied, 0 failed, 0 missing, 0 invalid
    semantics_conforming  402 required,  402 satisfied, 0 failed, 0 missing, 0 invalid
    CLAIM: semantics_conforming

    $ cargo run -p lcl-conformance --example m8_conformance_gate
    ACCEPTED: claim semantics_conforming
      source_conforming: 2011 required by the reviewed inventory, all satisfied
      semantics_conforming: 402 required by the reviewed inventory, all satisfied
      mapping digest c592f8d9...d780e
      package identity 00d648b1...67ed
    exit 0

402/402. The gate accepts for the first time. The claim upgrade is produced by
`report.rs::claim()` on its own — nothing asserts it into place.

## 5. Two pins that recorded the blocked state, now corrected

Both encoded "blocked" as the expected answer and would have failed against a
correct run. Neither was changed to hide a defect; both were changed because
the state they pinned is no longer the state, and that was established from the
report *before* either was touched.

- `impl/crates/lcl-conformance/tests/production_report.rs` pinned
  `ClaimLevel::Source` and `"source_conforming"`. Every per-probe assertion in
  that test passed unchanged; only the two claim pins failed. They now pin
  `Semantics` / `"semantics_conforming"`, so a regression at either level still
  fails the test.
- `/mnt/F/.lcl-pretest/c1/c5-gate.sh` recorded `run readiness-gate 1`. A
  wrapper that expects the gate to refuse is recording a blocked state; it now
  expects 0.

`run validate-0.2.0 1` was **not** changed. Core 0.2's
`language_decisions_and_release_state` is still BLOCKED on the owner's pending
independent review, which is owner-only and remains open.

## 6. Verification run

Wrapper selftest first, so the evidence below is from a harness proven to
catch failures: `exits-three` recorded 3, `quiet-failure` recorded 1 despite
printing `test result: ok. 9 passed; 0 failed`, and the gate failed on both —
"SELFTEST PASSED".

| phase | result |
|---|---|
| a | `fmt` 0, `clippy -D warnings` 0, workspace tests **1825 passed, 0 failed, 1 ignored** |
| b | MSRV 1.75.0 `check` 0, tests **1825 passed, 0 failed, 1 ignored** |
| c | 15 commands; Core 0.1 identity `00d648b1...67ed` **176 files**, Core 0.2 `00daee8d...e604` 216 files; conformance 402/402; **readiness-gate exit 0**; protected paths clean |
| d | 12 sequential + 18 concurrent `real_process` runs, all 0, no flake |

**50 commands, each with its expected status.** The single ignored test is the
known parent-driven child test recorded in `LCL_RELEASE_BASELINE.md` §5.7.

Gate `c` also confirmed no stray writes: `impl/target/test-tmp` 0 files,
`/tmp/lcl-apps` 0 files, and `canonical/ releases/ assets/` unmodified.

## 7. Worktree

Four entries, all this correction:

    D   impl/crates/lcl-conformance/src/obligations_v0.1.0_r3.json
    M   impl/crates/lcl-conformance/tests/production_report.rs
    M   reports/implementation/LCL_CONFORMANCE_OBLIGATIONS.md
    ??  reports/tasks/MAPPING_CORRECTION_R4_RESULT.md   (this report)

The three tracked changes are what the gates above were run against; the report
was written afterwards and changes no behaviour.

`obligations.rs`, `obligations_v0.1.0_r4.json` and the `LCL_RELEASE_BASELINE.md`
supersession note were already committed by the owner as `464fe36`. The r3
removal and the companion-doc update are the remainder of the established
process for that same revision; git will record the removal as a rename to r4,
exactly as r2 -> r3 was recorded (`R099`) in `a665cfc`.

## 8. What remains, and what this does not claim

This correction establishes the semantic half of the readiness contract. It
does **not** release anything.

Still open, all owner-only:

1. **Commit** these three files. No Git write was performed here.
2. **Freeze** that commit as the build source, then **rebuild the candidate**.
   The accepted `lcl-0.2.0` candidate (`...-8f2f0454e437`) carries
   `obligations_v0.1.0_r2.json` in its `SOURCE_INVENTORY.tsv`, so it is two
   revisions behind and is superseded with that mapping; the same is true of
   `...-a0006c38fb79`. Their conformance evidence should not be carried
   forward.
3. **Rerun the installed smoke** against the rebuilt candidate (150/150
   previously).
4. **Core 0.2 `independent_review`** — `validate-0.2.0` still exits 1 until an
   independent reviewer records that decision. Nothing in this correction
   touches it, and the conformance claim above is not a substitute for it.

The correction does not alter Core 0.1 semantics, does not expand any
operation's capabilities, and leaves `canonical/LCL_Core_0.1.0` byte-identical
at its pinned identity.
