# LCL Core 0.3 — Task 01: Multi-File Project Model

Governing pack: `/mnt/F/LCL_Core_0.3_Task_Pack/`, including
`tasks/TASK_01_MULTI_FILE_PROJECT_MODEL.md`, `GLOBAL_RULES.md` and
`ACCEPTANCE_GATES.md` (gates 0–3).

## 1. Starting HEAD and Git state

| When | HEAD | State |
| --- | --- | --- |
| Task start (2026-09-26) | `92cd1c6` = `origin/main` | Clean `main`. Core 0.1.0 identity `00d648b1…67ed` (176 files); Core 0.2.0 identity `061a79c9…8b3f` (216 files). |
| Phase B start (2026-09-27) | `1b55445` = `origin/main` | The owner committed and pushed phase A (canon only) as `1b55445`, "LCL core 0.3 task 1 part 1". The implementation was uncommitted in the working tree. |

## 2. Canonical decisions

The owner made four decisions on 2026-09-26, choosing the recommended option each time:

1. **One shared project namespace.** A project is one specification split
   over the files the entry lists. A plain `REF(x)` works across files. A
   duplicate ID across files is `error.id.duplicate`, reported at the later
   declaration in project source order. Check selection is project-wide.
   `IMPORT` keeps its mandatory namespace.
2. **Roles are document kinds.** The entry has kind `kind.project`. Parts have
   kinds `kind.part.{task,description,rules,context,data,output,checks,definitions}`.
   The entry lists its parts in `PART` blocks with fields `ID`,
   `SOURCE: PATH("relative")`, `KIND` and `REQUIRED` (optional, default
   `TRUE`). A library stays an `IMPORT`; it is not a role.
3. **Four new resolution-stage errors.** They are `error.project.part_missing`,
   `error.project.part_kind`, `error.project.part_duplicate` and
   `error.project.placement`. `error.version.mismatch` is widened to cover a
   part whose `SPECIFICATION VERSION` differs from the entry's.
4. **Unregistered KIND is fixed in every Core version.** It is now
   `error.field.type` for `SPECIFICATION.KIND`, and in 0.3.0 for `PART.KIND`.
   The same gap in `FORMAT`/`MODE`/`ENCODING`/`DEFINE.KIND` is reported, not
   fixed (see §7). Canon requires rejection but names no dedicated diagnostic.
   `error.field.type`, "a field value does not match its exact value kind", is
   the only registered identifier whose meaning covers the case. An earlier
   statement that canon *requires* `error.field.type` overstated this.

The following rules are derived from those decisions and stated in canon:
- The entry is identified by its content (`kind.project`), not by file name.
- `PART`, `IMPORT`, `EXTENSION` and `EXECUTE` appear only in the entry.
- Load order is the entry, then parts in `PART` order, then imports.
- Every unit declares `LCL VERSION "0.3.0"`.
- Localization is chosen per unit.
- Parts carry local authority, with no import ceiling.
- Admission requires steps 1–9 to pass for the whole project, with no partial
  execution.
- Standalone documents keep their 0.2 meaning.

Phase A found and settled two canon clarifications:
- A `kind.project` document named by `PART` is `placement`, never `part_kind`;
  the two are exclusive.
- The project namespace is resolved only once it is complete, so a missing part
  never cascades into unresolved-`REF` diagnostics.

## 3. Files changed

**Canon (committed by the owner as `1b55445`).** `canonical/LCL_Core_0.3.0/`
has 291 files. It was built from 0.2.0 by a scripted copy and relabel with a
reversal proof, followed by deliberate edits:
- Registries: the `PART` keyword, block and signatures; `part_source_path`;
  the `part_kind` domain; `project_part_kinds`; 9 document kinds; the 4 errors;
  and `block_schemas_v0.3.0.json#/project_contract`.
- EBNF, and a new `05_SEMANTICS/13_PROJECTS_PARTS_AND_PROJECT_ADMISSION.txt`.
- 22 new catalog cases (830 in total) and 74 witnesses.
- `09_CONFORMANCE/PROJECT_FIXTURES/`: 26 projects in 72 files, with
  `expected_results.json`.
