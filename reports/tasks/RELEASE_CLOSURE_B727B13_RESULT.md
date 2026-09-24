# Release closure from `b727b13` — result

Date: 2026-09-24. Entry and frozen build source: `main` @
`b727b13e9b16dc1ba748bb82c1a5f9520ad743e6` ("Record final release closure;
mark stale BLOCKED statements historical"), clean worktree, equal to the
locally recorded `origin/main` (0 ahead, 0 behind; no fetch was made).

**Verdict: `BLOCKED_ON_INDEPENDENT_REVIEW`.**

This run repeats the release closure of
`reports/tasks/FINAL_RELEASE_CLOSURE_RESULT.md`, this time from the commit that
now holds that report and its two documentation corrections. The earlier
candidate `…d7a26f4e34ff` was built from `43a9917`, so its source archive still
carries the two pre-correction texts (that report's finding L2). No evidence
from it was reused here. It is now superseded. Every requirement under the
implementation's control passed again. Core 0.2's `independent_review` is the
only thing that remains, and this run did not record it.

The run made no Git write and changed nothing under `canonical/`. The only
repository changes are the new candidate directory and this report.

## 1. Source

| | |
|---|---|
| `RELEASE_SOURCE_COMMIT` | `b727b13e9b16dc1ba748bb82c1a5f9520ad743e6` |
| Worktree at entry | 0 porcelain entries, no stash |
| Change since `43a9917` (the last implementation change) | `A releases/candidates/…d7a26f4e34ff/*` (6), `A reports/tasks/FINAL_RELEASE_CLOSURE_RESULT.md`, `M impl/README.md`, `M reports/implementation/LCL_CONFORMANCE_OBLIGATIONS.md`. No source, test, mapping or canonical file changed. The gate was re-run in full regardless. |
| Mapping | r4, `impl/crates/lcl-conformance/src/obligations_v0.1.0_r4.json`, SHA-256 `c592f8d9e0b5feb69785256395c9b67cac08932cec96c492a3786ac6b6cd780e` |
| Core 0.1 identity | `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, 176 files |
| Core 0.2 identity | `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`, 216 files |

## 2. Pre-build gate

The same gate as before, copied as `/mnt/F/.lcl-pretest/rc2/rc2-gate.sh`. The
copy differs only in its results file, marker and `RC2-G-` log prefix. Its
self-test ran first and passed: an exit-3 command, and a command printing
`test result: ok` while exiting 1, were both recorded and both failed the gate.

**50 commands, each at its expected status** (`rc2/rc2-results.tsv`).

| Command | Exit | Expected | Result |
|---|---:|---:|---|
| `cargo fmt --all -- --check` | 0 | 0 | clean |
| clippy `--workspace --all-targets -- -D warnings` (1.98.1) | 0 | 0 | clean |
| `cargo test --workspace --all-targets --no-fail-fast` (1.98.1) | 0 | 0 | 164 blocks, **1,825 passed, 0 failed, 1 ignored** |
| MSRV 1.75.0 `cargo check` | 0 | 0 | clean |
| MSRV 1.75.0 `cargo test` | 0 | 0 | 164 blocks, **1,825 passed, 0 failed, 1 ignored** |
| Core 0.1 `SHA256SUMS.txt` / `validate_release.py --scope all` | 0 / 0 | 0 / 0 | verifies; 31 PASS / 0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE |
| Core 0.2 `SHA256SUMS.txt` | 0 | 0 | verifies |
| Core 0.2 `validate_release.py --scope all` | 1 | **1** | 1 BLOCKED: `pending_decisions: ["independent_review"]` |
| Core 0.2 `validate_language_contracts` / `_localization` / `_source_fixtures` | 0 | 0 | pass |
| `validate_ebnf.py`, both grammars | 0 | 0 | pass |
| package identity, both packages | 0 | 0 | `00d648b1…` 176, `00daee8d…` 216 |
| brand assets | 0 | 0 | verify |
| `m8_conformance_report` | 0 | 0 | 2,413 required; source **2,011/2,011**, semantic **402/402**, 0 failed / missing / invalid; `CLAIM: semantics_conforming` |
| **`m8_conformance_gate`** | **0** | **0** | **`ACCEPTED: claim semantics_conforming`**, mapping `c592f8d9…`, package `00d648b1…` |
| protected paths | 0 | 0 | `canonical/`, `releases/` and `assets/` are clean; every candidate verifies; 0 stray writes |
| `real_process`: 12 sequential + 3 rounds of 6 concurrent | 0 ×30 | 0 | **30 of 30**, 12 of 12 tests each |

**Hardening suites** (inside the workspace run, all 0 failed):
`security_matrix` 6, `adversarial_inputs` 11, `fuzz_stages` 6,
`repeatability` 5, `applications` 9, `performance` 4, `installed_launcher` 22,
`release_build` 12, `protocol_and_tooling` 5, `packaging_smoke` 5 (finding
M1 still applies), `grants` 14, `capability_boundary` 11.

**Ignored-test accounting.** `grep -rn '#\[ignore'` finds exactly one:
`impl/crates/lcl-project/tests/manifest_input_bounds.rs:72`,
`child_parses_nested_manifest`, "driven by its parent, which supplies the
depth". Both toolchains report that test as the only ignored one. Its parent
passes in the same binary (3 passed, 1 ignored), and the parent runs the child
and asserts on the depth line the child prints.

## 3. Candidate

Built after the gate had passed, from the clean tree, with the established
`LCL_RELEASE_VERSION=0.2.0 packaging/build_release.sh` (TMPDIR
`/tmp/lcl-rc2-build`, log `logs/RC2-build.log`, exit 0):

```
releases/candidates/lcl-0.2.0-linux-x86_64-787fd7aca090/
```

| | |
|---|---|
| **Build-source commit** | `b727b13e9b16dc1ba748bb82c1a5f9520ad743e6` |
| **Uncommitted entries** | **0**; no `SOURCE_CHANGES.patch` |
| **Source id** (SHA-256 of `SOURCE_INVENTORY.tsv`) | `787fd7aca090d1db625452cd0f6eed5037703313245bca84a9f2bcc85d8a65c3`, **1,020 files** |
| Source archive SHA-256 | `72fb80d533054147dc8ea88c16d3f6db4df43a61260da7e855edd02562b835f2` |
| **Artifact SHA-256** | `1280a92a317a5767d87cd8844b589f3407655fc4b69b99455a3d2afabda358f9` |
| `bin/lcl` / `bin/lcl-workspace` | `413f34e7…4fbf0` / `66474a5d…79e30` |
| Toolchain | 1.98.1, declared minimum 1.75, `--release --offline --locked`, lockfile `c51be147…3a60` |
| Release / product / language / protocol | 0.2.0 / 0.1.0 / `0.1.0 0.2.0` / `lcl.engine/1` |

`rc/rc_verify_candidate.sh` was run unchanged against `b727b13`: **37
verdicts, 0 failures** (`rc2/verify-787fd7aca090.log`). The checks:

* Both archives verify their checksums.
* The source id recomputes, and the provenance names `b727b13` with 0
  uncommitted entries.
* The inventory's 1,020 paths and every digest equal the `b727b13` blobs, read
  from Git; 0 differ.
* The source archive holds exactly the inventory's files.
* The only mapping inside is r4 (`c592f8d9…`), and the archived `obligations.rs`
  pins that digest.
* The known strays are absent.
* Both bundled packages carry their identities and are byte-identical to
  `canonical/`.

**Nothing was copied forward.** Each of the seven candidates names a different
build commit in its provenance, the build wrote only its own directory, and all
seven candidates plus `releases/lcl-0.1.0-linux-x86_64.tar.gz` still verify.

`bin/lcl` has the same SHA-256 as the `43a9917` candidate's. That fits the
fact that no implementation source changed. It is an observation only;
bit-for-bit reproducibility is not claimed.

## 4. Installed verification

The installed chain ran against this candidate in one fresh root
(`rc2/accept`), under `env -i` with a disposable HOME/XDG, in the required
order.

| Harness | Verdicts | Exit |
|---|---|---:|
| `t4_g9_e1_install_cli.sh`: archive, install, installed CLI, all VALID/INVALID examples, package equality, installed binaries = provenance | 57 PASS, 0 FAIL | 0 |
| `t4_g9_e2_launch_http.py`: menu/document launch, loopback HTTP, token and Host refusal, grants | 35 PASS, 0 FAIL | 0 |
| `f4_localized_installed.py`: installed 0.2.0 package, localized `run` in 5 locales, fail-closed, 0.1.0 equality, drift | 33 PASS, 0 FAIL | 0 |
| `f4_extra_smoke.py`: F10 decision A, nested paths, locale-profile limits | 14 PASS, 0 FAIL | 0 |
| `t4_g9_e5_uninstall_reinstall.py`: uninstall, operator files preserved, reinstall | 11 PASS, 0 FAIL | 0 |

**`INSTALLED_CHECKS=150/150`.** The logs are `logs/RC2-E1.log`, `RC2-E2.log`,
`RC2-LOC.log`, `RC2-EXTRA.log` and `RC2-E5.log`.

`rc/rc_supplementary_installed.sh` ran in a separate fresh root
(`rc2/accept-s`): **29 PASS, 0 FAIL** (`logs/RC2-SUPP.log`).

* **S1/S2.** All 416 payload files and all 425 installed files contain none of
  `/mnt/F/LCL`, `/tmp/lcl-rc2-build`, the builder's home or `.cargo/registry`.
* **Elevation.** Installed as uid 1000; neither script calls an elevation tool.
* **S3, `env -i` with no PATH or HOME, from `/`:**
  * `lcl version` reports 0.1.0 and `lcl.engine/1`, and `0.1.0 0.2.0` with the
    localized package.
  * `lcl spec` reports `00d648b1…`, and `lcl check` reports `00daee8d…`.
  * VALID/01 runs to `status.succeeded`.
  * INVALID/01 exits 1 with `error.keyword.case`.
  * With no package, exit 4.
* **S4.** Uninstall leaves no installation-owned path (the residue is finding
  L4, as before).
* **S5.** Reinstall reproduces all 412 owned files byte for byte.

After all of this, the candidate's checksums still verify, and its provenance
is unchanged.

## 5. Documentation

Current-facing documents were inspected, and none needed a change:

* `README.md` states Core 0.2 exactly as the package does and names no current
  candidate.
* `impl/README.md` and `LCL_CONFORMANCE_OBLIGATIONS.md` already carry the
  dated 2026-09-24 status (committed in `b727b13`). The older "BLOCKED" text
  in them is marked historical.
* The only other mentions of a candidate identity are in `reports/tasks/`,
  which is historical by construction.

`FINAL_RELEASE_CLOSURE_RESULT.md` is left as written. This report supersedes
its candidate (`…d7a26f4e34ff`) with `…787fd7aca090`, and finding L2 of that
report is closed by it.

## 6. Core 0.2 — independent-review boundary

Read directly from `canonical/LCL_Core_0.2.0` on `b727b13`:

* `00_RELEASE/05_LANGUAGE_CLOSURE.json`: 8 of 9 decisions `closed`;
  `independent_review` **`pending`**; `release_gate_permitted` **`false`**.
* `01_RELEASE_STATUS_AND_BOUNDARY.txt`: `PACKAGE_STATUS: UNRELEASED_CANDIDATE`.
* `MANIFEST.json`: `localization_feature_candidate`, `release_ready: false`.

The decision's own summary gives the condition: the localization additions
"have not received an independent review, so this decision remains pending
until the owner accepts such a review."

### Independent reviewer handoff

* **Candidate**: `releases/candidates/lcl-0.2.0-linux-x86_64-787fd7aca090`,
  artifact `1280a92a…58f9`, built from `b727b13`.
* **Package to review**: Core 0.2 `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`.
* **Scope**: the localization additions against Core 0.1.0: 40 new files, 28
  changed files and 3 regenerated integrity files. The full list is in
  `/mnt/F/.lcl-pretest/rc/core-0.2-review-scope.txt`, and §6 of
  `FINAL_RELEASE_CLOSURE_RESULT.md` describes it. The package is unchanged
  since then, so that scope still applies exactly.
* **Questions the review must answer**:
  * Is the localization surface closed?
  * Are locale selection, the directive and pinning deterministic?
  * Are original-byte locations exact?
  * Do the nine localization errors carry correct stage, phase and status
    contracts?
  * Does every fixture's expected result follow from the text?
  * Does every valid Core 0.1.0 program keep its meaning, including F10 =
    atomic decode?
* **Who**: someone independent of the authoring sessions. Not a Claude session
  that worked on this repository.
* **Where the approval is recorded**:
  1. The review itself, outside the package, for example `reports/reviews/`.
     It names the reviewer, the date, identity `00daee8d…`, the scope, the
     findings and how each was resolved.
  2. The owner's acceptance, inside the package under `04_CHANGE_CONTROL.txt`.
     In `05_LANGUAGE_CLOSURE.json`, `independent_review` becomes
     `status: "closed"` with a summary naming the review, and
     `release_gate_permitted` becomes `true`. The release-status fields change
     only if the release is then being made.
  3. `MANIFEST.json`, `VALIDATION_REPORT.txt` and `SHA256SUMS.txt`
     regenerated, in that order.
* **Acceptance** means the review reports no unresolved definite finding and
  the owner records the closure. **Rejection** means any unresolved definite
  finding. The decision then stays `pending`, the findings go back as canon
  work, and a fresh review is required.
* **Rerun afterwards**:
  1. Recompute the Core 0.2 identity.
  2. Re-pin it in `impl/crates/lcl-spec/src/anchor.rs`, `README.md`,
     `f4_localized_installed.py`, `f4_extra_smoke.py`,
     `rc_verify_candidate.sh` and `rc_supplementary_installed.sh`.
  3. Set the gate's `validate-0.2.0` expectation to `0`, and require 0 BLOCKED
     and 0 FAIL.
  4. Run the full gate (§2); Core 0.1 must stay `00d648b1…`.
  5. After the owner commits, build a new candidate from that commit, then run
     its verifier, the 150-check chain and the 29 supplementary checks.
  6. Gate 9, the external validation report and the archive.

## 7. Findings

| Severity | Component | Finding | Status | Evidence |
|---|---|---|---|---|
| BLOCKER | Core 0.2 `05_LANGUAGE_CLOSURE.json` | `independent_review` pending, `release_gate_permitted` false | Open, reviewer + owner only | `RC2-G-validate-0.2.0.log` |
| MEDIUM | `lcl-hardening/tests/packaging_smoke.rs` (M1) | installs the historical `releases/lcl-0.1.0` tarball, not a candidate | Open; covered for this candidate by S1–S5 | test source, `RC2-SUPP.log` |
| MEDIUM | F30 real desktop | a real KDE menu launch and file association have not been accepted by a person | Open, owner | carried |
| LOW | `operation_cases/clauses.rs:1132` (L1) | rustc 1.75 warns about a redundant import; exit 0 | Open, cosmetic | `RC2-G-msrv-check.log` |
| LOW | candidate `…d7a26f4e34ff` (L2) | stale docs in its source archive | **Closed** by `…787fd7aca090` | §3 |
| LOW | F31 repository settings | no branch protection; commits unsigned | Open, owner | carried |
| LOW | `uninstall.sh` (L4) | shared XDG directories and databases remain, by design | As documented | `RC2-SUPP.log` |
| LOW | `real_process` (L5) | earlier intermittent failure; 30/30 again | Watch item | phase d |

## 8. Owner actions

1. If you want the candidate and this report committed, say so: the candidate
   as its own commit, then this report. Nothing has been pushed, tagged or
   published.
2. Commission the Core 0.2 independent review (§6).
3. After acceptance, run the rerun sequence in §6.
4. F30, F31.

## 9. Evidence

All under `/mnt/F/.lcl-pretest/`:

* **Gate**: `rc2/rc2-gate.sh`, `rc2/rc2-results.tsv` (50 rows) and
  `logs/RC2-G-*.log`.
* **Build**: `logs/RC2-build.log`.
* **Candidate verification**: `rc2/verify-787fd7aca090.log`.
* **Installed**: `logs/RC2-E1.log`, `RC2-E2.log`, `RC2-LOC.log`,
  `RC2-EXTRA.log`, `RC2-E5.log` and `RC2-SUPP.log`; `rc2/smoke.out`.

The HawkScan hook fired and was not run: there is no running web application,
`HAWK_API_KEY` is unset, and no code changed.
