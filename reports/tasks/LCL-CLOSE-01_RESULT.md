# LCL-CLOSE-01 — Correctness, safety and persistence — task result

## Identity and authorization

- Package: LCL-CLOSURE-4T v1.1, task **LCL-CLOSE-01**
- Report date: 2026-09-13, Europe/Riga (system local)
- Repository root / branch / HEAD / upstream: `/mnt/F/LCL` / `main` /
  `7f13aecd4af639f1373bb1180cb50f9b508cfb05` / `origin/main` (no remote contact)
- Starting state: **clean** worktree at that commit, which is also the package's
  historical audit baseline. Nothing was staged, stashed, recovered or reset.
- Ending state: 23 modified files and 4 new files, listed below; nothing staged.
- Approvals: the owner approved this task under the presented plan with
  decisions **D1** (safe std-only Unix `O_NOFOLLOW | O_EXCL`; no unsafe FFI, no
  parent-component walking), **D2** (verify canonical first, then ask — asked
  and answered mid-task: a declared network bound is a **total budget** across
  resolution, connection, send and receive), **D3** (per-path publication
  ordering plus an optional precondition), **D4** (probe Q-JSON before
  proposing), **D5** (new owned disk-backed scratch). Prior approvals for the
  residual repair and A1/A2/A3 were retained, not re-requested.
- Toolchains actually selected: system `rustc 1.98.1 (48a229cea 2026-09-01)` and
  `cargo 1.98.1 (797e8a9bc 2026-08-05)` at `/usr/bin`; minimum
  `rustc 1.75.0 (82e1608df 2023-12-21)` and `cargo 1.75.0 (1d8b05cdd 2023-11-20)`
  from `/mnt/F/.lcl-residual-repair-01-lxd8dwu8/toolchains/1.75.0/bin`, selected
  by PATH **and** `RUSTC`; Python 3.14.7; Node v26.8.2.
- Evidence directory: `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/logs/` — every gate has `.log`, `.exit` and
  `.record.json` (argv, cwd, relevant environment, toolchain paths and versions,
  start/end timestamps, real exit, log digest, HEAD and worktree state).
- Next task: **LCL-CLOSE-02**, entry state below.

## Summary status

Task: **COMPLETE**, with two items explicitly referred rather than claimed.

- Implementation correctness: every finding assigned to this task has a current
  disposition backed by an executed discriminating reproduction and control.
- Conformance level actually established: unchanged at **`source_conforming`**.
  This task preserved it and did not raise it; raising it is B4's work.
- Mandatory missing probe IDs: **319 semantics_conforming probes**, each named
  in the production report's "limited by" lines. Owned by LCL-CLOSE-02.
- Canonical identity and errata: identity re-verified by the implementation's own
  algorithm as `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`.
  No erratum was issued here; the 335/334 recount is DOC-01, LCL-CLOSE-02's item.
- Review scope: this task's own findings only. No unrelated subsystem was opened.
- Release candidate state: none built or changed. B5 has not begun.
- Publication: **NOT PERFORMED.** No staging, commit, push, tag, merge, release,
  remote setting, global install or change to the owner's installation.

## Finding dispositions

