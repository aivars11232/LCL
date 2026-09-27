# LCL Core 0.3 — Task 02: Role Scaffolds and Master Templates

Governing pack: `/mnt/F/LCL_Core_0.3_Task_Pack/`, including
`tasks/TASK_02_ROLE_TEMPLATES_AND_MASTERS.md`, `contracts/TEMPLATE_CONTRACT.md`,
`contracts/ARCHITECTURE_CONTRACT.md`, `GLOBAL_RULES.md` and `ACCEPTANCE_GATES.md`
(gate 4, with gates 0–3 and 8 rerun).

Result: **LCL_CORE_0_3_TASK_02_COMPLETE** (section 7).

## 1. Starting state

- Branch `main` at `865bef9` ("LCL core 0.3 task 1 part2"), clean, equal to
  `origin/main`. Task 01 was accepted by the owner: canon committed as
  `1b55445`, implementation as `865bef9`.
- During this task the session was interrupted by the usage limit. The owner
  then committed the engine work done so far as `75330e6` ("LCL core 0.3 task
  2 part1": `Engine::grammar`, `Engine::locale_profile` and
  `lcl-protocol/src/scaffold.rs`, not yet compiled into the crate) and pushed
  it. Everything else in this report is uncommitted.
- Scope: Task 02 only. That means role scaffolds, Minimal/Guided, Masters,
  project Masters, validation/default/priority, exact preview and
  all-or-nothing creation, and the required manual chapter. No Task 03 work.

## 2. Decisions

The Task 01 role contract was sufficient; no owner decision was needed and
no stop condition applies (section 8).

1. **Source of truth.** The roles are the canonical `part_kind` domain
   (`Grammar::closed_domain_members`). The blocks a role may hold are the
   canonical `document_kind_blocks`, and a block's fields and requiredness come
   from the canonical block schemas, all read from the engine's loaded Core
   0.3.0 package. Minimal scaffolds are derived from these alone. Guided
   scaffolds add a small presentation table in `scaffold.rs`, which automated
   parity tests hold to the canon (TEMPLATE_CONTRACT model 2). No canonical
   file changed. Scaffolds are creation aids, not language rules, so Core 0.3.0
   keeps its committed identity.
