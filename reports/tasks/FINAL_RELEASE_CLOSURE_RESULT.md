# Final release closure — result

Date: 2026-09-24. Entry and frozen build source: `main` @
`43a9917385260f2967af4e5a9690e1e02452ba80` ("LCL repairs part 4"), clean
worktree, equal to the locally recorded `origin/main` (no fetch was made).

**Verdict: `BLOCKED_ON_INDEPENDENT_REVIEW`.**

Every release requirement under the implementation's control was executed
against the exact committed source and passed: the full readiness gate, a fresh
candidate built from that commit, its integrity and provenance, and its
installed verification. Core 0.1 is `semantics_conforming` and the readiness
gate accepts. What remains is Core 0.2's `independent_review`, which only an
independent reviewer can supply and only the owner can accept. It was not
recorded here, and nothing in this report is a substitute for it (§6).

The closure run itself made no Git write, and nothing under `canonical/`
changed. Afterwards, at the owner's explicit request, the candidate was
committed on its own (`b9ccb4f`), followed by this report and the two
documentation corrections (§8, step 1).

## 1. Repository state at entry

| | |
|---|---|
| Branch / HEAD | `main` @ `43a9917` = `origin/main` (0 ahead, 0 behind) |
| Worktree | clean: 0 porcelain entries, nothing staged, no stash, no untracked file |
| Tracked files | 1,057 (1,019 outside `releases/`) |
| Change since the last validated state | `43a9917` is the owner's commit of the four r4 worktree files (`D obligations_v0.1.0_r3.json`, `M production_report.rs`, `M LCL_CONFORMANCE_OBLIGATIONS.md`, `A MAPPING_CORRECTION_R4_RESULT.md`), verified with `git show --stat`; the mapping files are byte-identical between `464fe36` and `43a9917`. All gates below were re-run on it regardless. |
| Mapping | r4, `obligations_v0.1.0_r4.json`, SHA-256 `c592f8d9e0b5feb69785256395c9b67cac08932cec96c492a3786ac6b6cd780e` = the compiled `MAPPING_DIGEST`; recounted from the file: 980 rows, 2,413 unique probes (2,011 source, 402 semantics), 319 rows with sub-runs, 3,720 sub-run pins; `core.execute precondition/profile-out-of-bounds` absent, its missing/ambiguous/incomplete siblings still pinned |
| Core 0.1 identity | `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, 176 files |
| Core 0.2 identity | `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`, 216 files |

## 2. Readiness gate

The established gate (`/mnt/F/.lcl-pretest/c1/c5-gate.sh`) was run as a copy
whose only differences are its log prefix and results file
(`/mnt/F/.lcl-pretest/rc/rc-gate.sh`, `RC-G-*.log`), so earlier evidence was not
overwritten. Its self-test ran first and passed: a command exiting 3, and one
printing `test result: ok` while exiting 1, were both recorded and both failed
the gate.

**50 commands, each at its expected status.**

| Command | Exit | Expected | Result |
|---|---:|---:|---|
| `cargo fmt --all -- --check` | 0 | 0 | clean |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` (1.98.1) | 0 | 0 | clean |
| `cargo test --offline --locked --workspace --all-targets --no-fail-fast` (1.98.1) | 0 | 0 | 164 blocks, **1,825 passed, 0 failed, 1 ignored** |
| MSRV 1.75.0 `cargo check --workspace --all-targets` | 0 | 0 | clean (one pre-existing warning, finding L1) |
| MSRV 1.75.0 `cargo test --workspace --all-targets --no-fail-fast` | 0 | 0 | 164 blocks, **1,825 passed, 0 failed, 1 ignored** |
| Core 0.1 `sha256sum -c SHA256SUMS.txt` | 0 | 0 | 175 entries verify (the 176th file is the checksum file) |
| Core 0.1 `validate_release.py --scope all` | 0 | 0 | **31 PASS / 0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE** — exactly baseline §5.7 |
| Core 0.2 `sha256sum -c SHA256SUMS.txt` | 0 | 0 | verifies |
| Core 0.2 `validate_release.py --scope all` | 1 | **1** | 31 PASS / 0 FAIL / 2 OUT_OF_SCOPE / **1 BLOCKED**: `language_decisions_and_release_state`, `pending_decisions: ["independent_review"]` |
| Core 0.2 `validate_language_contracts`, `validate_localization`, `validate_source_fixtures` | 0 | 0 | pass |
| `validate_ebnf.py` on both grammars | 0 | 0 | pass |
| package identity, both packages | 0 | 0 | `00d648b1…` 176, `00daee8d…` 216 |
| brand assets | 0 | 0 | verify |
| `m8_conformance_report` | 0 | 0 | 2,413 required; source **2,011/2,011**, semantic **402/402**, 0 failed / 0 missing / 0 invalid; witnesses **66 of 66**; `CLAIM: semantics_conforming` |
| **`m8_conformance_gate`** | **0** | **0** | **`ACCEPTED: claim semantics_conforming`**, mapping `c592f8d9…`, package `00d648b1…` |
| protected paths | 0 | 0 | `canonical/`, `releases/`, `assets/` clean; every candidate and the published release verify their checksums; 0 stray writes |
| `real_process`: 12 sequential + 3 rounds of 6 concurrent | 0 ×30 | 0 | **30 of 30**, 12 of 12 tests in each run; no flake this run |

