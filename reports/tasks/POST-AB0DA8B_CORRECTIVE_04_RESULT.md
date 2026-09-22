# Complete standalone corrective assignment — result (2026-09-22)

Answers the assignment prepared against the audit of `ab0da8b`. Twenty items in
five categories, not twenty proven defects: nine behavior findings, one
capability gap, three normative questions, one interpretation, one
test-reliability problem, four acceptance boundaries and one out-of-scope item.

Previous reports are intact. Working ledger: `/mnt/F/.lcl-pretest/c4/ledger.md`
(new, cross-referencing `c3/ledger.md`). Logs: `/mnt/F/.lcl-pretest/logs/C5-*`.

## 1. Identity and state

| | |
|---|---|
| Project | `/mnt/F/LCL`, branch `main`, 18 crates |
| **Entry HEAD** | `ab0da8b09f89a6f7e98350e6792ea188801e2eec` ("LCL correction") — the assignment's baseline |
| Worktree at entry | **clean**: 0 modified, 0 staged, 0 untracked |
| What `ab0da8b` is | verified with `git show --stat`: the previous session's 28 files plus `POST-FINAL-05_CORRECTIVE_03_RESULT.md`, committed verbatim. Nothing else. |
| Worktree now | **22 modified files, 0 untracked, 0 deleted** — every one a change this pass made |
| Git writes by me | **None.** No add, commit, amend, merge, rebase, reset, checkout, stash, clean, tag, push, pull or fetch. Read-only inspection only. No Git action was attempted and denied. |
| Core 0.1 identity | `00d648b1…67ed`, 176 files — unchanged, verified this session |
| Core 0.2 identity | `00daee8d…e604`, 216 files — unchanged, verified this session |
| Mapping digest | `9b32a28b…a8ad` — unchanged; no obligation added, removed or reclassified |
| F10 | decision A and its tests untouched |
| Protected areas | `canonical/`, `releases/`, `assets/` byte-identical |

Toolchains verified present and used: system rustc/cargo **1.98.1**; MSRV
**1.75.0** at `/mnt/F/.lcl-residual-repair-01-lxd8dwu8/toolchains/1.75.0/bin`;
node **v26.9.0**; python3 **3.14.7**. There is no `rustup`.

**Status: `READY_FOR_OWNER_COMMIT`, and `BLOCKED_ON_DECISION` for the items in
§5. `TESTING_READY` is not claimed and is not met** — §7 states exactly what is
unmet.

Entry baseline, before any edit:
`cargo test --offline --locked --workspace --all-targets --no-fail-fast` →
**exit 0, 164 blocks, 1,794 passed, 0 failed, 1 ignored** (`C5-baseline.log`).
The one ignored test is `child_parses_nested_manifest`, "driven by its parent,
which supplies the depth".

## 2. The nine behavior findings

Four of the six `AB-*` findings are residuals of my own previous session's
repairs. That is stated plainly rather than folded into the prose.

| ID | Disposition | What was wrong |
|---|---|---|
| AB-01 | **Repaired and verified** | *Introduced by C3.* Both `Resolution::Refused` arms raised a fault with a phase fixed at the call site, then `record_of` raised the same fault again with the phase the observation establishes — two emissions, and the caller returned the assumed one. Observed as `[(1, PostEffect), (0, Indeterminate)]`. |
| AB-02 | **Repaired and verified** | *Introduced by C3.* `graph_effects` folded by `(class, state, target, evidence)` — the effect's description, not the effect — so two appends to one file became one. |
| AB-03 | **Repaired and verified** | *Introduced by C3.* The same-file copy returned `Ok(0)` and `transfer` gave every successful copy an applied filesystem effect, so a no-op reported changing the file it had deliberately left alone. |
| AB-04 | **Repaired and verified** | Pre-existing. `create_dir_all(parent)` ran before the source open, and a later failure was converted as "Nothing began" while the directories were on disk. |
| AB-05 | **Repaired and verified** | `describes()` compared only the root revision; `inspect` resolves imports from disk, so two analyses at one revision can describe different inputs and the older could replace the newer. |
| AB-06 | **Repaired and verified** | *Introduced by C3.* The pre-effect admission was right, but the pre-existing post-run `--locked` block re-read the live lock and discarded the report of a run whose effects had already happened. |
| C-01 | **Repaired and verified, both layers** | `ALL: []` was rejected by the checker, and — once accepted — read by completion as one member whose value is an empty LIST. |
| C-02 | **Repaired and verified** | A MISSING, UNKNOWN or faulting first condition emitted and continued, so a later clause was still *selected* with its own status, classification and evidence. |
| C-03 | **Repaired and verified (2 parts); 1 part disproved** | Any non-empty CHECKSUM made required evidence traceable; a rendered SOURCE string counted as resolved with nothing observed. The PROVENANCE-reference half is already enforced a stage earlier. |