| ID | Current applicability | Baseline evidence class | Actual reproduction / control | Disposition | Gate | Remaining condition |
|---|---|---|---|---|---|---|
| RUN-02 | applies | source_traced | Real loopback product, operation pause with effect pauses off, deny: reported `error.host.constraint`, and with both pauses on the operator was asked twice | **FIXED_VERIFIED** | A-run02-before-corrected (101) → A-run02-after-formatted (0) | none |
| RUN-01 | applies | source_traced | Deterministic seam: a cancel in the check/register window returned `Err(Timeout)`; a hold outstanding at `finish` also parked | **FIXED_VERIFIED** | A-run01-before (101) → A-run01-after (0) | none |
| FS-01 | applies | source_traced_plus_OS_primitive | Real adapter **wrote outside the grant** through a dangling link; racing create, copy and rename each returned `[Ok(()), Ok(())]` | **FIXED_VERIFIED** | B-fs-before / B-fs-races-before (101) → B-fs-after-2 (0) | parent-component replacement and non-overwriting **directory** moves remain declared limits, documented in the module |
| FS-02 | applies | source_traced_plus_OS_primitive | A 0-byte cap returned 95 bytes | **FIXED_VERIFIED** | B-fs-before (101) → B-fs-after-2 (0) | none |
| EF-01 | applies | source_traced_plus_OS_primitive | Write to `/dev/full`: `failure_phase: PreEffect, effect_state: None, observed_effects: []` after the target was opened and truncated | **FIXED_VERIFIED** | B-ef01-before-3 (101) → B-ef01-after-2 (0) | none |
| NET-01 | applies | source_traced | Chunk delimiters returned as the downloaded content; 5 bytes reported as a completed 11-byte transfer; contradictory framing accepted | **FIXED_VERIFIED** | C-net-before (101) → C-net01-closure (0) | none |
| NET-02 | applies | source_traced | 2.003 s transfer against a 300 ms declared bound; connect unbounded | **FIXED_VERIFIED** under the owner's D2 answer | C-net-before (101) → C-net02-after (0) | the total-budget policy is the owner's decision, recorded in `net.rs` |
| WS-01 | applies | source_traced | Two lengths → longer believed and `GET /x` swallowed into the body; duplicate `Host`/`Origin` → **200**; 64 silent sockets locked the owner out | **FIXED_VERIFIED** | C-ws01-before (101) → C-ws01-after (0) | none |
| SPEC-01 | applies | source_traced | Package opened **authoritative**, matching the anchor, holding a registry that was never verified | **FIXED_VERIFIED** | D-spec01-before (101) → D-spec01-closure (0) | none |
| UI-02 | applies | production_JS_with_controlled_dependencies | Edits typed during a reload were overwritten and marked clean; the settle guard caught the clobber | **FIXED_VERIFIED** | E-ui02-before (101) → E-ui02-closure (0), real HTTP | none |
| UI-03 | applies | production_JS_with_controlled_dependencies | With ordering disabled, disk held `"older text"` after the newer save was acknowledged | **FIXED_VERIFIED** | E-ui03-before-discrimination (101) → E-ui03-after-repeat-1..3 (0) | orders writes **this process** accepted; cross-process ordering is not claimed |
| SEM-01 | applies | source_traced_not_Rust_executed | At exactly **129** parentheses an explicit 7 became the DEFAULT 42 | **FIXED_VERIFIED** | F-before (101) → F-after-2 (0) | none |
| MEASURE-01 | applies | source_traced_not_freshly_executed | `5 m + 3 m` → `Decimal(8)`; end to end the real CLI exited **2** with both VERIFYs FALSE and output `'8'` | **FIXED_VERIFIED** | F-measure-cli-discrimination (2) → F-measure-cli-restored (0) | none |
| HANDLER-01 | applies | verification_gap_not_confirmed_defect | Gap resolved **into a defect**: three malformed results each recovered the diagnostic | **FIXED_VERIFIED** | G-handler01-before-4 (101) → G-handler01-after-7 (0) | none |
| Q-JSON | applies | review_question | Answered: a user-writable manifest at depth 10,000 aborted the process (SIGABRT, stack overflow) | **FIXED_VERIFIED** | G-qjson-probe (101) → G-qjson-after (0) | host reader bound only; no LCL depth limit introduced |
| Q-READ | applies | review_question | Answered in two halves. Excessive bound reported as a malformed parameter; non-UTF-8 read substitutes U+FFFD and reports success with no diagnostic | bound half **FIXED_VERIFIED**; lossy half **REPRODUCED** | G-qread-probe (101) → G-qread-bound-after (0); G-qread-closure (0) | **owner decision required** — see Closure |
| Q-NETDOMAIN | applies | review_question | Answered: a **404 download reported `status.succeeded`** and wrote the 23-byte error page as the file; same for 500 and 302 | **FIXED_VERIFIED** (a defect, not only a question) | C-qnetdomain-before (101) → C-qnetdomain-after (0) | none |

No finding was closed as NOT_REPRODUCED, and none was dropped.

## Changes, in actual order

