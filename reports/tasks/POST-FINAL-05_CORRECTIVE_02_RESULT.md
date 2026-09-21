# POST-FINAL-05 corrective task — follow-up 02 (2026-09-21)

Continues `POST-FINAL-05_CORRECTIVE_01_RESULT.md`, which stays intact as the
record of the first checkpoint. This follow-up answers a review of that pass,
and it **withdraws several of its closure statements**. Where a review finding
was disproved, the evidence is recorded instead of a change.

## 1. Identity and state

| | |
|---|---|
| Entry HEAD of the first pass | `878187d2cd95d644462bcb30c8035a59a1624392` |
| **HEAD now** | `feffac5941b9d14409d77be3ee1c65258ea8acba` ("LCL final pretest task 5 part1") |
| What changed | The owner committed **while this session was running**, capturing a partial snapshot: 36 files, 4,157 insertions — the scope and fragment repairs, the first gate, the first packaging guard, the two stray-file deletions, and report 01. |
| Worktree now | 15 modified files, 0 untracked, 0 deleted — every one a correction this follow-up made **after** that commit. |
| Git writes by me | **None.** No add, commit, amend, merge, rebase, reset, checkout, stash, clean, tag, push, pull or fetch. |
| Core 0.1 identity | `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, 176 files — unchanged |
| Core 0.2 identity | `00daee8de1919c4945ef04ff65edb22164bd8046a493be08a87d5fa3b4c3e604`, 216 files — unchanged |
| Mapping digest | `9b32a28b79d9c3872cb3f810365a3275583ce5731a74c98a8d2013d07633a8ad` — unchanged; no obligation added, removed or reclassified |
| F10 | decision A and its original-byte behaviour untouched |

Because the commit landed mid-session, `feffac5` contains the *pre-review* form
of three things this follow-up corrects: the blanket archive rule in
`packaging/build_release.sh`, the unauthorized G7 equivalences in
`clauses.rs`, and the first gate that trusted the verdict's own counts. The
corrections are in the working tree for the next commit.

### A denied action, recorded as denied

While reverting an experimental edit in the first pass I ran `git checkout --
impl/crates/lcl-stdlib/src/fragment.rs`. **The action was denied** by the
environment's auto-mode classifier as irreversible local destruction. It was not
performed, and it was not a Git write. I reverted the file instead by editing my
own additions back out and confirmed with `git diff --stat` that the file
matched its committed content exactly. Report 01 did not mention the attempt;
this records it.

## 2. Closure statements withdrawn

| Statement in report 01 | Status now |
|---|---|
| "Repairs complete and verified; 4 of the original 79 sub-runs remain" | **Withdrawn.** 6 remain, and one of the four then-claimed closures was not established. |
| "G7 … **Closed.** The four profile-fault words apply to the declared key contract" | **Withdrawn**, see §7. The equivalence is not canon-authorized; all four sub-runs are reopened. |
| "A pattern selector therefore never decides a refusal here: an unresolved `INCLUDE` pattern admits, and an unresolved `EXCLUDE` pattern excludes nothing." | **Withdrawn.** That was a fail-open: an action gained permission because its restriction was written as a pattern. Repaired in §3. |
| `core.calculate path/unresolved-expression-reference` closed via an external `bindings` reference | **Replaced.** That exercised ordinary reference resolution outside the fragment. The original defect — names *inside* the STRING — is now implemented and is what the sub-run exercises (§4). |
| "The three earlier candidates and the published `releases/` archives still contain them" | **Corrected.** Checked per artifact: only `lcl-0.2.0-…-68529c1420ba` contains them (§6). |
| "binary archive outside `releases/`" refusal | **Removed.** It was the blanket exclusion the task forbade (§6). |
| The 79-sub-run group table | **Reconciled.** It summed overlapping groups to 80; the baseline is 79 unique `(probe, sub-run)` pairs (§8). |
| TASK-scope reading justified by "widening the rule would reject a canonical valid example" | **Re-grounded.** `00_RELEASE/02` ranks examples 8th and says "Examples are never normative exceptions"; the reading now rests on ranks 1 and 5, with the example as corroboration and the open question stated (§3). |
| `result.test engine/unknown-never-binds` "impossible" | **Disproved by me.** It is constructible; the obligation is closed with real evidence (§8). |
| `core.test path/incompatible-equality-operands` "impossible" | **Disproved by me.** It exposed a real defect, now repaired (§8). |

## 3. Scope enforcement no longer fails open

**Reproduction.** Five new tests in `lcl-semantics/tests/data_resolution.rs`
failed against the committed code: an `EXCLUDE` GLOB that matches the target was
ignored; an `INCLUDE` GLOB that matches nothing admitted everything; the same for
`REGEX`; an exact `INCLUDE` survived a matching `EXCLUDE` pattern; and a GLOB
"selected" an absolute path.

**Canonical basis.** `05_SEMANTICS/02`: "GLOB/REGEX select a finite set resolved
before affected execution. A GLOB is evaluated only against workspace-relative
paths under its closed profile; it cannot select an absolute path or escape the
WORKSPACE. EXCLUDE wins within the same SCOPE."
`types_v0.1.0.json#/pattern_profiles`: GLOB is `workspace_relative` with
`match_semantics: full_workspace_relative_path`; REGEX is `full_string`; and a
GLOB "input that cannot supply this form ... no root is inferred."

