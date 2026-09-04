# LCL-TASK-0005 Pre-Edit Baseline

Date: 2026-09-04

Scope: bare LCL Core 0.1.0 language specification; LCL-TASK-0005 only.

Live candidate: `/mnt/F/LCL/canonical/LCL_Core_0.1.0`

Git baseline:

- branch: `main`
- commit: `c11bdaadf86d5fd61bc4c14ef3ae1ad2e7f86c87` (`task4`)
- upstream: `origin/main`
- ahead/behind: `0/0`
- tracked/untracked changes: none

Candidate baseline:

- file count: 172
- sorted path-and-content SHA-256 stream: `4046bcd23c96176ae0450347bd47352f14003be9ea98cca9489dd5c81b010b35`
- full validator: 19 PASS, 2 expected stale-integrity FAIL, 3 BLOCKED, 2 OUT_OF_SCOPE
- `release_ready=false`; `scope_ready=false`
- Task-0005 blocker: `result_schema_cardinality_and_output_contracts`
- other deferred blockers: diagnostic selection and mixed-phase lifecycle contracts

Relevant pre-edit hashes:

```text
a04205e77115efe506a24fe2149adca09df58f1f08f2d34f8a01296f7a0f9c46  05_SEMANTICS/05_INPUT_DATA_OUTPUT_RESULT_AND_FORMAT.txt
d5442fbd0706cb86c756b29b5045f374401f923bec4b3c1ce6d8932b96c9e721  05_SEMANTICS/09_VALIDATION_EXECUTION_FAILURE_AND_TERMINATION.txt
730df0d8487fc5a24e96858d2abe3c9fc0fc225212e8cd9ab94878fdc46d0f46  05_SEMANTICS/10_VERIFY_TEST_EVIDENCE_SUCCESS_FAILURE_AND_STATUS.txt
ac55ac36e3346843fc9486808b0978579dcb79473a603b6d35836005334997e8  10_REGISTRIES/built_in_groups_and_results_v0.1.0.json
8e2cc34d93c0878a7f19c7c427940c9e1f01804b54527698e4c17231dead4fa4  10_REGISTRIES/operations_v0.1.0.json
fffc1175dceb3e42f02049a62d5b635540d02ed85ff128b330f164731b5864c0  10_REGISTRIES/statuses_and_errors_v0.1.0.json
0b40b697ed328b47eb9c5caae183c2bcda6840236c28658d6305f6598f2a939d  09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt
bfc7b9e6127fb7e07f55ac2198bd2bc330e16ceac841a2421eae44a57f152b41  09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json
038996099cfbbf7e586695385f89e44c7f060eb92ef8fd21e958c4bbec80cc00  09_CONFORMANCE/TOOLS/validate_release.py
bee3d2f71a1668b7e5794b54be9fe08687ca42b3da4e5b7dd03ef9632a2a8685  MANIFEST.json
4716d5993417506577643e2ea12d65bb0b7f0219944b6fd1f094948577630ac5  SHA256SUMS.txt
318e4fc95e836cdde1d771638d77ba77be10ede4f41588414536aa0dd90c5921  VALIDATION_REPORT.txt
```

Preservation baselines:

- historical extracted tree: 152 files
- historical sorted path-and-content SHA-256 stream: `7712e2e981e7140cdfde2b679b45b3a96d2b00834541ba6148946376f7d6a7d8`
- completion-package `SHA256SUMS.txt`: `bd0fa1acc9a475ec07ee01b386cf65f8e47a981bb8f870bf910436916cc340ec`
- completion-package checksum verification: all listed files PASS

Exact baseline validator command (exit 1 because expected stale integrity and deferred blockers remain):

```text
PYTHONDONTWRITEBYTECODE=1 python3 09_CONFORMANCE/TOOLS/validate_release.py --scope all
```
