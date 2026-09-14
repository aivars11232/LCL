# LCL review coverage ledger — scoped

- **Written:** 2026-09-14, under LCL-CLOSURE-4T Task LCL-CLOSE-02, for finding COVERAGE-01 and decision D5.
- **Companion file:** `LCL_REVIEW_COVERAGE_LEDGER.tsv`, one row per file, SHA-256 `5dd6bc333bc6640379c2d3afaa04a9e13b6058aadbb5cc55809bfb885ba689ce`.
- **Row population:** every tracked file of the repository, plus the files this task adds. The two ledger files
  themselves are excluded.
- **Hashes:** each row's SHA-256 is the file's content when the ledger was written.
- **Deleted file:** its hash is the HEAD blob it replaced.

## Scope

Decision D5 limits full review to three groups:

- the files LCL-CLOSE-01 and LCL-CLOSE-02 change;
- the conformance crate;
- the direct callers of that code.

Everything else is recorded as unreviewed. A directory listing, a fetched file or a matching hash is not a review, and
none is counted as one.

## Status definitions

| Status | Meaning |
| --- | --- |
| `protected-hash-verified` | One of the 192 protected canonical, brand-asset and release paths. Byte identity is verified against the recorded inventory; the content was not reviewed |
| `changed-LCL-CLOSE-01` | Changed by LCL-CLOSE-01 and reviewed there (`reports/tasks/LCL-CLOSE-01_RESULT.md`). Not re-reviewed in LCL-CLOSE-02 |
| `changed-LCL-CLOSE-02-authored` | Changed by this task. The changed ranges were authored under reproduce-before-repair and gated, as recorded in `LCL_RESIDUAL_REPAIR_REPORT.md`. The rest of the file was not re-reviewed unless ranges are listed |
| `changed-LCL-CLOSE-02-formatting` | A formatting-only change, shown by `strip_compare` to be identical apart from whitespace and rustfmt punctuation, and gated |
| `task-deliverable` | A report this task wrote |
| `conformance-crate-unchanged` | In the D5 scope, but unchanged and not re-reviewed |
| `partial-read` | An unchanged file read in part while diagnosing P4c. Only the listed 1-based line ranges were read |
| `unreviewed` | Everything else, including direct callers of changed code, which this ledger does not enumerate |

## Counts

| Status | Files |
| --- | --- |
| `protected-hash-verified` | 192 |
| `changed-LCL-CLOSE-01` | 24 |
| `changed-LCL-CLOSE-02-authored` | 39 |
| `changed-LCL-CLOSE-02-formatting` | 8 |
| `task-deliverable` | 2 |
| `conformance-crate-unchanged` | 3 |
| `partial-read` | 17 |
| `unreviewed` | 459 |
| **Total** | **744** |

## Limitations

- **Direct callers.** Callers of changed code are not enumerated, so they count as `unreviewed`. The D5 full-review
  scope is therefore not complete for callers.
- **Partial ranges.** They cover only the reads made while diagnosing P4c in this session. Reads in earlier LCL-CLOSE-02
  phases are described in the residual report but not itemized here.
- **No blanket claim.** No whole-repository review is claimed. This ledger does not override any open finding.