Every repair has an executed red run against the code as committed and a green
run after it, each with a positive control that fails if the repair were merely
"refuse everything". The ledger records each pair with its log.

### What the evidence had to be, in three cases

**AB-01 could not be shown by counting diagnostics.** `diagnostic::select` runs
`merge_duplicates`, which collapses the pair *when their phases coincide* — so
a count alone is satisfied by display deduplication, which the assignment
rightly forbids relying on. The discriminating observable is the phase
disagreement between the record and the diagnostic, and the test asserts both.

**AB-02's nested control passed before the repair** — because the content fold
happened to hide the inherited duplicate. Removing the fold without also
marking which rows are aggregates would have broken it. `GraphInvocation`
now carries `aggregate`, read from the canonical `result.command.mode`, and
the failure path declares that mode too, which the canon requires for a
REFERENCE target regardless of outcome.

**AB-06 needed the lock to change between the admission and the re-read.** The
run does it itself, under an explicit grant, writing to its own `lcl.lock` —
which is not source, so the admission's subject is untouched. Without that the
window can only be hit by racing the process.

### C-01, where the canon settled it

`field_signatures#/value_kind_registry/boolean_or_reference_list` is "A
boolean_expression **or a possibly empty LIST** containing only REF values".
So `ALL: []` is legal and is *zero* members.

The checker was rejecting it through a rule that is itself canonical — "An
empty bracket literal without one expected member type uses
error.type.mismatch" — so the rule was **not** weakened. What was missing was
the receiving context: the slot is now received as `Expected::Identity`, which
is what the bracket form is, and which passes a scalar through unchanged so the
`boolean_expression` half is judged exactly as before.

Completion then decided "is this a list?" by whether it yielded any *reference*,
so a list of nothing looked like no list. It now decides by the shape written.
A member that is not a REF is kept as UNKNOWN rather than dropped — dropping it
would make a list of unjudgeable members quantify vacuously.

### C-02, where the canon ordered it

`05_SEMANTICS/10`: "An existing primary unhandled diagnostic always fixes
status … A FAILURE mapping cannot override that diagnostic. **Otherwise**
evaluate applicable FAILURE clauses". Once a clause's own condition has raised
a diagnostic, the "otherwise" no longer holds. Handler selection happens at
execution; this step runs after it, so a diagnostic raised here is unhandled by
construction.

The existing tests asserted `terminal_status()`, which precedence already
protected. What they could not see is that `failure.later` was still selected,
carrying `requested_status: "status.stopped"` and its own classification into
the verdict. FALSE still continues the scan, and its control proves it.

### C-03, decomposed rather than answered in one piece

"EVIDENCE must be observable, typed, and traceable; declared required
provenance/checksum must resolve." Three parts, three different answers.

**CHECKSUM — repaired.** `sha256_string` is "A STRING containing 'sha256:'
followed by exactly 64 lowercase hexadecimal digits". `satisfied()` accepted
any non-empty string, so `CHECKSUM: "x"` made a required declaration
traceable. Now checked against that form — prefix, exact length, lowercase hex.
This is the **form**, not the content: whether the digest matches the bytes is
for whoever can read them, and this layer performs no effect.

**PROVENANCE reference — disproved with evidence.** A REF naming a
non-EVIDENCE declaration is already refused one stage earlier:
`error.reference.kind`, "`data.number` resolves to a DATA, but
EVIDENCE.PROVENANCE accepts EVIDENCE". No repair, and it is recorded so the
guard is not re-implemented above the layer that already has it.

**"Observable" — repaired for the reference form, open for the others.**
`Provision::Source` counted as resolved on a rendered string alone, which is
inferring existence from syntax. It now also carries what the source names, and
a required declaration whose SOURCE names a declaration is established only
when this run observed it — through `Observation`, the existing interface,
documented as "the whole admissible target universe for a targeted
post-execution check. Anything else is not observed." Nothing fetches anything;
completion still performs no effect. No canonical example declares an
EVIDENCE `SOURCE`, so nothing shipped changes behaviour.

The PATH and URI forms name something outside the document, and the canon gives
no procedure for establishing observability of one at completion. It is **not**
asserted either way; §5 states the question.

## 3. The chain, reviewed once, end to end

After the targeted repairs, the whole producer-to-consumer path was reviewed
together rather than finding-by-finding:

