# Approved mapping correction — obligations r3 (2026-09-22)

Applies one explicit owner approval, and nothing beyond it.

> Owner approves mapping correction for the four impossible sub-runs listed in
> §3. Do not change Core 0.1 semantics or expand operation capabilities merely
> to construct those cases. Preserve all other required coverage, regenerate the
> mapping identity through the established process, then rerun conformance and
> the real readiness gate. Core 0.2 independent-review acceptance remains
> pending until the corrected evidence is available.

## 1. Which four, and how that was established

The `§3` reference does not resolve in the assignment file that was open: its
§3 is "USAGE AND CONTEXT RULES" and lists no sub-runs. Rather than guess, the
four were identified from the record, where exactly one group of four is
characterised as impossible:

`POST-FINAL-05_CORRECTIVE_02_RESULT.md` §8 — "The four words are the
profile-role selection vocabulary; a custom `kind.operation` selects no profile
and `core.sort` requires no role. **The condition cannot exist.**" — with the
stated resolution "**An approved mapping correction for these four**".

Those four, all pinned on the single row `semantic/operation_errors/core.sort`:

    precondition/key-operation-profile-missing
    precondition/key-operation-profile-ambiguous
    precondition/key-operation-profile-incomplete
    precondition/key-operation-profile-out-of-bounds

`core.execute precondition/profile-out-of-bounds` is a **separate fifth**
sub-run. It is not one of "the four", the approval does not mention it, and it
was not touched.

### Why no implementation could close them instead

Re-verified against the registry this session, not carried forward on
assertion. `core.sort` has **no entry** in
`operations_v0.1.0.json#/axis_contract/implementation_profile/required_roles_by_operation`,
and `role_resolution` states: "A core operation absent from
required_roles_by_operation requires no local core profile." A custom
`kind.operation` key likewise "selects no profile role and resolves under
axis_contract.custom_operation_resolution".

Those four words are the registry's closed vocabulary for a **profile-role
selection** fault. With no role to select, none can be missing, ambiguous,
incomplete or out of bounds: no input reaches the condition. Constructing one
would have required changing Core 0.1 semantics or giving `core.sort` a profile
role it does not have — which the approval explicitly forbids and which this
pass did not do.

An earlier revision closed these by reading the four words as properties of the
declared key contract. That equivalence was withdrawn in
`POST-FINAL-05_CORRECTIVE_02_RESULT.md` §7 as not canon-authorized, and the
pins were reopened rather than left closed on it. The withdrawal is recorded in
the r3 doc comment so the reasoning is not lost.

## 2. The correction, through the established process

The r1 → r2 precedent (commit `eb86f77`) was a renamed revision file, an updated
digest constant and an updated companion document. r3 follows it.

| | |
|---|---|
| New mapping | `impl/crates/lcl-conformance/src/obligations_v0.1.0_r3.json`, `"revision": "r3"` |
| Replaces | `obligations_v0.1.0_r2.json`, removed |
| Old identity | `9b32a28b79d9c3872cb3f810365a3275583ce5731a74c98a8d2013d07633a8ad` |
| **New identity** | **`296fc2bef03cb4a4ec45f00d5b43601f5c234a9d55a22c188ce477630013c3f3`** |
| Pinned in | `obligations.rs::MAPPING_DIGEST`, with a doc comment recording the approval and its canonical basis |
| Companion doc | `reports/implementation/LCL_CONFORMANCE_OBLIGATIONS.md`, updated to r3 |

r3 was produced by a minimal textual edit of r2 — four lines removed and the
revision field changed — not by re-serialising the JSON, so the diff is four
deletions in 980 rows and is reviewable as such.

### The delta is exactly the approval

| | r2 | r3 |
|---|---:|---:|
| Rows | 980 | **980** |
| Probes | 2,413 (2,011 source + 402 semantic) | **2,413** |
| Semantic rows pinning sub-runs | 319 | **319** |
| Sub-run pins | 3,725 | **3,721** (−4) |
| `package_identity` | `00d648b1…67ed` | unchanged |

Mechanically verified: the removed set is exactly those four `core.sort` pins,
the added set is empty, and the probe sets are identical. No probe, row or level
was added, removed or reclassified. Nothing was made descriptive, optional,
ignored, known-failed or unpopulated. Core 0.1 was not edited and no operation's
capabilities were expanded.

**Retained in the same row**, because they rest on the `key` parameter's own
stated requirements rather than on the withdrawn profile reading:
`precondition/incompatible-key-operation-signature` and
`precondition/invalid-key-operation-axes`.

### Two stale figures corrected

The companion document had drifted from the code and is corrected rather than
carried forward: it recorded the mapping digest as `27e3271f…` where r2's actual
pin was `9b32a28b…`, and 3,724 sub-runs where r2 pinned 3,725.

## 3. Conformance and the real readiness gate, rerun

| | Before (r2) | After (r3) |
|---|---|---|
| Source probes | 2,011 / 2,011 | **2,011 / 2,011** unchanged |
| Semantic probes | 400 satisfied, 2 invalid | **401 satisfied, 0 failed, 0 missing, 1 invalid** |
| Limiting probes | `core.sort`, `core.execute` | **`core.execute` only** |
| Claim | `source_conforming` | `source_conforming` |
| Original 79-sub-run register | 74 closed, 5 open | **78 closed, 1 open** |

