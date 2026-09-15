# LCL-REPAIR-03 — Result

## Identity

- Task: LCL-REPAIR-03, localization profiles and engine dispatch (B-10, B-11)
- Repository: `/mnt/F/LCL`
- Branch: `main`
- Entry HEAD: `94dc07daa3cd842e75e00d8bcce3006d613fb4aa` ("LCL repair task2"). The owner committed REPAIR-02, so the predecessor's files were in their stated state.
- Exit HEAD: unchanged
- Entry worktree: clean
- Exit worktree: 8 modified tracked files (listed below), plus this report. `impl/target` is ignored, and no untracked file was created in it.
- Pack manifest: all 21 `MANIFEST_SHA256.md` hashes verified.
- Git writes performed: **NO**
- Background/sub/parallel agents used: **NO**
- Evidence logs: `/mnt/F/.lcl-repair-6t/logs/R3-*`
- Scratch: `/mnt/F/.lcl-repair-6t/tmp`

## Result

**Status:** PASS WITH OUT-OF-SCOPE FINDINGS

## Scoped findings

| Finding | Reproduction | Repair | Verification | Disposition |
|---|---|---|---|---|
| B-11: malformed-source engine attribution | New dispatch matrix `the_dispatch_matrix_attributes_each_document_to_one_package`. RED (`R3-B11-RED.log`): a canonical-English document declaring `VERSION "0.2.0"` with a tab, or a typographic quote, **after** the header was judged by Core 0.1.0 and reported the 0.1.0 package identity. The cause: a lexical failure leaves no parse tree, so `engine_for` read no `VERSION`. A grammar defect after the header was already attributed correctly, because the parser recovers the `LCL` block. | `Engines::engine_for` now also reads the declared `VERSION` from the token prefix when lexing rejected the unit. The reader `lexed_lcl_version` is private to `engine.rs`. The header line is `LCL : NEWLINE INDENT VERSION : SPACE STRING NEWLINE`, the grammar's `LCL_HEADER`, and `DOCUMENT` begins with it. It counts only when the whole line ends before the first lexical defect. | GREEN (`R3-B11-GREEN.log`, 5/5). All `lcl-protocol` tests, including `v07_matrix`, pass (`R3-B11-protocol.log`, exit 0). Clippy passes with `-D warnings`. | FIXED_VERIFIED |
| B-10: localized workspace/profile provisioning gap | RED (`R3-B10-RED.log`): `lcl check --localized-spec … --profile lv-LV.json` accepts `explicit_lv` under 0.2.0, and without the profile it reports `error.localization.profile_unavailable`. `lcl-workspace --profile …` failed with `unknown option --profile`, exit 1. | `lcl-workspace` gains a repeatable `--profile <FILE>`. `Workspace::open_with_profiles` is additive: `open_with` delegates to it, and no signature changed. Profile order is the CLI's: the manifest `profiles` directory, then each `--profile` file, a later file for a locale replacing an earlier one. The packaging README states the installed path: the launcher passes the 0.2.0 package and no profile, and a localized document takes its profiles from its project's declared `profiles` directory. | Workspace test `a_profile_file_serves_a_project_without_a_profile_directory`. Binary check (`R3-B10-GREEN-bin.log`): the option is accepted, and a missing profile file is refused with exit 1. Installed-launcher test `a_localized_document_uses_the_profiles_its_project_declares`: a real install of a payload carrying 0.2.0 refuses the document without `profiles` and selects lv-LV with it (`R3-B10-launcher.log`). | FIXED_VERIFIED |

### Dispatch matrix (11 rows, all asserting formal version, report package identity, and primary stage/id)

