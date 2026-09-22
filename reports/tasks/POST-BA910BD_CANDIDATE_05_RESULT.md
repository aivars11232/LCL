# Candidate build and installed verification — result (2026-09-22)

The acceptance step that every previous corrective report stopped at. It had
one precondition: an owner commit of the completed source. That commit is
`ba910bd`, so this pass froze that revision, built a new candidate from it and
put the installed candidate through its smoke chain.

No repair was made here. This is read-only verification plus one build, which
is what the contract separates from repair work.

## 1. Identity

| | |
|---|---|
| Project | `/mnt/F/LCL`, branch `main` |
| **Frozen build-source revision** | `ba910bdf4281af1540c627802e22e77714a1c939` ("LCL almost there") |
| Worktree at freeze | **clean** — 0 tracked modifications, 0 untracked |
| Worktree now | 0 tracked modifications; the one new untracked entry is the candidate directory this pass produced |
| Git writes by me | **None.** No add, commit, amend, merge, rebase, reset, checkout, stash, clean, tag, push, pull or fetch. Read-only inspection only. |
| Core 0.1 identity | `00d648b1…67ed`, 176 files — in the repository **and** inside the payload |
| Core 0.2 identity | `00daee8d…e604`, 216 files — in the repository **and** inside the payload |
| Mapping digest | `9b32a28b…a8ad` — unchanged |

`ba910bd` was verified to be the previous session's corrective-04 work
committed verbatim (22 files plus its report), by `git show --stat`.

## 2. The source was verified before it was frozen

The full gate ran against the clean committed source, through
`/mnt/F/.lcl-pretest/c1/c5-gate.sh`, whose self-test passes first: a command
exiting 3, and one printing a success line while exiting 1, are both recorded
and both fail the gate.

| Command | Exit | Expected | Result |
|---|---:|---:|---|
| `cargo fmt --all -- --check` | 0 | 0 | clean |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | 0 | clean |
| `cargo test --offline --locked --workspace --all-targets --no-fail-fast` | 0 | 0 | 164 blocks, **1,825 passed, 0 failed, 1 ignored** |
| MSRV 1.75.0 `cargo check --workspace --all-targets` | 0 | 0 | clean |
| MSRV 1.75.0 `cargo test --workspace --all-targets --no-fail-fast` | 0 | 0 | 164 blocks, **1,825 passed, 0 failed, 1 ignored** |
| `real_process`: 12 sequential + 3 × 6 concurrent | 0 | 0 | **30 of 30** |
| Core 0.1 checksums / `validate_release.py --scope all` | 0 / 0 | 0 / 0 | verifies |
| Core 0.2 checksums | 0 | 0 | verifies |
| Core 0.2 `validate_release.py --scope all` | 1 | **1** | BLOCKED on `independent_review`, the owner's |
| three 0.2 validators, both EBNF checks | 0 | 0 | pass |
| brand assets, protected areas | 0 | 0 | unchanged |
| `m8_conformance_report` | 0 | 0 | source **2,011/2,011**; semantic **400/402**, 0 failed, 0 missing, 2 invalid |
| **`m8_conformance_gate`** | **1** | **1** | **REFUSED** — unchanged and correct |

Gate verdict: **50 commands, each with its expected status** (`C6-gate-*.log`).

## 3. The candidate

Built with `LCL_RELEASE_VERSION=0.2.0 packaging/build_release.sh`, the existing
snapshot-and-inventory process. The build compiled from a snapshot unpacked
under `/tmp/lcl-corrective/tmp.OyXucw9lb7/snapshot`, not from the live checkout.

```
releases/candidates/lcl-0.2.0-linux-x86_64-8f2f0454e437/
```

| | |
|---|---|
| **Build-source commit** | `ba910bdf4281af1540c627802e22e77714a1c939` |
| **Uncommitted entries when the source was recorded** | **0** |
| **Complete source-inventory digest (source id)** | `8f2f0454e43784e6e71a428bd930a947b78712105551ba49105b74fe8e98c7be`, **1,016 files** |
| Source archive sha256 | `55b4cd6e6ba45d740cd88f9e43f8523c5831a7ed1afb2e5c18c7428980eb026f` |
| **Artifact digest** | `3fefa140a0053484706a4d4bdac20c20daef99a8e52299907ad5493e03f781c0` |
| Toolchain | cargo/rustc 1.98.1; declared minimum 1.75; `--release --offline --locked` |
| Lockfile sha256 | `c51be14776070134a45857c0606f2cff1ade401a3376b212bf1fcb081f153a60` |

These four identities are kept separate, as the contract requires: the
build-source commit, the source-inventory digest, the artifact digest, and any
later evidence-recording commit — which this report will become and which is
**not** the build source.

### What was checked about it