**Repair.** Deciding whether *one known entity* is in a pattern's set is not
enumerating that set, and needs no filesystem.

- `lcl-checker/src/pattern.rs:194` exposes `glob_selects` / `regex_selects` over
  the **existing** compiled matchers — the ones the checker uses for a declared
  `PATTERN` and the runtime for `MATCHES`. No second pattern implementation.
- `lcl-semantics/src/authority.rs` adds `Selector::Relative { workspace,
  relative }` for the WORKSPACE path form, so a GLOB has the subject its profile
  requires and identity still compares by "the resolved workspace declaration
  identity and exact decoded relative STRING".
- `lcl-semantics/src/scope.rs:204` `admits` returns `Admitted`, `Refused` or
  `Undecided`; `:207` asks EXCLUDE first, then INCLUDE; `:253` derives the
  normalized relative segments a GLOB consumes; `:314` the full string a REGEX
  matches.
- `lcl-semantics/src/order.rs:741` refuses on `Refused` **and** on `Undecided`,
  and no longer skips an action whose TARGET names no single entity. A
  restriction this layer cannot decide is never treated as absent.

**Undecidable cases, and what happens.** A pattern that exhausts its declared
finite resource limit, and a TARGET that identifies no one entity before
effects, both refuse with `error.scope.violation` carrying the reason. This is
fail-closed rather than a new rule: the layer cannot establish that the action is
inside its applicable scope, and `error.scope.violation` is "pre_effect only:
effective scope resolves at processing step 6 before the first authorized
effect". `error.pattern.resource_limit` is not emitted here because the preflight
layer does not mirror it; the refusal names the limit in its detail.

**Tests** (all green): matching and non-matching GLOB and REGEX, EXCLUDE
overriding INCLUDE, exact-plus-pattern interaction, normalized relative segments
(`./src/main.py` matches `GLOB("src/main.py")`), a GLOB selecting no absolute
path, the existing operation-filter test, and the existing controls that no
unauthorized effect or host request occurs (no host is reached at preflight at
all).

**The TASK-scope question, re-grounded.** `00_RELEASE/02_NORMATIVE_AUTHORITY_ORDER`
puts `10_REGISTRIES` first, `05_SEMANTICS` fifth and `08_EXAMPLES` eighth, and
says "Examples are never normative exceptions". So the reading may not rest on
example 04.

- Rank 1: `field_signatures_v0.1.0.json` declares `SCOPE` as an optional
  `reference(SCOPE)` on TASK, ACTION, ALLOW and FORBID, each with
  `"default": null`, and states no propagation from an enclosing block.
- Rank 1: `statuses_and_errors` says "outside **applicable** SCOPE" without
  defining applicable.
- Rank 5: `05_SEMANTICS/02` says "Nested scopes intersect unless a
  higher-authority rule explicitly replaces a referenced scope" — how two scopes
  combine when both are in force, not which ones are.

Neither rank answers it, so this is a **genuine normative gap**, recorded rather
than decided. This build applies the scope an ACTION itself names, and
`data_resolution.rs::an_enclosing_task_scope_is_not_applied_to_a_member_action_that_declares_none`
pins that reading together with its discriminator: canonical valid example 04
declares TASK `SCOPE: REF(scope.source)` (one source file) and reaches
`action.test`, whose TARGET is `PATH("/usr/bin/python3")`. Adopting the wider
reading would make that example invalid, and `canonical/LCL_Core_0.1.0` is
immutable — so the wider reading cannot be adopted without a Core 0.1 change this
pack forbids. **Owner decision requested**; G1 stays closed on the applicable
scope, and nothing silently admits.