2. **Placeholders.** LCL has no comment syntax (`02_LEXICAL/02`: "There is no
   ignorable comment syntax"). A **slot** is therefore a field line with
   nothing after its colon (`    NAME:`). The lexer rejects it with
   `error.indentation.empty_block`, so a file with an unfilled slot cannot be
   checked, validated or run, and no placeholder value ever sits in a semantic
   field. **Guidance** is a `COMMENT` block, which "has no operational effect"
   (`02_LEXICAL/12`), and appears in Guided mode only. **Generated IDs** are
   `<block>.<stem>`, where the stem is the file name without `.lcl`/`.lcl.txt`
   and must match `[a-z][a-z0-9_]*`; any other name is refused, not repaired.
   Every scaffold also carries machine-readable **marks** (`required_slot`,
   `optional_slot`, `guidance`, `generated_id`, one-based line, canonical block
   and field). Every unmarked line is real source. No new keyword was added.
3. **Minimal** is `LCL` + `SPECIFICATION` (generated `ID`; `NAME` and
   `VERSION` required slots; `KIND` = role). The canon requires no body block
   of any part kind. The project entry also gets one `PART` per planned file
   and the `EXECUTE` that `kind.project` requires (`REFERENCE` slot).
4. **Guided** is Minimal plus an optional `DESCRIPTION` slot, a guidance
   `COMMENT` listing the role's canonical legal blocks, and the role's common
   sections (section 3).
5. **Canonical project plans** (used when no project Master is chosen):
   Minimal = `main.lcl` + `task.lcl`, since the entry's required `EXECUTE`
   needs something to name. Guided = `main.lcl` + `description.lcl` +
   `rules.lcl` + `task.lcl`, the files of the Task 03 beginner flow. No other
   files are created.
6. **Localization.** A scaffold is spelled through the engine's validated
   locale profile. If that profile cannot spell a keyword the scaffold needs,
   the scaffold is refused and the missing words are named, because one file
   cannot mix spellings (`error.localization.mixed`). All four canonical
   fixture profiles are partial (60 words). Every Minimal part scaffold works
   in all four. Guided part scaffolds and project entries are refused under
   them (for example, lv-LV has no `COMMENT`, `CONTENT`, `DESCRIPTION` or
   `PART`).
7. **Master format** (JSON, `format` 1, closed key sets).
   - Role Master: `{format, id, name, core, role, text}`.
   - Project Master: `{format, id, name, core, role: "kind.project", mode,
     entry, parts: [{path, role, required?, master?}]}`.
   - `id` is `[a-z0-9][a-z0-9_-]*`, at most 64 bytes, and is also the file name.
8. **Master validation** (`check_master` / `check_text`), in order:
   - `core` must equal the engine's formal version; a Master is never
     reinterpreted under another version.
   - `role` must be a project file role.
   - The text must end with LF and be at most 1 MiB.
   - Every slot must be a registered field, taking an inline value, of a
     top-level block. `LCL VERSION` and `SPECIFICATION KIND` cannot be slots.
     The number of slots found must equal the number of the lexer's
     `empty_block` diagnostics.
   - The text with every slot filled by a form-correct probe must pass the
     localization, lexical and grammar-or-schema stages with zero diagnostics.
     This covers legal blocks for the role, unknown fields, required fields
     and conditional requirements.
   - The text must declare exactly this `LCL VERSION` and `SPECIFICATION KIND`
     = `role`.
   - A project Master's plan must pass `check_plan`: plain relative paths with
     an LCL ending; no duplicate paths; distinct stems, so generated IDs cannot
     collide; the entry in the project folder itself; part roles only.
   - Every part Master a project Master names must exist, be a role Master of
     that part's role, and be valid itself.
9. **Storage.** Masters live in `$XDG_CONFIG_HOME/lcl/masters/`, or
   `~/.config/lcl/masters/`, next to the workspace settings file, one
   `<id>.json` each. `defaults.json` is
   `{"format": 1, "defaults": {"<role>": "<id>"}}`.
   - Writes are atomic, through the settings module's temp-file-and-rename
     writer, which was extracted as `settings::write_atomically`.
   - `save` refuses an invalid Master.
   - `set_default` requires a valid Master of exactly that role.
   - `delete` also removes the Master from the defaults.
10. **Priority.** `Selection::Master(id)` first, then the role's default
    Master, then the canonical scaffold (`Selection::Automatic(mode)`).
    `Selection::Canonical(mode)` is an explicit choice too. A selected or
    default Master that is missing or invalid is an error, never a silent
    fallback.
11. **Copy, preview, create.**
    - A file made from a Master receives the Master's exact text, and nothing
      records which Master it came from.
    - `Masters::project` returns every file with its exact bytes. That is the
      preview; nothing is written.
    - `masters::create` checks every path first and refuses any existing
      file. It creates the needed folders and writes each file with the
      workspace's atomic no-overwrite `document::create`. On any failure it
      removes the files (digest-checked delete) and the folders it made.

## 3. Role/scaffold mapping

Header (both modes): `LCL: VERSION: "0.3.0"`, then `SPECIFICATION` with
`ID: specification.<stem>` (generated), `NAME:` and `VERSION:` (required
slots) and `KIND:` the role. Guided adds `DESCRIPTION:` (optional slot) and the
guidance `COMMENT`.

| Role | Minimal | Guided sections (`ID` fields generated) |
|---|---|---|
| `kind.part.task` | header | `GOAL` (`ASSERT` slot), `ACTION` (`OPERATION` slot), `SUCCESS` (`ALL` slot), `TASK` (`GOAL`/`ACTION`/`SUCCESS` = `REF` to those generated blocks) |
| `kind.part.description` | header | `COMMENT` (`CONTENT` slot) |
| `kind.part.rules` | header | `REQUIRE` (`ASSERT` slot), `FORBID` (`OPERATION`, `TARGET` slots) |
| `kind.part.context` | header | `CONTEXT` (`TYPE`, `SCOPE`, `VALUE` slots) |
| `kind.part.data` | header | `INPUT` (`TYPE`, `VALUE` slots), `DATA` (`TYPE`, `VALUE` slots) |
| `kind.part.output` | header | `OUTPUT` (`TYPE`, `FORMAT` slots) |
| `kind.part.checks` | header | `VALIDATE` (`ASSERT` slot), `VERIFY` (`ASSERT` slot) |
| `kind.part.definitions` | header | `DEFINE` (`KIND` slot, `MEANING` optional slot) |
| `kind.project` (entry, from a plan) | header, `PART` per planned file (`REQUIRED: FALSE` when optional), `EXECUTE` (`REFERENCE` slot) | same + description slot and guidance |

Where a block's conditional requirement needs one of a choice, the table picks
one alternative and marks it required, for example `ASSERT` for `GOAL` and
`REQUIRE`, `ACTION` for `TASK`, `ALL` for `SUCCESS` and `VALUE` for
`CONTEXT`/`INPUT`.

## 4. API for Task 03

The UI requests scaffolds and Masters here; it must not rebuild them.

- `lcl_protocol::scaffold`:
  - scaffolds: `roles`, `part`, `entry`, `Plan::canonical`;
  - checks: `check_plan`, `check_path`, `stem`, `check_id`, `check_text`,
    `parse_master`, `check_master`, `guided_shape`;
  - types: `Mode`, `Mark`, `MarkKind`, `Scaffold`, `ScaffoldError`, `Master`,
    `Content`, `Plan`, `PlannedPart`;
  - engine accessors: `Engine::grammar`, `Engine::locale_profile`.
- `lcl_workspace::masters`:
  - `Masters::{new, dir, ids, read, check, save, delete, defaults,
    set_default, file, project}`;
  - `Selection`, `Origin`, `Planned`, `create`, `location`.

No HTTP route, workspace UI, remote endpoint or Android code was added; those
belong to Task 03.

## 5. Files changed

| File | Change |
|---|---|
| `impl/crates/lcl-protocol/src/engine.rs` | `grammar()`, `locale_profile()` (committed in `75330e6`; since then rustfmt line wrapping only) |
| `impl/crates/lcl-protocol/src/scaffold.rs` | new module (first version committed in `75330e6`; since then: wired in, localized spelling refusal, probe spelling, slot naming before engine diagnostics, rustfmt) |
| `impl/crates/lcl-protocol/src/lib.rs` | `pub mod scaffold;` |
| `impl/crates/lcl-protocol/Cargo.toml` | dependency `lcl-project` (reuses `SUFFIXES`, `is_document`) |
| `impl/Cargo.lock`, `remote/Cargo.lock` | one line each: that dependency |
| `impl/crates/lcl-workspace/src/masters.rs` | new: storage, defaults, priority, preview, all-or-nothing creation |
| `impl/crates/lcl-workspace/src/lib.rs` | `pub mod masters;` |
| `impl/crates/lcl-workspace/src/settings.rs` | atomic write extracted into `write_atomically`; `store` unchanged in behaviour |
| `impl/crates/lcl-protocol/tests/scaffolds.rs` | new, 20 tests |
| `impl/crates/lcl-workspace/tests/masters.rs` | new, 10 tests |
| `users_manual/18_Starting_Files_From_Templates.md` | new chapter |
| `users_manual/README.md` | table row for chapter 18 |
| `reports/implementation/LCL_CORE_0_3_TASK_02_ROLE_TEMPLATES.md` | this report |

No file under `canonical/` changed.

## 6. Tests

### Required tests

| Required test | Test |
|---|---|
| New Task / Description / Rules / Context / Data / Output / Checks produces its scaffold | `new_task_produces_task_scaffold`, `new_description_…`, `new_rules_…`, `new_context_…`, `new_data_…`, `new_output_…`, `new_checks_…` (and `new_definitions_…`): KIND, exact block sequence per mode, engine validity, slot parity |
| Minimal scaffold behaviour | `minimal_scaffold_is_only_the_required_header` (exact bytes and marks; every role) |
| Guided scaffold behaviour | `guided_scaffold_adds_common_sections_and_guidance` (exact bytes and all 15 marks) |
| Scaffold/canonical parity | `scaffolds_match_the_canonical_role_contract`: roles = canonical part kinds (8); every Guided block legal for its role; every field registered; every required field present; optional slots only on optional fields; every scaffold accepted by the engine once filled |
| Invalid Master rejected | `invalid_master_is_rejected` (11 cases: not JSON, format 2, unknown key, bad id, empty name, no final LF, unknown field slot, illegal block for role, nested slot, lexical defect, fixed-header slot); `invalid_master_is_never_stored_or_made_default` |
| Wrong-role Master rejected | `wrong_role_master_is_rejected` |
| Incompatible-version Master rejected | `incompatible_version_master_is_rejected` (`core` 0.2.0; text `LCL VERSION` 0.2.0) |
| Explicit Master overrides default | `explicit_master_overrides_default` |
| Default Master overrides canonical scaffold | `default_master_overrides_canonical_scaffold` |
| Master copied, not linked | `master_is_copied_not_linked` |
| Edit Master does not mutate existing project | `editing_master_does_not_mutate_existing_project` |
| Edit project does not mutate Master | `editing_project_does_not_mutate_master` |
| Delete Master leaves existing projects unchanged | `deleting_master_leaves_existing_projects_unchanged` |
| Same Master produces deterministic starting bytes | `same_master_produces_deterministic_starting_bytes`; `same_request_produces_deterministic_bytes` (canonical scaffolds; two separately opened engines) |
| Localized project scaffolds remain valid | `localized_project_scaffolds_remain_valid` (all four fixture locales; section 2, item 6) |

Also: `project_entry_lists_planned_parts_in_order`,
`valid_master_is_accepted_with_its_slots`,
`invalid_project_file_plan_is_rejected` (10 cases),
`project_master_names_only_valid_role_masters`,
`project_preview_is_exactly_what_is_created`,
`project_creation_is_all_or_nothing_and_never_overwrites`.

The tests were written with the features. On their first run, 2 of 20
failed on real defects of the first version, both fixed:

- A localized Guided scaffold was emitted with mixed spellings, which the
  engine rejects. It is now refused, naming the missing words.
- An unknown slot field was reported only as
  `error.localization.detection_failed`. The slot is now named precisely
  first.

### Direct Task-02 test results

| Test file | Direct run | Inside the full gate |
|---|---|---|
| `impl/crates/lcl-protocol/tests/scaffolds.rs` | 20 passed, 0 failed | 20 passed, 0 failed |
| `impl/crates/lcl-workspace/tests/masters.rs` | 10 passed, 0 failed | 10 passed, 0 failed |

### Full gate

- Runner: `/mnt/F/.lcl-pretest/c03/t2/c03t2-run.sh`. It is the Task 01 gate
  relabelled (`c03t2-gate.sh`; phases selftest, a, c, b, d, verdict) plus two
  extra steps.
- Logs: `/mnt/F/.lcl-pretest/logs/C03T2-G-*.log`. The run log
  `C03T2-G-gate-run.log` ends with `GATE-DONE`.
- Results: `/mnt/F/.lcl-pretest/c03/t2/c03t2-results.tsv` records 61 steps,
  and every actual exit status equals the expected one.
- It ran against a frozen tree: no Task-02, canonical, manual or report file
  changed after the gate's start marker (2026-09-27 21:20:22).
- A first attempt was stopped after clippy found 3 lints in the new code:
  complex types in `guided_shape` and in one test, and a manual char
  comparison. They were fixed, clippy came back clean, and the full gate was
  rerun once from the start.

| Step | Command (abridged) | Exit | Expected |
|---|---|---|---|
| selftest | the gate's own runner test | phase 0 | 0 |
| fmt | `cargo fmt --all -- --check` | 0 | 0 |
| clippy | `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | 0 |
| test-workspace | `cargo test --offline --locked --workspace --all-targets --no-fail-fast` | 0 | 0 |
| msrv-check | Rust 1.75.0 `cargo check --workspace --all-targets` | 0 | 0 |
| msrv-tests | Rust 1.75.0 `cargo test --workspace --all-targets` | 0 | 0 |
| sha-0.1.0, sha-0.2.0, sha-0.3.0 | package `SHA256SUMS` | 0 | 0 |
| validate-0.1.0 | `validate_release` | 0 | 0 |
| validate-0.2.0 | `validate_release` | 1 | 1 |
| release-state-0.2.0 | assertion on the 0.2.0 result | 0 | 0 |
| validate-0.3.0 | `validate_release` | 1 | 1 |
| release-state-0.3.0 | assertion on the 0.3.0 result | 0 | 0 |
| validate_language_contracts, validate_localization, validate_source_fixtures (0.2.0 and 0.3.0); validate_projects-0.3.0 | package validators | 0 | 0 |
| ebnf, identity (0.1.0, 0.2.0, 0.3.0); anchors; brand | grammar, identity and trust-anchor checks | 0 | 0 |
| conformance-text | `cargo run -p lcl-conformance --example m8_conformance_report` | 0 | 0 |
| readiness-gate | `cargo run -p lcl-conformance --example m8_conformance_gate` | 0 | 0 |
| protected | protected-path check | 0 | 0 |
| realproc-seq-1 to 12 | `cargo test -p lcl-capabilities --test real_process`, 12 sequential runs | 0 each | 0 |
| realproc-par (36 runs) | the same suite in parallel rounds | 0 each | 0 |
| remote-check (extra) | `cargo check --offline --locked --manifest-path remote/Cargo.toml --all-targets` | 0 | 0 |
| manual-examples (extra) | `users_manual/tools/verify_examples.py --spec canonical/LCL_Core_0.1.0` | 0 | 0 |
| verdict | recorded results | 0 | 0 |

Test counts:

- test-workspace and msrv-tests: 174 test binaries, 1926 passed, 0 failed,
  1 ignored each.
- The Task 01 gate had 172 binaries, 1896 passed, 0 failed, 1 ignored. The +2
  binaries and +30 tests are exactly the Task-02 tests.
- The ignored test is pre-existing.

Manual verifier: 97 examples checked, 42 fragments skipped, 0 failures.

### Expected non-zero statuses

- **selftest.** `exits-three` (exit 3) and `quiet-failure` (exit 1) print
  `FAIL` on purpose. They prove the runner detects failing commands, and the
  selftest phase itself exits 0.
- **validate-0.2.0, exit 1 by design.** Its only BLOCKED item is the
  unreleased-candidate release metadata, as `release-state-0.2.0` (exit 0)
  asserts.
- **validate-0.3.0, exit 1 by design.** The result is 32 PASS / 0 FAIL / 1
  BLOCKED / 2 OUT_OF_SCOPE. The BLOCKED item is
  `language_decisions_and_release_state`, pending only `independent_review`,
  as `release-state-0.3.0` (exit 0) asserts.

### Canonical preservation

- `git status -- canonical` shows nothing, and `git diff HEAD -- canonical`
  is empty.
- The identities the gate recomputed equal the Task 01 baseline, and the
  `anchors` step confirms the trust anchors equal them:

  | Core | Identity | Files | Status |
  |---|---|---|---|
  | 0.1.0 | `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed` | 176 | unchanged |
  | 0.2.0 | `061a79c92c76ed7bb05968adde24b016cae3b30666b8fb289c6ab968907e8b3f` | 216 | unchanged |
  | 0.3.0 | `bf66e36902dbef480db75772494d384847220959d76088dbe102b8ebff04aa12` | 291 | unchanged from the Task 01 canon `1b55445` |

## 7. Result

**LCL_CORE_0_3_TASK_02_COMPLETE.** Every Task-02 requirement has a passing
test (section 6). The full gate reached its expected final state and no
canonical byte changed. Core 0.3.0 remains an unreleased candidate; this task
does not change its release state.

## 8. Stop conditions

- Task 1 roles sufficiently closed: yes. There are 8 part kinds and the
  entry, frozen in `1b55445`.
- A role needing semantics not in Core 0.3: none. Scaffolds use only existing
  blocks and fields.
- Language rules hardcoded in JS/Kotlin: none. Everything is in the engine and
  project layers, and the UI will call section 4.
- Ambiguous placeholders: no. A slot is lexically rejected and marked.
- Migration or version behaviour needing the owner: no. A Master targets one
  version and is otherwise refused; no Master migration exists.

## 9. Known limitations (Task 02 only)

- With the partial canonical fixture profiles, localized Guided scaffolds and
  localized project entries are refused (section 2, item 6). A complete
  profile would allow them. Localized projects remain possible file by file.
- Master validation is structural (through grammar-or-schema). Reference
  resolution and types are checked when the created file is checked.
- A slot must be a field of a top-level block. A Master with a slot inside a
  nested block (such as `PARAMETER`) is refused.
- Master ids are the storage file names. Editing a Master's id creates a new
  Master; the old file stays until deleted.

## 10. Manual updates

- New chapter `users_manual/18_Starting_Files_From_Templates.md`. It covers
  file roles, slots, Minimal and Guided, Guided examples for Task,
  Description, Rules, Context, Data, Output, Checks and Definitions (exact
  engine output), Master templates (format, location, validation, priority,
  copy semantics), project Masters, and versions/localization.
- A tool note says it describes the Core 0.3.0 candidate and that the
  workspace menus come later.
- The README table has a new row.
- The 12 new `lcl` blocks are annotated fragments.
- `users_manual/tools/verify_examples.py`: 97 examples checked, 42 fragments
  skipped, 0 failures.

## 11. OUT_OF_SCOPE_FINDINGS

- The Task 01 report
  (`reports/implementation/LCL_CORE_0_3_TASK_01_MULTI_FILE_PROJECT_MODEL.md`)
  still shows PENDING for its gate results, final Git state and
  ready-to-commit sections. Its gate
  (`/mnt/F/.lcl-pretest/c03/t1/c03t1-results.tsv`) finished with every step
  at its expected status. Not changed. No effect on Task 02 acceptance.

## 12. DEFERRED_TO_TASK_03

- Workspace HTTP routes and remote/Android endpoints for scaffolds and
  Masters.
- Desktop New Project and New File by role, and Settings → Templates.
- Showing slot and guidance marks in the editors.
- Android role-aware creation.
- The Users Manual icon, window and tab.

## 13. Final Git state

`git status --short --branch --untracked-files=all` after the gate:

```text
## main...origin/main
 M impl/Cargo.lock
 M impl/crates/lcl-protocol/Cargo.toml
 M impl/crates/lcl-protocol/src/engine.rs
 M impl/crates/lcl-protocol/src/lib.rs
 M impl/crates/lcl-protocol/src/scaffold.rs
 M impl/crates/lcl-workspace/src/lib.rs
 M impl/crates/lcl-workspace/src/settings.rs
 M remote/Cargo.lock
 M users_manual/README.md
?? impl/crates/lcl-protocol/tests/scaffolds.rs
?? impl/crates/lcl-workspace/src/masters.rs
?? impl/crates/lcl-workspace/tests/masters.rs
?? reports/implementation/LCL_CORE_0_3_TASK_02_ROLE_TEMPLATES.md
?? users_manual/18_Starting_Files_From_Templates.md
```

- HEAD is `75330e6`, equal to `origin/main`, and nothing is staged.
- Only Task-02 files and this report appear.
- There are no build products, APK/AAB files, keystores, secrets or temporary
  files, and no canonical drift. Build output and gate logs are outside the
  repository.

## 14. Commit state

- Committed: **NO**. `75330e6` (Task 02 part 1) was committed and pushed by
  the owner.
- Pushed: **NO**.
- Ready to commit: **YES**.