* `sha256sum -c` on the payload and on the source archive: both verify.
* The source id equals the SHA-256 of `SOURCE_INVENTORY.tsv`, recomputed here.
* **Inventory against the clean checkout: 1,016 / 1,016 paths and digests
  identical.** Nothing was built that the inventory does not name, and nothing
  named differs from the committed bytes.
* Known stray inclusions absent from **both** archives: the `gk_3.1.75`
  GitKraken zip (F5-N1) and `.directory` (F5-N2) — 0 matches in each.
* Both Core packages are bundled, and inside the unpacked payload they carry
  the exact recorded identities: `00d648b1…` 176 files and `00daee8d…` 216
  files.
* Every pre-existing candidate and the published `releases/` release still
  verify their own checksums. Nothing historical was relabelled, rebuilt or
  overwritten; the script writes only into its own new directory.

## 4. Installed verification

Run against the **unpacked candidate installed into a disposable HOME/XDG**
under `env -i` — never a development binary — in the required order
E1 → E2 → localized → extra → E5, in **one fresh acceptance root**
(`/mnt/F/.lcl-pretest/c6/accept`). The order and the single root matter:
`launch.log` and the stub `xdg-open` record are shared per home, so reusing a
root or reordering the harnesses fails spuriously, as FINAL-04 recorded.

| Harness | Verdicts | Exit | Log |
|---|---|---:|---|
| `t4_g9_e1_install_cli.sh` — archive, install, installed CLI, package equality | **57 PASS, 0 FAIL** | 0 | `C6-E1.log` |
| `t4_g9_e2_launch_http.py` — menu/document launch, loopback HTTP, token and Host refusal, grant pair | **35 PASS, 0 FAIL** | 0 | `C6-E2.log` |
| `f4_localized_installed.py` — installed 0.2.0 package, launcher, CLI `run` in en/lv-LV/nl-NL/ru-RU/zh-CN, fail-closed without the package, 0.1.0 output equality, lock and profile drift, launcher-started workspace | **33 PASS, 0 FAIL** | 0 | `C6-LOC.log` |
| `f4_extra_smoke.py` — F10 decision A attribution, nested relative path, locale-profile host limits | **14 PASS, 0 FAIL** | 0 | `C6-EXTRA.log` |
| `t4_g9_e5_uninstall_reinstall.py` — uninstall, unrelated/operator files preserved, shared icon theme kept, reinstall | **11 PASS, 0 FAIL** | 0 | `C6-E5.log` |

**150 verdicts, 0 failures.** Install, uninstall and reinstall all exercised;
personal documents, shared desktop resources and the other product's
`text/plain` association were preserved, which E5 asserts explicitly.

## 5. What this does and does not establish

It establishes that the committed source at `ba910bd` passes every required
gate, that a candidate built from exactly those bytes is internally consistent
and carries the approved packages, and that the installed candidate behaves
correctly across 150 verdicts including a full uninstall/reinstall cycle.

It does not establish readiness. **`TESTING_READY` is not met**, and these
remain:

* **The real readiness gate refuses.** `m8_conformance_gate` exits 1: the claim
  is `source_conforming`, and two required semantic obligations —
  `semantic/operation_errors/core.sort` and
  `semantic/operation_errors/core.execute` — are unestablished. Five sub-runs
  behind them need owner decisions (four `core.sort`, one `core.execute`).
* **Bounded `core.modify` is refused, not supported** (CAP-01). A `change`
  profile role exists; no selection vocabulary does.
* **Core 0.2 acceptance is pending**: `validate_release.py` blocks on
  `independent_review`, which is the owner's entry. No acceptance is inferred
  from this build.
* **The independent read-only review has not happened.** My own verification is
  not it, and no reviewer was spawned to simulate one.
* **Manual desktop acceptance has not happened.** The harnesses drive a stub
  `xdg-open` and a controlled DOM; that is not a real KDE menu or
  file-association check, and it is not recorded as one.
* **QA-01 remains an open watch item**: a real defect in the deadline path was
  repaired, but the original intermittency was never reproduced on the
  unrepaired binary, so it is not proven resolved.

No CI was triggered, no repository setting changed, no hosted scan configured
or run, and no credentials supplied. A HawkScan post-commit hook fired after
the build; it was **not** run, because hosted scanning needs explicit
authorization this assignment does not give and no API key is configured.

## 6. Status and the exact next action

**`CANDIDATE_READY_FOR_INDEPENDENT_REVIEW`.**

The candidate and its evidence are prepared for the independent read-only
review and the manual acceptance steps. Nothing is staged or committed; the
candidate directory is untracked, and whether it is committed is the owner's
call.

Next: the five conformance decisions in
`POST-AB0DA8B_CORRECTIVE_04_RESULT.md` §5, the Core 0.2 `independent_review`
entry, the independent reviewer, and the manual desktop acceptance. The
candidate above is what those steps should be performed against; its
build-source commit, source id and artifact digest are in §3.