## 4. Names inside expression fragments now resolve

**Reproduction.** `expression: "ghost + 1"` produced `error.operator.operand`;
`expression: "REF(data.absent) + 1"` produced nothing at all. Both are now red
tests in `lcl-resolver/tests/references.rs` that fail without the repair.

**Owning stage, established from canon rather than from the mirror list.**
`error.reference.unresolved` is registered `"stage": "resolution"`, and
`expression_fragment_contract/evaluation` says "Static checks cover the complete
fragment; dynamic errors arise only from evaluated subexpressions". The resolver
is that stage's layer, already mirrors the identifier, and already holds the
grammar and lexicon it needs. **No mirror extension was required**, so the
mapping identity is unchanged — the argument report 01 used to leave this
unimplemented does not arise.

**Repair.** `lcl-resolver/src/references.rs:159` `fragment` runs with every
block: it reads `OPERATION`, and for the three fragment rows the contract names
— `core.calculate.expression`, `core.select.predicate`, `core.filter.predicate`
— takes the fragment from a STRING literal or through a `kind.constant`
reference, parses it with the same `expression_fragment` entry the standard
library uses, and resolves every name it reads:

- `REF(x)` against the same declaration index every other reference uses;
- a bare name against the declared `bindings` keys, the reserved `target`, and
  `item` for a predicate;
- a **qualified** name is contextual identifier data and is never treated as a
  binding, and a bare name a `DEFINE kind.enum` declares as an `ITEM` is
  contextual enum data.

The diagnostic is attributed to the written STRING in the document, because a
span inside the fragment would name bytes the document does not contain.

**Tests**: unknown bare name; unresolved `REF` inside the fragment; the controls
(`REF` to a real declaration, a declared binding, `target`, `item`, a qualified
identifier); a `kind.constant`-borne fragment, good and bad; a name in an
**unevaluated** branch (`FALSE AND (ghost > 1)`) — whole-fragment static
resolution; and identity, stage (`resolution`), source attribution and span.

**Conformance.** `semantic/operation_errors/core.calculate`'s pinned
`path/unresolved-expression-reference` now exercises a reference **inside** the
fragment. The external-`bindings` case remains covered by the resolver tests. An
extra run under an unpinned label was written and removed: an unpinned sub-run
makes its probe invalid, which is itself evidence that the inventory is closed.

## 5. The gate cannot lose a failure

The transcript's "REFUSED … exit: 0" was a pipeline's status, not the gate's.

`/mnt/F/.lcl-pretest/c1/c2-gate.sh` replaces the first wrapper. Each command's
output goes to its own log and its status is captured **from the command**,
never from a pipe, tee or later echo; every status is appended to
`c2-results.tsv` with the status that command is expected to have; and the
script's own exit is nonzero when any differs. Collecting more diagnostics
cannot overwrite an earlier failure, because the record is append-only and the
verdict is computed from it.

`c2-gate.sh selftest` proves it against controlled failures:

| Case | Recorded | Gate |
|---|---|---|
| `exit 3`, no output | 3 | FAILED |
| prints `test result: ok. 9 passed; 0 failed`, exits 1 | 1 | FAILED |
| honest success | 0 | passed |

Two expected-nonzero commands are declared as such rather than hidden:
`validate_release.py` for Core 0.2 (exit 1, BLOCKED on `independent_review`) and
the readiness gate (exit 1 while obligations remain open). A 0 from either would
fail the gate.

I also stopped reading my own invocations through `tail`: every gate phase below
was run as `… > log 2>&1; echo "EXIT: $?"`.

## 6. The packaging repair is narrow again

**Removed**: the refusal of every `.zip`, `.tar.gz`, `.tgz`, `.7z` and `.rar`
outside `releases/`. It judged files by extension and would have classified a
legitimate compressed fixture as invalid — the solution the task excluded.

**Kept**: `packaging/build_release.sh:322` refuses exactly `.directory`,
`.DS_Store` and `Thumbs.db`, which carry machine-local settings and are not
source in any tree.

**Prevention**: `.gitignore` carries those three names plus `/archive-*/`,
anchored to the repository root so a directory named `archive-*` anywhere else is
unaffected, and nothing is judged by extension. Verified: no tracked file is
ignored by it, and a new `reports/.directory` is.

