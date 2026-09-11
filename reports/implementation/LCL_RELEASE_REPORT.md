# LCL 0.1.0 release report

The closing position of `LCL-TASK-0020`, beside the opening position recorded in
`LCL_RELEASE_BASELINE.md`.

## 1. What is being released

| Field | Value |
| --- | --- |
| Product version | `0.1.0` |
| Engine protocol | `lcl.engine/1` |
| Language implemented | LCL Core `0.1.0` |
| Canonical identity digest | `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, 176 files |
| Artifact | `releases/lcl-0.1.0-linux-x86_64.tar.gz` |
| Binaries | `lcl`, `lcl-workspace` |
| Third-party dependencies | none |

Provenance, including the payload's per-file checksums, the source commit, the
toolchain and how to rebuild it, is `releases/lcl-0.1.0-PROVENANCE.txt`.

## 2. Conformance

| Population | Opening | Closing |
| --- | --- | --- |
| Executed cases | 59 | 61 |
| Passed | 56 | 61 |
| Failed | 3 | **0** |
| Descriptive-only entries | 25 | 16 |
| Claim | none, withdrawn by three failures | **`semantics_conforming`** |

The indexed catalogs are unchanged and are not results: 799 descriptive
requirements and 66 decision witnesses, counted in their own columns with no
total, because an indexed requirement and an executed case are different
evidence and adding them would claim something neither supports.

## 3. The five defects the release was opened with

All five are closed. Each was reproduced first, repaired at the stage the
canonical text assigns it to, and given a regression test.

| # | Defect | Repaired in | Witnesses freed |
| --- | --- | --- | --- |
| 1 | An object-valued declaration never resolved. `03_TYPES_AND_VALUES/10` puts an object in "an indented VALUE block", which the preflight's inline read could not see, so every such declaration resolved to MISSING with nothing missing. | M5 `lcl-semantics` | CLOSURE-005, CLOSURE-042 |
| 2 | A demand fault raised by a check's own `ASSERT` was discarded. `expression_demand_resolution` covers a demand made "during a reachable invocation, condition, verification, or completion step", and this layer recorded UNKNOWN and emitted nothing, so a registered diagnostic was raised here or nowhere. | M8 `lcl-completion` | CLOSURE-015 |
| 3 | A malformed expression fragment was never checked. `expression_fragment_contract` says "Static checks cover the complete fragment", and no stage did, so a fragment holding two expressions reached execution and was reported with a substituted identifier. | M4 `lcl-checker` | CLOSURE-019 |
| 4 | `core.read` ignored its `range` parameter, and an object-valued parameter never reached any operation at all. | M6 `lcl-runtime`, M7 `lcl-stdlib` | CLOSURE-048, CLOSURE-049, CLOSURE-050 |
| 5 | A declared parameter family outside the row's contract was accepted: a `BYTES` content for `core.append`, an `OBJECT` schema for `core.validate`. | M4 `lcl-checker` | CLOSURE-051, CLOSURE-052 |

Defect 2's diagnosis differs from the register's. The register named the M6
reduction; the reduction was correct, and the fault it raised was being dropped
one layer later. Defect 4 was likewise two defects: the range was never applied,
and the parameter that carried it never arrived, because an object-valued
`PARAMETER` body was read only in its inline form.

## 4. What hardening found, and what it fixed

### A crash from untrusted source

Deeply nested expressions ended the process by `SIGABRT` rather than by a
diagnostic. Canonical LCL declares no maximum nesting depth and no registered
diagnostic permits an implementation-defined nesting rejection, so a limit was
not available as a repair; M2 had drawn the same conclusion for the parser and
made its four nesting paths iterative. Two layers had not.

| Shape | Before | After |
| --- | --- | --- |
| `(((…)))` groups | aborted at 16,000 | 50,000 |
| `[[[…]]]` collections | aborted at 2,000 | 50,000 |
| `NOT NOT …` chains | aborted at 4,000 | 50,000 |
| `1 + 1 + …` chains | aborted at 4,000 | 50,000 |
| `ABS(ABS(…))` arguments | aborted at 2,000 | 50,000 |
| `ROUND(ROUND(…))` chains | aborted at 2,000 | 50,000 |
| `.a.a.a` and `[0][0][0]` chains | aborted | 50,000 |

Two repairs. The static checker's expression walk now judges a node's
descendants through an explicit worklist and never recurses into a child it did
not have to. The syntax tree's `Clone` is now iterative, like the `Drop` beside
it: every consumer that copies a subtree to satisfy the borrow checker was
paying derived recursion, and the checker's own document walk was one of them.

Depth now costs heap, which fails as an allocation rather than as an
unrecoverable abort.

### What the fuzzing did not find

No panic, no invariant violation and no non-termination across the canonical
examples, every truncation of every example, 2,000 mutations, 2,000 documents
generated from the real lexicon, 2,000 arbitrary byte strings, and the
adversarial shapes above. Every report was checked for span containment,
registry closure, stage monotonicity and the absence of a false success claim.

## 5. Measurements

Reference machine, `dev` profile, against the ceilings in
`LCL_RELEASE_BASELINE.md` 5.2.

| Work | Measured | Ceiling |
| --- | --- | --- |
| Opening and verifying the package, 176 files | about 150 ms | 2,000 ms |
| Slowest canonical stage over any valid example | 5.4 ms | 1,000 ms |
| Slowest full 13-step run over any valid example | 4.1 ms | 2,000 ms |
| One hundred passes over all thirteen valid examples | 22.8 ms first, 19.6 ms hundredth | no growth |

A `ROUND` chain costs time proportional to the square of its depth, because the
row materializes a direct quotient and re-reads its argument to do it. At 1,000,
2,000, 4,000 and 8,000 levels it takes 0.47 s, 0.51 s, 1.29 s and 4.50 s, where
an `ABS` chain of the same depths takes 0.17 s, 0.18 s, 0.21 s and 0.26 s. It is
bounded, and it is why the adversarial suite proves totality for that one shape
at 8,000 levels rather than 50,000.

Declaration count scales superlinearly: 500 declarations check in 0.38 s, 1,000
in 0.30 s, 2,000 in 0.55 s and 4,000 in 1.68 s, in an unoptimized build. That is
within every ceiling and is recorded rather than repaired.

## 6. Security

Executed, not inspected.

| Property | Result |
| --- | --- |
| A path outside its grant, in six spellings including `..` traversal and a doubled separator | refused, each as `error.permission.denied` |
| A read grant used to write | refused, and no effect crossed the boundary |
| No grant at all | refused |
| A `FORBID`den effect with the host grant given | refused at preflight, before the host was consulted |
| An ungranted program, an ungranted network host | refused |
| Workspace transport: wrong token, foreign origin, rebound host name, oversized body, ambiguous framing | refused, from the M10 suite |

## 7. Repeatability

| Property | Result |
| --- | --- |
| Two calls in one process, over every canonical example | byte-identical |
| Two runs against the same host | byte-identical records |
| Three separate processes, empty environment | byte-identical |
| Two different working directories | byte-identical |
| Every application in the ladder, run twice | identical status, checks, evidence and outputs |

## 8. The application ladder

Four projects under `apps/`, each run through the installed-shape binary with an
empty environment, a working directory outside the repository, and exactly the
capabilities it declares.

| Application | What it exercises | Result |
| --- | --- | --- |
| `small-invoice-total` | inputs, exact integer arithmetic, two outputs, two verifications | `status.succeeded`, subtotal 5525, total 6630 |
| `medium-release-notes` | an imported rule library, a typed object context read, a bounded `FOR EACH`, evidence, one granted filesystem write | `status.succeeded`, 11 invocations, 21 steps, `NOTES.txt` written |
| `large-release-pipeline` | two phases, an `ALLOW`/`FORBID` pair with an `OVERRIDE`, a retry handler, a declared domain extension, a ranged `core.read`, five verifications | `status.succeeded`, 9 invocations, 15 steps |
| `large-release-pipeline-decomposed` | the same work as two sub-tasks composed by a parent phase, with types and rules in imported libraries | `status.succeeded`, 9 invocations, 15 steps |

### Whole against decomposed

The two forms agree on every observable the engine decides: the stage reached,
the outcome, the terminal status, all five check outcomes, the evidence, the
diagnostics, and the published outputs. The one difference is the workspace each
was told to write into, which the comparison names explicitly rather than
ignores.

Two of these applications could not have been written before this task. The
medium one reads a property of an object-valued `CONTEXT`, and the large one
reads a range of a file; both were defects 1 and 4.

## 9. Packaging

`packaging/build_release.sh` builds `--release --offline --locked`, stages the
payload, and writes the tarball, its checksum and the provenance record. The
specification package travels with the binaries, because the engine loads one
exact approved package and nothing searches for one.

Executed, into a temporary home with an empty environment:

| Step | Result |
| --- | --- |
| Install | every declared path present, no elevation |
| `lcl version`, `lcl spec` from the installed location | the approved package identity, `authoritative` |
| `lcl run` over a canonical example | `status.succeeded`, exit 0 |
| Any installed file naming the build directory | none |
| Uninstall | every installed path removed |
| Reinstall afterwards | succeeds |

## 10. Known limitations

Each is measured, has an owner, and is not a silent waiver.

1. **Nested indented bodies are bounded at 512 levels.** The static checker's
   declaration walk still recurses once per level and `Statement`'s `Clone` is
   still derived. Past the bound the process aborts rather than emitting a
   diagnostic. A nested body costs four bytes of indentation per level per line,
   so the trigger is also a large document: 2,000 levels is 7.7 MB and 4,000 is
   31 MB. The repair is the one applied to expressions, extended to statements.
2. **Sixteen decision witnesses stay descriptive.** Each records why: seven need
   a host scripted to fail, which the canonical text does not supply; two need a
   declaration form this build does not admit; and the rest need test data the
   canonical text does not name. `CLOSURE-021` is the clearest: it groups a
   `LIST` whose members are objects, and `04_GRAMMAR/10` gives collection
   members as expressions while an object is an indented body, so Core 0.1.0
   supplies no way to write one. Turning any of them into a pass would mean
   inventing the missing input.
3. **`core.read`'s range reports a substituted identifier for a shape defect.**
   A wrong key, a wrong type or a unit that does not index the representation is
   reported as `error.operation.precondition`, which the row admits, rather than
   the `error.operation.parameter` the row's prose names, because that
   identifier's registered stage is `static_or_expression` and the demand map
   does not make it eligible. The bounds contract, which the witnesses test,
   uses its own registered identifier.
4. **A `ROUND` chain is quadratic in its depth**, measured in section 5. Every
   other nesting shape is linear. Bounded, recorded, not repaired.
5. **A `REF` to a `DEFINE kind.constant` does not resolve to its value** where an
   operation parameter admits one. Found while writing a regression fixture,
   outside this task's register.
6. **No model or provider adapter.** A document that needs one is refused rather
   than served by a stand-in.
7. **One platform is verified.** Arch Linux x86_64, KDE Plasma 6, Wayland.
8. **The tarball is not bit-reproducible.** Rust embeds build paths, and no
   attempt was made to normalize them. Provenance is the recorded checksum of
   every payload file plus the build recipe, not a claim that two builds produce
   identical bytes.

## 11. Release-blocking defects

**Zero.** The crash found in phase B was a release blocker and is closed. Every
limitation in section 10 is bounded, measured and recorded.
