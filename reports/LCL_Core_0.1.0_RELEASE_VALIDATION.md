# LCL Core 0.1.0 Bare Language Release Validation

## Outcome

- Task: `LCL-TASK-0007` (corrected final task: static specification validation,
  integrity regeneration, and bare-language release packaging)
- Package scope: `BARE_LANGUAGE_SPECIFICATION_ONLY`
- Language-definition status: `BARE_SPECIFICATION_COMPLETE`
- Package status: `BARE_LANGUAGE_RELEASE`
- Release tree: `/mnt/F/LCL/canonical/LCL_Core_0.1.0`
- Release archive: `/mnt/F/LCL/releases/LCL_Core_0.1.0_Final.zip`
- Overall gate result: `PASS` — 25 validator checks PASS, 0 FAIL, 0 BLOCKED,
  2 OUT_OF_SCOPE; `release_ready=true`

## Scope statement

This release defines the language only. A lexer, parser, interpreter, compiler,
runtime, semantic execution engine, UI, IDE, provider integration, agent
framework, and deployment tooling are outside the LCL Core 0.1.0 project scope.
Their absence is not a missing part and not a release blocker. Any gate whose
only possible evidence is such an implementation is reported `OUT_OF_SCOPE`. It
is never reported `PASS`, and it never contributes to release readiness, which
is computed from `FAIL` and `BLOCKED` counts only.

## Gate summary

| Gate | Contract name | Status | Evidence |
|---:|---|---|---|
| 1 | Filesystem and path safety | `PASS` | 172 files; zero symlinks, case collisions, cache, bytecode, backup, temporary, or non-canonical-mode paths. |
| 2 | Text and concrete source fixtures | `PASS` | 159 clean UTF-8/LF text files with 13 deliberately invalid fixtures isolated; `validate_source_fixtures.py` matched 15 of 15 expected lexical outcomes. |
| 3 | Structured formats and tools | `PASS` | 15 JSON files pass strict parse with duplicate-key rejection; 4 Python tools compile with no bytecode written. |
| 4 | Grammar | `PASS` | Static EBNF graph: 65 productions, 211 terminals, zero undefined, unreachable, unproductive, nullable, or left-recursive entries. |
| 4b | Complete example parse matrix | `OUT_OF_SCOPE` | Requires a reference lexer/parser. Retired with the original LCL-TASK-0007. |
| 5 | Registry and type closure | `PASS` | 141 keywords match the grammar reserved set; 41 block schemas match 41 field-signature blocks; 334 field uses resolve over 65 distinct value kinds with zero unresolved kinds, heads, or templates; zero nested-parent contradictions; 11 constructors and 2 pattern profiles closed. |
| 6 | Semantics | `PASS` | Division, SET/sort, PRIORITY-omission, determinism/dependency/effect, diagnostic-selection (77 errors, 12 executed selection vectors), and failure-lifecycle (77 errors, 4 baseline + 11 additional mixed-phase profiles) contracts are closed and internally consistent. Static contract evidence only. |
| 7 | Examples | `PASS` | 12 valid and 21 invalid examples with 21 registered expectation pairs, 7 focused primary diagnostics, and 14 pinned byte fingerprints; each listed exactly once in `INDEX.txt`. Statically decidable rules only. |
| 8 | Conformance | `PASS` | 797-entry requirements index across 23 categories with unique IDs and consistent per-category counts, covering every closed registry category. |
| 8b | Semantic case execution | `OUT_OF_SCOPE` | Requires a semantic execution engine. Retired with the original LCL-TASK-0008 and LCL-TASK-0009. |
| 9 | Manifest and checksums | `PASS` | 169 manifest records and 171 checksum records match exact path sets, sizes, and hashes; `sha256sum -c` exits 0 over all 171 records. |
| 10 | Archive | `PASS` | 172-member single-root ZIP; `testzip` clean; extraction compares byte-for-byte with the release tree; extracted copy revalidates at 25 PASS / 0 FAIL / 0 BLOCKED. |
| 11 | Clean final state | `PASS` | Temporary extraction data removed; zero `__pycache__`/`.pyc` artifacts in the release tree or the extracted copy; historical inputs byte-unchanged. |

## Integrity evidence

Acyclic order: `MANIFEST.json`, then `VALIDATION_REPORT.txt`, then `SHA256SUMS.txt`.

| Artifact | Records | SHA-256 |
|---|---:|---|
| `MANIFEST.json` | 169 | `c725aaac186323dbfb855063f66422ddc84be0290846008b091ab6458f47ed42` |
| `VALIDATION_REPORT.txt` | n/a | `2e528d5875747de7d1acf7fe3013f458dc9259d89782d12f2c1e1cef8d2395c8` |
| `SHA256SUMS.txt` | 171 | `eec7cc1d98ca8640044e7625dee7193e59674b4dff36f9d27dce6207676576de` |

