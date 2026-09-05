# LCL-TASK-0010 Result — Close the Registry Contract Type Notation

Date: 2026-09-05

Task status: `COMPLETE` (for the approved scope)

Bare-spec status: NOT YET COMPLETE — findings 1, 3, 4, 5, 6A, 6B, 7, 8, 9, 10, 11, 12,
13, and 14 of the approved plan remain open, as do the release/metadata items.

Release status: NOT RELEASE READY. Integrity artifacts are deliberately left stale;
regeneration is gated to the final release task by
`00_RELEASE/03_COMPLETENESS_CRITERIA.txt`.

## 1. Scope and approval

Approved plan: the read-only plan presented in this session, recommending
**LCL-TASK-0010 — Registry contract type notation (metagrammar)** as the first task,
covering audit finding **2** and the parse-registry-notation part of finding **15**.
Owner approval: message `Approved`. No file was modified before that approval.

Confirmations: exactly one foreground agent; no background, parallel, delegated, or
sub-agent; no `&`, `parallel`, `xargs -P`, or concurrent command; bare LCL specification
and its static conformance material only. No lexer, parser, interpreter, compiler,
runtime, execution engine, UI, IDE, workspace application, provider integration, OS
integration, daemon, deployment, or product code was written. The validator addition is
static specification validation and parses no LCL document.

## 2. Baseline verified from disk before the first edit

- Repository `/mnt/F/LCL`, branch `main`, HEAD `38893b861add4f2b09e2e32e88cab5619699f0b3`,
  working tree clean, 0 ahead / 0 behind `origin/main`.
- Canonical root `/mnt/F/LCL/canonical/LCL_Core_0.1.0`, 172 files, fingerprint
  `a3fd1330ee9eeb38fb3327a0e43a0e334b5fb2d03870f9c0a55f074233f1a2cc`.
- Baseline validator: **26 PASS / 2 FAIL / 0 BLOCKED / 2 OUT_OF_SCOPE**, the two FAILs
  being the intentionally stale `MANIFEST.json` and `SHA256SUMS.txt`.
- Latest prior task: `LCL-TASK-0009`. `LCL-TASK-0010` was unused.

## 3. Defect reproduced, and one further defect found

The audit finding was that the operation/result registries carry a rich internal type
notation with **no formally defined closed metagrammar**, while the field-signature
registry already defines its own (`value_kind_templates`). Reproduced against current
bytes: 53 distinct `type` strings in `operations_v0.1.0.json` and 20 in
`built_in_groups_and_results_v0.1.0.json` (164 occurrences, 66 distinct), using unions,
`ENUM[...]`, `LIST[T]`, `REFERENCE[...]`, `meta.*`, `qualified_identifier(d)`,
`target_expression`, and `result.effect`. The shipped validator handled every one of
them only as a pinned literal and never parsed one.

A prototype parser written in the session scratchpad (never in the repository) turned
that formal gap into two concrete, machine-checkable defects:

1. **`qualified_identifier(status)` resolved against nothing.** It is used by
   `result_contract.common_fields.status`, but `status` was absent from the eight
   declared `qualified_identifier_domains`. This was named in the plan.
2. **Two named value kinds were spelled inconsistently.** Five type strings wrote
   `BOOLEAN_EXPRESSION` and `TYPE_EXPRESSION` in uppercase while the single normative
   definition of each is the lowercase `value_kind_registry` key `boolean_expression` /
   `type_expression`. This was **not** in the plan; it was found by prototyping the
   notation before writing it, and is reported here rather than quietly absorbed.

## 4. Design applied

**One closed notation, defined once, at authority tier 1.** `contract_type_notation` was
added to `operations_v0.1.0.json` — the registry holding 53 of the 66 distinct forms and,
per `00_RELEASE/02_NORMATIVE_AUTHORITY_ORDER.txt`, the highest authority tier. The
results registry gains a one-line `contract_type_notation_source` pointer and defines no
notation of its own, so there is exactly one definition and no second surface to drift.

**The notation is kept separate from source syntax**, as the objective requires. It
declares explicitly that no form it defines is admissible in a source `TYPE` field and
that source type syntax remains governed by EBNF `TYPE_EXPRESSION` and the field-signature
value kinds. Where both cite the same designator they share that one *definition* — they
never share *syntax*. No EBNF production and no keyword was touched.

**What the notation fixes**, each closed: an ISO-14977-profile grammar in the same profile
as the shipped EBNF; nine atom classes each naming the exact registry source, JSON
pointer, and selection it resolves against; union semantics (order is not meaning,
duplicates forbidden, at least one member); `ENUM` value sets closed in place; permitted
nesting with a declared maximum; a whitespace prohibition; the type-variable binding rule;
the meaning of an omitted `REFERENCE` or `qualified_identifier` parameter; a lexical
uppercase/lowercase rule separating language-level names from specification-level
designators; a closed list of invalid forms; and a defect rule stating that a
non-conforming string is a specification defect repaired in the registry, never a document
diagnostic.