filesystem state → adapter (`Copied`, `create_parents`, `final_drain`) →
observation (`ObservedEffect`, `proven_effect_free`) → runtime record
(`record_of`, `phase_of`, one emission) → aggregation (`aggregate`,
governing attempt) → recovery/retry (`retry_is_safe`) → completion (success,
failure, evidence) → public protocol/CLI/editor → conformance and the
readiness gate.

Two things that review established, both now asserted:

* **Retry safety consumes the repaired evidence.** `retry_safety` admits a
  further attempt only for "a `pre_effect` attempt with `effect_state none`".
  A graph that changed something and reported `none` would have been admitted
  on the strength of an untruth. `a_graph_that_changed_something_is_not_
  admitted_for_a_blind_retry` runs the whole chain and asserts a single
  attempt.
* **One cross-crate consumer needed updating** for the `Provision::Source`
  shape: `lcl-protocol/src/engine.rs`. Found by compiling the workspace, not
  by assuming.

Nothing else in the sweep turned up a further defect. Where a search found a
guard already in place — the PROVENANCE reference kind — it is recorded as
such rather than reported as a gap.

## 4. QA-01 — a real defect, and an honest limit

The case is
`lcl-stdlib/tests/external_operations.rs::a_real_process_timeout_retains_
partial_stdout_and_effect_uncertainty`. The script writes `observed-prefix`,
then `exec /bin/sleep 3`; the deadline is 200 ms; the test requires the
already-written prefix to be reported.

**Category: product behaviour, not test synchronization.** In
`process/linux.rs::run`, the supervision loop drains, checks the deadline,
sleeps. When the deadline fires it returns at once and the descriptors are then
closed — with **no final drain**. Bytes the child had already written but that
had not been read were discarded. `result.command` requires that "after start,
started is TRUE and stdout and stderr are present", and an empty stdout for a
program that wrote is not that. Under load the window between the last drain
and the deadline widens, which matches the intermittency.

Repaired by terminating first, so nothing more can be written, then draining
both pipes — bounded by EOF and by `TEARDOWN`, never waiting for output that is
not already there — and only then closing them. No sleep was lengthened, no
assertion relaxed, nothing skipped, and the cleanup assertions are untouched.

**What this does not establish.** The original failure did **not** reproduce
under 150 concurrent (25 × 6) runs of the *unrepaired* binary, so this is not
proven to be the cause of the one observed failure. It is a real defect that
produces exactly that symptom. After the repair: 150 concurrent plus 25
sequential runs, 0 failures. The earlier failure is preserved in the C3 record
and this stays an open watch item rather than a closed one.

## 5. Capability, normative questions, and what needs a decision

Each was re-checked against the registries this session rather than carried
forward on assertion.

**CAP-01 — bounded `core.modify` is still unimplemented.**
`required_roles_by_operation` gives `core.modify` the role `{"all":
["change"]}`, so a change **profile role** does exist and is selected as for any
other row. What does not exist anywhere in the registries is a **selection
vocabulary**: nothing defines what an "exact bounded selection" is for this row,
and `range` remains scoped to `core.read`. The refusal-before-write therefore
stays exactly as it is, and its tests — unselected content preserved,
`expected_before` match and mismatch, whole-target control,
unsupported-selection refusal before any write — are retained and pass. This is
a mitigation, not a finished capability.
*Decision needed:* define the selection/change semantics for `core.modify`, or
record that bounded modification is out of scope for this testing round.

**SPEC-01 — the four `core.sort` key-operation-profile sub-runs.**
`core.sort` has **no entry** in `required_roles_by_operation`, and
`role_resolution` states "A core operation absent from
required_roles_by_operation requires no local core profile." Confirmed
mechanically this session. The four sub-runs name a profile-selection fault for
a row that selects no profile.
*Decision needed:* an approved mapping correction for these four, **or** canon
defining what a key operation's "profile" is. Parameter count and result type
are not automatically profile evidence, and no pin was dropped.

**SPEC-02 — `core.execute precondition/profile-out-of-bounds`.**
The row declares `{"non_graph": ["execution"], "graph": []}`, and "graph mode
has no local execution profile and resolves every reachable operation profile
transitively". Its maxima remain the entire closed vocabulary of both axes, so
no profile can exceed them; an *invocation* exceeding a selected profile's
narrower declaration is a different condition.
*Decision needed:* an approved mapping correction, **or** canon narrowing the
row's maxima, **or** an approved re-reading toward the invocation condition
with the catalog defect addressed first.

**SPEC-03 — enclosing-scope applicability.** Remains an open normative
question, re-grounded in pass 2 on authority ranks 1 and 5 rather than on a
canonical example. Tested pattern enforcement and authorization-before-effects
are preserved and pass.