The hardening suites run inside the workspace test run and all pass:
`security_matrix` 6, `adversarial_inputs` 11, `fuzz_stages` 6,
`repeatability` 5, `applications` (the ladder) 9, `performance` 4,
`installed_launcher` 22, `release_build` 12, `protocol_and_tooling` 5,
`packaging_smoke` 5 (but see finding M1), `grants` 14,
`capability_boundary` 11.

### Ignored-test accounting

The workspace holds exactly one `#[ignore]` (`grep -rn '#\[ignore'` over
`impl/` and `apps/`), and it is the one both toolchains report:
`lcl-project/tests/manifest_input_bounds.rs::child_parses_nested_manifest`. It
is the child half of a pair that its parent re-executes with `--exact …
--ignored` and a depth, and the parent asserts on the child's printed
`depth N:` line, so a child that did not run would fail the parent. This is
the case baseline §5.7's supersession note (2026-09-22) already reconciles; no
new ignored test exists, and the threshold was not touched.

## 3. The candidate

Built with the established process, `LCL_RELEASE_VERSION=0.2.0
packaging/build_release.sh` (TMPDIR `/tmp/lcl-rc-build`), from the clean tree
at `43a9917`, after the gate above had passed on that tree:

```
releases/candidates/lcl-0.2.0-linux-x86_64-d7a26f4e34ff/
```

| | |
|---|---|
| **Build-source commit** | `43a9917385260f2967af4e5a9690e1e02452ba80` |
| **Uncommitted entries when recorded** | **0**; no `SOURCE_CHANGES.patch` was written |
| **Source id** (SHA-256 of `SOURCE_INVENTORY.tsv`) | `d7a26f4e34ff7f6f44f35248e4a085dceb8d9733169753a1b7e5b8b0022064b4`, **1,019 files** |
| Source archive SHA-256 | `f37ff65c6fcda78a97da566e1ba374b38dd3f59466358429ce25e3a0d41b612f` |
| **Artifact SHA-256** | `7dcdb03ba1294085078cf8b5901df9fe3a999517239ca227b95942f2c95e9e3a` |
| `bin/lcl` SHA-256 | `413f34e7d04ef3670cee53040a1bd1dc8ddd21efef16172c2f0252801e94fbf0` |
| Toolchain | cargo/rustc 1.98.1, declared minimum 1.75, `--release --offline --locked`, lockfile `c51be147…3a60` |
| Release / product / language | 0.2.0 / 0.1.0 / `0.1.0 0.2.0`; protocol `lcl.engine/1` |
| Packages carried | Core 0.1 `00d648b1…`, Core 0.2 `00daee8d…` |

Checked with `/mnt/F/.lcl-pretest/rc/rc_verify_candidate.sh`: **37 verdicts,
0 failures**. The checks: both archives verify their checksums; the source id
recomputes; the provenance names `43a9917` with 0 uncommitted entries. The
inventory's 1,019 paths and digests equal the `43a9917` blobs, read from Git
rather than the live tree, so later worktree edits cannot affect the check. The
source archive holds exactly the inventory's files. The only mapping inside is
`obligations_v0.1.0_r4.json` (`c592f8d9…`), which the archived `obligations.rs`
pins. The known strays are absent (`archive-EWXwoU/`, `.directory`,
`gk_3.1.75`); the repository-root `.directory` is ignored and was not
enumerated. Both bundled packages carry their pinned identities, verify their
`SHA256SUMS.txt`, and are byte-identical to `canonical/`.

