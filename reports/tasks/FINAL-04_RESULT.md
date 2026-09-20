# FINAL-04 result — current-source candidate

## Identity
- Pack: `/mnt/F/LCL_Final_PreTesting_Blocker_Closure_Pack_v3_825694f/`, task `TASKS/FINAL-04_CURRENT_SOURCE_CANDIDATE.md`.
- Entry HEAD: `4dead55f15d537ae5952c054e3d6ab8acb86432c` ("LCL final pretest task 3"), the owner's commit of FINAL-03.
- Exit HEAD: unchanged.
- Entry status: **clean**, 1,033 tracked files.
- Exit status: 6 untracked candidate files under `releases/candidates/lcl-0.2.0-linux-x86_64-68529c1420ba/` plus this report. **No tracked file changed.**
- Git writes by agent: **NO**
- Agents or background workers: **NO**
- Network or new dependencies: **NO**
- Scratch: `/mnt/F/.lcl-pretest/f4/`. Logs: `/mnt/F/.lcl-pretest/logs/F4-*`.
- `TMPDIR` for the build was `/mnt/F/.lcl-pretest/f4/tmp`, outside the source tree as `build_release.sh` requires.

## Status
**PASS, with one unmet pack prerequisite recorded below.**

B2/F28 is closed: a candidate now exists that was built from the exact clean committed HEAD, its provenance records zero uncommitted entries, and every candidate smoke gate passes on the unpacked artifact — 150 verdicts, 0 failures.

The pack's FINAL-04 prerequisite "FINAL-02 PASS with `semantics_conforming`" is **not** met and was not made met here: the claim at this HEAD is still `source_conforming` (B1/F27). That is FINAL-02's recorded `BLOCKED_NEW_ROOT_CAUSE` boundary, not a regression, and it is outside FINAL-04's bounded scope. It is restated under *Residual* because it alone prevents `TESTING_READY` in FINAL-05.

## The candidate

`releases/candidates/lcl-0.2.0-linux-x86_64-68529c1420ba/`

| Provenance field | Value |
|---|---|
| git commit | `4dead55f15d537ae5952c054e3d6ab8acb86432c` |
| uncommitted entries when the source was recorded | **0** (no `SOURCE_CHANGES.patch` was written) |
| origin | git checkout |
| source id (SHA-256 of `SOURCE_INVENTORY.tsv`) | `68529c1420babf24d85d857c280a18ff683444b515462c903dfd6437b1844b17` |
| source files | 1,007 (1,033 tracked less the 26 under `releases/`, which is never source) |
| source archive SHA-256 | `ce8153743f84ea93afbe373482b3ed9795b7512884201f21257302b3abae9f8f` |
| artifact SHA-256 | `bfbec817053fcbd952314e89dd4179a61d320fcf885bb4a74295506214099272` |
| release / product version | 0.2.0 / 0.1.0 |
| language versions carried | `0.1.0 0.2.0` |
| engine protocol | `lcl.engine/1` |
| Core 0.1.0 identity | `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed` |
| Core 0.2.0 identity | `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604` |
| cargo / rustc | 1.98.1 (`/usr/bin`) |
| declared minimum | 1.75 |
| lockfile SHA-256 | `c51be14776070134a45857c0606f2cff1ade401a3376b212bf1fcb081f153a60` |
| build flags | `--release --offline --locked`, in the run's own directory |

Independently verified, not merely read back from the provenance:

- both `sha256sum -c` files verify, before and again after the smoke runs;
- `SOURCE_INVENTORY.tsv` names **exactly** `git ls-files` minus `releases/` — 1,007 paths, byte-identical listing — and every one of the 1,007 recorded digests equals the digest of the file in the clean HEAD worktree;
- the source id recomputes from `SOURCE_INVENTORY.tsv`;
- the two canonical identities recompute from the tree (176 and 216 files);
- the build's own staging directory was removed on success, and no run wrote into `impl/target/test-tmp` or `/tmp/lcl-apps`.

## Historical candidates

The three earlier candidates and the published `releases/` archives are byte for byte what they were at entry: 26 files, digest list identical before the build and after every smoke run. The new candidate was written into a directory of its own that did not previously exist.

## Candidate smoke

Every gate ran against the **unpacked candidate installed into a disposable HOME/XDG** under `env -i`, never against a development binary, in the FEATURE-04 order E1 → E2 → localized → extra → E5 in one acceptance root (`/mnt/F/.lcl-pretest/f4/accept3`).