- A new `TOOLS/validate_projects.py` with 6 negative controls.
- Re-derived validator pins.

Core 0.1.0 and 0.2.0 are untouched.

**Implementation (uncommitted):** 28 files modified and 6 new, 1230 insertions and 143 deletions.

| Area | Files | Change |
| --- | --- | --- |
| Package authority | `lcl-spec/src/{lib,anchor}.rs` | 0.3.0 registry and catalog file names. New `APPROVED_PACKAGE_0_3_0` (291 files, `bf66e369…04aa12`). The 0.1.0 and 0.2.0 anchors are unchanged. |
| Stages | `lcl-diagnostics/src/lib.rs` | 0.3.0 uses the 0.2.0 stage order. |
| Localization | `lcl-localization/src/lib.rs` | A contract loads from 0.2.0 or 0.3.0 and accepts only its own version's profiles. Selection records now carry the contract's language version; this is **defect 2** below. |
| Grammar | `lcl-parser/src/{grammar,schema}.rs` | Closed `document_kind` and `part_kind` domains, loaded from the registry and checked at grammar stage in every version. The `part_source_path` form. `EXECUTE` is required for `kind.project`. |
| Resolution | `lcl-resolver/src/{project.rs (new),lib,imports,diagnostic,rules,source,references}.rs` | Part loading in `PART` order. The 4 project errors. Version checks. One shared namespace, resolved only when complete. Project-rank diagnostic ordering. Placement checks for parts and projects used as root or import. "Absent" is distinguished from other load failures. |
| Checker | `lcl-checker/src/declarations.rs` | A relative `PATH` is legal for value kind `part_source_path`; this is **defect 1** below. |
| Preflight and completion | `lcl-semantics/src/{authority,validate}.rs`, `lcl-completion/src/{check,success}.rs` | Parts are root documents: local authority and project-wide check selection. Tie-breaks follow project source order. |
| Projects and CLI | `lcl-project/src/{lib,manifest,provider}.rs`, `lcl-cli/src/{args,main}.rs` | Manifest key `project_spec`, option `--project-spec` and variable `LCL_PROJECT_SPEC`. The provider reports NotFound as absent. |
| Protocol | `lcl-protocol/src/{engine,record,lib}.rs` | `Engine::open_project`, and `Engines::with_project` for version-based dispatch. The `project` record carries entry, order, parts, completeness and admission. Per-file `status` is an addition; see §7. |
| Conformance | `lcl-conformance/src/source_cases.rs` | Generated `SPECIFICATION.KIND` form probes follow decision 4; see §5. |
| Tests | `lcl-protocol/tests/{project_engine,document_kinds}.rs`, `lcl-cli/tests/core_0_3_projects.rs`, `lcl-spec/tests/package_0_3_0.rs`, `lcl-localization/tests/contract_0_3_0.rs` (all new), `lcl-resolver/tests/rules_authority.rs` | See §5. |
| Docs | `impl/README.md` | Core 0.3.0 rows and a "multi-file projects" section. |

No project semantics are in Workspace JavaScript or Android Kotlin; neither was touched.

**Defects found and fixed in phase B, each red first:**
1. **Relative `PART.SOURCE` rejected.** The checker allowed a one-STRING
   relative `PATH` only in `IMPORT` and `EXTENSION`, so every valid project was
   rejected with a false `error.literal.invalid`.
   - Red: the CLI fixture harness matched 20/26, and `project_engine` gave
     "4 passed; 2 failed".
   - Green after the fix: 26/26, and 6/6.
2. **Wrong `lcl_version` under 0.3.0.** Locale records reported
   `lcl_version: "0.2.0"` for units judged under the 0.3.0 contract.
   - Red: `contract_0_3_0::every_selection_records_its_contracts_language_version`
     and `project_engine::a_localized_project_records_the_0_3_0_language_version`
     failed (log `C03T1-B4-targeted-red.log`).
   - Green: gate phase a.

## 4. Compatibility proof

