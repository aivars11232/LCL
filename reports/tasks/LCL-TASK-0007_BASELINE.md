# LCL-TASK-0007 Baseline — Static Validation, Integrity Regeneration, and Bare-Language Release Packaging

Date: 2026-09-04
Recorded before the first edit of the task, per Global Rule 5.

## 1. Corrected task identity

The owner scope correction retires the original package definitions of
LCL-TASK-0007 (reference lexer/parser), LCL-TASK-0008 (reference semantic
interpreter), and LCL-TASK-0009 (executable conformance). They are outside the
LCL Core 0.1.0 project scope and must not be executed.

LCL-TASK-0007 is re-defined as the single final task:

    Static specification validation, integrity regeneration, and bare-language
    release packaging.

Scope: BARE LANGUAGE SPECIFICATION ONLY. No parser, interpreter, compiler,
runtime, semantic execution engine, UI, IDE, provider integration, agent
framework, or deployment tooling is built, and the absence of each is not a
release blocker.

## 2. Live candidate

Writable target: `/mnt/F/LCL/canonical/LCL_Core_0.1.0`

| Property | Value |
|---|---|
| Package file count | 172 |
| Tree fingerprint (`find . -type f \| LC_ALL=C sort \| xargs -d'\n' sha256sum \| sha256sum`) | `82514489c94c4314d2d8160eeafe56b283015ca130357ebec8f71468aed1bdc0` |
| Language-definition status | `BARE_SPECIFICATION_COMPLETE` through LCL-TASK-0006 |
| Release status | unpublished; integrity metadata deliberately frozen |

The fingerprint is byte-identical to the LCL-TASK-0006 closure fingerprint, so
the local tree has not drifted since the previous task closed.

## 3. Git baseline

- Branch `main`, commit `bb2bb8891ec7034666e7c890a947a6726d838fff`
  (`LCL-TASK-0006: close bare specification and add task result report`).
- Working tree clean; no untracked or unknown files.

## 4. Frozen integrity artifacts (pre-edit bytes)

```text
bee3d2f71a1668b7e5794b54be9fe08687ca42b3da4e5b7dd03ef9632a2a8685  MANIFEST.json
4716d5993417506577643e2ea12d65bb0b7f0219944b6fd1f094948577630ac5  SHA256SUMS.txt
318e4fc95e836cdde1d771638d77ba77be10ede4f41588414536aa0dd90c5921  VALIDATION_REPORT.txt
```

## 5. Baseline validator result

`python3 canonical/LCL_Core_0.1.0/09_CONFORMANCE/TOOLS/validate_release.py --root canonical/LCL_Core_0.1.0 --scope all`

- counts: 23 PASS, 2 FAIL, 0 BLOCKED, 2 OUT_OF_SCOPE
- `scope_ready=false`, `release_ready=false`, exit code 1

Both FAIL results are `integrity/manifest_set_size_hash` and
`integrity/checksum_set_and_hash`, caused solely by the deliberately frozen
`MANIFEST.json` and `SHA256SUMS.txt` missing the fifteen example files added by
Tasks 0001-0003. They are expected stale-integrity failures, not language
defects.

Both OUT_OF_SCOPE results are `grammar/complete_example_parse_matrix` and
`catalog/semantic_case_execution`. They are already classified OUT_OF_SCOPE with
`classification=BARE_LANGUAGE_IMPLEMENTATION_ARTIFACT`, and
`release_ready` is computed as `FAIL == 0 and BLOCKED == 0`, so neither blocks
release. No hard-coded parser/interpreter release blocker remains to be
reclassified.

## 6. Preserved historical inputs (pre-edit bytes)

| Input | State |
|---|---|
| `/mnt/F/LCL/LCL_Core_0_1_Final` | 152 files, fingerprint `7712e2e981e7140cdfde2b679b45b3a96d2b00834541ba6148946376f7d6a7d8` |
| `/mnt/F/LCL_Completion_Task_Package_v2.0` | `SHA256SUMS.txt` = `bd0fa1acc9a475ec07ee01b386cf65f8e47a981bb8f870bf910436916cc340ec`; all 20 records verify (exit 0) |
| `/mnt/F/LCL/reports/tasks/LCL-TASK-000{1..6}_*.md` | unchanged |
| `/mnt/F/LCL/releases/` | does not exist |

## 7. Planned coherent changes

1. Static validation sweep of the whole candidate, including checks outside
   `validate_release.py`.
2. Correct release documentation that still describes a frozen, non-releasable
   candidate.
3. Correct `generate_integrity.py`, which hard-codes a
   `blocked_repair_candidate` manifest status.
4. Regenerate `MANIFEST.json`, then `VALIDATION_REPORT.txt`, then
   `SHA256SUMS.txt`, in the documented acyclic order.
5. Package `/mnt/F/LCL/releases/LCL_Core_0.1.0_Final.zip`, verify archive
   integrity, extract, compare byte-for-byte, and revalidate the extracted copy.
6. Regenerate the external validation/provenance reports and write
   `LCL-TASK-0007_RESULT.md`.

No successor task is started.