**Type-variable binding was written from the actual bytes, not assumed.** All five `T`
occurrences were located first. `core.filter` and `core.sort` declare `LIST[T]` /
`LIST[T]|SET[T]` targets and select `result.collection`, whose `items` is `LIST[T]`; so
the binding scope is stated as one operation invocation spanning the contract *and the
result schema that contract selects*. `core.group` (T in target, `result.value`) and
`core.append` (T bound from its own argument) are both consistent with that rule.

**Both concrete defects were repaired**: the `status` domain was added, and the five
uppercase spellings were normalized. The `status` domain was deliberately given **no**
`defined_kind` key, unlike `error`, `event`, and `format`, so that the question of whether
extensions may define statuses stays open for the extension-diagnostics task rather than
being silently decided here.

## 5. Files changed (8)

```text
10_REGISTRIES/operations_v0.1.0.json            contract_type_notation added; 5 type strings normalized
10_REGISTRIES/field_signatures_v0.1.0.json      status qualified-identifier domain added
10_REGISTRIES/built_in_groups_and_results_v0.1.0.json  notation source pointer added
06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt  normative paragraph; source-syntax separation
09_CONFORMANCE/TOOLS/validate_release.py        new contract_type_notation check; 12 pins updated honestly
09_CONFORMANCE/CASES/core_conformance_cases_v0.1.0.json  CONTRACT-TYPE-NOTATION-0799 added
09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt  798 -> 799
README.txt                                      798 -> 799
```

No file was created, deleted, renamed, or moved. Canonical inventory remains **172 files**.
No `.lcl` example, no EBNF production, and no keyword changed. Post-task fingerprint
`b81e5c0cb641a5511ac2d5a970c948298cb6f6f6a37d93a719488066a3b50b68`.

## 6. The spelling normalization is provably meaning-preserving

Changing five type strings moved five Task-0004 contract fingerprints and the aggregate.
Rather than simply re-pinning, the change was proven to be spelling-only: reverting just
the spelling in memory reproduced **every** old pinned value exactly.

| pinned value | reverted == old pin |
|---|---|
| `core.verify` row | yes |
| `core.test` row | yes |
| `core.convert` row | yes |
| `core.ask` row | yes |
| `core.retry` row | yes |
| aggregate 39-row matrix | yes |

The 34 unchanged rows were separately confirmed not to have drifted. Only then were the
six pins updated to the values the registry actually holds.

## 7. Validation

Full gate: `validate_release.py --root canonical/LCL_Core_0.1.0 --scope all`

```text
PASS:         27   (26 baseline + 1 new contract_type_notation)
FAIL:          2   (pre-existing stale MANIFEST.json / SHA256SUMS.txt)
BLOCKED:       0
OUT_OF_SCOPE:  2   (parser matrix, semantic execution — retired package tasks)
```

The two failures are the same two intentional integrity failures present at baseline, on
the same file and for the same reason. They are not repaired here.

`registry/contract_type_notation` PASS reports: 164 type strings checked, 66 distinct,
observed maximum nesting depth 2, and all nine atom classes exercised
(`SCALAR_TYPE_NAME` 116, `META_TYPE_NAME` 47, `ENUM_VALUE` 29, `BLOCK_NAME` 17,
`NAMED_VALUE_KIND` 10, `DOMAIN_NAME` 10, `TYPE_VARIABLE` 6, `SPECIAL_VALUE_NAME` 5,
`RESULT_RECORD_NAME` 1).

**Honest coverage statement.** The check is an invariant check, not a snapshot. It reads
the notation from the registry, resolves each atom class through the source, pointer, and
selection the registry *itself* declares, and parses every string reachable under the
notation's own `applies_to` patterns. If a declared source moves, the check follows it. It
also holds the notation's own metadata honest: it verifies that every declared
`applies_to` pattern matches at least one real string, that no string escapes those
patterns, that no atom class is declared but unused, and that
`observed_maximum_in_this_release` equals the depth actually observed. It states its own
limit in `static_limit`: no LCL document is parsed or executed.

**Non-vacuity proven.** Fourteen mutation probes ran on isolated temporary copies; the
real tree was never mutated. The unmutated control passes and every mutation fails with
the exact violation named:

