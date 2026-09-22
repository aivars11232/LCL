# LCL 0.1.0 release baseline, threat model and acceptance criteria

Frozen by `LCL-TASK-0020` phase A, 2026-09-11. Every later phase of that task
asserts against this document. A phase that cannot meet a criterion recorded
here stops and reports, rather than lowering the criterion.

This document describes the **product**: the engine, the `lcl` tool and the
workspace. It does not describe the language. LCL Core 0.1.0 is frozen
canonical authority that this product consumes, and nothing in this file may be
read as changing it.

## 1. Candidate identity

| Field | Value |
| --- | --- |
| Product version | `0.1.0` |
| Engine protocol | `lcl.engine/1` |
| Language version implemented | LCL Core `0.1.0` |
| Canonical package | `canonical/LCL_Core_0.1.0` |
| Canonical identity digest | `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, 176 files |
| Canonical authority | `authoritative` |
| Baseline commit | `093d64c8980e55ce7edbf4cf5fd5a6e24b43acd3` |
| Binaries | `lcl`, `lcl-workspace` |
| Dependencies | none; 16 workspace packages, no third-party crate |

The product version is **not** the language version. They are equal by
coincidence of both being a first release, and the tool prints them on separate
lines so that they can diverge later without ambiguity.

`lcl version` is the authority for what a build claims:

```
lcl 0.1.0
protocol lcl.engine/1
language 0.1.0
```

## 2. Reference environment

Every measured criterion in section 5 is measured here. A number produced
anywhere else is evidence about that machine, not about this baseline.

| Field | Value |
| --- | --- |
| OS | Arch Linux, kernel `7.2.4-arch1-2`, `x86_64` |
| Desktop | KDE Plasma 6, Wayland session |
| Toolchain | `rustc 1.98.1 (48a229cea 2026-09-01)`, `cargo 1.98.1` |
| Edition / MSRV | 2021 / `1.75` |
| CPU / memory | 8 logical cores, 14 GiB |
| Build profile for measurement | `dev` (unoptimized, debuginfo), because that is the profile the test suites run in |
| Network | none required; every build and test runs `--offline` |

## 3. Supported scope

### Platforms

| Tier | Scope |
| --- | --- |
| Verified | Arch Linux x86_64, KDE Plasma 6, Wayland. Install, launch, run, uninstall and recovery are executed here. |
| Expected to work, not verified | other Linux x86_64 distributions with a `glibc` and an XDG desktop |
| Out of scope for 0.1.0 | macOS, Windows, non-x86_64, system-wide or multi-user installation, sandboxed distribution formats |

### Capabilities

The host boundary offers exactly what `lcl-capabilities` implements: filesystem
read and write, process execution, and network access, each behind an explicit
grant, each bounded. There is no model or provider adapter in 0.1.0, and a
document that needs one is refused rather than served by a stand-in.

### Installation

A user-level install only: the payload lands under the invoking user's own data
and binary directories, needs no elevation, and is reversed exactly by its
uninstall script. No system-wide path is written, and no service is registered.

## 4. Threat and failure model

What this product must survive, and what it does not defend against.

### The untrusted surfaces

1. **Source bytes.** Any byte sequence may be handed to the engine, by file,
   by editor buffer, or over the workspace transport. It is untrusted in full:
   length, encoding, nesting depth, identifier shape, literal size and
   structural cycles.
2. **Project metadata.** Manifests, lock files and cache entries are read
   before any document is. A hostile or corrupt manifest must not reach past
   the project layer.
3. **Invocation inputs.** Values supplied by `--input` and by the workspace are
   parsed expressions, not trusted values.
4. **The workspace transport.** A loopback HTTP socket is reachable by any
   local process. Request framing, headers, body size and origin are untrusted.
5. **Capability requests made by a document.** A document is an adversary with
   respect to the host: it may name any path, any program, any address.

### The properties that must hold on those surfaces

| Property | Meaning |
| --- | --- |
| Totality | No input causes a panic, an abort, an infinite loop or unbounded memory growth. Every entry point returns. |
| Span containment | Every span in every diagnostic lies inside the source it describes. |
| Registry closure | Every diagnostic identifier is registered, and is registered to the stage that emitted it. |
| Stage monotonicity | No later stage repairs or re-judges an earlier stage's rejection. |
| Determinism | Identical bytes, inputs, state and capabilities produce identical observable results, in the same process, in a new process, from a different working directory, and with an empty environment. |
| Double gate | An effect requires both language authorization and host permission. Neither alone suffices, and a prohibition is not defeated by either. |
| Boundedness | Every host operation carries a size, depth, entry-count and deadline bound, and reports truncation rather than hiding it. |

### Explicitly not defended against

- A hostile **specification package**. The trust anchor detects a substituted
  package, and that is the whole defense; an attacker who can rewrite the
  compiled anchor has already won.
- A hostile **local user** on the same account. The workspace token stops other
  origins and rebound names, not a process that can read the user's memory or
  files.
- **Side channels**: timing, cache and power analysis are out of scope.
- **Denial of service by resource exhaustion the operator asked for**: a
  document granted a large deadline and a large byte bound may use them.
- **Supply chain**: there is nothing to attack, since there are no
  dependencies. This is a property to preserve, not an assumption to make.

## 5. Acceptance criteria and thresholds

These are the numbers phases B through F assert. Each is a regression guard,
not a benchmark: the ceilings carry deliberate headroom so that an ordinarily
loaded machine does not turn a correctness gate into a coin flip, while a real
regression of an order of magnitude still fails.

### 5.1 Robustness, phase B

| Criterion | Threshold |
| --- | --- |
| Panics, aborts or hangs from any generated, mutated or truncated input | 0 |
| Span containment violations | 0 |
| Unregistered diagnostic identifiers | 0 |
| Stage monotonicity violations | 0 |
| Generated cases per stage boundary, per run | at least 2,000 |
| Mutations of every canonical example | every single-byte mutation of a sampled set, and every truncation of every valid example |
| Reproducibility of a failure | every generated case is reconstructible from a recorded seed, with no stored corpus needed |

#### Nesting depth, and the one bound that is declared

Canonical LCL Core 0.1.0 declares **no** maximum nesting depth for source
syntax: `04_GRAMMAR/02` and the EBNF state the shape and no bound, and no
registered diagnostic permits an implementation-defined nesting rejection. M2
drew that conclusion for the parser and made its four nesting paths iterative.
Phase B found that three layers had not:

| Path | Before | After |
| --- | --- | --- |
| Groups, collections, call arguments, unary prefixes, binary operands, property and index chains, `ROUND` chains | aborted by `SIGABRT` between 2,000 and 16,000 levels | 50,000 levels, the depth M2 proved for the parser |
| Copying a syntax subtree, which every consumer does to satisfy the borrow checker | derived `Clone` recursed and aborted at the same depths | iterative, like the `Drop` beside it |
| Nested indented bodies | aborted at about 2,000 levels on a 2 MiB stack | unchanged; **declared bound below** |

**One shape is quadratic.** A `ROUND` chain costs time proportional to the
square of its depth, because the row materializes a direct quotient and re-reads
its argument to do it. Measured on the reference machine, `dev` profile:
1,000 levels in 0.47 s, 2,000 in 0.51 s, 4,000 in 1.29 s and 8,000 in 4.50 s,
against an `ABS` chain of the same depths at 0.17 s, 0.18 s, 0.21 s and 0.26 s.
It is bounded, not unbounded, and it is recorded rather than repaired.

**The declared bound.** A nested indented body is judged by a walk that still
recurses once per level, in the static checker's declaration walk, and copied by
a `Statement` clone that is still derived. This build is declared to handle
**512 levels of nested indented bodies**, which the hardening suite asserts. A
nested body costs four bytes of indentation per level per line, so depth and
document size cannot be separated: 512 levels is already a 500 KB document,
2,000 levels is 7.7 MB and 4,000 levels is 31 MB. Past the bound the process
aborts rather than emitting a diagnostic. This is a known limitation, it is
recorded in the release report, and the repair is the same one applied above.

### 5.2 Performance, phase C

Measured on the reference machine, `dev` profile. Observed baseline, taken
during phase A:

| Work | Observed |
| --- | --- |
| Opening and verifying the canonical package, 176 files, per process | about 150 ms |
| `lcl run` over a 123-line document, whole process including the package open | about 155 ms |
| `lcl check`, `validate`, `inspect` and `run` over all 13 valid examples, 52 process invocations | about 8.3 s total |

The package open dominates every measurement above, which is the expected shape:
SHA-256 over 176 files in an unoptimized build. Thresholds are therefore set on
the two costs separately:

| Criterion | Ceiling |
| --- | --- |
| Package open and verify, once per process | 2,000 ms |
| Every canonical stage over any valid canonical example, package already open | 1,000 ms per document per stage |
| Full 13-step run over any valid canonical example, package already open | 2,000 ms |
| Peak additional memory for any single document in the suite | must not grow without bound; measured and recorded, with a hard ceiling of 512 MiB |

### 5.3 Repeatability, phase C

| Criterion | Threshold |
| --- | --- |
| Repeated machine-readable output, same process | byte-identical, 100 % |
| Repeated output, separate processes | byte-identical, 100 % |
| Output from a different working directory, empty environment | byte-identical, 100 % |
| Repeat runs of every application in the ladder | identical terminal status, outputs and evidence |

### 5.4 Security, phase C

| Criterion | Threshold |
| --- | --- |
| Path traversal, symbolic-link escape or absolute path outside a grant | refused, 100 %, with no filesystem access attempted |
| Program execution without a grant naming it | refused, 100 % |
| Network access without a grant naming the host | refused, 100 % |
| A prohibited effect with the host grant given | refused before the host is consulted, 100 % |
| Workspace request with a wrong or missing token, foreign origin, or rebound host name | refused, 100 % |
| Declared deadlines and byte, entry and depth bounds | enforced, with truncation reported rather than hidden |
| Cancellation mid-run | leaves no partial write and no corrupt project state |

### 5.5 Application ladder, phase D

| Criterion | Threshold |
| --- | --- |
| Small, medium and larger applications | each reaches exactly one terminal status, with its declared outputs and evidence |
| Whole-program against task-decomposed | equivalent observable semantics where equivalence is expected; every difference explained in the release report |
| Workspace against CLI, over the ladder | byte-identical machine output |

### 5.6 Packaging and recovery, phase E

| Criterion | Threshold |
| --- | --- |
| Clean install into an empty home | succeeds with no elevation |
| Installed binaries run from outside the repository, empty environment | succeed |
| Any installed file referencing a repository path | 0 |
| Uninstall | removes every installed path and leaves nothing behind |
| Reinstall after uninstall | succeeds |

### 5.7 Release gate, phase F

| Criterion | Threshold |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | clean |
| `cargo test --offline --workspace --all-targets` | 0 failed, 0 ignored |
| Executable conformance | 0 failed cases, and a claim that is not withdrawn |
| Canonical release validator | 31 PASS / 0 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE |
| Canonical checksums | exit 0, 175 files |
| Known release-blocking defects | 0 |

> **Supersession note, 2026-09-22.** The `0 ignored` threshold above predates
> `lcl-project/tests/manifest_input_bounds.rs::child_parses_nested_manifest`,
> and the current gate reports `1,825 passed, 0 failed, 1 ignored`. The two
> statements are reconciled here rather than left standing against each other,
> and the threshold above is **not** edited: it is this task's opening position
> and stays as written.
>
> That test is not skipped. It is the child half of a pair: `parse_in_child`
> re-executes this test binary with `--exact child_parses_nested_manifest
> --ignored` and `LCL_Q_JSON_DEPTH`, so the manifest is parsed at depth in a
> separate process and a stack overflow kills that child instead of the suite.
> `#[ignore]` is how the Rust harness expresses a case that only its parent may
> start; the case runs on every gate, through its parent.
>
> So the criterion's intent — no case silently skipped, no failure hidden behind
> a skip — is met, while its literal text is not. A future gate should read this
> as "0 failed, and no ignored case that is not driven by another case", and any
> *new* ignored test still has to justify itself the same way. The current
> criteria and their executed evidence are in
> `reports/tasks/POST-AB0DA8B_CORRECTIVE_04_RESULT.md` §6 and
> `reports/tasks/MAPPING_CORRECTION_R3_RESULT.md` §4.
>
> The `175 files` canonical-checksum threshold above is **not** stale, despite
> the package identity reporting 176: `SHA256SUMS.txt` lists and verifies 175
> files, and the 176th is that checksum file itself.