| Case | Engines | Expected |
|---|---|---|
| valid 0.1.0 (`valid_minimum`) | with profiles | 0.1.0, no diagnostic |
| invalid 0.1.0 (`invalid_tab`) | with profiles | 0.1.0, lexical `error.source.tab` |
| valid 0.2.0 canonical English | with profiles | 0.2.0, no diagnostic |
| valid 0.2.0 localized (`auto_lv`) | with profiles | 0.2.0, no diagnostic |
| explicit-locale localization failure | with profiles | 0.2.0, localization `profile_unavailable` |
| 0.2.0 English, tab after `VERSION` | with profiles | **0.2.0**, lexical `error.source.tab` (was 0.1.0) |
| 0.2.0 English, typographic quote after `VERSION` | with profiles | **0.2.0**, lexical `non_ascii_outside_string` (was 0.1.0) |
| 0.2.0 English, grammar defect after `VERSION` | with profiles | 0.2.0, grammar `error.field.duplicate` |
| `VERSION` string unclosed | with profiles | 0.1.0, lexical `error.literal.unclosed` |
| 0.2.0 English, tab **before** `VERSION` | with profiles | 0.1.0, lexical `error.source.tab` |
| localized with directive, profile unavailable | no profiles | 0.2.0, localization `profile_unavailable` (D9) |
| localized without directive, no profile | no profiles | 0.1.0, lexical `error.keyword.unknown` (D9) |

### Attribution rule applied, and its authority

- `02_LEXICAL/13` applies its rules "only to a document whose LCL block declares VERSION "0.2.0"".
- `07_VERSIONING_AND_EXTENSIONS/01` keeps every other document under Core 0.1.0.
- D9 governs localization failures.
- A lexically rejected document "declares" `0.2.0` only when its whole header line precedes the first lexical defect, because `01_FOUNDATION/03` says "No later stage repairs an earlier invalid stage by guessing intent". A defect inside or before the header line therefore leaves the document with no declaration, and it stays with Core 0.1.0.
- This defines only when a declaration that canon already requires is readable; it adds no language rule.
- The owner may still prefer another boundary for the pre-version rows. Changing it would change only those two matrix rows.

## Files changed

| File | Why | Minimal change |
|---|---|---|
| `impl/crates/lcl-protocol/src/engine.rs` | Owns dispatch (B-11) | `engine_for` falls back to `lexed_lcl_version`, a new private function of about 40 lines, plus a `TokenKind` import and a doc line |
| `impl/crates/lcl-protocol/tests/dispatch.rs` | Direct regression | The dispatch matrix test |
| `impl/crates/lcl-workspace/src/project.rs` | Owns workspace engine assembly (B-10) | Additive `open_with_profiles`; profile files appended after the manifest directory |
| `impl/crates/lcl-workspace/src/main.rs` | Workspace CLI (B-10) | `--profile` parsing, usage text, call |
| `impl/crates/lcl-workspace/tests/localized.rs` | Direct regression | One test: with the profile file accepted, without it refused |
| `impl/crates/lcl-hardening/tests/installed_launcher.rs` | Installed-workspace evidence (B-10) | `get` delegates to a new `request(url, method, path, body)`; one installed 0.2.0 test |
| `packaging/README.md` | The user's provisioning path | One paragraph: profiles come from the project's `profiles`, or from `--profile` in a terminal |
| `impl/README.md` | Crate table accuracy | `lcl-workspace` row names `--localized-spec` and `--profile` |

## Evidence

| Gate | Command/Test | Exit | Result |
|---|---|---:|---|
| B-11 RED | `cargo test -p lcl-protocol --test dispatch` (`R3-B11-RED.log`) | 101 | 2 attribution rows wrong, as predicted; 1 wrong oracle id of mine corrected |
| B-11 GREEN | same (`R3-B11-GREEN.log`) | 0 | 5 passed |
| Protocol regression | `cargo test -p lcl-protocol --no-fail-fast` (`R3-B11-protocol.log`) | 0 | 82 passed |
| Protocol lint | `cargo clippy -p lcl-protocol --all-targets -- -D warnings` | 0 | clean |
| B-10 RED | built binaries, `lcl check` with and without `--profile`, and `lcl-workspace --profile` (`R3-B10-RED.log`) | 0 / 1 / 1 | CLI accepts with a profile and refuses without; workspace rejects the option |
| B-10 GREEN | `cargo test -p lcl-workspace --test localized` (`R3-B10-GREEN.log`) | 0 | 4 passed |
| B-10 binary | `lcl-workspace --profile` with an existing and a missing file (`R3-B10-GREEN-bin.log`) | 124 (timeout while serving) / 1 | serves; refuses the missing file |
| Installed launcher | `cargo test -p lcl-hardening --test installed_launcher -- a_localized_document…` (`R3-B10-launcher.log`) | 0 | 1 passed |
| Workspace and hardening lint | clippy `-D warnings` for `lcl-workspace` and `lcl-hardening` | 0 | clean |
| Integration: CLI (0.1 and localized) | `cargo test -p lcl-cli --no-fail-fast` (`R3-INT-lcl-cli.log`) | 0 | 63 passed |
| Integration: workspace (incl. CLI equivalence) | `cargo test -p lcl-workspace --no-fail-fast` (`R3-INT-lcl-workspace.log`) | 0 | 111 passed |
| Integration: packaging | `cargo test -p lcl-hardening --test installed_launcher --test release_build` (`R3-INT-hardening.log`) | 0 | 31 passed |
| Formatting | `cargo fmt --all -- --check` (`R3-INT-fmt.log`) | 0 | no diff |
| Scope | `git status --short --untracked-files=all`, `git diff --stat` | 0 | only the 8 files above |