**The verifier discriminates.** Run against the superseded `…8f2f0454e437`
while demanding `43a9917`, it failed exactly six checks: the commit
(`ba910bd`), the path set (1,016 vs 1,019), 5 changed digests, mapping r2
instead of r4, the absent r4 digest, and the pinned digest `9b32a28b…`.

The build wrote only its own new directory. The five earlier candidates and
`releases/lcl-0.1.0-linux-x86_64.tar.gz` still verify, and the tracked
`releases/` tree is unchanged. **No earlier candidate's evidence was reused.**
All five embed `obligations_v0.1.0_r2.json` and are superseded for the current
source.

## 4. Installed verification

### The established chain: 150 verdicts, 0 failures

Run against the unpacked fresh candidate, installed into a disposable HOME/XDG
under `env -i`, in the required order and in one fresh root
(`/mnt/F/.lcl-pretest/rc/accept`).

| Harness | Verdicts | Exit | Log |
|---|---|---:|---|
| `t4_g9_e1_install_cli.sh` — archive, install, installed CLI, all VALID/INVALID examples, package equality | **57 PASS, 0 FAIL** | 0 | `RC-E1.log` |
| `t4_g9_e2_launch_http.py` — menu/document launch, loopback HTTP, token and Host refusal, grant pair | **35 PASS, 0 FAIL** | 0 | `RC-E2.log` |
| `f4_localized_installed.py` — installed 0.2.0 package, CLI `run` in en/lv-LV/nl-NL/ru-RU/zh-CN, fail-closed without the package, 0.1.0 output equality, lock and profile drift | **33 PASS, 0 FAIL** | 0 | `RC-LOC.log` |
| `f4_extra_smoke.py` — F10 decision A, nested relative paths, locale-profile host limits | **14 PASS, 0 FAIL** | 0 | `RC-EXTRA.log` |
| `t4_g9_e5_uninstall_reinstall.py` — uninstall, operator files preserved, reinstall | **11 PASS, 0 FAIL** | 0 | `RC-E5.log` |

The current equivalent of the earlier 150-check chain is still exactly 150.
E1 confirms that the installed `lcl` is this candidate's binary (`413f34e7…`).

### Supplementary checks: 29 verdicts, 0 failures

The chain above does not check three things for a fresh candidate. It never
looks for repository or build paths in installed files (the only such test,
finding M1, installs a different tarball). It never runs the binaries with no
environment at all. And E5 checks uninstall against a fixed list of expected
paths, not the whole tree. `/mnt/F/.lcl-pretest/rc/rc_supplementary_installed.sh`
covers these in a separate fresh root; it is counted separately and
`RC-SUPP.log` holds the output.

* **S1/S2.** Scanned all 416 payload files and all 425 installed files (both
  specification packages included) for the repository `/mnt/F/LCL`, the build
  staging `/tmp/lcl-rc-build`, the builder's home and `.cargo/registry`:
  **0 hits** for each.
* **Elevation.** The installer ran as uid 1000, and neither script calls an
  elevation tool.
* **S3, working directory `/`, completely empty environment (`env -i`, no PATH
  or HOME).** The binaries were run against the installed packages:
  * `lcl version` reports language 0.1.0 and protocol `lcl.engine/1`; with the
    0.2.0 package named, languages `0.1.0 0.2.0`.
  * `lcl spec` reports `00d648b1…`, and `lcl check` reports the 0.2.0
    identity `00daee8d…`.
  * VALID/01 runs to `status.succeeded`.
  * INVALID/01 returns exit 1 with `error.keyword.case` and `status.invalid`,
    decided by the example's own expected file.
  * Without a package, nothing is searched for (exit 4).
* **S4, a full-tree comparison across uninstall.** No installation-owned file
  remains, and `~/.local/share/lcl` is gone. The residue is 22 paths: 9 shared
  XDG directories and 13 regenerated mime/desktop database files, none of which
  mentions LCL (finding L4).
* **S5.** Reinstall reproduces all 412 owned files byte for byte.

**The harness discriminates.** Fed a sabotaged copy of this payload (the
repository path planted in the desktop entry, and an uninstaller that
"forgets" `bin/lcl`), it failed exactly the five verdicts those defects touch
and passed the other 24.