**Controls** in `lcl-hardening/tests/release_build.rs`, through the real script:

- desktop metadata refused in four positions, before the source is recorded and
  before anything compiles;
- **a positive control asserting the intended outcome**: a `.tar.gz` fixture at
  `impl/fixture.tar.gz` builds successfully and is inventoried as
  `file`, size `14`, with a 64-character digest — not merely the absence of a
  message.

**Historical artifacts, checked one by one** rather than claimed:

| Candidate | `.directory` | GitKraken archive |
|---|---|---|
| `lcl-0.1.0-…-b4506c8aa3d4` | 0 | 0 |
| `lcl-0.1.0-…-bf78a0890e5f` | 0 (its `SOURCE_MANIFEST.sha256`) | 0 |
| `lcl-0.2.0-…-a0006c38fb79` | 0 | 0 |
| `lcl-0.2.0-…-68529c1420ba` | **1** | **1** |

Only the newest candidate is contaminated. Report 01's claim that all of them
were is withdrawn. Nothing was repackaged or rewritten.

## 7. G7 reopened

**The question.** Does canon authorize equating "a missing, ambiguous,
incomplete, or out-of-bounds immutable profile" with key-contract properties?

**Answer: no.** `axis_contract.implementation_profile.selection` defines those
words over profile-role selection — "the exact operation identifier, profile
role, target or address class, arguments, implementation identifier, and
implementation version select exactly one immutable profile for each role" — and
its `required_properties` are the ten a profile states; a PARAMETER count and a
RESULT type are not among them. `required_roles_by_operation` gives `core.sort`
`null`, and "A core operation absent from the map requires no local core
profile"; `custom_operation_resolution` says a custom `kind.operation` "selects
no implementation profile". Both clauses are in the same registry, so the
authority order — which resolves *between* artifacts — does not decide it.

**Consequence.** A key operation has no profile that could be missing,
ambiguous, incomplete or out of bounds. Receiving the same
`error.operation.precondition` is not evidence that the required condition was
exercised. The four sub-runs are **reopened**, and semantic coverage fell from
398 to 394 before the other repairs brought it back to 399.

**Kept**, because they are canon-stated requirements in their own right: the key
contract checks in `lcl-stdlib/src/pure.rs:621` and their tests. `core.sort`'s
key needs "exactly one RESULT of a concrete registered ordered type"; violating
that is `error.operation.precondition` by the row's own trigger.

**A defect this review exposed in my own repair.** The first pass applied the
*sort* rule to every `key` parameter, including `core.group`'s — whose key needs
"exactly one **material** RESULT usable as a grouping key", not an ordered one.
`pure.rs:621` now branches per row, and
`pure_operations.rs::a_group_key_admits_a_material_result_a_sort_key_would_not`
pins the difference: a BOOLEAN key groups and does not sort.

## 8. The ledger, and the reassessed obligations

Reconciled mechanically by unique `(parent probe, sub-run label)` from the entry
report, one primary group each, secondary dependencies recorded in the group they
depend on. Written to `/mnt/F/.lcl-pretest/c1/ledger.tsv`.

| Primary group | Sub-runs | Open |
|---|---:|---:|
| G1 scope enforcement | 31 | 0 |
| G2 graph execution | 22 | 0 |
| G4 host-backed observation | 6 | 0 |
| G5 store failure behaviour | 6 | 0 |
| G6 determinism mismatch | 3 | 0 |
| G7 key-operation profiles | 4 | **4** |
| Group A ambiguities | 5 | **1** |
| FINAL-02 additions | 2 | **1** |
| **Total** | **79** | **6** |

Report 01's table added overlapping group counts to 80 and stated Group A
inconsistently; this replaces it. 73 of 79 are closed.

The 6 open sub-runs and the report's "3 invalid" are the same fact counted at
two granularities: a probe is invalid if any of its pinned sub-runs is missing,
and the six fall in three probes. The live verdict names them:

```
CLAIM: source_conforming
  limited by: ... probe semantic/operation_errors/core.execute invalid
              [missing sub-runs: precondition/profile-out-of-bounds]
  limited by: ... probe semantic/operation_errors/core.group invalid
              [missing sub-runs: error/operator.operand]
  limited by: ... probe semantic/operation_errors/core.sort invalid
              [missing sub-runs: precondition/key-operation-profile-ambiguous,
               precondition/key-operation-profile-incomplete,
               precondition/key-operation-profile-missing,
               precondition/key-operation-profile-out-of-bounds]
```

