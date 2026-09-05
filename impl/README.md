# LCL implementation — milestone M0 (foundation)

This directory is a **consumer** of the canonical specification at
`../canonical/LCL_Core_0.1.0`. It is not part of the release, is not listed in
`MANIFEST.json` or `SHA256SUMS.txt`, and never writes to `canonical/`.

Building this does not change the release's status. The `complete_example_parse_matrix`
and `semantic_case_execution` gates remain `OUT_OF_SCOPE` for LCL Core 0.1.0;
implementation conformance is separate evidence that M0 does not produce.

## Crates

| Crate | Role |
| --- | --- |
| `lcl-spec` | Specification authority loader. Verifies package integrity against `MANIFEST.json` and `SHA256SUMS.txt`, checks an **external trust anchor**, pins the formal version, loads the 12 closed registries and 2 catalogs as data. |
| `lcl-diagnostics` | Diagnostics skeleton. The 7 normative stages, 12 statuses and 77 errors, loaded and closure-checked. |
| `lcl-conformance` | Conformance skeleton. Indexes the 799 descriptive requirements and 66 decision witnesses. Executes nothing. |

## Trust boundary (M0.1)

`MANIFEST.json` and `SHA256SUMS.txt` live inside the package they describe, so
verifying against them proves only **internal self-consistency**. Anyone who
alters a payload file and regenerates both records produces a package that
verifies perfectly and is not the approved release.

`lcl-spec::APPROVED_PACKAGE` closes that gap. It is an immutable anchor
compiled into this crate — outside the package, where the package's own
metadata cannot reach it — holding the expected **identity digest**:

```text
SHA-256( "LCL-PACKAGE-IDENTITY-V1\n"
         || for each file, in ascending package-root-relative path order:
            "<sha256 hex>  <path>\n" )
```

All 176 files are covered, `MANIFEST.json`, `VALIDATION_REPORT.txt` and
`SHA256SUMS.txt` included: nothing is self-excluded, because the digest is not
stored in the package.

Approved identity for `canonical/LCL_Core_0.1.0`:
`00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed` (176 files).

| Entry point | Gates | Result |
| --- | --- | --- |
| `SpecPackage::open` | version pin + internal integrity + trust anchor | `Authority::Authoritative` |
| `SpecPackage::open_with_anchor` | same, against a supplied anchor | `Authority::Authoritative` |
| `SpecPackage::open_unverified` | none enforced; integrity computed and reported | `Authority::Unverified` — **never normative input** |

Internal verification is preserved, not replaced, and still runs first.
Approving a different package is a source change to `anchor.rs`, subject to
review; `mint_anchor` computes a candidate digest but never writes one.

## Design rules

1. **No transcription.** No registry table is written into Rust source. Every
   keyword, error, status and requirement is read from the canonical registries
   at load time. The `Stage` enum is the sole named exception and is *validated
   against* `diagnostic_selection.stage_order` rather than trusted.
2. **Fail closed.** Any hash mismatch, size mismatch, missing file, unrecorded
   file, count disagreement or version mismatch refuses the load.
3. **No third-party dependencies.** The trust root carries no supply-chain
   surface; SHA-256 and the JSON reader are implemented in `lcl-spec`.
4. **No false claims.** `lcl-conformance` has no `Pass` state and no `run()`.
   It is structurally incapable of reporting a conformance result.
5. **Authority is explicit.** `Authority` is a two-state enum, not a boolean
   buried in a struct, so an unverified package cannot be quietly mistaken for
   the approved release.

## Not in M0

No lexer, parser, AST, resolver, type checker, evaluator, capability kernel,
runtime, CLI or UI. The diagnostic *selection* algorithm
(`expression_demand_resolution`, supersession, duplicate suppression, stable
ordering, primary/secondary) is exposed as data and deliberately not
implemented.

## Use

```bash
cargo test --offline                                          # 50 tests
cargo run --offline -p lcl-conformance --example m0_report    # foundation report
cargo run --offline -p lcl-spec --example mint_anchor         # compute a package identity
```

Both are read-only with respect to `canonical/`. Integrity tests that need to
mutate a package operate on a throwaway copy under `target/test-tmp/`.
