# LCL Core 0.1.0 Repair-Candidate Provenance

## Identity

- Task: `LCL-REPAIR-0001`
- Canonical candidate: `/mnt/F/LCL/canonical/LCL_Core_0.1.0`
- Formal language version: `0.1.0`
- Implementation status: `IMPLEMENTATION_PARTIAL`
- Release status: `NOT_READY_FOR_RELEASE`
- Workspace Git status: not a Git repository (`git` probe exit 128)

## Lineage

The canonical candidate was copied byte-for-byte from:

`/mnt/F/LCL/LCL_Core_0_1_Final`

Pre-repair base evidence:

- files: 152
- directories including root: 17
- file bytes: 629,917
- relative-path/content digest: `4bad5434d76eecc789ed94cc49e938b9d1310b12e1e1739041395523f949c6e8`
- canonical copy before editing: path diff 0, byte diff 0

The base tree was byte-identical to every member extracted from:

`/mnt/F/LCL/LCL_Core_0.1_Final_VERIFIED.zip`

Historical archive evidence:

- SHA-256: `1f3057e3e186bfca218843976da050f1d504f4beaae8aad10bb144437cbfcfd0`
- size: 144,572 bytes
- members: 152 under `LCL_Core_0_1_Final/`
- `unzip -tqq`: exit 0
- extracted archive/tree path and byte comparison: 152/152 identical

After repair, the original workspace inputs still pass:

- baseline content hashes: 158/158
- baseline type/mode/size records: 176/176
- historical archive SHA-256: unchanged
- historical archive ZIP test: exit 0

## Artifact classification

| Artifact | Classification | Role |
|---|---|---|
| `/mnt/F/LCL/canonical/LCL_Core_0.1.0` | `CANONICAL_REPAIR_CANDIDATE` | Lead-owned repaired tree; blocked, not a release. |
| `/mnt/F/LCL/LCL_Core_0_1_Final` | `DUPLICATE_IMMUTABLE_BASE` | Exact historical source copied before repair; preserved unchanged. |
| `/mnt/F/LCL/LCL_Core_0.1_Final_VERIFIED.zip` | `DUPLICATE_IMMUTABLE_ARCHIVE` | Byte-equivalent archive of the historical base; preserved unchanged. |
| Loose `LCL_Core_0_1_Final_VALIDATION*` and SHA report | `SUPERSEDED_HISTORICAL_CANDIDATE_EVIDENCE` | Describes unavailable 126-file ZIP/TAR candidate bytes; not validation of either present tree. |
| `LC_Language_Word_and_Symbol_Audit_v0.1.txt` | `SUPPORTING_RESEARCH` | Explicitly experimental word/symbol research; not normative package input. |
| `LC language structure tree.txt` | `SUPPORTING_SOURCE` | Unversioned structure material; not part of the canonical package. |

No DeepSeek audit artifact was found. The prior Codex audit was therefore the
primary finding register; K-001 through K-005 from the repair task were verified
as the secondary register.

## Source precedence

Global language-source precedence is defined only by:

`00_RELEASE/02_NORMATIVE_AUTHORITY_ORDER.txt`

Same-tier specificity is defined by:

`07_VERSIONING_AND_EXTENSIONS/06_NORMATIVE_SOURCE_PRECEDENCE.txt`

Examples and historical reports cannot override normative registries/grammar.
Unavailable candidate bytes were not reconstructed or inferred.

## Version rationale

The workspace contains materially different same-version candidate metadata but
no evidence that either candidate was an established published release. The
repair therefore retains formal version `0.1.0` only as a blocked, unpublished
candidate identifier. Before any release, the owner must decide whether accepted
syntax/semantic repairs require a patch, minor, or major language version.

## Candidate integrity

- package files: 157
- total file bytes: 691,407
- stable payload/manifest records: 154
- checksum records: 156
- `MANIFEST.json` SHA-256: `bee3d2f71a1668b7e5794b54be9fe08687ca42b3da4e5b7dd03ef9632a2a8685`
- `VALIDATION_REPORT.txt` SHA-256: `318e4fc95e836cdde1d771638d77ba77be10ede4f41588414536aa0dd90c5921`
- `SHA256SUMS.txt` SHA-256: `4716d5993417506577643e2ea12d65bb0b7f0219944b6fd1f094948577630ac5`
- strict manifest set/size/hash validation: PASS
- strict checksum set/hash validation: PASS
- `sha256sum --check --strict SHA256SUMS.txt`: exit 0

The manifest excludes itself, the later validation report, and the checksum file.
The checksum file excludes only itself and therefore binds the manifest and
embedded validation report. External reports are outside the package root.

## Archive disposition

No repair archive or release checksum was created. `/mnt/F/LCL/releases` remains
absent because Gates 1 through 9 did not all pass. The immutable historical archive
is preserved, but it is not presented as the repaired candidate.