## 6. Known-defect register

Seven conformance witnesses are failing or unexecutable because of five defects
in milestones that were closed before this task began. They are recorded here
as the release's opening position. `LCL-TASK-0020` phase A prime closes them;
the release report will state the closing position beside this one.

| # | Defect | Owner | Witnesses |
| --- | --- | --- | --- |
| 1 | An object-valued declaration never resolves to a material value, because data resolution reads a `VALUE` only through the inline form. Every read yields MISSING. | M5 `lcl-semantics` | CLOSURE-005 failing; CLOSURE-021, CLOSURE-042, CLOSURE-051 unexecutable |
| 2 | `SUM` over a declared empty typed collection yields UNKNOWN instead of the registered operand error, although the registry gives it `minimum_count: 1`. | M6 `lcl-runtime` | CLOSURE-015 failing |
| 3 | A calculate fragment holding two expressions is refused with `error.operator.operand` rather than the `error.operation.parameter` the operation contract names. | M7 `lcl-stdlib` | CLOSURE-019 failing |
| 4 | `core.read` ignores its `range` parameter: the whole content is returned and an inverted range raises nothing. | M7 `lcl-stdlib` | CLOSURE-048, CLOSURE-049, CLOSURE-050 unexecutable |
| 5 | `core.append` accepts a `BYTES` content parameter, though the contract types content as `STRING|LIST[T]` and states that "BYTES is a count and is not content". | M7 `lcl-stdlib` | CLOSURE-052 unexecutable |

Opening conformance position, phase A:

| Population | Count |
| --- | --- |
| Executed cases | 59 |
| Passed | 56 |
| Failed | 3 |
| Descriptive-only entries | 25 |
| Claim | none, withdrawn by the three failures |

Thirteen further witnesses stay descriptive for reasons recorded by
`LCL-TASK-0017`: they need a host scripted to fail, a declaration form this
build does not admit, or test data the canonical text does not supply. Those
are not defects, and inventing the missing input to make them pass is
forbidden. They are release limitations, and the release report names each.

## 7. Phase gates

A phase begins only when the previous phase's gate is green.

| Phase | Gate |
| --- | --- |
| A | This document exists and names every threshold later phases assert. |
| A prime | Conformance failed count is 0 and the claim is no longer withdrawn by a failed case. |
| B | Section 5.1 met. |
| C | Sections 5.2, 5.3 and 5.4 met. |
| D | Section 5.5 met. |
| E | Section 5.6 met. |
| F | Section 5.7 met, and the release report, provenance and limitations list exist. |
