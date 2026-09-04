# LCL-TASK-0006 Pre-Edit Baseline

Date: 2026-09-04

Scope: bare LCL Core 0.1.0 language specification; LCL-TASK-0006 only.

Live candidate: `/mnt/F/LCL/canonical/LCL_Core_0.1.0`

Git baseline:

- branch: `main`
- commit: `cd8d6293ef2d57d481bcfbd9e1a154d2eb727833` (`task5`)
- upstream: `origin/main`
- ahead/behind: `0/0`
- tracked/untracked changes before this baseline report: none

Candidate baseline:

- file count: 172
- sorted `./path` plus content-SHA-256 stream:
  `696f09a4be0a48eaeceb226678392f4f852b7f55789fbc7d30ea5813fbe0655e`
- full validator: 20 PASS, 2 expected stale-integrity FAIL, 2 BLOCKED,
  2 OUT_OF_SCOPE
- `release_ready=false`; `scope_ready=false`
- Task-0006 blockers: `diagnostic_selection_contract` (`LCL-AUDIT-014`)
  and `mixed_phase_lifecycle_contracts` (`LCL-AUDIT-015`)
- mixed-phase identifiers reported by the validator:
  `error.execution.order`, `error.scope.violation`,
  `error.required.missing`, and `error.dependency.unsatisfied`
- parser/example-matrix and semantic-execution checks are accurately
  OUT_OF_SCOPE under the bare-language scope correction

Relevant pre-edit hashes:

```text
aff9213679d193897428d28fd5235e96967f7669cb8c5d3eb8fb0358110113f2  10_REGISTRIES/statuses_and_errors_v0.1.0.json
68d0e4ff8c33b0cb8fb38e9b24f94b73ceb5a42f55ea356be6208d02bcc95513  05_SEMANTICS/09_VALIDATION_EXECUTION_FAILURE_AND_TERMINATION.txt
7fbd5c6203828479ab15885f07ca963355bdae50813a775159cba73fb5746740  06_STANDARD_LIBRARY/06_CORE_ERROR_IDENTIFIERS_PART_1.txt
7b831e68c21d26ea2187783c6f21831cc90e17f3e328f5d53d08172eb264c040  06_STANDARD_LIBRARY/07_CORE_ERROR_IDENTIFIERS_PART_2.txt
3b56e5f10d02abcc2ec19445fe06ca84ada998e48bd9a3ded7acec13ce6a1927  09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt
e3ae56d4591baa2eb59645a8151bbf63fd89fd357ad3fe7c5c181dc5e885636a  09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json
5ca775b8f5cdf5ae0afb447741abada2e8152570a3e0e67abb7172a5cd8a3024  09_CONFORMANCE/TOOLS/validate_release.py
370849bf7b467fba3add5d4a73499fa776af105649fedf4122324aebf6f080e4  00_RELEASE/00_CANONICAL_SOURCE_AND_PROVENANCE.txt
670462d539fbde7d59bc08669bcfc60540dc97c38f585fffb8cb7d0bc2039e10  00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt
7be8245a04fed1a9325fff451ef48fbc6b24676c8ed7adee05e2b755b71064d1  00_RELEASE/03_COMPLETENESS_CRITERIA.txt
20d4790efcad5fcea13b6d25a6c3eb5252ccec1d8ccfc82defd2b7605c108e58  README.txt
a10507756b4504ab6a54fb6040995a9e4ebaeee8a0248ba23a878da2024f2583  CHANGELOG.txt
```

Frozen integrity artifacts retained for the corrected final release task:

```text
bee3d2f71a1668b7e5794b54be9fe08687ca42b3da4e5b7dd03ef9632a2a8685  MANIFEST.json
4716d5993417506577643e2ea12d65bb0b7f0219944b6fd1f094948577630ac5  SHA256SUMS.txt
318e4fc95e836cdde1d771638d77ba77be10ede4f41588414536aa0dd90c5921  VALIDATION_REPORT.txt
```

Preservation baselines:

- historical extracted tree: 152 files
- historical sorted `./path` plus content-SHA-256 stream:
  `7712e2e981e7140cdfde2b679b45b3a96d2b00834541ba6148946376f7d6a7d8`
- historical ZIP: absent at task start, consistent with the owner-confirmed
  intentional deletion recorded by LCL-TASK-0001
- completion-package `SHA256SUMS.txt`:
  `bd0fa1acc9a475ec07ee01b386cf65f8e47a981bb8f870bf910436916cc340ec`
- completion-package checksum verification: all 20 listed files PASS

Exact baseline validator command (exit 1 because the two Task-0006 blockers and
the deliberately stale integrity files remain):

```text
PYTHONDONTWRITEBYTECODE=1 python3 canonical/LCL_Core_0.1.0/09_CONFORMANCE/TOOLS/validate_release.py \
  --root canonical/LCL_Core_0.1.0 --scope all
```