| probe | result |
|---|---|
| unmutated copy (control) | PASS |
| unparseable form `LIST[` | FAIL — expected `]` |
| `qualified_identifier(bogus)` | FAIL — unresolved domain |
| unregistered type variable `LIST[Q]` | FAIL — unregistered type variable |
| duplicate union member `STRING\|STRING` | FAIL — duplicate union member |
| whitespace `STRING \| REFERENCE` | FAIL — whitespace |
| nesting beyond maximum depth | FAIL — nesting too deep |
| **uppercase named value kind restored** | **FAIL — the repaired defect is caught** |
| **`status` domain removed** | **FAIL — the found defect is caught** |
| declared-but-unused atom class | FAIL — atom class never used |
| `applies_to` pattern matching nothing | FAIL — pattern matches no string |
| `observed_maximum_in_this_release` falsified | FAIL — declared 1, observed 2 |
| `contract_type_notation` deleted | FAIL — notation absent |
| results-registry pointer deleted | FAIL — registry not bound to the definition |
| atom class pointer made unresolvable | FAIL — pointer does not resolve |

Two intermediate results are recorded rather than silently fixed. First, the check
initially reported two violations of my own rule text: `ENUM` is also a registered scalar
type name and `qualified_identifier` is also a registered value kind, so the rule "a
constructor keyword never resolves as an atom" was false as written. The rule was
corrected to state what is actually true — a constructor keyword is consumed structurally
before atom resolution — and the assertion was replaced with the property that is
meaningful and checkable: no parsed atom use may be a constructor keyword. Second, the
parser initially accepted any single uppercase letter as a type variable while the
notation declares `registered_variables: ["T"]`; that gap was closed and is now covered by
the `LIST[Q]` probe.

Independent re-verification outside the validator: 39 operation contracts, 9 result
schemas and 77 errors intact; notation declared closed; `status` domain resolving to the
12 registered statuses and carrying no `defined_kind`; zero remaining uppercase
named-value-kind spellings; EBNF and keyword registry byte-identical; catalog at 799 cases
across 25 categories.

## 8. Pins updated honestly

Twelve pinned literals were updated, each because a real normative surface changed, none
weakened, skipped, or given a pass-through branch:

- operations registry root field set (+`contract_type_notation`)
- groups/result registry root field set (+`contract_type_notation_source`)
- Task-0002 qualified-identifier domain contract (+`status`)
- five Task-0004 operation row fingerprints and the aggregate matrix fingerprint (proven
  spelling-only in section 6)
- two `core.verify` / `core.test` assertion type literals
- catalog `expected_category_counts`, `expected_sources`, the `case_count` assertion, and
  the category-count message (24 -> 25)

## 9. Deliberate non-changes

- No release regeneration. `MANIFEST.json`, `VALIDATION_REPORT.txt`, `SHA256SUMS.txt`, the
  release ZIP, and the external release validation are untouched.
- No `BARE_SPECIFICATION_COMPLETE`, `VERSION.txt`, README release-status, or
  change-control edit. `README.txt` changed only its catalog count.
- No commit, push, stage, tag, PR, or release. All work is left in the working tree.
- No successor task started.

## 10. Adjacent surface found and deliberately left

`10_REGISTRIES/operators_and_functions_v0.1.0.json` writes operator and function
signatures in a *related but distinct* notation, using atoms this notation does not define
(`numeric`, `same_unit_MEASURE_collection`, `same_family_nonnegative`,
`promoted_member_family`). The approved finding scoped this task to operation/result
registry `type` strings, so that surface is **not** governed by
`contract_type_notation` and was not touched. The notation's `applies_to` says so
explicitly rather than leaving the boundary implied. Closing that second notation is a
candidate for a later task and is offered to the owner as a scope decision, not assumed.

Two prose occurrences of `BOOLEAN_EXPRESSION` inside registry `invocation_resolution`
text, and the `TYPE_EXPRESSION` references in `type_or_format_base` and in the new
paragraph, were deliberately left uppercase: they name the EBNF nonterminal or read as
prose, and `applies_to` scopes the notation to type strings only.

## 11. Preservation

- `/mnt/F/LCL/LCL_Core_0_1_Final`: unchanged, 152 files, fingerprint
  `7712e2e981e7140cdfde2b679b45b3a96d2b00834541ba6148946376f7d6a7d8` — identical to the
  value recorded by Task 0009.
- `/mnt/F/LCL_Completion_Task_Package_v2.0`: unchanged, 20/20 checksum records verify.
- Earlier task reports under `reports/tasks/`: unmodified. `releases/`: unmodified.

## 12. Next permitted task

**LCL-TASK-0011 — Expression-valued parameter contract** (audit finding 1), which now has
a closed notation in which to express the embedded-expression contract for
`core.calculate.expression` and the five sibling parameters (`core.select.predicate`,
`core.filter.predicate`, `core.sort.key`, `core.group.key`, `core.analyze.method`). It
requires owner approval before any file is modified.