- Package files: 172 (18 directories including root), 1,277,786 bytes.
- Release-tree fingerprint (`find . -type f | LC_ALL=C sort | xargs -d'\n' sha256sum | sha256sum`):
  `e202677f29fcf106bb1ffb7220b7ad17d31b275a4fe2c5af499d4cc3e3082b0b`
- `MANIFEST.json` excludes itself, `VALIDATION_REPORT.txt`, and `SHA256SUMS.txt`.
- `SHA256SUMS.txt` excludes only itself.
- `VALIDATION_REPORT.txt` binds the exact manifest SHA-256 above.

## Archive evidence

| Property | Value |
|---|---|
| Path | `/mnt/F/LCL/releases/LCL_Core_0.1.0_Final.zip` |
| Bytes | 314,012 |
| SHA-256 | `1b9c472c53638dc211377cdc7fc810eb5235ca724407b42237de3e511bc39bc0` |
| Archive root | `LCL_Core_0.1.0/` (single root) |
| Member count | 172 |
| Duplicate or unsafe members | 0 |
| `testzip` | clean (no bad member) |
| Extraction comparison | `diff -r` exit 0; per-file SHA-256 streams identical |
| Extracted revalidation | 25 PASS, 0 FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE; `release_ready=true` |

The archive is stored outside the package root, so no archive file is a member of
the tree it packages. Members are written in bytewise path order with a fixed
timestamp, so the archive is reproducible from the release tree.

## Command evidence

Working directory `/mnt/F/LCL/canonical/LCL_Core_0.1.0` unless noted, with
`PYTHONDONTWRITEBYTECODE=1`.

| Purpose | Exact command | Exit | Result |
|---|---|---:|---|
| Gate 1 | `python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope filesystem` | 0 | 1 PASS |
| Gate 2 | `python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope text` | 0 | 3 PASS |
| Fixture detail | `python3 09_CONFORMANCE/TOOLS/validate_source_fixtures.py` | 0 | 15/15 matched |
| Gate 3 | `python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope structured` | 0 | 2 PASS |
| EBNF static | `python3 09_CONFORMANCE/TOOLS/validate_ebnf.py 04_GRAMMAR/10_COMPLETE_EBNF.ebnf --start DOCUMENT` | 0 | 65 productions, 211 terminals, zero diagnostics |
| Gate 4 | `python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope grammar` | 0 | 1 PASS, 1 OUT_OF_SCOPE |
| Gates 5-6 | `python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope registry` | 0 | 15 PASS |
| Gates 7-8 | `python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope catalog` | 0 | 1 PASS, 1 OUT_OF_SCOPE |
| Manifest generation | `python3 09_CONFORMANCE/TOOLS/generate_integrity.py --root . manifest --generated-utc 2026-09-04T17:38:32Z --status bare_language_release` | 0 | 169 records; `release_ready=true` |
| Checksum generation | `python3 09_CONFORMANCE/TOOLS/generate_integrity.py --root . checksum` | 0 | 171 records |
| Gate 9 structural | `python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope integrity` | 0 | 2 PASS |
| Gate 9 native | `sha256sum -c SHA256SUMS.txt` | 0 | 171 of 171 OK |
| Final aggregate | `python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope all` | 0 | 25 PASS, 0 FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE |
| Gate 10 revalidation | `python3 <extracted>/09_CONFORMANCE/TOOLS/validate_release.py --root <extracted> --scope all` | 0 | 25 PASS, 0 FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE |

## Validator honesty checks

- No check has a pass-through branch that reports absence of an implementation
  as `PASS`. The two implementation-dependent checks emit `OUT_OF_SCOPE` with an
  explicit `classification` and `reason`.
- The release-documentation gate was tamper-tested twice on a scratch copy:
  reinstating the phrase `deliberately frozen and stale` and replacing
  `BARE_LANGUAGE_RELEASE` each turned `registry/set_and_sort_semantics` to
  `FAIL`.
- `release_ready` is `scope == "all" and FAIL == 0 and BLOCKED == 0`, so an
  `OUT_OF_SCOPE` result can neither create nor mask release readiness.

## Preserved historical inputs

| Input | Verification |
|---|---|
| `/mnt/F/LCL/LCL_Core_0_1_Final` | 152 files; fingerprint `7712e2e981e7140cdfde2b679b45b3a96d2b00834541ba6148946376f7d6a7d8` — unchanged |
| `/mnt/F/LCL_Completion_Task_Package_v2.0` | `SHA256SUMS.txt` = `bd0fa1acc9a475ec07ee01b386cf65f8e47a981bb8f870bf910436916cc340ec`; all 20 records verify (exit 0) — unchanged |
| `reports/LCL_Core_0.1.0_VALIDATION.{md,json}`, `reports/LCL_Core_0.1.0_PROVENANCE.md` | LCL-REPAIR-0001 reports retained unmodified; this release report is a new file |
| `reports/tasks/LCL-TASK-000{1..6}_*.md` | unchanged |