### Two obligations reassessed and closed with evidence

**`result.test engine/unknown-never-binds`.** Report 01 argued that `passed`
could never be UNKNOWN because a declared UNKNOWN is refused at static checking.
That reasoned from an invalid literal fixture. A **canonically permitted
invocation input** does it: `03_TYPES_AND_VALUES/09` registers "TRUE AND UNKNOWN
= UNKNOWN", so `assertion: TRUE AND UNKNOWN` resolves UNKNOWN at evaluation. The
engine already behaved correctly — producer `status.succeeded`, `passed`
UNKNOWN, OUTPUT `unbound` — and the obligation is now exercised in
`result_cases/engine.rs`.

**`core.test path/incompatible-equality-operands`.** Probing rather than
assuming exposed a real defect: `actual: REF(task.subject)` reported
`error.host.constraint` — "result.test.actual requires meta.material_value,
found MISSING" — converting a language-level operand defect into a host
limitation no host took part in, which `05_SEMANTICS/09` and the row's closed
errors list both forbid. `06_STANDARD_LIBRARY/03` states the answer: "operands
outside its equality_compatible domain use error.operator.operand", and
`equality_compatible` is "Any two material values or permitted singleton
sentinels" — a reference naming an execution unit is neither.
`lcl-stdlib/src/control.rs:672` now refuses it under the row's own identifier,
with a test asserting no host was involved.

### The six that remain — owner decisions

| Sub-run | Established finding | Decision needed |
|---|---|---|
| `core.sort precondition/key-operation-profile-{missing,ambiguous,incomplete,out-of-bounds}` (4) | The four words are the profile-role selection vocabulary; a custom `kind.operation` selects no profile and `core.sort` requires no role. The condition cannot exist. | An approved mapping correction for these four, or canon naming what a key operation's "profile" is. |
| `core.group error/operator.operand` (1) | The canonical requirement says to "union every applicable error of a referenced key operation". The embedder surface `PureOperation` returns `Result<Value, String>` — a failure with **no registered identifier** — so every key-operation failure is reported as `error.operation.precondition` and the union cannot carry `error.operator.operand`. Not a missing trigger: a missing failure channel. | Approve extending the pure-operation failure channel to carry a registered identifier the custom operation's contract admits (mirroring `CapabilityOutcome::Refused`), or an approved mapping correction. |
| `core.execute precondition/profile-out-of-bounds` (1) | The requirement names "an out-of-bounds required **execution profile role**". A profile is out of bounds when its axes exceed the row's maxima — and `core.execute`'s maxima are the **entire** closed vocabulary of both axes (proven mechanically: `{host, network, model, human}` and all seven effect classes). The other out-of-bounds case, a nondeterministic profile under a deterministic base row, cannot apply either: `core.execute` is `derived`. A different condition — an *invocation* exceeding its selected profile's narrower bounds — is not what this obligation names, and FINAL-02 recorded that enforcing it refuses 23 legitimate pinned runs because the shipped catalog declares narrower axes than ordinary invocations resolve. | An approved mapping correction for this one sub-run, or canon narrowing `core.execute`'s maxima, or an approved re-reading toward the invocation-vs-profile condition with the catalog defect fixed first. |

None was removed, reclassified or hidden: all six are `invalid` in the live
report and named by the readiness gate.

## 9. Gates

Run through `c2-gate.sh`, whose status handling is proven in §5. Isolated
`TMPDIR=/tmp/lcl-corrective`, out-of-tree `CARGO_TARGET_DIR`. Logs
`/mnt/F/.lcl-pretest/logs/C2-*`; recorded statuses in
`/mnt/F/.lcl-pretest/c1/c2-results.tsv`.

