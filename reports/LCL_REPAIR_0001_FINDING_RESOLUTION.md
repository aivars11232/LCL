# LCL-REPAIR-0001 Finding Resolution

- Task status: `IMPLEMENTATION_PARTIAL`
- Release status: `NOT_READY_FOR_RELEASE`
- Canonical candidate: `/mnt/F/LCL/canonical/LCL_Core_0.1.0`
- Primary register: the prior Codex audit, LCL-AUDIT-001 through LCL-AUDIT-018
- Secondary register: K-001 through K-005 from the repair task
- DeepSeek audit: not present in the workspace; no secondary findings were imported
- Final ZIP: not created

## Resolution ledger

| ID | Verification | Disposition | Evidence and action |
|---|---|---|---|
| LCL-AUDIT-001 | Confirmed | `PARTIALLY_REPAIRED_BLOCKED` | Removed VALUE/ITEM/PROPERTY/SCHEMA from `BLOCK_WORD`; made them field-only; admitted enum `ITEM` in DEFINE prose and both machine schemas. Exact LIST/SET ITEM representation conflicts with bracket multiline form and remains a design decision. |
| LCL-AUDIT-002 | Confirmed | `REPAIRED_STATICALLY` | Declared the EBNF profile; grouped `NESTED_BODY`; separated REF from generic calls; made qualified identifiers require a dot; removed duplicate multiline-string and signed-number derivations; separated type/value primaries; made unary recursion right-associative. Static graph validation passes. |
| LCL-AUDIT-003 | Confirmed | `PARTIALLY_REPAIRED` | Added `BLANK_LINE`, top-level blank positions, and a lexer rule that blank lines do not change indentation. The 15 concrete fixtures pass, but no complete parser exercises every possible blank-line position. |
| LCL-AUDIT-004 | Confirmed | `REPAIRED_STATICALLY` | Added NORMAL/STRING/MULTILINE_STRING modes, maximal keyword/identifier rules, longest exact symbol selection, excluded-form rejection, and 23 machine-readable excluded lexemes. Declared an ISO-14977-shaped LCL EBNF profile. |
| LCL-AUDIT-005 | Confirmed | `DESIGN_DECISION_REQUIRED` | No constructor overload registry or closed GLOB/REGEX profiles can be recovered without inventing anchoring, flags, escaping, resource, canonicalization, and argument rules. |
| LCL-AUDIT-006 | Confirmed | `REPAIRED_STATICALLY` | Added `COMMENT_BLOCK` to `EXECUTABLE_STATEMENT`; static EBNF validation passes. A complete parser fixture remains unavailable. |
| LCL-AUDIT-007 | Confirmed | `PARTIALLY_REPAIRED_BLOCKED` | Changed both accepted header-only fixtures to `kind.data`; repaired core.modify/core.execute/core.generate/core.download parameter defects in three examples. Full example validity is blocked by result-to-OUTPUT binding/cardinality and the absent complete parser/semantic validator. |
| LCL-AUDIT-008 | Confirmed | `REPAIRED` | Made `00_RELEASE/02_NORMATIVE_AUTHORITY_ORDER.txt` the sole global precedence order; `07_VERSIONING_AND_EXTENSIONS/06_NORMATIVE_SOURCE_PRECEDENCE.txt` now handles same-tier specificity only. |
| LCL-AUDIT-009 | Confirmed | `REPAIRED` | Added canonical-source provenance, immutable base/archive identity, competing-artifact classifications, formal-version rationale, blocker list, and no-archive status. |
| LCL-AUDIT-010 | Confirmed | `PARTIALLY_REPAIRED_BLOCKED` | Added enum ITEM signature and normalized three `meta.execution_unit` references. Current closure is 334 fields, 64 distinct value-kind strings, 12 registered exact strings, 52 unregistered strings, 24 unregistered heads, and seven nested-parent contradictions. |
| LCL-AUDIT-011 | Confirmed | `PARTIALLY_REPAIRED_BLOCKED` | Aligned EXISTS(UNKNOWN)=TRUE; reconciled conditional SET ordering; removed DURATION from unary minus and added negative-subtraction out-of-range behavior. Division result mapping and the unordered-SET sort mechanism remain unresolved. |
| LCL-AUDIT-012 | Confirmed | `PARTIALLY_REPAIRED_BLOCKED` | Normalized all six PRIORITY signatures to `-1000..1000`. Omission behavior/default remains undefined. |
| LCL-AUDIT-013 | Confirmed | `PARTIALLY_REPAIRED_BLOCKED` | Corrected `core.ask` communication and `core.continue` control effects; normalized meta-type references. Twenty-five operation determinism declarations and result cardinality/openness/binding remain unresolved. |
| LCL-AUDIT-014 | Confirmed | `DESIGN_DECISION_REQUIRED` | Cross-stage order exists, but same-stage diagnostic precedence, multiplicity, and severity/rank do not. The registry now states that this blocks release. |
| LCL-AUDIT-015 | Confirmed | `PARTIALLY_REPAIRED_BLOCKED` | Moved hard-conflict, operation-parameter, undefined-operation, and invalid-override failures to pre-effect stages with `status.invalid`. Four mixed-phase identifiers still need a representation decision. |
| LCL-AUDIT-016 | Confirmed | `PARTIALLY_REPAIRED_BLOCKED` | Added executable EBNF, source-fixture, release-gate, and integrity tools. The 791-entry catalog is now explicitly a requirements index; it has zero concrete semantic inputs/results and no semantic implementation. |
| LCL-AUDIT-017 | Confirmed | `REPAIRED_FOR_CANDIDATE` | Regenerated an acyclic 154-record manifest and 156-record checksum file; both exact set/hash validators and `sha256sum --check --strict` pass. |
| LCL-AUDIT-018 | Confirmed | `REPAIRED_FOR_CANDIDATE` | Updated index, README, changelog, version, release metadata, candidate headings, file modes, and archive truth. No repair archive exists, so archive member modes are not claimed. |
| K-001 | Not reproduced | `NO_REPAIR_WARRANTED` | Current JSON, EBNF, and prose keyword inventories contain the same 141 case-sensitive words. |
| K-002 | Partially confirmed | `CONFIRMED_PART_REPAIRED` | The `NESTED_BODY` grouping defect was real and repaired. The claimed BACKSLASH terminal defect was not reproduced under the declared EBNF profile. |
| K-003 | Confirmed | `DESIGN_DECISION_REQUIRED` | This is the field value-kind closure defect tracked under LCL-AUDIT-010. Definitions cannot be inferred safely from combinator names. |
| K-004 | Not reproduced | `NO_REPAIR_WARRANTED` | The immutable 152-file tree and 152-member verified ZIP are byte-identical. The alleged omitted later material is not present as recoverable package bytes. |
| K-005 | Not reproduced as quoted | `DOCUMENTED_HISTORICAL_CONFLICT` | The quoted alternative 139/15/73 or 141/19/79 inventories are absent. Current baseline is 141 keywords, 19 operators, 75 errors, 41 blocks, and 791 requirements; loose reports describe unavailable 45/95/1090 candidate bytes. |