**C-03's third part — observability of an external SOURCE.**
*Decision needed:* whether a required EVIDENCE whose `SOURCE` is a PATH or URI
must have been observed during the run to count as established, and by what
means, given that completion performs no effect. Currently not asserted either
way.

**V-01 — interpretation pending, with the missing case now covered.**
Report 03's three cases are retained and pass. The case it did not cover — a
required *inner* EVIDENCE that is not established — is now executed: the inner
TASK's required evidence **is** collected and **does** block the containing
run, unlike its SUCCESS and its OUTPUT. The canonical basis for the difference
is that SUCCESS and root OUTPUT are scoped to "a TASK **execution root**",
while evidence "is collected because something that ran named it" — collected
*from what ran* rather than evaluated *at* a root. This is executed behaviour
with its clauses recorded; it is not a claim that the hierarchy is settled, and
it goes to the independent language review. Not a confirmed bug, not a PASS.

## 6. Gates, with their real exit statuses

Run through `/mnt/F/.lcl-pretest/c1/c5-gate.sh`, which records each command's
own status **before** anything formats, filters or displays its output. Its
self-test was executed first and **passed**: a command exiting 3, and one
printing `test result: ok. 9 passed; 0 failed` while exiting 1, are both
recorded and both fail the gate. Isolated `TMPDIR`, out-of-tree
`CARGO_TARGET_DIR`. Statuses in `/mnt/F/.lcl-pretest/c1/c5-results.tsv`.

| Command | Exit | Expected | Result |
|---|---:|---:|---|
| `cargo fmt --all -- --check` | 0 | 0 | clean |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | 0 | clean (two findings of mine repaired) |
| `cargo test --offline --locked --workspace --all-targets --no-fail-fast` | 0 | 0 | 164 blocks, **1,825 passed, 0 failed, 1 ignored** |
| MSRV 1.75.0 `cargo check --workspace --all-targets` | 0 | 0 | clean; one pre-existing `redundant_imports` warning at `clauses.rs:1132`, untouched by this pass |
| MSRV 1.75.0 `cargo test --workspace --all-targets --no-fail-fast` | 0 | 0 | 164 blocks, **1,825 passed, 0 failed, 1 ignored** |
| `real_process`: 12 sequential + 3 × 6 concurrent | 0 | 0 | **30 of 30**, original cleanup assertions intact |
| Core 0.1 `sha256sum -c --strict --quiet` / `validate_release.py --scope all` | 0 / 0 | 0 / 0 | verifies |
| Core 0.2 `sha256sum -c --strict --quiet` | 0 | 0 | verifies |
| Core 0.2 `validate_release.py --scope all` | 1 | **1** | BLOCKED on `independent_review`, the owner's — unchanged |
| `validate_language_contracts` / `validate_localization` / `validate_source_fixtures` | 0 | 0 | pass |
| `validate_ebnf` 0.1.0 / 0.2.0 | 0 / 0 | 0 / 0 | pass |
| Core 0.1 / Core 0.2 identity | 0 / 0 | 0 / 0 | `00d648b1…` 176 / `00daee8d…` 216 |
| `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 0 | 0 | verifies |
| `m8_conformance_report` | 0 | 0 | source **2,011/2,011**; semantic **400 satisfied, 0 failed, 0 missing, 2 invalid** |
| **`m8_conformance_gate`** | **1** | **1** | **REFUSED**, naming the claim and the two open probes |
| Protected areas + candidate checksums | 0 | 0 | no change under `canonical`, `releases`, `assets` |
| `impl/target/test-tmp`, `/tmp/lcl-apps` | — | — | 0 entries written |

Gate verdict: **50 commands, each with its expected status.**

An earlier phase-a run reported 1,824 because it predated the end-to-end
retry-safety test added during the §3 review. Phase a was re-run on the
coherent final source — that is the 1,825 above, and it is the number both
toolchains report. The intermediate figure is recorded here rather than
quietly replaced.

The editor suite additionally ran through `editor_save.rs` against the real
workspace server over real HTTP — 25 cases, 0 failed, production `app.js`
sha256 recorded in the log. That is exact-production-function plus
controlled-DOM plus real-HTTP evidence; it is **not** a real-browser test, and
the harness says so.

### Populations, kept distinct

| Population | Value |
|---|---|
| Workspace tests | **1,825** on stable and on MSRV 1.75.0 |
| Source probes | 2,011 required, 2,011 satisfied |
| Semantic probes | 402 required, **400 satisfied**, 0 failed, 0 missing, 2 invalid |
| Original 79-sub-run register | **74 closed, 5 open** — unchanged by this pass |
| Required sub-runs still unestablished | 5: four `core.sort`, one `core.execute` |

These are different populations and nothing here equates them. No obligation
was added, removed, reclassified, or hidden as optional, descriptive, ignored,
known-failed or unpopulated. The mapping digest is unchanged.

**Not run, and why.** The candidate build and its installed smoke checks: they
belong after the owner commits, against that frozen revision, and building from
an uncommitted worktree would produce a candidate whose source nobody can name
(F5-N3's point). The every-file coverage ledger and the independent read-only
review: that contract requires a separate reviewer, and my own verification is
not it.

### Coverage, honestly

Source-reviewed and test-exercised this pass: the 22 files below and the
boundaries named in §3. Inventory-only: the rest of the 18 crates, exercised by
the existing suite but not individually reviewed here. Unavailable: a real
browser, a real desktop session, and an independent reviewer. 1,824 passing
tests and 400/402 probes are not a claim of correctness for the repository.

## 7. Exactly what changed, and where this stops

22 files, +3,953 / −147. No file outside `impl/crates/` was modified.

**Implementation (13).** `lcl-capabilities/src/{fs.rs,lib.rs,process/linux.rs}`;
`lcl-checker/src/declarations.rs`; `lcl-cli/src/main.rs`;
`lcl-completion/src/{evidence.rs,success.rs}`;
`lcl-conformance/src/operation_cases.rs`; `lcl-protocol/src/engine.rs`;
`lcl-runtime/src/{execute.rs,operations.rs}`;
`lcl-stdlib/src/{data.rs,fixtures/fs.rs,host.rs}`;
`lcl-workspace/assets/app.js`.

**Tests and test support (9).** `lcl-capabilities/tests/real_filesystem.rs`;
`lcl-cli/tests/projects_and_capabilities.rs`;
`lcl-completion/tests/success_and_failure.rs`;
`lcl-stdlib/tests/{boundary.rs,external_operations.rs}`;
`lcl-workspace/tests/{editor_save.cjs,editor_save.rs}`.

Internal interfaces that changed, all within this workspace:
`lcl_capabilities::Copied` (new, returned by `FileSystem::copy`);
`FsError` unchanged; `create_parents`/`after_created`/`final_drain` (private);
`GraphInvocation::aggregate`; `Provision::Source` now carries what it names;
`EvidenceRecord::observed`; `Expected::Identity` for
`boolean_or_reference_list`. No language meaning, error identifier, owning
stage, status rule, operation maximum, scope, profile meaning or output
contract was changed.

### `READY_FOR_OWNER_COMMIT`

Nothing is staged and nothing is committed. Proposed message:

```
LCL corrective 04: nine behavior repairs and a process-output defect