| Command | Exit | Expected | Result |
|---|---:|---:|---|
| `cargo fmt --all -- --check` | 0 | 0 | clean |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | 0 | clean (three findings of mine repaired) |
| `cargo test --offline --locked --workspace --all-targets --no-fail-fast` | 0 | 0 | 164 blocks, **1,736 passed, 0 failed, 1 ignored** |
| MSRV 1.75.0 `cargo check --workspace --all-targets` | 0 | 0 | clean |
| MSRV 1.75.0 `cargo test --workspace --all-targets --no-fail-fast` | 0 | 0 | 164 blocks, **1,736 passed, 0 failed, 1 ignored** |
| `real_process` stress: 12 sequential + 3 × 6 concurrent | 0 | 0 | **30 of 30**, 0 failures in log |
| Core 0.1 `sha256sum -c --strict --quiet` / `validate_release.py --scope all` | 0 / 0 | 0 / 0 | verifies; `release_ready: true` |
| Core 0.2 `sha256sum -c --strict --quiet` | 0 | 0 | verifies |
| Core 0.2 `validate_release.py --scope all` | 1 | **1** | `release_ready: false` — BLOCKED on `independent_review`, the owner's |
| `validate_language_contracts` / `validate_localization` / `validate_source_fixtures` | 0 | 0 | pass |
| `validate_ebnf` 0.1.0 / 0.2.0 | 0 / 0 | 0 / 0 | pass |
| Core 0.1 / Core 0.2 identity | 0 / 0 | 0 / 0 | `00d648b1…` 176 / `00daee8d…` 216 |
| `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 0 | 0 | verifies |
| `m8_conformance_report` | 0 | 0 | source 2,011/2,011; semantic **399 satisfied, 0 failed, 0 missing, 3 invalid** |
| **`m8_conformance_gate`** | **1** | **1** | **REFUSED**, naming the claim and the open obligations |
| protected areas + candidate checksums | 0 | 0 | no change under `canonical`, `releases`, `assets` |
| `impl/target/test-tmp`, `/tmp/lcl-apps` | — | — | 0 entries written |

`c2-gate.sh` phases a and c each exited 0; phase b exited 0; the self-test
exited 0 after proving a failing command fails the gate.

**Not run, and why.** The candidate build and its installed smoke: they belong
after the owner commits the completed source state, against that frozen
revision. The every-file ledger and the independent read-only audit: FINAL-05's
contract requires another reviewer, and my own verification is not that review.

## 10. The readiness gate validates the inventory

`acceptance.rs:58` `accept(verdict, inventory, package_identity)` now takes the
**independently loaded** `Obligations` — the reviewed mapping under its pinned
digest, refused unless the approved package verifies — and checks the verdict
against it rather than against its own counts:

- both required levels, each exactly once, under recognized names, with a
  correctly typed `required` field that must equal the inventory's count;
- exact membership per level: nothing the inventory requires may be absent, and
  nothing it does not require may be counted — so substituted, omitted, moved or
  invented ids are all refused;
- a non-string array member is a refusal, never something to skip;
- no obligation in two states; no failed, missing or invalid obligation; no
  retained problem record;
- `executed = passed + failed`, no failed records, no named failed cases;
- the mapping digest and package identity.

`tests/acceptance.rs` drives it over **inventory-grounded** fixtures: a complete
verdict is accepted; and refusals are proven for a `source_conforming` claim from
an exit-zero command, arbitrary ids under a copied valid digest, required ids
replaced by the same number of others, an obligation listed under the wrong
level, a missing / duplicate / unnamed / unknown level, each unsatisfied state, a
non-string member, a duplicate id, contradictory counters, a retained problem, a
named failed case, digest and identity mismatches, and absent / malformed /
unrecognized documents. The synthetic fixtures are labelled as gate structure,
never as executed language-conformance evidence. The real production verdict is
still refused.

## 11. Files changed since `feffac5`

All 15 are modifications; nothing new, nothing deleted.

| File | Why |
|---|---|
| `impl/crates/lcl-semantics/tests/data_resolution.rs` | the scope fail-open tests and the applicable-scope discriminator |
| `impl/crates/lcl-resolver/src/references.rs` | one clippy finding in the fragment hook |
| `impl/crates/lcl-resolver/tests/references.rs` | whole-fragment, constant-borne and attribution tests |
| `impl/crates/lcl-stdlib/src/control.rs` | `non_value_operand` — a comparison operand that is no value |
| `impl/crates/lcl-stdlib/tests/control_operations.rs` | its red-first regression and control |
| `impl/crates/lcl-stdlib/src/pure.rs` | per-row key RESULT rule |
| `impl/crates/lcl-stdlib/tests/pure_operations.rs` | the group-vs-sort discriminator |
| `impl/crates/lcl-conformance/src/acceptance.rs` | inventory-grounded gate |
| `impl/crates/lcl-conformance/examples/m8_conformance_gate.rs` | loads the inventory independently |
| `impl/crates/lcl-conformance/tests/acceptance.rs` | inventory-grounded negative suite |
| `impl/crates/lcl-conformance/src/operation_cases/clauses.rs` | G7 sub-runs removed; calculate and core.test sub-runs corrected |
| `impl/crates/lcl-conformance/src/result_cases/engine.rs` | `result.test engine/unknown-never-binds` |
| `impl/crates/lcl-hardening/tests/release_build.rs` | narrow refusal cases and the real positive control |
| `packaging/build_release.sh` | blanket archive rule removed |
| `.gitignore` | `/archive-*/`, anchored |

Plus this report, `reports/tasks/POST-FINAL-05_CORRECTIVE_02_RESULT.md`.

## 12. Limitations, and what is not claimed

1. **Six obligations remain open** (§8). The claim stays `source_conforming` and
   `TESTING_READY` cannot be issued.
2. **A normative gap is recorded, not decided**: whether an enclosing TASK,
   PHASE or SEQUENCE scope intersects into its members (§3).
3. **GLOB/REGEX scope selectors decide membership for the target in hand**; they
   do not enumerate a workspace, and a pattern that exhausts its resource limit
   refuses rather than admits.
4. **Behaviour changes visible to existing documents** remain as recorded in
   report 01, plus two from this pass: an action whose applicable pattern scope
   excludes its target is now refused, and a `core.test` operand naming an
   execution unit is now an operand defect instead of a host constraint.
5. **No candidate was built**, and the one that exists (`…-68529c1420ba`) is
   superseded and contaminated.
6. **No independent audit** has been run over this tree, and this report is not
   one.

## Proposed commit message

```
LCL post-FINAL-05 corrective 02: close the review findings