- **Identities unchanged.** Core 0.1.0 and 0.2.0 identities were recomputed
  externally and equal their anchors; gate phase c runs `identity-*` and
  `anchors`.
- **Nothing protected changed.** `git status` shows no change under
  `canonical/`, `releases/` or `assets/`; gate phase c runs `protected`.
- **Standalone documents are unaffected by 0.3.0.** A standalone 0.1.0 document
  and a localized 0.2.0 document produce byte-identical `--machine` output with
  and without the 0.3.0 package attached
  (`core_0_3_projects::standalone_0_1_and_0_2_documents_are_unchanged_by_the_0_3_package`).
  The whole pre-existing suite passes; see §5.
- **Dispatch without 0.3.0 is unchanged.** When no 0.3.0 engine is attached,
  0.1.0/0.2.0 routing is identical to before, `Engines::new` is unchanged, and
  JSON for non-project documents gains no field.
- **One deliberate, owner-approved meaning change: unregistered `KIND`.**
  - Before: the installed 2026-09-25 candidate accepted `KIND: kind.bogus` in a
    0.1.0 document (exit 0).
  - After: this build rejects it with `error.field.type` at 8:11 (exit 1). Log
    `C03T1-B3-kind-defect-before-after.log`.
- **Tests updated because the change is legitimate, not weakened:**
  - `rules_authority`: the 0.1.0 assertions still require exactly the 14 core
    resolution errors. A new test requires all 18 to equal the 0.3.0
    registry's resolution stage. The emitted-set count went from 12 to 16, and
    the 4 project errors are asserted emitted.
  - The generated `source/field/SPECIFICATION/KIND/form/qualified` probe now
    uses a registered kind, so it still proves that form passes.
  - `…/form/simple` now expects `error.field.type`, because no registered
    document kind has the simple form.
  - Everything else in the suite is unchanged.

## 5. Tests and exit statuses

The required tests map to the implementation tests as follows. "Fixture" means
the canonical `PROJECT_FIXTURES` case, run through the engine
(`project_engine::every_canonical_project_fixture_behaves_as_the_package_states`,
26/26) and through the real `lcl` binary
(`/mnt/F/.lcl-pretest/c03/t1/cli_fixtures.py`: 26/26, exit 0, log
`C03T1-B1-cli-fixtures.log`).

| # | Required test | Evidence |
| --- | --- | --- |
| 1 | Standalone Core 0.1 works | `standalone_0_1_and_0_2_documents_are_unchanged_by_the_0_3_package`, and the full existing suite |
| 2 | Core 0.2 localized document works | Same test, plus the existing `localized_engine` and `localized_cli` |
| 3 | Valid 0.3 multi-file project | Fixtures `valid_minimal` and `valid_all_roles`; `an_admitted_project_has_every_file_ready_in_part_order` |
| 4 | Missing required file blocks | Fixture `missing_required_part`; `a_missing_required_part_is_missing_and_blocks_the_project` |
| 5 | Optional missing file allowed | Fixture `valid_optional_absent` |
| 6 | Unreadable required file blocks | `an_unreadable_part_blocks_the_project_whether_required_or_optional` (mode 000; unreadable optional ≠ absent) |
| 7 | Invalid UTF-8 blocks | Fixture `invalid_utf8_part` |
| 8 | Malformed file blocks | Fixture `malformed_part`; the `malformed` case of `one_invalid_part_prevents_every_effect` |
| 9 | Role violation blocks | Fixtures `role_violation`, `part_with_execute`, `part_with_import` |
| 10 | Unknown role rejected | Fixtures `unknown_part_kind`, `unknown_document_kind`, `part_standalone_kind`; `document_kinds` (4 tests, versions 0.1/0.2/0.3) |
| 11 | Duplicate definition rejected deterministically | Fixtures `duplicate_id_across_parts`, `part_duplicate` |
| 12 | Cross-file REF success | Fixture `valid_all_roles` |
| 13 | Unresolved cross-file REF rejected | Fixture `unresolved_cross_file_reference` |
| 14 | Dependency cycle rejected | Fixtures `import_cycle`, `reference_cycle_across_parts` |
| 15 | Load order independent of filesystem order | `load_order_is_part_order_whatever_the_filesystem_order`: opposite creation orders give byte-identical output, and unlisted files are never read |
| 16 | Core version mismatch rejected | Fixtures `part_lcl_version`, `part_specification_version`, `part_as_root`, `nested_project`, `part_imported` |
| 17 | Localized multi-file project | Fixture `valid_localized_part`; `a_localized_project_records_the_0_3_0_language_version`; `contract_0_3_0` (3 tests) |
| 18 | One invalid required file prevents all execution | `one_invalid_part_prevents_every_effect` (malformed, role and missing cases; the write is granted, and no file appears) |
| 19 | No effects before admission | Same test, plus the control `an_admitted_project_performs_its_effect` |
| 20 | Deterministic, reproducible project graph | `project_reports_are_byte_identical_across_engines`; test 15 |