One graph refusal now raises one diagnostic whose phase agrees with the record
(AB-01). Distinct effect occurrences survive aggregation without double-counting
inherited ones (AB-02). A transfer that changed nothing reports no effect, and a
zero-byte transfer that did change something still does (AB-03). Directories
created before a transfer failure are carried through the error, and the source
is opened first so the commonest failure creates nothing (AB-04). Analyses are
correlated by request generation, not only by root revision (AB-05). A run that
was admitted and completed is reported rather than discarded by a second
admission check (AB-06).

Empty SUCCESS lists are accepted by the checker and quantify over zero members
(C-01). A FAILURE clause whose condition failed no longer lets a later clause be
selected (C-02). A declared CHECKSUM must have its registered sha256 form, and a
required EVIDENCE source must have been observed (C-03).

core.run's deadline path drains what the child already wrote before closing its
pipes, instead of discarding it (QA-01).

Five sub-runs remain open on owner decisions; m8_conformance_gate still refuses,
which is the correct blocked state. TESTING_READY is not met.
```

### `BLOCKED_ON_DECISION`

The five items in §5 — CAP-01, SPEC-01, SPEC-02, SPEC-03, and C-03's external
source — plus the two owner items this pass did not touch: Core 0.2
`independent_review`, and the manual desktop/menu/file-association acceptance.
No repository setting was changed, no CI was triggered, no hosted scan was
configured or run, and no manual interaction was marked complete (OPS-01).

### `TESTING_READY` is not met

The readiness gate refuses with exit 1; two required semantic obligations are
unestablished; bounded `core.modify` is refused rather than supported; the
QA-01 flake is not proven resolved; no candidate has been built from a frozen
revision; and the independent review has not happened. Each is a real boundary.

### The exact next action

Review §7 and commit, or say what to change first. After the commit the
candidate can be built from the frozen revision and put through its installed
smoke checks. The decisions in §5 are independent of that and can be taken at
any time.
