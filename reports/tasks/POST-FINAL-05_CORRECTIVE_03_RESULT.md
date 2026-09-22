# POST-FINAL-05 corrective task — follow-up 03 (2026-09-22)

Continues `POST-FINAL-05_CORRECTIVE_02_RESULT.md`, which stays intact. That
report closed its own pass; this one takes up the standalone corrective work
order prepared against the audit of `5dc8c8d`, whose nineteen findings had not
been worked since sessions 1 and 2 closed RO-01, RO-02 and N-01.

Working ledger: `/mnt/F/.lcl-pretest/c3/ledger.md`, continued rather than
replaced. Logs: `/mnt/F/.lcl-pretest/logs/C4-*`.

## 1. Identity and state

| | |
|---|---|
| Project | `/mnt/F/LCL`, branch `main` |
| **Entry HEAD** | `5dc8c8d68c9b5a2bd1bcc56e67f21783b66489b1` ("LCL repair part2") — exactly the audited snapshot |
| Worktree at entry | **clean**: 0 modified, 0 staged, 0 untracked |
| Worktree now | 28 modified files, 0 untracked, 0 deleted — every one a change this pass made |
| Git writes by me | **None.** No add, commit, amend, merge, rebase, reset, checkout, stash, clean, tag, push, pull or fetch. Read-only inspection only. |
| Core 0.1 identity | `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, 176 files — unchanged |
| Core 0.2 identity | `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`, 216 files — unchanged |
| Mapping digest | `9b32a28b79d9c3872cb3f810365a3275583ce5731a74c98a8d2013d07633a8ad` — unchanged; no obligation added, removed or reclassified |
| F10 | decision A and its original-byte behaviour untouched |
| Canonical packages | not edited; `canonical/`, `releases/` and `assets/` are byte-identical |

Toolchains actually present and used: system `rustc`/`cargo` 1.98.1; MSRV
`1.75.0` at `/mnt/F/.lcl-residual-repair-01-lxd8dwu8/toolchains/1.75.0/bin`
(version verified); `node` v26.9.0; `python3` 3.14.7. There is no `rustup`.

**Status: `READY_FOR_OWNER_COMMIT`, and `BLOCKED_ON_DECISION` for the five
conformance sub-runs below. `TESTING_READY` is not claimed and is not true** —
§7 states exactly what is unmet.

## 2. Baseline, before any edit

`cargo test --offline --locked --workspace --all-targets --no-fail-fast`
against the committed source: **exit 0, 164 blocks, 1,741 passed, 0 failed, 1
ignored** (`C4-baseline-full.log`). The one ignored test is
`child_parses_nested_manifest`, "driven by its parent, which supplies the
depth" — a parent-driven case, not a suppressed failure.

One thing the audit did not report, found here: **`cargo fmt --all -- --check`
failed at the entry HEAD.** `impl/crates/lcl-semantics/tests/ordering_graph.rs`
was committed unformatted in `5dc8c8d` (confirmed by `git diff` being empty for
that file before it was formatted here). It is formatted in this pass, and it
is the only file changed that no finding assigned.

## 3. Disposition of every one of the nineteen audit IDs

States are distinguished on purpose. "Disproved with evidence" is not "not
reproduced"; a repair that removes an unsafe acceptance is not the same as
implementing the capability it refuses.

| ID | State | What was actually wrong |
|---|---|---|
| RO-01 | **Verified** (fixed in session 1) | Re-ran both regressions, exit 0. Its failed-lookup branch is examined in §4.1. |
| RO-02 | **Verified** (fixed in session 1) | Re-ran `ordering_graph`, 19/19. |
| N-01 | **Verified** (fixed in session 2) | `clippy --workspace --all-targets -- -D warnings` exit 0, covering this pass's new code. |
| N-04 | **Fixed and verified** | `--locked` refused *after* `engine.run` had already written the file. |
| N-05 | **Fixed and verified** | `check` returned `Accepted` with an empty diagnostic list for a document M4 rejected; `validate`/`inspect`/`run` reported the wrong stage and an empty span. |
| RO-03 | **Fixed and verified** | Two wrappers over one action shared a child and an invocation identity; one record survived where two must. |
| RO-04 | **Fixed and verified** | Two writes to two files collapsed into one effect; a graph that failed after writing reported `effect_state none`. |
| RO-05 | **Fixed and verified** | A child recovered by its own retry still failed the graph. |
| RO-06 | **Fixed (safety) — capability still unimplemented** | A supplied `selection` was ignored and the whole file replaced. |
| A-01 | **Fixed and verified** | An out-of-set effect was rejected through `Completed` and `Failed` and accepted through `Refused`. |
| A-02 | **Fixed and verified** | Absent and wrong-typed `problems` both read as "none"; `executed: 0` with every obligation satisfied was accepted; extreme counters made the gate itself panic. |
| A-03 | **Fixed and verified** | **Data loss**: copying a file onto itself emptied it. A move onto itself reported success while nothing moved. |
| A-04 | **Fixed and verified** | A half-done move and a truncating overwrite copy both reported the error whose meaning is "nothing began". |
| A-05 | **Fixed and verified** | Six ways a reply about text that no longer existed was stored and painted as a description of the text that did. |
| A-06 | **Fixed and verified** | Unhandled expected types accepted any text; an explicitly empty closed option list admitted every answer; `INTEGER` was an `i64`. |
| N-02 | **Fixed and verified** | `Shift+F12` ran `goToDefinition`; the `findReferences` branch was unreachable. |
| N-03 | **Fixed and verified** | The ingress bound was per-read, so a peer dribbling one byte at a time held a pre-authentication slot indefinitely. |
| D-01 | **Fixed** (comment) | The comment still asserted the key-contract equivalence that report 02 §7 withdrew. |
| V-01 | **Disproved with evidence — no defect, no source change** | See §5. |

Every repair carries an executed red run against the committed code and an
executed green run after it, with a positive control that fails if the repair
were simply "refuse everything". The ledger records each pair with its log.

## 4. The repairs that needed a decision, and what decided them

### 4.1 Pre-effect admission (N-04), and why a second load would not do

`stage_command` called `engine.run` and asked `locked_drift` afterwards. All
four ways a lock can disagree — absent, unreadable, changed root, changed
import — returned exit 4 *and* left the file written. An exit code cannot tell
a run that was stopped from a run that was regretted, so the regressions assert
the target's bytes.

Checking first by loading the source a second time is not a repair: the second
load is a different read of a mutable filesystem, and what it admits is not
necessarily what the first one executes. So the question is asked inside the
one staged walk, through `Engine::run_admitted`, after step 4 has made the
units known and before step 10 may reach outside the language. Absence and
unreadability need no source at all and are settled before a byte is read.

`an_admission_check_sees_the_snapshot_that_is_executed` drives a provider that
answers every request with different bytes and asserts that the identities the
check saw are the identities the report records, and that the import was read
**once**. A check-then-run design fails that assertion by construction.

### 4.2 The complete checker verdict (N-05), and what the audit overstated

`Checked::outcome` rejects for either of two channels; `Checked::primary` sees
only the static one. For the alias-cycle fixture the static list is **empty**,
so `Engine::check` returned `Accepted` and said nothing about why.

The audit's stronger reading — that the same invalid source executes through
`run` — is **disproved**: `Preflight::plan` already refused it, and the
regression records that. What `run`, `validate` and `inspect` did lose was the
provenance: the generic skipped-stage fallback reported `StaticOrExpression`
and an empty span where `StageSkipped` carried `Resolution` and the real locus.

The conformance runner had the same two defects and is repaired with the same
rule. That matters for scoring: a probe expecting `error.reference.cycle` at
resolution was being observed at the wrong stage.

### 4.3 Caller-specific delegation (RO-03)

Step 6 already distinguishes activations by declaration, loop context **and**
enclosing delegating invocations, so two rows delegating to one unit are two
planned children. `execute_graph` searched the whole plan for a name and handed
every caller the first one — another invocation's child. The invocation path
came from `graph_depth`, a counter reused as soon as a call returned, so both
wrappers shared one identity and the aggregation's `before` snapshot then
omitted the second execution entirely.

The child is now the caller's own planned child, and the path is derived from
the caller's identity, which already carries its loop context and its retry
attempt. `graph_depth` is gone. Two of the four new regressions were red; the
loop and nested cases pass against the committed code too and are recorded as
**controls, not reproductions**.

### 4.4 Truthful graph effects and outcomes (RO-04, RO-05)

A failed graph returned `Resolution::failed`, which is the *pre-effect* answer:
`05_SEMANTICS/09` allows one only with "effect_state none, an empty
observed_effects list, and no bound or partial OUTPUT". A graph whose first
child wrote a file cannot give it. `Resolution::Refused` now carries the
observation, mirroring `CapabilityOutcome::Refused`, and a graph may claim
proven-effect-free only when every invocation it aggregates established that
nothing began — which the pre-effect control exercises.

On success, observations were deduplicated by effect class and state alone, so
two writes to two files became one. The class and dependency *unions* the
contract speaks of are over the axes, which the request already carries; the
observed-effect list is separate, ordered, target-specific evidence.

For RO-05, every retained attempt voted on the graph's outcome, so a child
recovered by its authorized retry still failed it — and offered a second
primary result, turning "exactly one material primary" into none. The verdict
is now the governing final attempt per `(node, iteration)`; every attempt is
still retained as evidence.

### 4.5 `core.modify` — a refusal, not a capability (RO-06)

The postcondition is "only declared selection/properties change". The adapter
wrote `change` over the whole target whatever `selection` said, which is
exactly what that forbids.

There is no selection vocabulary to implement instead. `range` is defined for
`core.read` and scoped to it, and no registered change profile fixes what a
bounded *replacement* means — what happens to a line terminator under
`unit: line`, for one. Any syntax accepted here would be this build's
invention. So a supplied selection is refused before anything is read or
written.

**This removes an unsafe acceptance. It does not make bounded modification a
supported capability**, and nothing in this report should be read as claiming
it does. Closing that gap needs canon or an owner decision.

### 4.6 File safety (A-03, A-04)

A-03 destroyed data: `std::fs::copy` opens its destination truncating, so
copying a file onto itself — by a second spelling, a symbolic link or a hard
link — emptied it. The regression caught the source at zero bytes.

Path strings cannot answer this. Two spellings and a symlink resolve to one
canonical path, but a hard link is a second directory entry for one inode with
a canonical path of its own, so the file's identity is asked for by device and
inode. Where a platform offers no such identity the code says so rather than
pretending; a hard link would go undetected there.

The two rows differ and the canon says so. `core.move` states "resolved source
and destination addresses are distinct" as a precondition, so it **refuses**.
`core.copy` states no such precondition and both of its postconditions already
hold when the two ends are one file, so it **does nothing** and reports that.

A-04: a move links the destination and then unlinks the source. A failure of
the unlink was converted with `io()`, whose documented meaning is "Nothing
began" — with the new link already on disk. The overwrite copy is now staged
like the reserving path beside it, so a failure to open the destination is
still "nothing began" and a failure after it has been truncated is not. Both
directions have a control.

### 4.7 The frontend (A-05, N-02)

Both are tested against the **unmodified production `app.js`**, in the existing
`editor_save.cjs` harness, and then again through `editor_save.rs` against the
real workspace server over real HTTP. No production function is replaced. This
is exact-function plus controlled-DOM plus real-HTTP evidence; it is **not**
real-browser evidence, and the harness says so.

`refreshTokens`, `runAnalysis` and the explicit check each captured the current
document, sent its text, and assigned whatever came back. The document already
carries what settles it: `revision`, which `save` and `reload` have always
tested before applying their own answers. The same test now guards the three
paths that lacked it, and painting is separated from per-document caching — a
background tab keeping its own verdict is correct; that verdict appearing in
the shared diagnostics panel is not.

Revision is the whole of the question here because each answer is a function of
the submitted bytes alone: two replies for one revision are interchangeable,
and a reply for any other revision describes text that is gone.

N-02: `e.key === "F12"` is true with Shift held, and it was tested first, so
`Shift+F12` ran `goToDefinition` and the `findReferences` branch was dead. Both
ordering and specificity are fixed; `Ctrl+Shift+F` is untouched and asserted.

All seven new frontend cases fail against the committed `app.js` and pass
against the repaired one; the sixteen pre-existing cases pass in both. The
suite's coverage floor moved from 15 to 23.

### 4.8 Ingress (N-03)

`INGRESS_TIMEOUT` was a socket read timeout, which bounds one idle wait. A peer
sending a byte just inside it resets it and holds a slot for as long as it
likes, and the token is not looked at until the request has been read. It is
now a total budget over request line, headers and body, on a monotonic clock,
re-armed before every read, with `read_exact` replaced by a loop that honours
it. A platform that will not install the bound is refused rather than served
unbounded. After admission the bound is lifted, so a debugging event stream
stays long-lived — the repair must not break the feature it protects.

The cases use a **400 ms budget on their own server** through
`Server::ingress_budget`, so the suite does not sleep and the shipped
ten-second policy is untouched. The product never calls that method.

### 4.9 `core.ask` (A-06)

An unhandled `expected_type` defaulted to accepting text, so `OBJECT` answered
with a sentence was "compatible". An explicitly empty `options` list — which is
*supplied*, since its default is `null` — was folded into the absent case, so
the one parameter whose purpose is to admit only some answers admitted all.
And `INTEGER` was `text.parse::<i64>()`, which is a width the language does not
have, while `DECIMAL` was `f64`, which accepts `inf`, `NaN` and `1e5`.

The answer is now *constructed* as a value of the expected type using the same
functions that decode an `INTEGER_LITERAL` and a `DECIMAL_LITERAL`, so what is
recorded is compatible because it was built that way. A type this adapter
cannot construct from text yields the row's own registered answer for "no
authorized valid answer" — and the question that was asked keeps its message
effect on every refusal.

`compatible_answer`, which judges already-typed **options** rather than typed
text, is deliberately left alone: an `OBJECT` option really is compatible with
`OBJECT`, and that is a different question.

## 5. V-01 — the investigation, and its answer

The audit asked whether `core.execute` over a TASK enforces that TASK's own
`SUCCESS`, required outputs and evidence. It is a question, and the answer is
no — correctly.

`05_SEMANTICS/10` scopes the obligation every time it states it: "At a TASK
**execution root**, status.succeeded is legal only when its SUCCESS is TRUE";
the heading "**ROOT SUCCESS** — A TASK root evaluates its referenced SUCCESS";
"The **execution root** still fails whenever its own required SUCCESS, OUTPUT,
rule, or evidence condition is unsatisfied." And for anything that is not the
root: "A result record's status is instead scoped to the producer invocation …
its domain outcome remains independent."

The sentence the audit leaned on — "Explicit graph-valued operation invocations
create their own child invocation under the same rules" — is in
`05_SEMANTICS/01` under **CANDIDATE GRAPH**, and the rules it is about are the
activation rules. A delegated TASK is a child invocation, not a root.

Three cases were executed: an inner `SUCCESS` of `FALSE`, an inner required
`OUTPUT` never bound, and a fully satisfied control. All three behave as the
canon prescribes, on the current *and* the committed runtime. **No defect, and
no source change.** The regressions are retained so the reading cannot drift
silently.

The first OUTPUT fixture was rejected at grammar for a missing required
`FORMAT` field. That is the "unrelated fixture rejection" the work order warns
about, and it was corrected so the intended path is genuinely reached.

## 6. Conformance: one sub-run closed, five open

`core.group error/operator.operand` is **closed by implementation**, which this
work order authorized.

The row must "union every applicable error of a referenced key operation" and
lists `error.operator.operand` among its own. The embedder surface
`PureOperation` returned `Result<Value, String>` — a failure with no registered
identifier — so every key-operation failure arrived as
`error.operation.precondition` and the union could never contain anything else.
That is a missing failure channel, not a missing trigger.

`PureFailure` now carries an optional registered identifier. **The row decides,
not the implementation**: an identifier the row's closed `errors` list does not
admit is not adopted, which a regression proves with `error.permission.denied`.
An implementation may classify its own failure; it may not move a diagnostic
into a row that never listed it.

| Population | Before | After |
|---|---|---|
| Source probes | 2,011 required, 2,011 satisfied | **unchanged** |
| Semantic probes | 402 required, 399 satisfied, 3 invalid | **402 required, 400 satisfied, 0 failed, 0 missing, 2 invalid** |
| Original 79-sub-run register | 73 closed, 6 open | **74 closed, 5 open** |

These are different populations and nothing here equates them. No obligation
was added, removed, reclassified or hidden; the mapping digest is unchanged.

### The five that remain — owner decisions

Report 02 §8 established each of these and they are unchanged. They are
restated because they are the remaining boundary, not to re-open them.

| Sub-run | Decision needed |
|---|---|
| `core.sort precondition/key-operation-profile-{missing,ambiguous,incomplete,out-of-bounds}` (4) | An approved mapping correction for these four, **or** canon naming what a key operation's "profile" is. A custom `kind.operation` selects no profile and `core.sort` requires no role, so as written the condition cannot exist. D-01's comment now records this as open rather than asserting the withdrawn equivalence. |
| `core.execute precondition/profile-out-of-bounds` (1) | An approved mapping correction, **or** canon narrowing `core.execute`'s maxima, **or** an approved re-reading toward the invocation-vs-profile condition with the catalog defect fixed first. This row's maxima are the entire closed vocabulary of both axes, so no profile can exceed them. |

Nothing in this pass may settle these: each removes or reclassifies required
evidence, or decides what the canon means.

## 7. Gates, with their real exit statuses

Run through `/mnt/F/.lcl-pretest/c1/c4-gate.sh`, which records each command's
own status **before** anything formats, filters or displays its output. Its
self-test was executed first and **passed**: a command exiting 3, and one
printing `test result: ok. 9 passed; 0 failed` while exiting 1, are both
recorded and both fail the gate. Isolated `TMPDIR=/tmp/lcl-corrective`,
out-of-tree `CARGO_TARGET_DIR`. Statuses in
`/mnt/F/.lcl-pretest/c1/c4-results.tsv`; logs `C4-G-*`.

| Command | Exit | Expected | Result |
|---|---:|---:|---|
| `cargo fmt --all -- --check` | 0 | 0 | clean (after formatting the flagged files, §2) |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | 0 | clean |
| `cargo test --offline --locked --workspace --all-targets --no-fail-fast` | 0 | 0 | 164 blocks, **1,794 passed, 0 failed, 1 ignored** |
| MSRV 1.75.0 `cargo check --workspace --all-targets` | 0 | 0 | clean; one pre-existing `redundant_imports` warning at `clauses.rs:1132`, a line this pass did not touch |
| MSRV 1.75.0 `cargo test --workspace --all-targets --no-fail-fast` | 0 | 0 | 164 blocks, **1,794 passed, 0 failed, 1 ignored** — identical to stable |
| `real_process`: 12 sequential + 3 × 6 concurrent | 0 | 0 | **30 of 30**, original cleanup assertions intact |
| Core 0.1 `sha256sum -c --strict --quiet` / `validate_release.py --scope all` | 0 / 0 | 0 / 0 | verifies |
| Core 0.2 `sha256sum -c --strict --quiet` | 0 | 0 | verifies |
| Core 0.2 `validate_release.py --scope all` | 1 | **1** | BLOCKED on `independent_review`, the owner's — unchanged |
| `validate_language_contracts` / `validate_localization` / `validate_source_fixtures` | 0 | 0 | pass |
| `validate_ebnf` 0.1.0 / 0.2.0 | 0 / 0 | 0 / 0 | pass |
| Core 0.1 / Core 0.2 identity | 0 / 0 | 0 / 0 | `00d648b1…` 176 / `00daee8d…` 216 |
| `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 0 | 0 | verifies |
| `m8_conformance_report` | 0 | 0 | source 2,011/2,011; semantic **400 satisfied, 0 failed, 0 missing, 2 invalid** |
| **`m8_conformance_gate`** | **1** | **1** | **REFUSED**, naming the claim and the two open probes |
| Protected areas + candidate checksums | 0 | 0 | no change under `canonical`, `releases`, `assets` |
| `impl/target/test-tmp`, `/tmp/lcl-apps` | — | — | 0 entries written |