**Gate:** `/mnt/F/.lcl-pretest/c03/t1/c03t1-gate.sh` is a copy of the CI1 gate
with 3 anchor pins and the Core 0.3.0 checks. Every command's exit status is
recorded in `c03t1-results.tsv` and compared with its expected status. Logs are
`/mnt/F/.lcl-pretest/logs/C03T1-G-*.log`.

GATE RESULTS: PENDING — the gate was still running when this draft was written.

## 6. Integrity identity

- Core 0.3.0 identity: `bf66e36902dbef480db75772494d384847220959d76088dbe102b8ebff04aa12`, over 291 files.
- `MANIFEST.json` SHA-256: `cdac90a6a1d871909f23604ec02d49860b0b2393badcbeed2e59dfe386905e53`.
- It was generated with the package's own `generate_integrity.py` and
  recomputed externally with `/mnt/F/.lcl-pretest/p3/identity.py`.
- It is pinned only in the new `APPROVED_PACKAGE_0_3_0`. The 0.1.0 and 0.2.0
  anchors are unchanged.
- Status: `UNRELEASED_CANDIDATE`. `independent_review` is pending and
  `release_gate_permitted` is false, so `validate_release` correctly reports 1
  BLOCKED.
- **Core 0.3.0 is not released and not marked released.**

## 7. Known limitations and findings

- **Per-file `status` is an unrequested addition.** The project record's
  `entry_status` and each part's `status` and `source_span` were added in this
  session after I misread the owner's "usage rules" as the pack's usage
  contract; the owner meant the AI usage budget. The addition is small, purely
  additive (protocol `lcl.engine/1`), computed by the engine, and tested. The
  owner may ask for it to be removed.
- **Locale profiles are per language version.** A profile names one
  `lcl_version`, and the command line takes one profile set. Supplying a 0.2.0
  profile for a 0.3.0 document, or the reverse, is
  `error.localization.profile_invalid`, which fails closed.
- **No hint when the 0.3.0 package is not named.** A 0.3.0 project validated
  that way is rejected by the 0.2.0 engine with `error.keyword.unknown`
  (`PART`). It fails closed but gives no hint to name the package.
- **Workspace and Android** are not project-aware yet (Task 03).
- **Reported, not fixed (decision 4):** `FORMAT`, `MODE`, `ENCODING` and
  `DEFINE.KIND` still accept unregistered qualified identifiers.
- **Pre-existing canon defects, reported and not fixed in 0.1/0.2:**
  - `04_GRAMMAR/13` states the counts 335/67, while the registry has 334/68.
    Corrected in 0.3.0 as 339/70.
  - `validate_source_fixtures.py` returns the unregistered
    `error.block.forbidden`. Corrected in 0.3.0 to `error.block.context`.
- **Pre-existing quirk:** in 0.1/0.2, early diagnostics across imported units
  are not stage-sorted. Projects are.
- **HawkScan was not run.** It requires hosted-scan authorization the work
  orders do not give, and `HAWK_API_KEY` is unset.

## 8. Final Git state

PENDING — recorded after the gate.

## 9. Ready to commit

PENDING — decided after the gate. Nothing was committed, pushed, branched or stashed.