## Owner decisions required before release

1. Freeze the exact block-form LIST/SET `ITEM` representation and its relationship to bracket multiline lists.
2. Define closed overloads and canonical/error behavior for all typed constructors.
3. Define the GLOB profile and the exact ECMAScript REGEX edition, flags, matching mode, Unicode behavior, and resource limits.
4. Define the field value-kind template language, all argument domains/aliases, and resolve seven nested-parent contradictions.
5. Define exact division result/error behavior for INTEGER, DECIMAL, and MEASURE overloads.
6. Define whether unordered-member SETs are legal and the exact explicit-sort mechanism.
7. Define PRIORITY omission behavior: default, inheritance, or invalid absence.
8. Define operation determinism/effect declaration channels and result schema requiredness, openness, failure form, and OUTPUT binding.
9. Define same-stage diagnostic selection, multiplicity, ranking/severity, and expected-result normalization.
10. Define phase/status behavior for identifiers that can be detected before or after effects.
11. Supply or designate a complete lexer/parser/schema validator and semantic implementation, then replace descriptive catalog entries with executable cases.

## Validation-attempt record

The repair stopped and corrected each same-file failure before proceeding. Recorded non-passing attempts included two stale shorthand path lookups, one EBNF comment-delimiter syntax defect, one wrapped-text assertion defect, an unintended `__pycache__` created by `py_compile` and immediately removed, one wrong JSON-key assertion, several over-specific heading-preservation assertions, and one unsupported delete/add patch shape. None was bypassed; corrected checks passed and no cache/temp artifact remains.

## Release decision

The candidate contains material objective repairs and reproducible integrity metadata, but mandatory language and conformance gates remain blocked. It must not be named Final, Verified, Complete, or release-ready. Gate 10 packaging was skipped.