## 5. Documentation changes

After the technical verification, two current-facing documents still stated
the obsolete blocked state. Both were corrected by a dated addition that marks
the older statement historical, without rewriting it:

* `impl/README.md`, M11 correction note: it presented "`source_conforming`
  only … Full semantic conformance is BLOCKED" (mapping r2) as current. An
  appended 2026-09-24 line records r4, 2,011/2,011 + 402/402,
  `semantics_conforming` and the accepting gate.
* `reports/implementation/LCL_CONFORMANCE_OBLIGATIONS.md`: its header status
  still read "full semantic conformance is BLOCKED" (2026-09-14), although the
  body already documents r4. A dated current status was added above it, and one
  sentence was appended to the LCL-CLOSE-02 closing paragraph.

**These edits postdate the candidate's frozen source.** The candidate's
*source archive* therefore still carries the two pre-correction texts. The
installed *payload* is unaffected, since it ships only `packaging/README.md` of
the documentation (finding L2).

Reviewed and deliberately left unchanged:

* `README.md`: the Core 0.2 row, "Unreleased localization feature candidate:
  independent review pending, `release_gate_permitted` false", is exactly
  current.
* `packaging/README.md`: makes no status claim.
* `LCL_RELEASE_BASELINE.md`: §5.7's 2026-09-22 supersession note matches
  today's figures (1,825 / 0 / 1; 175 checksum entries; 31/0/0/2).
* `LCL_RELEASE_REPORT.md`: explicitly the Task 20 closing position, with
  dated corrections, describing the published `releases/lcl-0.1.0` artifact.
* `LCL_RESIDUAL_REPAIR_REPORT.md`: a dated 2026-09-12 execution record.
* `LCL_REVIEW_COVERAGE_LEDGER.md`: dated and scoped to LCL-CLOSE-02.
* All task reports: historical by construction.

## 6. Core 0.2 independent review — what is required

### State

| | |
|---|---|
| Identity | `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`, 216 files |
| `PACKAGE_STATUS` | `UNRELEASED_CANDIDATE` (`00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt`) |
| `00_RELEASE/05_LANGUAGE_CLOSURE.json` | 8 of 9 decisions `closed`; `independent_review` **`pending`**; `release_gate_permitted` **`false`** |
| `MANIFEST.json` | `status: localization_feature_candidate`, `release_ready: false` |
| Validators | three pass; `validate_release.py` BLOCKED on that one decision only |

The ledger's own text sets the condition: the localization additions "have not
received an independent review, so this decision remains pending until the
owner accepts such a review." Tests passing is not that review, and neither is
this report.

### What the reviewer must inspect

A read-only review of the **Core 0.2.0 localization additions** and of their
alignment with every surface they touch. Against Core 0.1.0, with the version
token normalized, the package holds exactly:

* **40 new files**:
  * `02_LEXICAL/13_LOCALIZED_SOURCE_AND_LOCALE_DIRECTIVE.txt`;
  * `10_REGISTRIES/localization_surface_v0.2.0.json` and
    `locale_profile_schema_v0.2.0.json`;
  * `09_CONFORMANCE/LOCALIZATION_FIXTURES/`: `expected_results.json`, 4
    profiles and 31 sources;
  * `09_CONFORMANCE/TOOLS/validate_localization.py`.
* **28 changed files**:
  * `00_RELEASE/00`, `01`, `03`, `04` and `05`;
  * `01_FOUNDATION/02` and `03`;
  * `02_LEXICAL/01` (the F10 decision A bullet), `02` and `06`;
  * `03_TYPES_AND_VALUES/02` and `07`;
  * `05_SEMANTICS/09`;
  * `06_STANDARD_LIBRARY/06` and `07` (the nine localization errors);
  * `07_VERSIONING_AND_EXTENSIONS/01`;
  * `09_CONFORMANCE/01` and `CASES/core_conformance_cases_v0.2.0.json`;
  * `09_CONFORMANCE/TOOLS/generate_integrity.py` and `validate_release.py`;
  * `10_REGISTRIES`: `built_in_groups_and_results`, `keywords`,
    `statuses_and_errors` and `types`;
  * `CHANGELOG.txt`, `INDEX.txt`, `README.txt` and `VERSION.txt`.
* **The 3 regenerated integrity files**: `MANIFEST.json`,
  `VALIDATION_REPORT.txt` and `SHA256SUMS.txt`.