Gate verdict: **50 commands, each with its expected status.**

One flake, recorded rather than hidden:
`a_real_process_timeout_retains_partial_stdout_and_effect_uncertainty` failed
once under full-workspace load, expecting partial stdout `"observed-prefix"`
and observing `""`, then passed 3/3 in isolation and in every later full run.
It is timing-sensitive under load and matches the `real_process` flake recorded
at PRETEST-05. It is **not** caused by this pass — it exercises `core.run` with
no graph involvement — and it was not suppressed, ignored or weakened.

**Not run, and why.** The candidate build and its installed smoke checks: they
belong after the owner commits, against that frozen revision, and building from
an uncommitted worktree would produce a candidate whose source nobody can name.
The every-file ledger and the independent read-only audit: FINAL-05's contract
requires a separate reviewer, and my own verification is not that review.

## 8. Exactly what changed

28 files, +3,484 / −160. No file outside `impl/crates/` was modified.

**Implementation (14).** `lcl-capabilities/src/fs.rs`;
`lcl-checker/src/contracts.rs`; `lcl-cli/src/main.rs`;
`lcl-conformance/src/{acceptance.rs,runner.rs,operation_cases/clauses.rs}`;
`lcl-protocol/src/engine.rs`; `lcl-runtime/src/{execute.rs,operations.rs}`;
`lcl-stdlib/src/{data.rs,host.rs,lib.rs,pure.rs}`;
`lcl-workspace/src/{http.rs,server.rs}`; `lcl-workspace/assets/app.js`.