**The real readiness gate still refuses**, and this is the honest result of the
approval rather than a shortfall in applying it:

```
REFUSED: the verdict does not establish testing readiness
  - the claim is "source_conforming", and readiness requires "semantics_conforming"
  - level semantics_conforming has 1 invalid obligation(s): ["semantic/operation_errors/core.execute"]
  - level semantics_conforming does not establish 1 required obligation(s): ["semantic/operation_errors/core.execute"]
  - level semantics_conforming retains 1 problem record(s)
```

The approval closes four of the five open sub-runs. The fifth —
`core.execute precondition/profile-out-of-bounds` — still blocks
`semantics_conforming`, and therefore still blocks readiness.

## 4. Gates on the corrected source

Through `c5-gate.sh`, whose self-test passes first.

| Command | Exit | Expected | Result |
|---|---:|---:|---|
| `cargo fmt --all -- --check` | 0 | 0 | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 0 | clean |
| `cargo test --workspace --all-targets --no-fail-fast` | 0 | 0 | 164 blocks, **1,825 passed, 0 failed, 1 ignored** |
| MSRV 1.75.0 check / tests | 0 / 0 | 0 / 0 | **1,825 passed, 0 failed, 1 ignored** |
| `real_process` 12 sequential + 3 × 6 concurrent | 0 | 0 | **30 of 30** |
| Core 0.1 / Core 0.2 checksums, identities | 0 | 0 | `00d648b1…` 176, `00daee8d…` 216 |
| Core 0.2 `validate_release.py --scope all` | 1 | **1** | BLOCKED on `independent_review` |
| validators, both EBNF, brand, protected areas | 0 | 0 | pass; `canonical/` unchanged |
| `m8_conformance_report` | 0 | 0 | 2,011/2,011; **401/402**, 1 invalid |
| **`m8_conformance_gate`** | **1** | **1** | **REFUSED** on `core.execute` alone |

Gate verdict: **50 commands, each with its expected status.** The
`lcl-conformance` suite passes 88/88 — those tests pin the probe and sub-run
sets deliberately, so the correction had to be consistent with them, and it is.

## 5. State, and what this changes about the candidate

HEAD moved during this work: the owner committed `9b4bfbc` ("LCL reapars 2"),
which is the candidate directory and `POST-BA910BD_CANDIDATE_05_RESULT.md` —
the two untracked items the previous pass left. It touches none of the files
edited here, so there was nothing to reconcile.

| | |
|---|---|
| HEAD | `9b4bfbc3a7ffb34c8f57d0d222aca67f476156f7` |
| Worktree | 4 entries, all this correction: `obligations.rs` (M), `obligations_v0.1.0_r2.json` (D), `obligations_v0.1.0_r3.json` (new), `LCL_CONFORMANCE_OBLIGATIONS.md` (M) |
| Git writes by me | **None.** |

**The existing candidate is now superseded for the current source.**
`lcl-0.2.0-linux-x86_64-8f2f0454e437` names build-source `ba910bd` and remains
valid *for that revision* — `9b4bfbc` only records it, and an evidence-recording
commit is kept separate from a build source. But this correction changes the
source, so once it is committed the candidate no longer matches and **a new
candidate must be built from the new revision** before the installed evidence
means anything about it. Nothing was relabelled or rebuilt here.

`Core 0.2 independent-review acceptance remains pending`, as the approval
states. No acceptance was recorded and no anchor or metadata was touched.

## 6. Status and the exact next action

**`READY_FOR_OWNER_COMMIT`** for the correction; **`BLOCKED_ON_DECISION`** for
what remains. `TESTING_READY` is not met.

Proposed commit message:

```
LCL conformance obligations r3: approved mapping correction

Removes exactly four sub-run pins from semantic/operation_errors/core.sort —
precondition/key-operation-profile-{missing,ambiguous,incomplete,out-of-bounds}
— under explicit owner approval. core.sort has no entry in
required_roles_by_operation, and an operation absent from it "requires no local
core profile", so a profile-role selection fault has no input that reaches it.

Mapping identity 9b32a28b…a8ad -> 296fc2be…c3f3. 980 rows and all 2,413 probes
unchanged; 3,725 -> 3,721 sub-run pins; no probe, row or level added, removed or
reclassified. Core 0.1 untouched; no operation capability expanded. The key
signature and axes checks stay pinned on their own requirements.

Semantic probes 400 -> 401 satisfied, invalid 2 -> 1. The readiness gate still
refuses on core.execute precondition/profile-out-of-bounds, which this approval
does not cover.
```

After that commit, in order:

1. **Rebuild the candidate** from the new revision and rerun the installed smoke
   chain; the current candidate is superseded.
2. `core.execute precondition/profile-out-of-bounds` — the one remaining
   decision, unchanged from `POST-AB0DA8B_CORRECTIVE_04_RESULT.md` §5: an
   approved mapping correction, **or** canon narrowing the row's maxima, **or**
   an approved re-reading toward the invocation condition with the catalog
   defect addressed first.
3. The Core 0.2 `independent_review` entry, the independent reviewer, and the
   manual desktop acceptance — all still the owner's.
