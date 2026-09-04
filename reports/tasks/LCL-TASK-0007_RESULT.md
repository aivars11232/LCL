# LCL-TASK-0007 Result — Static Specification Validation, Integrity Regeneration, and Bare-Language Release Packaging

Date: 2026-09-04

Task status: COMPLETE

Scope: BARE LANGUAGE SPECIFICATION ONLY

Language-definition status: `BARE_SPECIFICATION_COMPLETE`

Package status: `BARE_LANGUAGE_RELEASE`

## 1. Corrected task identity and scope

The owner scope correction retires the original package definitions of
LCL-TASK-0007 (reference lexer/parser), LCL-TASK-0008 (reference semantic
interpreter), and LCL-TASK-0009 (executable conformance). They were not executed
and no substitute parser/interpreter task was created. The former final task
concept was replaced by this task: static specification validation, integrity
regeneration, and bare-language release packaging.

No lexer, parser, interpreter, compiler, runtime, semantic execution engine, UI,
IDE, editor extension, desktop application, model-provider integration, agent
framework, deployment tooling, or production application code was built, and no
such absence was treated as a missing part or a release blocker.

Work was performed only against the actual extracted candidate at
`/mnt/F/LCL/canonical/LCL_Core_0.1.0`. The Claude web-session handoff remained a
decision/change record only; no web-session edit was assumed to exist locally.
Accepted decisions D-001 through D-005 and the accepted operation/result design
constraints were preserved and not reopened.

## 2. Live candidate and baseline

Baseline recorded before the first edit in
`/mnt/F/LCL/reports/tasks/LCL-TASK-0007_BASELINE.md`:

- Git baseline: `main`, commit `bb2bb8891ec7034666e7c890a947a6726d838fff`
  (`LCL-TASK-0006: close bare specification and add task result report`), clean
  working tree, no untracked files.
- Candidate baseline: 172 files, tree fingerprint
  `82514489c94c4314d2d8160eeafe56b283015ca130357ebec8f71468aed1bdc0`, byte-identical
  to the LCL-TASK-0006 closure fingerprint.
- Baseline validator: 23 PASS, 2 FAIL (stale integrity), 0 BLOCKED,
  2 OUT_OF_SCOPE; `release_ready=false`.
- Frozen integrity artifacts: `MANIFEST.json`
  `bee3d2f71a1668b7e5794b54be9fe08687ca42b3da4e5b7dd03ef9632a2a8685`,
  `SHA256SUMS.txt` `4716d5993417506577643e2ea12d65bb0b7f0219944b6fd1f094948577630ac5`,
  `VALIDATION_REPORT.txt` `318e4fc95e836cdde1d771638d77ba77be10ede4f41588414536aa0dd90c5921`.

Fingerprint method throughout:
`find . -type f | LC_ALL=C sort | xargs -d'\n' sha256sum | sha256sum`.

## 3. Decisions applied

No new user decision was required. Every change followed either an already
accepted decision or an objective inconsistency between the documented state and
the actual bytes, so no question was raised under Global Rule 3.

Two classification facts were verified rather than assumed:

- `grammar/complete_example_parse_matrix` and `catalog/semantic_case_execution`
  were already classified `OUT_OF_SCOPE` with
  `classification=BARE_LANGUAGE_IMPLEMENTATION_ARTIFACT` by LCL-TASK-0001/0006.
  No hard-coded parser, interpreter, semantic-execution, or executable-case
  release blocker remained in `validate_release.py` to be reclassified.
- `release_ready` is computed as `scope == "all" and FAIL == 0 and BLOCKED == 0`,
  so an `OUT_OF_SCOPE` result can neither block release nor be mistaken for a
  `PASS`. Neither check is reported as `PASS`.

## 4. Static validation performed

All applicable static checks from the corrected scope were executed against the
actual bytes:

| Static check | Result |
|---|---|
| JSON syntax, strict, duplicate-key rejection | PASS — 15 files |
| Python tool compilation, no bytecode | PASS — 4 tools |
| Path/mode/cache/symlink/case-collision hygiene | PASS — 172 files, 0 violations |
| UTF-8/LF/control-character hygiene | PASS — 159 clean files, 13 intentional invalid fixtures isolated |
| Registry structure and counts | PASS — 141 keywords, 21 adopted symbols, 23 excluded lexemes, 21 types, 8 semantic meta-types, 41 blocks, 334 field signatures, 19 operators, 11 functions, 39 operations, 12 statuses, 77 errors, 9 result schemas |
| Duplicate detection | PASS — no duplicate manifest paths, checksum paths, requirement IDs, or JSON keys |
| Unresolved-reference detection | PASS — zero undefined errors, results, or meta-types; zero cross-registry conflicts |
| Field/type closure | PASS — 334 field uses over 65 distinct value kinds; zero unresolved kinds, heads, or templates |
| Keyword/grammar closure | PASS — 141/141 parity in both directions |
| Operator/semantic closure | PASS — 39 operation contracts, 39 prose mirrors, 9 result schemas, division, SET/sort, PRIORITY, constructor/pattern profiles |
| Error/status closure | PASS — 77 errors with complete selection and lifecycle metadata; 12 executed selection vectors |
| EBNF structural validity | PASS — 65 productions, 211 terminals, zero undefined/unreachable/unproductive/nullable/left-recursive |
| Examples against statically decidable rules | PASS — 12 valid, 21 invalid, 21 expectation pairs, 7 focused diagnostics, 14 pinned fingerprints |
| Concrete source fixtures | PASS — 15/15 expected lexical outcomes |
| Manifest consistency | PASS — 169 records, exact path set, sizes, and hashes |
| SHA-256 checksums | PASS — 171 records; `sha256sum -c` exit 0 |
| Archive integrity | PASS — see section 7 |

An independent count sweep outside `validate_release.py` reproduced every
registry count above directly from the JSON files, and an independent
`INDEX.txt` audit confirmed it lists all 172 files exactly once.

## 5. Files created, modified, deleted

Release-tree files modified (13; none created, deleted, renamed, or moved;
inventory remains 172 files):

```text
00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt
00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt
00_RELEASE/03_COMPLETENESS_CRITERIA.txt
00_RELEASE/04_CHANGE_CONTROL.txt
09_CONFORMANCE/TOOLS/generate_integrity.py
09_CONFORMANCE/TOOLS/validate_release.py
CHANGELOG.txt
INDEX.txt
MANIFEST.json
README.txt
SHA256SUMS.txt
VALIDATION_REPORT.txt
VERSION.txt
```

Reasons, one coherent change at a time:

1. **Release documentation** (`VERSION.txt`, `README.txt`, `INDEX.txt`,
   `CHANGELOG.txt`, and the four `00_RELEASE` files) still described a frozen,
   unpublished, not-release-ready candidate with no archive. That is false once
   the integrity metadata is regenerated and the archive is produced, and
   "accurate release documentation" is part of the bare-language completion
   boundary. Each file now states the packaged `BARE_LANGUAGE_RELEASE` state and
   repeats that the release covers the language definition only and must never be
   described as parser-verified, executed, or runtime-tested.
2. **`validate_release.py` release-documentation gate** pinned the exact strings
   `deliberately frozen and stale` and `no repair archive is produced` as
   *required* in the two `00_RELEASE` status documents. After the release those
   requirements assert a falsehood, so under Global Rule 6 the check was updated
   to represent the resolved rule: it now requires the release wording and
   additionally rejects five pre-release phrases as stale claims. The check kept
   its teeth (section 8).
3. **`validate_release.py` out-of-scope labels** referred to the retired parser
   work as `LCL-TASK-0007`, which is now this release task's own identifier. The
   fields are now `retired_package_task` / `retired_package_tasks` naming the
   original package definitions explicitly.
4. **`generate_integrity.py`** hard-coded `"status": "blocked_repair_candidate"`,
   `"release_ready": False`, and a repair-candidate package label, so a
   regenerated manifest would have misdescribed the release. The manifest
   subcommand now requires an explicit `--status` from a closed set that
   determines the package label and `release_ready`, and records
   `package_scope: bare_language_specification` plus the out-of-scope artifact
   list. There is no default, so the status cannot be asserted by omission.
5. **Integrity artifacts** regenerated in the documented acyclic order.

Files created outside the release tree:

```text
/mnt/F/LCL/releases/LCL_Core_0.1.0_Final.zip
/mnt/F/LCL/reports/LCL_Core_0.1.0_RELEASE_VALIDATION.md
/mnt/F/LCL/reports/LCL_Core_0.1.0_RELEASE_VALIDATION.json
/mnt/F/LCL/reports/tasks/LCL-TASK-0007_BASELINE.md
/mnt/F/LCL/reports/tasks/LCL-TASK-0007_RESULT.md
```

Deleted: none.

## 6. Integrity regeneration

Regenerated only after all normative specification files were stable, in the
order documented by `00_RELEASE/03_COMPLETENESS_CRITERIA.txt`:

| Artifact | Records | SHA-256 |
|---|---:|---|
| `MANIFEST.json` | 169 | `c725aaac186323dbfb855063f66422ddc84be0290846008b091ab6458f47ed42` |
| `VALIDATION_REPORT.txt` | n/a | `2e528d5875747de7d1acf7fe3013f458dc9259d89782d12f2c1e1cef8d2395c8` |
| `SHA256SUMS.txt` | 171 | `eec7cc1d98ca8640044e7625dee7193e59674b4dff36f9d27dce6207676576de` |

- Manifest excludes itself, `VALIDATION_REPORT.txt`, and `SHA256SUMS.txt`
  (169 = 172 - 3); checksums exclude only themselves (171 = 172 - 1).
- `VALIDATION_REPORT.txt` binds the exact manifest hash above and reports Gates 1
  through 8 with the two out-of-scope checks named explicitly and never as PASS.
- Released tree: 172 files, 18 directories including root, 1,277,786 bytes,
  fingerprint `e202677f29fcf106bb1ffb7220b7ad17d31b275a4fe2c5af499d4cc3e3082b0b`.

## 7. Release archive

| Property | Value |
|---|---|
| Path | `/mnt/F/LCL/releases/LCL_Core_0.1.0_Final.zip` |
| Bytes | 314,012 |
| SHA-256 | `1b9c472c53638dc211377cdc7fc810eb5235ca724407b42237de3e511bc39bc0` |
| Archive root | `LCL_Core_0.1.0/` (single root) |
| Member count | 172 |
| Duplicate/unsafe members | 0 / 0 |

Verification performed: `testzip` reported no bad member; the archive was
extracted to a temporary directory; `diff -r --no-dereference` against the
release tree exited 0; independent per-file SHA-256 streams were identical; and
`validate_release.py --scope all`, `validate_ebnf.py`,
`validate_source_fixtures.py`, and `sha256sum -c` were rerun against the
extracted copy, all exiting 0 with 25 PASS, 0 FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE and
`release_ready=true`. The temporary extraction was then removed, and both the
release tree and the extracted copy were confirmed free of `__pycache__`/`.pyc`
artifacts. The archive is stored outside the package root, so no archive file is
a member of the tree it packages.

## 8. Exact validation commands and exit codes

Run from `/mnt/F/LCL/canonical/LCL_Core_0.1.0` with `PYTHONDONTWRITEBYTECODE=1`
unless noted.

```text
python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope filesystem   exit 0
python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope text         exit 0
python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope structured   exit 0
python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope grammar      exit 0
python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope registry     exit 0
python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope catalog      exit 0
python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope integrity    exit 0
python3 09_CONFORMANCE/TOOLS/validate_release.py --root . --scope all          exit 0
python3 09_CONFORMANCE/TOOLS/validate_ebnf.py 04_GRAMMAR/10_COMPLETE_EBNF.ebnf --start DOCUMENT  exit 0
python3 09_CONFORMANCE/TOOLS/validate_source_fixtures.py                       exit 0
python3 09_CONFORMANCE/TOOLS/generate_integrity.py --root . manifest \
        --generated-utc 2026-09-04T17:38:32Z --status bare_language_release     exit 0
python3 09_CONFORMANCE/TOOLS/generate_integrity.py --root . checksum            exit 0
sha256sum -c SHA256SUMS.txt                                                     exit 0  (171/171 OK)
sha256sum -c SHA256SUMS.txt   (in /mnt/F/LCL_Completion_Task_Package_v2.0)      exit 0  (20/20 OK)
python3 <extracted>/09_CONFORMANCE/TOOLS/validate_release.py --root <extracted> --scope all  exit 0
```

Every command exits 0. No validation failure was left unresolved, and no step was
continued past a failure caused by the current edit.

Validator honesty checks:

- Tamper A — reinstating `deliberately frozen and stale` in
  `00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt` on a scratch copy turned
  `registry/set_and_sort_semantics` to `FAIL`.
- Tamper B — replacing `BARE_LANGUAGE_RELEASE` in
  `00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt` on a scratch copy turned the
  same check to `FAIL`.
- `generate_integrity.py manifest` without `--status` exits non-zero rather than
  defaulting to a release claim.
- No check gained a pass-through branch, and no out-of-scope condition is
  reported as PASS.

## 9. Current validator summary

`--scope all`: 25 PASS, 0 FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE; `scope_ready=true`,
`release_ready=true`, exit 0.

```text
PASS  filesystem  path_mode_and_cache_hygiene
PASS  text        utf8_lf_and_control_hygiene
PASS  text        concrete_source_fixtures
PASS  text        static_example_contracts
PASS  structured  strict_json_parse
PASS  structured  python_source_compile
PASS  grammar     static_ebnf_graph
OOS   grammar     complete_example_parse_matrix     (retired original LCL-TASK-0007)
PASS  registry    keyword_grammar_parity
PASS  registry    block_field_schema_parity
PASS  registry    collection_value_and_item_contract
PASS  registry    field_value_kind_closure
PASS  registry    nested_parent_closure
PASS  registry    operation_reference_closure
PASS  registry    priority_contract
PASS  registry    operation_contracts
PASS  registry    operation_prose_contracts
PASS  registry    result_schema_cardinality_and_output_contracts
PASS  registry    division_semantics
PASS  registry    set_and_sort_semantics
PASS  registry    constructor_and_pattern_profiles
PASS  registry    diagnostic_selection_contract
PASS  registry    mixed_phase_lifecycle_contracts
PASS  catalog     requirements_index_integrity      (797 cases, 23 categories)
OOS   catalog     semantic_case_execution           (retired original LCL-TASK-0008/0009)
PASS  integrity   manifest_set_size_hash            (169 records)
PASS  integrity   checksum_set_and_hash             (171 records)
```

## 10. Resolved requirements and remaining issues

Resolved by this task:

- static specification validation across every applicable check in the corrected
  scope;
- release-documentation accuracy;
- manifest, validation-report, and checksum regeneration;
- release packaging with archive integrity, extraction comparison, and
  revalidation;
- external validation/provenance evidence tied to the exact hashes.

Remaining issues: none that block the bare-language release. The two
implementation-dependent checks remain `OUT_OF_SCOPE` and are documented as
possible future work outside LCL Core 0.1.0. They are reported accurately as
out-of-scope, never as PASS.

Open items O-001 through O-007 were closed by Tasks 0001 through 0006. O-008
(reference implementation and executable conformance) is retired as outside the
project scope rather than resolved.

## 11. Preservation of historical inputs

- `/mnt/F/LCL/LCL_Core_0_1_Final`: still 152 files, fingerprint
  `7712e2e981e7140cdfde2b679b45b3a96d2b00834541ba6148946376f7d6a7d8`, unchanged.
- Historical ZIP: absent, consistent with the owner-confirmed intentional
  deletion recorded by LCL-TASK-0001; its verified lineage hash remains in
  `00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt`.
- `/mnt/F/LCL_Completion_Task_Package_v2.0`: `SHA256SUMS.txt` still
  `bd0fa1acc9a475ec07ee01b386cf65f8e47a981bb8f870bf910436916cc340ec`; all 20
  records verify. The task package was not edited, including the retired task
  files.
- The LCL-REPAIR-0001 reports `reports/LCL_Core_0.1.0_VALIDATION.md`,
  `reports/LCL_Core_0.1.0_VALIDATION.json`, and
  `reports/LCL_Core_0.1.0_PROVENANCE.md` were left unmodified; this task's
  external evidence was written to new `..._RELEASE_VALIDATION.*` files.
- Earlier task reports under `reports/tasks/` were not modified.

## 12. Next permitted task

None. LCL Core 0.1.0 is a complete bare language specification, statically
validated, integrity-regenerated, and packaged. The original LCL-TASK-0007,
LCL-TASK-0008, LCL-TASK-0009, and LCL-TASK-0010 are retired or superseded by this
task and must not be started. Any further work is a new version under
`00_RELEASE/04_CHANGE_CONTROL.txt`, never an in-place edit of these released
bytes.