The other 145 files are identical to Core 0.1.0 apart from the version string.
The complete list is at `/mnt/F/.lcl-pretest/rc/core-0.2-review-scope.txt`.

Questions the review has to answer:

* Is the localization surface closed?
* Are locale selection, the directive and pinning deterministic?
* Are original-byte locations exact?
* Do the nine errors carry correct stage, phase and status contracts?
* Does every fixture's expected result follow from the normative text?
* Is the meaning of every valid Core 0.1.0 program unchanged, including
  F10 = atomic decode?

The earlier readiness record in `FINAL-03_RESULT.md`, "Independent-review
readiness record", still describes F10 correctly; its counts are from that
date.

Useful evidence, which does not replace the review:

* this report's §2 (all four 0.2 validators, both EBNF checks, identities);
* the installed localized chain in §4;
* the implementation's localization stage, `impl/crates/lcl-protocol` and
  `lcl-lexer`'s encoding gate.

### Who may perform it

Someone independent of the authoring sessions. In particular, not a Claude
session that worked on this repository, and not a sub-agent spawned by one.

### Where the result is recorded

1. **The review itself**, outside the package root (for example
   `reports/reviews/`). It must name the reviewer, the date, the exact package
   identity reviewed (`00daee8d…`), the scope, the findings, and how each
   definite finding was resolved.
2. **The owner's acceptance.** The owner then records it inside the package,
   under `00_RELEASE/04_CHANGE_CONTROL.txt`:
   * In `00_RELEASE/05_LANGUAGE_CLOSURE.json`, set `independent_review` to
     `status: "closed"`, with a summary naming the review, and set
     `release_gate_permitted: true`.
   * Only if the release is then being made, also:
     * `LANGUAGE_STATUS: BARE_SPECIFICATION_COMPLETE` and
       `PACKAGE_STATUS: BARE_LANGUAGE_RELEASE` in
       `00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt`;
     * `status: bare_language_release`, `release_ready: true` in
       `MANIFEST.json`;
     * `VERSION.txt`, `README.txt` and `CHANGELOG.txt` aligned.

   `validate_release.py` checks all of these, and it fails an active release
   claim that precedes the closed decision.
3. **The integrity files, regenerated** in their documented acyclic order:
   `MANIFEST.json`, then `VALIDATION_REPORT.txt`, then `SHA256SUMS.txt`, per
   `00_RELEASE/03_COMPLETENESS_CRITERIA.txt`.

### What must be rerun after a legitimate approval

The package's bytes change, so its identity changes, and every pin of
`00daee8d…` must move with it:

1. Recompute the Core 0.2 identity, then re-pin
   `APPROVED_PACKAGE_0_2_0.identity_digest` (and, if appropriate, its label) in
   `impl/crates/lcl-spec/src/anchor.rs`, the `README.md` row, and the smoke
   oracles `f4_localized_installed.py` / `f4_extra_smoke.py` (and
   `rc_supplementary_installed.sh`).
2. Change the gate's `validate-0.2.0` expectation from `1` to `0`, and require
   `validate_release.py --scope all` for 0.2 to report
   **0 BLOCKED, 0 FAIL**.
3. The whole gate (§2) on the committed result; Core 0.1 must stay
   `00d648b1…`.
4. Once the owner commits, a new candidate from that commit (§3), its
   verifier, and the installed chain plus supplementary checks (§4). The
   candidate `…d7a26f4e34ff` carries the pre-review package and becomes
   superseded.
5. Gate 9 and the archive: an external validation report records the final
   hashes, and a Core 0.2 archive is permitted only after Gates 1–9 pass.

## 7. Findings