| Harness | Verdicts | Exit | Log |
|---|---|---:|---|
| `t4_g9_e1_install_cli.sh` — archive, payload-vs-provenance, install, icons, desktop entry, media type, installed CLI over every VALID and INVALID example | **57 PASS, 0 FAIL** | 0 | `F4-E1.log` |
| `t4_g9_e2_launch_http.py` — menu and document launches, loopback HTTP session/API, token and Host refusal, grant pair | **35 PASS, 0 FAIL** | 0 | `F4-E2.log` |
| `f4_localized_installed.py` — installed 0.2.0 package, launcher, CLI `run` in en/lv-LV/nl-NL/ru-RU/zh-CN, fail-closed without the package, 0.1.0 output equality, lock and profile drift, launcher-started workspace | **33 PASS, 0 FAIL** | 0 | `F4-LOC.log` |
| `f4_extra_smoke.py` — F10 attribution, nested relative path, profile host limits (new, below) | **14 PASS, 0 FAIL** | 0 | `F4-EXTRA.log` |
| `t4_g9_e5_uninstall_reinstall.py` — uninstall, 29 unrelated/operator files preserved, shared icon theme kept, reinstall | **11 PASS, 0 FAIL** | 0 | `F4-E5.log` |

Total **150 verdicts, 0 failures**.

### The three items no existing harness covered

`/mnt/F/.lcl-pretest/f4/f4_extra_smoke.py`, written for this task because nothing already owned these behaviours at the installed-candidate level.

**Invalid UTF-8 attribution, through the installed CLI, under F10 decision A.** The same corruption method as `lcl-protocol/tests/dispatch.rs::invalid_utf8_is_attributed_to_core_0_1_0`: one `0xFF` replaces the ASCII byte after a marker, so the first invalid byte sits at a known original offset.

| Case | Fixture | Result |
|---|---|---|
| immediately after a complete `VERSION: "0.2.0"` line | `canonical_en.lcl` | 0.1.0, `error.encoding.invalid`, original byte 26 |
| late in the document, after a complete VERSION | `canonical_en.lcl` | 0.1.0, same error, original byte 202 |
| inside the `VERSION` keyword | `canonical_en.lcl` | 0.1.0, same error, original byte 14 |
| after a complete localized `VERSIJA: "0.2.0"` line | `explicit_lv.lcl` | 0.1.0, same error, original byte 40 |
| inside the `@locale` directive | `explicit_lv.lcl` | 0.1.0, same error, original byte 5 |
| control: intact document | `canonical_en.lcl`, `explicit_lv.lcl` | accepted under the 0.2.0 identity, locale `None` / `lv-LV` |

Rows 1, 2 and 4 each carry a complete readable 0.2.0 VERSION declaration before the first invalid byte, so Reading B would attribute them to 0.2.0. The installed candidate attributes them to Core 0.1.0 at the original byte, which is decision A, and the CLI's `spec.identity_digest` is the Core 0.1.0 anchor.

**A nested relative document path.** Inside a project, `a/b c/d/minimal.lcl` — a nested path whose middle component holds a space — is accepted and identified root-relative as `a/b c/d/minimal.lcl`; reached from the deeper cwd `a/b c` as `d/minimal.lcl` it produces the identical unit record, digest and verdict; by absolute path with no project it is accepted as a control.

**Locale profile host limits, at their boundaries.**

| Case | Result |
|---|---|
| a profile of exactly `MAX_PROFILE_BYTES` (1,048,576) | read, exit 0 |
| one byte over | exit 1, `error.localization.profile_invalid`, "profile exceeds 1048576 bytes" |
| `MAX_PROFILE_FILES` + 1 (257) profile files | exit **4**, "257 locale profile files exceed the host limit of 256" on stderr, no language diagnostic on stdout |
| control: the same files one under the limit | exit 4 for a different reason — the `<locale>.json` name rule — so the refusal above is triggered by the count, not by "many files" |

The boundary controls matter for global rule 9: the size limit is a language-level localization diagnostic, the file-count limit is an environment failure, and neither is converted into the other.

## Findings

No product defect was found. Five findings, all in reused **scratch** oracles and harness assumptions, each reproduced before disposition.