Scope enforcement no longer fails open. A GLOB or REGEX selector now decides
membership for the target in hand, through lcl-checker's existing compiled
matchers, so an EXCLUDE pattern that matches refuses and an INCLUDE pattern
that does not match no longer admits. A restriction this layer cannot decide
- an exhausted pattern budget, or a TARGET naming no one entity - refuses
instead of being treated as absent. The applicable-scope reading is
re-grounded on the authority order, and the open normative question about an
enclosing TASK scope is recorded with its discriminator.

Names inside expression fragments resolve where the identifier's registered
resolution stage lives. The resolver parses core.calculate, core.select and
core.filter fragments with the same entry the standard library uses and
resolves every REF and bare name against the declared bindings, the reserved
target and item, and contextual identifier data, over the whole fragment
including unevaluated branches. No mirror was extended and the mapping digest
is unchanged.

The readiness gate validates against the independently loaded verified
inventory rather than the verdict's own counts: exact membership per level,
typed arrays, consistent states and record accounting. Its negative suite
covers arbitrary ids under a copied digest, substituted ids, misplaced levels
and malformed documents. The gate wrapper records each command's own exit
status and fails the gate on any mismatch, proven against controlled failures.

The packaging repair is narrow again: the blanket rejection of every archive
outside releases/ is removed, desktop metadata alone is refused, and a
positive control proves a compressed source fixture is still inventoried.
Only lcl-0.2.0-...-68529c1420ba was checked to contain the two stray files.

G7's four key-operation-profile sub-runs are reopened: canon defines those
four words over profile-role selection, and a custom kind.operation selects no
profile. Two obligations previously called impossible are closed with
evidence, one of them after repairing a real defect - a core.test operand that
names an execution unit was reported as error.host.constraint, converting a
language-level operand defect into a host limitation.

The 79-sub-run baseline reconciles by unique (probe, sub-run): 73 closed, 6
open, each an owner decision. fmt, clippy, 1,736 workspace tests on 1.98.1 and
on MSRV 1.75.0, 30 real_process runs, both packages' checksums, the validators
and the identities are green; the readiness gate refuses, as it must.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

## No Git writes

HEAD is `feffac5941b9d14409d77be3ee1c65258ea8acba`, exactly as the owner left
it. Every change above is unstaged in the working tree. One `git checkout --`
was attempted in the first pass and **denied**; it was not performed and was not
worked around.

## Next action

The sequence is unchanged: verified source repairs → the owner's decisions on
the six obligations → the owner's commit of the completed source state → freeze
that clean revision → build the candidate from it → installed-candidate
verification → the required independent read-only audit.
