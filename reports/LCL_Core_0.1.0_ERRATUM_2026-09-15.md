# LCL Core 0.1.0 — non-normative archive-name erratum

- **Erratum date:** 2026-09-15.
- **Status:** NON-NORMATIVE external erratum.
  - It changes no byte of `canonical/LCL_Core_0.1.0`.
  - It regenerates no manifest or checksum and changes no trust anchor.
  - It is not a new language version, and it renames, replaces or removes no archive.
- **Finding:** B-09 of the LCL Six-Task Repair Pack, resolved under LCL-REPAIR-02.
- `reports/LCL_Core_0.1.0_ERRATUM_2026-09-13.md`, the count erratum, stays in force and is unchanged.

## Canonical identity, unchanged

- **Package identity:** `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, the identity `lcl-spec` pins as its trust anchor. On 2026-09-15, `lcl spec` reported it as authoritative.
- **`SHA256SUMS.txt`:** all 175 entries verify against the package on 2026-09-15.

## The contradiction

The package names two different archives as its release archive.

| Where in `canonical/LCL_Core_0.1.0` | What it says |
| --- | --- |
| `VERSION.txt`, line 11 | "Release archive: LCL_Core_0.1.0_Final.zip, created outside this package root" |
| `README.txt`, lines 43–44 | "the older LCL_Core_0.1.0_Final.zip remains historical. The current archive is LCL_Core_0.1.0_Bare_Language_2026-09-05.zip." |
| `00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt`, lines 18–21 | "Task reports and the older LCL_Core_0.1.0_Final.zip describe earlier snapshots." and "The current archive is LCL_Core_0.1.0_Bare_Language_2026-09-05.zip" |
| `VALIDATION_REPORT.txt`, lines 111–118 | Binds `releases/LCL_Core_0.1.0_Bare_Language_2026-09-05.zip` and `reports/completion/2026-09-05/archive-verification.json`, then: "Historical task reports and LCL_Core_0.1.0_Final.zip remain unchanged. They are lineage evidence and do not substitute for the fresh results of this release." |

## Reconciliation

- **The current archive** of the LCL Core 0.1.0 bare-language release is `releases/LCL_Core_0.1.0_Bare_Language_2026-09-05.zip`.
  - Size 410,424 bytes; SHA-256 `1385ae06539d30b2bd710a70f10e3373c095e032cf1cfaa39686faf8dfd698c5`.
  - `reports/completion/2026-09-05/archive-verification.json` binds it: 176 members, the member set equal to the canonical tree, and every member byte-identical to it.
  - `reports/completion/2026-09-05/COMPLETION_REPORT.md` records that it was created after the full gate, under a distinct name that does not overwrite `LCL_Core_0.1.0_Final.zip`.
  - The comment on `lcl-spec`'s trust anchor names this archive and this SHA-256 for the approved package.
- **`releases/LCL_Core_0.1.0_Final.zip`** is the earlier LCL-TASK-0007 archive, kept as lineage evidence.
  - Size 314,012 bytes; SHA-256 `1b9c472c53638dc211377cdc7fc810eb5235ca724407b42237de3e511bc39bc0` on 2026-09-15.
  - `reports/LCL_Core_0.1.0_RELEASE_VALIDATION.md` records it as the LCL-TASK-0007 release archive.
- **`VERSION.txt` line 11 is a stale carry-over from LCL-TASK-0007.** Read it as naming that historical archive. `README.txt`, `00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt` and `VALIDATION_REPORT.txt` name the current one.
- **Both archives carry the stale line.** Each holds a `VERSION.txt` whose line 11 names `LCL_Core_0.1.0_Final.zip`. In the current archive, that member is byte-identical to the canonical file.

## Why the package is not edited

`VERSION.txt` is covered by `SHA256SUMS.txt` and by the package identity the trust anchor pins, so editing line 11 would change the approved package. `00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt` requires a modified tree to pass fresh validation, and `00_RELEASE/04_CHANGE_CONTROL.txt` binds any new archive to the new bytes. The LCL Six-Task Repair Pack also protects these bytes. Any correction inside the package is therefore a package change under change control, and that decision is the owner's.

## Classification

- Metadata only: one stale file name in one line.
- No registry, grammar, example, manifest count or language rule is affected.
- No language meaning changes, and no reissue is required for this erratum.

## Historical reports

`reports/LCL_Core_0.1.0_RELEASE_VALIDATION.md` names `LCL_Core_0.1.0_Final.zip` as the release archive. That was accurate for LCL-TASK-0007, before the 2026-09-05 completion created the current archive. The report is left unchanged as a historical record.

## Inputs (SHA-256)

| File | SHA-256 |
| --- | --- |
| `canonical/LCL_Core_0.1.0/VERSION.txt` | `05e137d5baa328114cf2915530ca92a29c4a06aa3db816517dc6758f7132b06b` |
| `canonical/LCL_Core_0.1.0/README.txt` | `ee0d8292062ba9e9f28dca1465136e3be54e441e7ea69222ffbd2af174b694ab` |
| `canonical/LCL_Core_0.1.0/00_RELEASE/01_RELEASE_STATUS_AND_BOUNDARY.txt` | `f030c35ed6852638fb675dc73b066bf31264ab9b929a37a3dbfe9618956dff8f` |
| `canonical/LCL_Core_0.1.0/VALIDATION_REPORT.txt` | `1dfab2932d43bb77a1d21b643829675630ec70f4b6ede9132ed57c5a9298094b` |
| `canonical/LCL_Core_0.1.0/SHA256SUMS.txt` | `f4a119ed1b8418623d9265c77f61d9cffc5c5250ce486bbbe80e2102ba0943ab` |
| `releases/LCL_Core_0.1.0_Bare_Language_2026-09-05.zip` | `1385ae06539d30b2bd710a70f10e3373c095e032cf1cfaa39686faf8dfd698c5` |
| `releases/LCL_Core_0.1.0_Final.zip` | `1b9c472c53638dc211377cdc7fc810eb5235ca724407b42237de3e511bc39bc0` |
| `reports/completion/2026-09-05/archive-verification.json` | `ecc26f63a73f0debb43e559ce8889b3f91c2d08300143515242fec879ad63e3f` |
| `reports/completion/2026-09-05/COMPLETION_REPORT.md` | `10584756792725e9e99a7a5a82f3da8004f8cb3574bf96962eba49c7cf9a8c4d` |
| `reports/LCL_Core_0.1.0_RELEASE_VALIDATION.md` | `45e5236b6dffe8b9a46a339e166397ea0f21ebcf8b6cec7a930c92164abeeb91` |