| Finding | Reproduction | Root cause | Disposition | Verification |
|---|---|---|---|---|
| Localized harness pinned a superseded 0.2.0 identity | 6 verdicts failed comparing `spec.identity_digest` | `IDENTITY_0_2_0` was FEATURE-04's `e86121c7…`; PRETEST-03 and FINAL-03 have since regenerated the package | Re-pointed to FINAL-03's `00daee8d…`, the identity `lcl-spec::anchor::APPROVED_PACKAGE_0_2_0` and this candidate's provenance both carry | 6 verdicts pass; a wrong identity still fails |
| Harness expected an unquoted launcher path | `localized_spec={path}` not found in `lcl-workspace-launch` | `packaging/install.sh` substitutes `shell_quote(path)` "so a space or a quote in a path stays part of the path"; the installed launcher writes `localized_spec='…'` | Expectation corrected to the quoted shell word | verdict passes; `@LOCALIZED_SPEC@` still asserted absent |
| Harness expected `lcl version` to name 0.2.0 with no package named | `lcl version` printed `language 0.1.0` only | PRETEST-03 F13 made `version` name Core 0.2.0 **only** for a named package that opens (`packaging/README.md`; `localized_cli.rs::version_names_core_0_2_0_only_when_its_package_is_named`) | Split into two stricter verdicts: alone → 0.1.0 only; with `--localized-spec` → both | both pass; the old oracle would now be satisfied by a tool that ignores the flag |
| Connection refused in the localized workspace section | second run in a reused acceptance root | the stub `xdg-open` record accumulates across runs, so the harness read a previous run's dead URL | Ran the chain in a fresh acceptance root, as the harness assumes | `F4-LOC.log`: 33/33 |
| E2's launch-count verdict failed | `launch.log` held 5 launches, expected 4 | `launch.log` is shared per home, and the localized harness had launched first | Restored the FEATURE-04 order E1 → E2 → localized | `F4-E2.log`: 35/35 |

No oracle was weakened: each correction pins the current normative behaviour, and the two `version` verdicts and the four profile-limit boundary controls are strictly more discriminating than what they replace.

## Files changed

**None in the repository.** No tracked file was created, modified or deleted. The task produced, as untracked output for the owner to commit:

| Path | What |
|---|---|
| `releases/candidates/lcl-0.2.0-linux-x86_64-68529c1420ba/` | the candidate: payload, source archive, both `.sha256`, `SOURCE_INVENTORY.tsv`, `lcl-0.2.0-PROVENANCE.txt` |
| `reports/tasks/FINAL-04_RESULT.md` | this report, written after the build so it could not appear as an uncommitted source entry |

Scratch, outside the repository and not for commit: `/mnt/F/.lcl-pretest/f4/f4_localized_installed.py` (corrected copy), `f4_extra_smoke.py` (new), `patch_localized.py`, and the acceptance roots.

## Gates

| Command | Exit | Result | Log |
|---|---:|---|---|
| `LCL_RELEASE_VERSION=0.2.0 packaging/build_release.sh` | 0 | candidate `…-68529c1420ba`, 1,007 source files | `F4-build.log` |
| `sha256sum -c --strict` payload and source | 0 | both OK, re-verified after the smoke | `F4-integrity.log`, `F4-exit.log` |
| inventory vs clean HEAD worktree (paths and digests) | 0 | 1,007 / 1,007 identical | `F4-integrity.log` |
| E1 installed candidate + CLI | 0 | 57 PASS | `F4-E1.log` |
| E2 launcher + loopback HTTP API | 0 | 35 PASS | `F4-E2.log` |
| localized installed candidate (corrected) | 0 | 33 PASS | `F4-LOC.log` |
| F4 extra: F10, nested relative path, profile limits | 0 | 14 PASS | `F4-EXTRA.log` |
| E5 uninstall + reinstall | 0 | 11 PASS | `F4-E5.log` |
| `cargo run -p lcl-conformance --example m8_conformance_report` | 0 | `CLAIM: source_conforming` | `F4-conformance.log` |
| historical `releases/` bytes | 0 | 26 files unchanged | `F4-exit.log` |

Reused rather than re-run: `cargo fmt`, `clippy -D warnings`, the workspace test run (1,695 passed), the MSRV 1.75.0 check and the four Core 0.2 validators. FINAL-03 ran all of them at exactly this content — the owner committed its exit tree unchanged as `4dead55`, and the two canonical identities recomputed here prove the canonical half of that byte for byte. FINAL-05 re-runs them independently.