**Tests and test support (13).** `lcl-capabilities/tests/real_filesystem.rs`;
`lcl-cli/tests/projects_and_capabilities.rs`;
`lcl-conformance/tests/acceptance.rs`; `lcl-protocol/tests/engine_stages.rs`;
`lcl-stdlib/tests/{boundary.rs,external_operations.rs,pure_operations.rs}`;
`lcl-workspace/tests/{common/mod.rs,editor_save.cjs,editor_save.rs,transport.rs}`.

**Formatting only (1).** `lcl-semantics/tests/ordering_graph.rs` — the
pre-existing violation in §2.

Public interfaces that changed, all of them internal to this workspace:
`Engine::run_admitted`; `Resolution::Refused` / `Resolution::refused`;
`GraphInvocation::effect_state`; `PureOperation`'s error type, now
`PureFailure`; `Contracts::default_specificity_rank`; `FsError::SameFile`;
`Server::ingress_budget`; `http::Deadline`. No language meaning, error
identifier, owning stage, status rule, operation maximum, scope, profile
meaning or output contract was changed.

## 9. Where this stops

**`READY_FOR_OWNER_COMMIT`.** Nothing is staged and nothing is committed. The
proposed message:

```
LCL post-final-05 corrective 03: sixteen audit repairs and one sub-run

Pre-effect --locked admission (N-04) and the complete checker verdict with its
provenance (N-05). Caller-specific graph delegation with stable invocation
identities (RO-03), truthful graph effects (RO-04) and recovery-aware graph
outcomes (RO-05). core.modify refuses an unsupported selection instead of
replacing the whole target (RO-06). Effect-set validation on every
observation-bearing outcome (A-01). Readiness-report validation of required
fields, impossible accounting and extreme counters (A-02). Same-underlying-file
copy and move (A-03) and truthful filesystem error phases (A-04). Frontend
replies bound to the document revision they describe (A-05) and Shift+F12
dispatch (N-02). core.ask answer typing and closed choices (A-06). A total
pre-authentication HTTP ingress bound (N-03). The withdrawn key-contract
equivalence corrected in its comment (D-01). V-01 investigated and disproved,
with its regressions retained. core.group carries a referenced key operation's
registered error, closing one required sub-run: semantic 399 -> 400 satisfied.

Five sub-runs remain open on owner decisions; m8_conformance_gate still
refuses, which is the correct blocked state.
```

**`BLOCKED_ON_DECISION`** on the five sub-runs in §6, and on the two owner
items this pass did not touch: Core 0.2 `independent_review`, and the manual
desktop/menu/file-association acceptance. No repository setting was changed, no
CI was triggered, and no manual interaction was marked complete.

**`TESTING_READY` is not met.** The readiness gate refuses with exit 1, two
required semantic obligations are unestablished, `core.modify`'s bounded
selection is refused rather than supported, no candidate has been built from a
frozen source revision, and the independent review the contract requires has
not happened. Each of those is a real boundary, not a formality.

### The exact next action

Review §8 and commit, or say what to change first. After the commit the
candidate can be built from the frozen revision and put through its installed
smoke checks; the five decisions in §6 are independent of that and can be taken
at any time.