| ID | Severity | File / component | Evidence | Fixed | Verification |
|---|---|---|---|---|---|
| B1 | **BLOCKER** | `canonical/LCL_Core_0.2.0/00_RELEASE/05_LANGUAGE_CLOSURE.json` | `independent_review: pending`, `release_gate_permitted: false`; `validate_release.py` BLOCKED on it alone | No — reviewer + owner only (§6) | validator output, `RC-G-validate-0.2.0.log` |
| M1 | MEDIUM | `impl/crates/lcl-hardening/tests/packaging_smoke.rs` | Its `tarball()` installs the newest `releases/lcl-*-linux-x86_64.tar.gz`, i.e. the 2026-09-11 Task 20 artifact (`fbb177e`, 34 uncommitted files at build), so its 5 passing tests, including "nothing installed refers to where it was built", are evidence about that historical artifact, not about the current source or candidate | Not changed (test semantics are outside this task); covered for this candidate by S1–S5 | `releases/` listing, test source |
| M2 | MEDIUM | F30 — real desktop acceptance | A real KDE menu launch and file association have never been accepted by a person. The harnesses drive a stub `xdg-open`. This needs a graphical session and changes to the real home, so it was not attempted | No — owner | carried from FINAL-04/05 |
| L1 | LOW | `impl/crates/lcl-conformance/src/operation_cases/clauses.rs:1132` | rustc 1.75.0 warns `Determinism` is imported redundantly (already imported at line 21); present in the previous gate's MSRV logs too; stable clippy `-D warnings` is clean, MSRV gate exits 0 | No — would move the frozen source for a cosmetic change | `RC-G-msrv-check.log`, `C5-G-msrv-check.log` |
| L2 | LOW | candidate `…d7a26f4e34ff` source archive | carries `impl/README.md` and `LCL_CONFORMANCE_OBLIGATIONS.md` as they were before §5's corrections; payload unaffected | Docs fixed in worktree; a rebuild after the owner commits them would carry them, and one is needed anyway after B1 | `build_release.sh` payload list |
| L3 | LOW | F31 — repository settings | branch protection and required checks; commits are unsigned | No — owner | carried from FINAL-05 |
| L4 | LOW | `packaging/uninstall.sh` (by design) | an uninstall from an empty home leaves 9 shared XDG directories (e.g. `~/.local/bin`, `icons/hicolor`, `mime/packages`, an emptied `mime/text`) and 13 regenerated database files, none mentioning LCL; the script documents keeping shared directories and regenerating shared databases | No — consistent with its documented contract | S4 residue list, `RC-SUPP.log` |
| L5 | LOW | `lcl-capabilities` process deadline path (QA-01) | earlier intermittent `real_process` partial-stdout failure never reproduced on the unrepaired binary; 30/30 here | Watch item | phase d |

Carried limitations, unchanged and not release blockers under the current
contract (conformance and the gate are complete without them):

* **CAP-01**: bounded `core.modify` is refused rather than supported, because
  no selection vocabulary exists.
* **SPEC-03**: the enclosing TASK scope is an open normative question.
* **C-03**: whether a required EVIDENCE with an external SOURCE must have been
  observed during the run.
* The **512-level** bound on nested indented bodies (baseline §5.1).
* **One verified platform.**

Each needs canon or owner input, not implementation work.

## 8. Owner actions, in order

1. **Done at the owner's request, 2026-09-24.** The candidate directory
   `releases/candidates/lcl-0.2.0-linux-x86_64-d7a26f4e34ff/` was committed
   alone as `b9ccb4f`, which touches only `releases/`, so its source tree is
   identical to the build source `43a9917`. This report and the two
   documentation corrections in §5 followed in the next commit. Neither was
   pushed.
2. Commission the **independent review** of Core 0.2 (§6), then accept it or
   send it back.
3. After acceptance: the re-pin and full rerun sequence in §6, then a new
   candidate and its installed verification.
4. F30 on a real desktop; F31 repository protection.

## 9. Evidence

All under `/mnt/F/.lcl-pretest/`:

* Gate: `rc/rc-gate.sh`, `rc/rc-results.tsv` (50 rows), and
  `logs/RC-G-*.log`.
* Build: `logs/RC-build.log`.
* Candidate verification: `rc/rc_verify_candidate.sh`,
  `rc/verify-d7a26f4e34ff.log`, and the negative control `rc/ctl-verify.log`.
* Installed chain: `logs/RC-E1.log`, `RC-E2.log`, `RC-LOC.log`,
  `RC-EXTRA.log` and `RC-E5.log`.
* Supplementary checks: `rc/rc_supplementary_installed.sh` and
  `logs/RC-SUPP.log`. The first run, whose `comm` lacked `LC_ALL=C`, is kept
  as `logs/RC-SUPP-run1-comm-locale.log`. The negative control is
  `rc/ctl-supp/control.log`.
* Review scope: `rc/core-0.2-review-scope.txt`.

The HawkScan post-commit hook fired repeatedly and was not run. No web
application is running, `HAWK_API_KEY` is unset, and this work order does not
authorize a hosted scan.