## Identities
- Core 0.1: `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, 176 files — unchanged, immutable.
- Core 0.2: `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`, 216 files — FINAL-03's, carried by the candidate's payload and reported by the candidate's own tool.

## Conformance
Measured at this HEAD, unchanged by this task (`F4-conformance.log`):
- claim: `source_conforming`
- source: 2,011 required, 2,011 satisfied, 0 failed, 0 missing, 0 invalid
- semantic required: 402
- satisfied: 357
- failed: 0
- missing: 0
- invalid: 45

## Residual / out-of-scope

- **B1 / F27 — the unmet prerequisite.** 45 semantic probes remain invalid, every one because 79 pinned sub-runs have no constructible input in this build; the claim is `source_conforming`, not `semantics_conforming`. FINAL-02 recorded this as a `BLOCKED_NEW_ROOT_CAUSE` needing six engine capabilities. Under `05_TESTING_READY_CONTRACT.md` this alone forces `BLOCKED` in FINAL-05.
- **B3 / F29** — the Core 0.2 independent review is still pending and remains the single `validate_release.py` BLOCKED. Owner action.
- **B5** — the `real_process::many_flooding_children_in_sequence_leave_nothing_behind` isolation race was not exercised: no broad test gate ran in this task.
- **F30** — real KDE desktop-menu and file-association acceptance still needs a graphical session; none exists here. The launcher, desktop entry, media type and icons were verified installed, valid and correctly removed.
- The E4 headless-browser harness was not run: FINAL-04's smoke list asks for workspace launch/API, which E2 and the localized harness cover, and the browser gate belongs to the application acceptance FEATURE-04 already recorded.
- The candidate is an `UNRELEASED_CANDIDATE`. Nothing here promotes it.

## AI quota / reuse
- Evidence reused: the FEATURE-04 acceptance harnesses E1, E2, E5 unchanged and the localized harness with three cited corrections; PRETEST-03's `identity.py`; FINAL-03's gate evidence for the unchanged product gates; F01–F09 and F11–F26 were not re-audited.
- Broad gates avoided: one conformance run for the claim of record; no fmt/clippy/workspace/MSRV re-run, since FINAL-03 proved them at this content and FINAL-05 re-proves them independently.
- Minimum-code notes: zero repository changes; one new scratch harness of 14 verdicts for the three uncovered smoke items; the localized harness was patched by exact string replacement, refusing unless each target appeared exactly once, rather than being rewritten.

## Proposed commit message
```
LCL FINAL-04: current-source 0.2.0 candidate from the clean final commit

Builds lcl-0.2.0-linux-x86_64-68529c1420ba with the existing offline
release process, into a new directory. Its provenance names commit
4dead55 with zero uncommitted entries and no SOURCE_CHANGES.patch; the
inventory's 1,007 paths and digests equal git ls-files minus releases/ at
that commit, and it carries Core 0.1 00d648b1... and Core 0.2 00daee8d...
with language versions 0.1.0 0.2.0. B2/F28 is closed.

The unpacked candidate was installed into disposable HOME/XDG roots and
accepted in 150 verdicts with no failure: install and CLI (57), launcher
and loopback HTTP (35), localized 0.2.0 across en/lv-LV/nl-NL/ru-RU/zh-CN
with lock and drift (33), uninstall and reinstall preserving 29 unrelated
files (11), and 14 new ones for the items no harness covered - invalid
UTF-8 attributed to Core 0.1.0 at the original byte under F10 decision A
including three cases a valid 0.2.0 VERSION prefix would have swung under
reading B, a nested relative path with a space identified root-relative
from two cwds, and the locale profile size and count limits at their
boundaries, the size one a language diagnostic and the count one an
environment failure.

Three stale FEATURE-04 scratch oracles were re-pointed at the current
contract (the 0.2.0 identity, the quoted launcher path, and `version`
naming 0.2.0 only for a named package); none was weakened. No tracked
file changed. Historical candidates and releases/ are byte for byte
unchanged. The claim stays source_conforming: B1/F27 is untouched.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

## Next task
FINAL-05 is **unlocked** once the owner reviews and commits the candidate and this report. It must run read-only against that commit. On the present evidence its verdict will be `BLOCKED`, on B1/F27 (`source_conforming`) and B3/F29 (independent review), not on the candidate.