| # | File | Reason | Meaningful change | Verification |
|---|---|---|---|---|
| 1 | `lcl-protocol/tests/engine_stages.rs` | entry baseline | stale success **count** replaced by the named set | C01-baseline-oracle-fixed (0) |
| 2 | `lcl-workspace/tests/debugging.rs` | RUN-02 | operation-pause reproduction + Continue/Cancel/pure/both-mode/later-run controls | A-run02-before-corrected (101) |
| 3 | `lcl-workspace/src/execution.rs` | RUN-02, RUN-01 | refusal carried to the host gate; check and registration made one critical section; waiting observes terminal state | A-run02-after-formatted (0), A-run01-after (0) |
| 4 | `lcl-capabilities/tests/real_filesystem.rs` **(new)** | FS-01, FS-02 | real-adapter reproductions on owned directories | B-fs-before (101) |
| 5 | `lcl-capabilities/src/fs.rs` | FS-01, FS-02, EF-01 | dangling-link resolution, `O_NOFOLLOW`, atomic non-overwrite reservations, bytes-collected read bound, post-open failure reporting | B-fs-after-2 (0) |
| 6 | `lcl-stdlib/tests/external_operations.rs` | EF-01, Q-NETDOMAIN, Q-READ | `/dev/full` effect-truth case, HTTP-status cases, read probes | B-ef01-before-3 (101) |
| 7 | `lcl-stdlib/src/host.rs` | EF-01, Q-NETDOMAIN, Q-READ | indeterminate effect on post-open failure; 2xx required before a transfer claim; excessive bound is out of range | B-ef01-after-2 (0), C-qnetdomain-after (0), G-qread-bound-after (0) |
| 8 | `lcl-capabilities/tests/transport.rs` **(new)** | NET-01, NET-02 | real loopback framing and bound cases | C-net-before (101) |
| 9 | `lcl-capabilities/src/net.rs` | NET-01, NET-02 | RFC 9112 §6.3/7.1/8 framing; total-budget deadline with bounded resolution and connection | C-net01-closure (0), C-net02-after (0) |
| 10 | `lcl-workspace/tests/transport.rs` | WS-01 | duplicate-field and ingress cases | C-ws01-before (101) |
| 11 | `lcl-workspace/src/http.rs` | WS-01 | repeated security/framing fields refused as malformed | C-ws01-headers |
| 12 | `lcl-workspace/src/server.rs` | WS-01 | ten-second ingress bound, lifted once a request passes all three gates | C-ws01-after (0) |
| 13 | `lcl-spec/src/lib.rs` | SPEC-01 | capture every file once; hash and parse the same capture | D-spec01-closure (0) |
| 14 | `lcl-workspace/tests/editor_save.cjs` | UI-02 | harness holds a GET; three mandated interleavings + inactive-tab control | E-ui02-before (101) |
| 15 | `lcl-workspace/assets/app.js` | UI-02 | revision checked at the real update point | E-ui02-closure (0) |
| 16 | `lcl-workspace/tests/editor_save.rs` | UI-02 | stale literal count replaced by "nothing failed, nothing skipped, coverage not lost" | E-ui02-closure (0) |
| 17 | `lcl-workspace/src/document.rs` | UI-03 | per-path publication ordering under one critical section, replacing saves only | E-ui03-after-repeat-1..3 (0) |
| 18 | `lcl-workspace/src/routes.rs` | UI-03 | superseded save reported 409, never as success | E-phase-closure (0) |
| 19 | `lcl-semantics/tests/data_resolution.rs` | SEM-01, MEASURE-01 | grouping probes and the measure matrix | F-before (101) |
| 20 | `lcl-semantics/src/eval.rs` | MEASURE-01 | result family and exact unit preserved, matching the runtime | F-after-2 (0) |
| 21 | `lcl-semantics/src/data.rs` | SEM-01 | undecided is UNKNOWN, not MISSING, so DEFAULT does not apply | F-after-2 (0) |
| 22 | `lcl-runtime/tests/handler_result_parity.rs` **(new)** | HANDLER-01 | malformed-claim matrix and both controls | G-handler01-before-4 (101) |
| 23 | `lcl-runtime/src/handler.rs`, `src/execute.rs` | HANDLER-01 | handler results validated against their registered schema | G-handler01-after-7 (0) |
| 24 | `lcl-project/tests/manifest_input_bounds.rs` **(new)** | Q-JSON | child-process depth probe | G-qjson-probe (101) |
| 25 | `lcl-spec/src/json.rs` | Q-JSON | host reader nesting bound, measured against real files | G-qjson-after (0) |
| 26 | `reports/implementation/LCL_RESIDUAL_REPAIR_REPORT.md` | R12 | dated continuations, history untouched | — |