The background gate command reported exit 1 because its last step, `grep -c 'Diff in'`, counted zero fmt diffs. Every gate inside it exited 0.

## Protected-material check

- Core 0.1.0: `SHA256SUMS.txt` verifies (exit 0). `lcl spec` identity is `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, the pinned value. No git change.
- Core 0.2.0: `SHA256SUMS.txt` verifies (exit 0). Report identity is `e86121c734cb51065b791329738b9ed6b77c53e03e0eebbb2ad1076bea973ef0`, the pinned value. No git change.
- Existing candidates: tarball and source `.sha256` files verify for all three candidates, and `releases/` has no git change. See OOS-3 for one manifest that cannot be checked in place.
- Historical evidence: untouched. No prior result file was edited.

## Out-of-scope findings

- **OOS-1: the CLI and the workspace find a document's project differently.**
  - `lcl` (`impl/crates/lcl-cli/src/main.rs`, `open_project`) takes the document's own directory, with no upward walk.
  - `lcl-workspace --document` (`impl/crates/lcl-workspace/src/project.rs`, `locate_document`) takes the nearest ancestor holding `lcl.project.json`.
  - With the same explicit project root, both now judge a document the same way. With only a document path, their manifests, and so their `profiles`, can differ.
  - This predates the task and is a path-semantics decision. It is related to REPAIR-02 OOS-1.
- **OOS-2: `--profile` is silently ignored when no Core 0.2.0 package is named, in both `lcl` and `lcl-workspace`.** The behaviour is consistent between them but not fail-closed (G-17). Refusing it would change the CLI's established interface.
- **OOS-3: `releases/candidates/lcl-0.1.0-linux-x86_64-bf78a0890e5f/SOURCE_MANIFEST.sha256` cannot be checked in place.** All 702 entries name source-tree paths (`LCL_Core_0_1_Final/…`) that are not in the candidate directory, so `sha256sum -c` exits 1 with "No such file". The candidate's 7 tracked files have no git change, and its tarball checksums verify. Nothing was modified.
- **Note: the installed menu launch opens `~/.local/share/lcl/workspace`, which has no manifest.** A localized document created there has no profile until the user adds an `lcl.project.json` naming `profiles`. The README now says so. No launcher-side profile source was added, because it would have to be a guessed or ambient location.

## Remaining blockers

- None.

## Owner actions

- O-03: review and commit decisions for the 8 modified files and this report.
- Confirm or override the pre-version attribution boundary described above. The current choice is the conservative one; the matrix rows name it.
- Decide OOS-1 and OOS-2.
- O-01 and O-02 are unchanged: the Core 0.2.0 independent review is pending, and no desktop-menu launch result is recorded.

## Quota/efficiency notes

- Reused:
  - the existing dispatch fixtures and helpers (`core`, `localized`, `fixture`);
  - the canonical localization fixtures;
  - the workspace `Scratch` and `localized_root` helpers;
  - the CLI's profile-order rule and `Project::profile_files`;
  - the `installed_launcher.rs` install/launch harness;
  - the Task 4 compile cache.
- No new dependency and no new mechanism: the profile source is the existing manifest `profiles` plus the CLI's existing option, mirrored.
- RED and GREEN were targeted per defect. The integration gate ran once. The workspace-wide suite, MSRV and conformance run are deferred to REPAIR-06.

## Handoff

The task passed its acceptance contract. LCL-REPAIR-04 is unlocked.