Test-oracle corrections, each recorded with its cause and none weakening an
assertion: the stale example count (superseded by a repaired defect, corrected
to a named set); a `ResultRecord` method that does not exist and an assertion
vacuous for a document with no OUTPUT; an `ALLOW` block placed before the `LCL:`
header; duplicate-header cases expecting the gate's 403 where the parse-stage
400 applies, which is earlier and stricter; a saturation case expecting no
refusal where the property is **recovery**; a guard excluding MISSING but not
UNKNOWN; a handler oracle reading the wrong record; and two fixture claims that
were not actually malformed when checked against the registry.

## Verification records

Every gate is recorded under `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/logs/` with the V04 fields. Expected
reproductions are labelled and are **not** counted as acceptance passes. The
full-suite runs cover the focused targets; focused reruns are not double-counted.

## Contract and preservation review

- Canonical files before/after: **192 of 192 match**; validator 31 PASS /
  2 OUT_OF_SCOPE / 0 FAIL; `SHA256SUMS.txt` 175 matching; identity re-verified by
  the implementation's own algorithm.
- Historical archives/candidates: untouched. Brand: 17 matching.
- Existing behaviour retained: B1 process supervision, B2 atomic creation, B3
  SET materialisation, A1 contextual ENUM, A2/UI-01's three editor fixes, the
  66 decision witnesses, `.lcl`/`.lcl.txt` handling and exact save paths.
- Changes stayed inside the approved files and scope; no new dependency, no
  `unsafe`, no new platform mechanism, no language-semantic change.
- No unauthorised Git, remote or system action. Owned processes: none running.
  Owned fixture directories removed by exact verified path.

## Closure and handoff

**Two items are referred rather than claimed:**

1. **Q-READ, lossy decoding.** Established: `core.read` of non-UTF-8 bytes
   substitutes U+FFFD and reports success with no diagnostic, contradicting
   "exact content" and "no ambient encoding conversion". Not repaired, because
   the row's registered `errors` list does not admit
   `error.operation.postcondition`, and refusing would change what happens to
   every read of a non-UTF-8 file — a product behaviour change. **Owner
   decision required.**
2. **The two pre-existing workspace gates** (`cargo fmt --all -- --check`, 23
   B4 files; one `clippy::type_complexity` in
   `lcl-conformance/tests/semantic_cases.rs:35`) remain failing and are
   **LCL-CLOSE-02's** B4 items.

**LCL-CLOSE-02 entry state.** HEAD `7f13aec…`, worktree carrying this task's 27
files, nothing staged. Its owned findings are B4, DOC-01, HISTORY-01 and
COVERAGE-01. B4 begins from: 980 obligations, 2,413 required probes, 2,094
executed and passed, 0 failed, 66/66 witnesses, 0 missing source probes, **319
missing semantic probes** enumerated by ID, plus the two formatting/lint gates
above. The obligation mapping digest is
`17fb6df8dbe055ae12be61e341fd3ca6ac637ee94f4ba218b9cc55a18164fbed`.

Latest reusable successful gates and their source identity: FINAL-tests,
FINAL-msrv-check, FINAL-msrv-tests, FINAL-clippy-scoped, FINAL-witnesses,
FINAL-m8-report, FINAL-canonical-validator, FINAL-canonical-checksums,
FINAL-brand-checksums — all against the worktree as it stands at this report.
**Any further edit invalidates all of them** and they must rerun.

Plain-text session handoff: `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/LCL_CLOSE_01_HANDOFF.txt`.

No probability estimate and no universal bug-free guarantee is offered. "No
known blocker within the verified scope" is not proof of the absence of every
possible defect.
