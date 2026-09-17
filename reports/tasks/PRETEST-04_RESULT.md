# PRETEST-04 — Result (Phases A–D checkpoint)

## Identity

- Task: PRETEST-04, product integration, project/cache/CLI/workspace/packaging, conformance and clean candidate (F14–F28)
- Pack: `/mnt/F/LCL_PreTesting_Closure_Pack_v2_c50618a/`. All `machine/SHA256SUMS.txt` entries verified.
- Repository: `/mnt/F/LCL`, branch `main`
- Entry HEAD: `6ac3b254b447ec1935ce065cfb76c6cd1e251df7` ("LCL pretest task3"), the owner's commit of PRETEST-03
- Entry worktree: clean
- Exit HEAD: unchanged
- Exit worktree: 28 modified tracked files, the new root `README.md` and this report. `canonical/` and `releases/` are unchanged.
- Git writes: **NO**. Publication: **NO**. Background, sub or parallel agents: **NO**. Network or new dependencies: **NO**.
- Evidence logs: `/mnt/F/.lcl-pretest/logs/P4-*`. The two red logs that dumped 8 MiB fixtures are gzipped.
- Scratch: `/mnt/F/.lcl-pretest/p4/`
  - `env.sh`, `p4-gate.sh`;
  - `probe/`: a scratch binary over `lcl_conformance::fixtures::runner` and fixture hosts;
  - `gen.py` and `docs/`: the probe documents;
  - `TMPDIR=/tmp/lcl-pretest-04`, and `CARGO_TARGET_DIR` is the existing isolated `/mnt/F/.lcl-closure-4t-4c1cd4c659b7/target-current`.

## Result

**Status: BLOCKED at Phase D (F27).** Phases A–C are complete.

| Finding | Status |
|---|---|
| F14–F26 | FIXED_VERIFIED, red-first |
| F27 semantic conformance | **BLOCKED**: a new root-cause engine defect outside F01–F26 (below). Conformance expansion stopped as the task requires. |
| F28 current-source candidate | NOT RUN. Phase E needs a committed tree and green PRETEST-01–04 technical gates, and F27 is not green. |

The production claim is still `source_conforming`. `semantics_conforming` is **not** established. The task's own rule applies: "If that target cannot be established, the task is not complete."

The Phase A–C repairs are independent of the blocker. They are gated below and can be committed on their own.

## F27 — the blocker

### New root cause: the `FALLBACK` operation-identifier invocation site is never checked

- **Canon.** Four normative texts require it. The rule is unambiguous.
  - `04_GRAMMAR/09_CORE_BLOCK_SCHEMAS_B.txt`, HANDLER: "FALLBACK admits one REF to a HANDLER, or one operation identifier that requires no named parameter and whose required TARGET the handler-context binding supplies."
  - `05_SEMANTICS/06`: "One operation identifier is legal only when the operation registers no required named parameter and its required target, if any, is supplied by the original handler-context binding above. An operation identifier that cannot satisfy its contract under those limits uses error.operation.parameter and never executes with an unsatisfied contract."
  - `06_STANDARD_LIBRARY/10`: that error "is emitted during static/expression checking when an ACTION, HANDLER, or FALLBACK invocation site …", and "every required target and named parameter rule applies identically on each of them".
  - `statuses_and_errors_v0.1.0.json`, `error.operation.parameter` (stage `static_or_expression`): "A FALLBACK operation identifier that requires any named parameter, or whose required TARGET no handler-context binding can supply, uses this error."
- **Implementation.**
  - `lcl-checker/src/operation.rs` checks only `INVOCATION_BLOCKS = ["ACTION", "HANDLER"]`. The `FALLBACK` field of a HANDLER is never judged against the named operation's contract.
  - At runtime, `lcl-runtime/src/handler.rs::fallback` invokes an operation-identifier FALLBACK with the *handler's own* block (`invoke_handler(&declared, handler, …)`), so no second check exists either.
- **Reproduction** (`P4-D-blocker-fallback.log`, documents under `p4/docs/blocker/`). One task with `HANDLER: OPERATION core.stop, EVENT event.host_constraint`, attached to the TASK, with four values of `FALLBACK`:

  | FALLBACK | Contract | Canon | Observed |
  |---|---|---|---|
  | `core.stop` (control) | target admits `REFERENCE[meta.execution_unit]`, no required parameter | legal | accepted, `status.succeeded` |
  | `core.append` | target `meta.mutable_target`, requires `content` | `error.operation.parameter` | accepted, `status.succeeded`, no diagnostic |
  | `core.delete` | target `meta.mutable_target`, which no handler-context binding supplies | `error.operation.parameter` | accepted, `status.succeeded`, no diagnostic |
  | `core.write` | target `meta.mutable_target`, requires `content` | `error.operation.parameter` | accepted, `status.succeeded`, no diagnostic |

- **Which obligations this blocks.** `semantic/error_contract/error.operation.parameter` cannot be satisfied: its pinned sub-runs `behavior/fallback-operation-requires-named-parameter` and `behavior/fallback-target-omitted` would fail.
- **Not owned by F01–F26.** F01–F05 are value, schema, demand, VERIFY and FAILURE; F06–F08 are PATH and WORKSPACE; F09–F13 are localization; F14–F26 are product. The repair is a new checker rule, the invocation-site check extended to the FALLBACK operation-identifier form. It is not a same-root follow-on of any owned finding. So it is recorded here and was **not** fixed.
- **Suggested owner:** a dedicated semantic-conformance task, like CLOSE-02's B-16. It can close this checker rule together with the remaining population below.

### Conformance state at the stop

Production report (`P4-D-conformance-text.log`, mapping digest `27e3271f…`, package `00d648b1…67ed`):

| Level | Required | Satisfied | Failed | Missing | Invalid |
|---|---:|---:|---:|---:|---:|
| source_conforming | 2,011 | 2,011 | 0 | 0 | 0 |
| semantics_conforming | 402 | 269 | 0 | 2 | 131 |

- These numbers are unchanged by this task. PRETEST-01 had already moved `division/declared-bound` from failed to satisfied.
- The 133 unsatisfied probes are CLOSE-02's list with that one repair removed:
  - 750 pinned sub-runs have no run: error_contract 40, operation_binding 127, operation_effects 219, operation_errors 302, result_schemas 62.
  - 54 belong to the two unpopulated families: diagnostic_policy 24 and failure_lifecycle 30.
  - The exact labels are in `reports/tasks/LCL-CLOSE-02_RESULT.md`, appendix.
- **What was executed before the stop:**
  - 16 exploratory full-pipeline probes of the `error.operation.parameter` behaviours (`P4-D-probe-error-contract.log`).
  - ACTION target omitted, required named parameter omitted, duplicate, unregistered, `core.sort` `stable` and `comparator`, HANDLER target omitted: `error.operation.parameter`, as canon requires.
  - A bound violation (`core.inspect depth 101`): `error.value.out_of_range`, the row-specific error, as canon requires.
  - A positional argument: `error.field.required`. The grammar requires `PARAMETER.NAME`, so the earliest stage wins, which matches the existing `positional-earliest-stage` oracle.
  - Both FALLBACK forms: accepted (the blocker).
- **Why no conformance case was added.**
  - A group record passes only with its complete pinned sub-run set.
  - Populating these families partially would turn `invalid` probes into `failed` ones, and it would re-pin the tests that pin the failed set. It would add no satisfied probe.
  - Stopping expansion keeps the pinned evidence unchanged. No row was reclassified, weakened or skipped.

## Phases A–C: root causes and repairs

### F14 — the project tree followed links out of the project

- **Root cause:** `lcl-workspace/src/project.rs::walk` used `path.is_dir()`, which follows symbolic links. The textual `strip_prefix(root)` check passes for any link inside the root, whatever it names.
- **Repair:** `symlink_metadata` first. A link is listed only when its canonical target is contained in the canonical project root (`lcl_capabilities::contains`), which is the same containment `document::resolve` requires to open it. A link that leaves is skipped, so it is neither listed nor descended. A link that stays inside is still listed, and reads through it still work.
- **Red → green:** `persistence.rs::the_project_tree_never_lists_through_a_link_out_of_the_project` (`P4-A-red-workspace.log.gz` → `P4-A-green-workspace.log`).

### F15 — startup error through `innerHTML`

- **Root cause:** `assets/app.js` boot `catch` interpolated `e.message` into `document.body.innerHTML`.
- **Repair:** the message is a `div` built with the existing `el()` helper (`textContent`), styled through `style.cssText`, and set with `replaceChildren`. The Node DOM harness (`editor_save.cjs`) asserted boot success through `document.body.innerHTML` and now asserts `document.body.children`. Otherwise the fix would have blinded it to a failed boot.
- **Red → green:** `routes.rs::the_frontend_never_assigns_markup` rejects `innerHTML`, `outerHTML`, `insertAdjacentHTML` and `document.write` in non-comment code. The editor harness, `production_editor_save_logic_survives_real_http`, still passes.

### F16 and F19 — unbounded document, manifest, lock, cache and vendor reads

- **Root cause:** nine production reads used `std::fs::read` or `read_to_string` and allocated the whole file:
  - the workspace `document::read`;
  - `FileProvider::root_unit` and PATH imports;
  - `Manifest::read` and `Lock::read`;
  - the `Cache::open` index, `Cache::get` and `Cache::verify` blobs;
  - `lcl package vendor`.
- **Repair:**
  - One shared `lcl_project::read_file`, which holds at most `MAX_FILE_BYTES + 1` via `take`, with a `read_text` UTF-8 wrapper. All nine sites use it.
  - `MAX_FILE_BYTES` = 8 MiB. It is documented as a host/product limit, not a language rule, and equals the workspace request-body ceiling, so the editor can open whatever it can save.
  - The refusal is an I/O fault of the caller, never a language diagnostic.
- **Red → green:**
  - `source_boundary.rs::project_documents_are_read_within_the_product_limit`
  - `manifest_and_lock.rs::project_machinery_reads_are_bounded`
  - `persistence.rs::an_oversized_document_is_refused_rather_than_read`
  - CLI `oversized_project_inputs_are_refused_before_they_are_read` (vendor and manifest)

### F17 — a configured cache that would not open was silently dropped

- **Root cause:** `Project::provider` matched `Err(_) => Ok(provider)`, so a damaged declared cache became "no cache" and URI imports became missing ones.
- **Repair:** a new `ProjectError::Cache(CacheError)`, returned by `provider()`. The CLI already maps provider errors to the environment exit (4), and the workspace routes return them as the request error.
- **Red → green:** `manifest_and_lock.rs::a_declared_cache_that_will_not_open_is_a_project_fault` and CLI `a_configured_cache_that_will_not_open_stops_the_command`.

### F18 — vendor URI line injection into the cache index

- **Root cause:** `Cache::put` wrote any `uri` text into the line-oriented index, so a newline forged a second entry.
- **Repair:**
  - `Cache::put` holds the URI to the lexer's own closed `URI` literal profile (RFC 3986 absolute-URI, ASCII, no fragment), exposed once as `lcl_lexer::uri_profile`. That profile admits no whitespace or control character, and it is the only form an import can name.
  - Refusals are `CacheError::Uri`, which `lcl package vendor` maps to a usage error (exit 3).
  - `lcl-lexer` moved from `lcl-project`'s dev-dependencies to its dependencies. It was already in `Cargo.lock` for `lcl-project`, so `Cargo.lock` is unchanged.
- **Red → green:** `manifest_and_lock.rs::a_uri_outside_the_uri_profile_is_not_cached` (forged newline, space, relative, empty; IPv6 and URN still cached) and CLI `a_vendored_uri_cannot_inject_a_cache_entry`.

### F20 and F21 — double-resolved relative CLI path; CLI and workspace project roots differed

- **Root cause:**
  - The CLI took the root as the document's parent, spelled relative to the working directory. `FileProvider::root_unit` then joined the same relative path to that root again, so `lcl check src/main.lcl` read `src/src/main.lcl`.
  - Independently, the CLI used "the document's own directory" while `lcl-workspace --document` used "the nearest ancestor holding a manifest". The same document therefore got a different root, identity and declared spec, cache and localized package depending on the product. The README's documented `lcl check src/main.lcl` could not work at all.
- **Repair, unify (option 1 of requirement 5):**
  - The workspace's rule moved, with its documentation, into `lcl_project::locate_document`: nearest ancestor holding `lcl.project.json`, else the document's own directory.
  - `Workspace::locate_document` and the CLI's `open_project` both call it.
  - A named CLI document is canonicalized once from the working directory in `entry()`. The provider derives the root-relative identity from that one resolution and never joins it to the root again.
  - `--project` still names the root outright.
  - The CLI module documentation, `--project` help text and error wording were updated. The missing-document message stays "is not readable", which an existing CLI test pins.
- **Red → green:**
  - CLI `a_nested_document_is_judged_in_its_enclosing_project`: the same bytes as the manifest-entry run from the root with `src/main.lcl`, from `src/` with `main.lcl`, and from elsewhere with an absolute path.
  - `source_boundary.rs::a_document_belongs_to_its_nearest_enclosing_project`

### F22 — a manifest could direct `package lock` writes outside the project

- **Root cause:** `lock_command` wrote to `Project::lock_path()`, which takes a manifest's absolute or `..` path as written.
- **Repair:**
  - `Project::lock_destination` confines the *write* to the project root. It is judged at the path's real location, links included, via `lcl_capabilities::fs::resolve`, the capability layer's existing resolver, now `pub`.
  - Reads (`package verify`, `--locked`) still use `lock_path`, so a manifest may still name an external lock to read.
  - External `spec` paths are untouched.
- **Red → green:** CLI `package_lock_never_writes_outside_the_project` (absolute external lock refused; external lock still verified; dangling in-project link out refused) and `manifest_and_lock.rs::the_lock_destination_is_inside_the_project`.

### F23 — the uninstaller removed shared icon roots

- **Root cause:** `packaging/uninstall.sh` ended with `rmdir "$icons" "$data/icons"`, which removed `~/.local/share/icons/hicolor` and `~/.local/share/icons` whenever they were empty.
- **Repair:** that line is removed. The per-file removal and the `rmdir` of the size and context directories this installation filled are unchanged. The script comment and `packaging/README.md` say the shared roots are never removed.
- **Test changed, not weakened:** `installed_launcher.rs::an_empty_theme_directory_is_removed_but_the_shared_root_is_never_recursive` asserted `!theme.exists()`, which is exactly the behaviour F23 names as the defect. It is now `…_but_the_shared_icon_roots_stay`. It still asserts that the size directory this installation filled is removed, and it asserts that `icons/` and `icons/hicolor/` remain.
- **Red → green:** `P4-C-red-f23.log` → `P4-C-green-installed_launcher.log`, 22 of 22.

### F24 — application tests used the fixed `/tmp/lcl-apps`

- **Constraint:** `04_GRAMMAR/08` requires `WORKSPACE.PATH` to be absolute, so an application's own source must name one absolute path. The repository applications keep `/tmp/lcl-apps/<app>`. A run touches it only under an explicit `--allow-write` grant.
- **Repair (test roots parameterized):**
  - `lcl-hardening/tests/applications.rs` runs each application from a private copy under `$TMPDIR/lcl-apps-<pid>/copy-<n>`, beside a `canonical` link, so the manifest's relative package path is unchanged. Its declared `/tmp/lcl-apps/` prefix is rewritten to `$TMPDIR/lcl-apps-<pid>/workspaces/`. The copy is removed after the run; removal does not follow the link.
  - `lcl-protocol/tests/v07_matrix.rs` relocates the in-memory sources of all five language variants identically, so the reports stay comparable.
- **Red → green:** `applications.rs::application_workspaces_belong_to_this_run`. After the green runs, nothing new was written under `/tmp/lcl-apps` (`find -newer`: 0).

### F25 — valid URI forms and unsupported transports misclassified

- **Root cause:** `Address::parse` (`lcl-capabilities/src/net.rs`) split on `://` and on the last `:`, and the stdlib's `host_of` and `is_secure` re-parsed URIs their own way. As a result:
  - `http://[::1]/x` was Malformed ("no valid port");
  - `http://[2001:db8::1]:8080/…` was granted against host `[2001` and denied;
  - `http://h?q=1` put the query into the host;
  - `ftp://…` and `mailto:` became operation failures instead of host limitations;
  - `http://h:65536` was Malformed;
  - an uppercase `HTTPS://h:443/…` was classified not-secure, a cleartext path.
- **Repair:**
  - `Address::parse` handles userinfo, IP literals (host keeps its brackets, as the URI, grants and `Host` header spell it), query-only targets, empty ports and case-insensitive schemes.
  - A valid URI no host transport can reach becomes `Refused(Unavailable)`, the host limitation that maps to `error.host.constraint`. That covers no authority (`mailto:`, `urn:`, `file:///`), a scheme other than http or https, an empty host and an out-of-range port. `Malformed` remains only for text the URI profile would reject.
  - `connect_within` strips the IP-literal brackets for the socket.
  - The stdlib's `host_of` and `is_secure` now reuse `Address::parse`, so the grant decided and the address reached are the same.
  - `capability_missing` reports an unaddressable network URI as unavailable before grants, except for messages to a person.
- **Red → green:**
  - `transport.rs::valid_uri_forms_are_addresses_or_host_limitations` and `an_ipv6_literal_host_is_reached`
  - `external_operations.rs::valid_uri_forms_are_reached_or_are_host_limitations`: IPv6, query-only and uppercase HTTP succeed; ftp, mailto, uppercase HTTPS and port 65536 give `error.host.constraint`.

### F26 — no root authority note

- **Repair:** a new root `README.md`. `canonical/` is the only language authority: the Core 0.1.0 identity, and Core 0.2.0 as an unreleased candidate with review pending and the gate false. `LCL_Core_0_1_Final/`, the loose `LCL_Core_0_1_Final_*` validation and SHA files, and the two research texts are historical lineage evidence that nothing reads. The README cross-references `00_CANONICAL_SOURCE_AND_PROVENANCE.txt`. No historical file was touched.

## Files changed

- **`lcl-project`:**
  - `src/lib.rs`: `MAX_FILE_BYTES`, `read_file`, `read_text`, `locate_document`, `ProjectError::Cache`, `Project::lock_destination`, `provider()`
  - `src/cache.rs`, `src/manifest.rs`, `src/lock.rs`, `src/provider.rs`
  - `Cargo.toml`
- **`lcl-lexer`:** `src/lib.rs`, `uri_profile`
- **`lcl-capabilities`:** `src/net.rs` (`Address::parse`, `connect_within`) and `src/fs.rs` (`resolve` made `pub`)
- **`lcl-stdlib`:** `src/host.rs` (`address_of`, `host_of`, `is_secure`, `capability_missing`)
- **`lcl-cli`:** `src/main.rs` (docs, `open_project`, `entry`, `lock_command`, `vendor`) and `src/args.rs` (help text)
- **`lcl-workspace`:** `src/project.rs` (`locate_document` delegates; `walk`), `src/document.rs` (bounded read) and `assets/app.js`
- **`packaging/`:** `uninstall.sh` and `README.md`
- **Root:** `README.md` (new)
- **Tests:**
  - `lcl-project/tests/{manifest_and_lock,source_boundary}.rs`
  - `lcl-cli/tests/projects_and_capabilities.rs`
  - `lcl-workspace/tests/{persistence,routes}.rs` and `editor_save.cjs`
  - `lcl-capabilities/tests/transport.rs`
  - `lcl-stdlib/tests/external_operations.rs`
  - `lcl-hardening/tests/{applications,installed_launcher}.rs`
  - `lcl-protocol/tests/v07_matrix.rs`
- **This report**

## Tests added or changed

| File | Test | Finding |
|---|---|---|
| `lcl-workspace/tests/persistence.rs` | `the_project_tree_never_lists_through_a_link_out_of_the_project`, `an_oversized_document_is_refused_rather_than_read` | F14, F16 |
| `lcl-workspace/tests/routes.rs` | `the_frontend_never_assigns_markup` | F15 |
| `lcl-workspace/tests/editor_save.cjs` | boot-failure detection through `body.children` | F15 (keeps the harness sighted) |
| `lcl-project/tests/source_boundary.rs` | `project_documents_are_read_within_the_product_limit`, `a_document_belongs_to_its_nearest_enclosing_project` | F16, F21 |
| `lcl-project/tests/manifest_and_lock.rs` | `a_declared_cache_that_will_not_open_is_a_project_fault`, `a_uri_outside_the_uri_profile_is_not_cached`, `project_machinery_reads_are_bounded`, `the_lock_destination_is_inside_the_project` | F17, F18, F19, F22 |
| `lcl-cli/tests/projects_and_capabilities.rs` | `a_nested_document_is_judged_in_its_enclosing_project`, `a_configured_cache_that_will_not_open_stops_the_command`, `a_vendored_uri_cannot_inject_a_cache_entry`, `oversized_project_inputs_are_refused_before_they_are_read`, `package_lock_never_writes_outside_the_project` | F17–F22 |
| `lcl-hardening/tests/installed_launcher.rs` | `an_empty_theme_directory_is_removed_but_the_shared_icon_roots_stay` (was `…_shared_root_is_never_recursive`) | F23 (expectation changed as the finding requires) |
| `lcl-hardening/tests/applications.rs` | `application_workspaces_belong_to_this_run`, plus staged copies for every application run | F24 |
| `lcl-protocol/tests/v07_matrix.rs` | relocated medium-application workspace | F24 |
| `lcl-capabilities/tests/transport.rs` | `valid_uri_forms_are_addresses_or_host_limitations`, `an_ipv6_literal_host_is_reached` | F25 |
| `lcl-stdlib/tests/external_operations.rs` | `valid_uri_forms_are_reached_or_are_host_limitations` | F25 |

Every new behavioural test was run red before its repair. Red logs:

- `P4-A-red-{cli.log, project.log.gz, workspace.log.gz}`
- `P4-C-red-f23.log`, `P4-C-red-f24.log`
- `P4-C-red-f25-{capabilities,stdlib}.log`

## Commands and exit results

Final gate: `/mnt/F/.lcl-pretest/p4/p4-gate.sh`, summary `P4-D-gate-summary.log`. It ran once, after Phases A–C were green, with `TMPDIR=/tmp/lcl-pretest-04` and the isolated target roots.

| # | Gate | Command | Exit | Result |
|---|---|---|---:|---|
| 1 | Format | `cargo fmt --all -- --check` | 0 | clean. `cargo fmt --all` had been applied to the new code first. |
| 2 | Clippy | `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | 0 | no warnings |
| 3 | Workspace tests, 1.98.1 | `cargo test --offline --locked --workspace --no-fail-fast` | 0 | 168 result blocks: **1,671 passed, 0 failed, 1 ignored** (the manifest-bounds child helper). Includes every `lcl-hardening` target (`installed_launcher`, `release_build`, `applications`), the Node editor harness and `v07_matrix`. |
| 4a | MSRV check, 1.75.0 | `cargo check --offline --locked --workspace --all-targets` | 0 | clean |
| 4b | MSRV tests, 1.75.0 | `cargo test --offline --locked --workspace --all-targets --no-fail-fast` | 0 | 162 result blocks: **1,671 passed, 0 failed, 1 ignored** |
| 5 | Core 0.1.0 integrity | `sha256sum -c --strict --quiet SHA256SUMS.txt`; `validate_release.py --scope all` | 0 / 0 | OK; PASS 31, OUT_OF_SCOPE 2 |
| 5 | Core 0.1.0 identity | built `lcl spec --spec canonical/LCL_Core_0.1.0` (`P4-D-identity.log`) | 0 | `00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed`, authoritative |
| 6 | Core 0.2.0 integrity | `sha256sum -c --strict --quiet SHA256SUMS.txt`; `identity.py` | 0 / 0 | OK; `6e7303157f5ba378b4e8c0b89852a7a81b00b6dd26b85cac36a8977532690ceb`, 216 files |
| 6 | Core 0.2.0 release validator | `validate_release.py --root canonical/LCL_Core_0.2.0 --scope all` | 1 | PASS 31, OUT_OF_SCOPE 2, **BLOCKED 1**: `pending_decisions: ["independent_review"]`, `release_gate_permitted: false`. Expected, and identical to PRETEST-03. |
| 6 | Core 0.2.0 identity through the tool | built `lcl check --machine --spec … --localized-spec … canonical_en.lcl` | 0 | `formal_version` 0.2.0, `identity_digest` `6e730315…90ceb`, authoritative. The relative document path worked (F20). |
| 7 | Localization validator | `validate_localization.py --root canonical/LCL_Core_0.2.0` | 0 | pass |
| 8 | Brand | `sha256sum -c assets/brand/BRAND_ASSETS.sha256` | 0 | OK |
| 9 | Conformance | `cargo run -p lcl-conformance --example m8_conformance_report [-- --json]` | 0 / 0 | claim `source_conforming`. Executed 2,411, failed 0, failed case IDs none. Semantic probes 402 = 269 satisfied + 0 failed + 2 missing + 131 invalid. **`semantics_conforming` not established (F27).** |
| 10 | Protected | `git status` under `canonical releases assets`; every candidate's product and source `.sha256` | 0 | no change under the protected areas; 6 of 6 candidate checksum files OK |
| 11 | Isolation | `find … -newer` gate marker | — | checkout `impl/target/test-tmp`: 0 entries written; `/tmp/lcl-apps`: 0 entries written |

The `real_process` flake (residual 2) did not recur in either full test run.

Targeted runs during the repair (targets, passed/failed):
- `P4-A-green-project`: `lcl-project` 5, 52/0
- `P4-A-green-workspace`: `lcl-workspace` 13, 115/0
- `P4-A-green-cli`: `lcl-cli` 7, 69/0
- `P4-C-green-installed_launcher`: 1, 22/0
- `P4-C-green-applications`: 1, 9/0
- `P4-C-green-v07`: 1, 12/0
- `P4-C-green-stdlib`: `lcl-stdlib` 11, 128/0
- `P4-C-green-capabilities`: `lcl-capabilities` 8, 91/1. The one failure is the `real_process` flake, which then passed 3 of 3 alone.

Baseline and blocker evidence:
- `P4-D-base-conformance-{text.log,.json}`: the claim before any Phase D work, the same as at the gate
- `P4-D-probe-error-contract.log`
- `P4-D-blocker-fallback.log`

## Protected material

- `canonical/LCL_Core_0.1.0/`: 0 changes, and its checksums verify.
- `canonical/LCL_Core_0.2.0/`: 0 changes since HEAD. Its identity is `6e730315…90ceb`. Independent review is pending, `release_gate_permitted` is false and `release_ready` is false.
- `releases/`: 0 changes. The historical candidates' checksums verify.

## Residual risks and out-of-scope observations

1. **F27 blocker** (above). `semantics_conforming` is not established, so PRETEST-04 is not complete and Phase E must not run.
2. **Flaky test, not a product change:** `lcl-capabilities/tests/real_process.rs::many_flooding_children_in_sequence_leave_nothing_behind` failed once in a parallel run (`zombie_children()` was 1) and passed 3 of 3 alone (`P4-C-real_process-rerun{1,2,3}.log`). `zombie_children()` counts every child of the test process, so a sibling test's child can be observed mid-reap. The process code was not touched.
3. **CLI document argument semantics:** a relative document is now resolved from the working directory in every case. Before, with `--project`, it was joined to the project root. No test or documented use relied on the old reading, and a mismatch is now loud ("is not readable" or "outside the project root").
4. **`/tmp/lcl-apps` in application sources:** it remains the applications' declared WORKSPACE, which is required to be absolute. Only an explicit operator grant reaches it; tests never do.
5. **Carried from PRETEST-03, not in F14–F28, not changed:**
   - `spec`, `syntax`, `package list` and `package vendor` accept and ignore unused common options.
   - A manifest `profiles` directory without `localized_spec` is silently unused. Both products behave the same way, so it does not give a document different engine authority.
6. **F10** (PRETEST-03) still awaits the owner's Reading 1 or Reading 2 choice.

## Git status

At this checkpoint (HEAD `6ac3b25`):

```
 M impl/crates/lcl-capabilities/src/fs.rs
 M impl/crates/lcl-capabilities/src/net.rs
 M impl/crates/lcl-capabilities/tests/transport.rs
 M impl/crates/lcl-cli/src/args.rs
 M impl/crates/lcl-cli/src/main.rs
 M impl/crates/lcl-cli/tests/projects_and_capabilities.rs
 M impl/crates/lcl-hardening/tests/applications.rs
 M impl/crates/lcl-hardening/tests/installed_launcher.rs
 M impl/crates/lcl-lexer/src/lib.rs
 M impl/crates/lcl-project/Cargo.toml
 M impl/crates/lcl-project/src/cache.rs
 M impl/crates/lcl-project/src/lib.rs
 M impl/crates/lcl-project/src/lock.rs
 M impl/crates/lcl-project/src/manifest.rs
 M impl/crates/lcl-project/src/provider.rs
 M impl/crates/lcl-project/tests/manifest_and_lock.rs
 M impl/crates/lcl-project/tests/source_boundary.rs
 M impl/crates/lcl-protocol/tests/v07_matrix.rs
 M impl/crates/lcl-stdlib/src/host.rs
 M impl/crates/lcl-stdlib/tests/external_operations.rs
 M impl/crates/lcl-workspace/assets/app.js
 M impl/crates/lcl-workspace/src/document.rs
 M impl/crates/lcl-workspace/src/project.rs
 M impl/crates/lcl-workspace/tests/editor_save.cjs
 M impl/crates/lcl-workspace/tests/persistence.rs
 M impl/crates/lcl-workspace/tests/routes.rs
 M packaging/README.md
 M packaging/uninstall.sh
?? README.md
?? reports/tasks/PRETEST-04_RESULT.md
```

## Proposed commit message

```
LCL pretest task4 (phases A-C; F27 blocked)

PRETEST-04: product integration and boundary repairs (F14-F26)

- F14: the workspace tree lists a link only when it resolves inside the
  project root; links out are neither listed nor walked.
- F15: startup errors are rendered as text through DOM nodes.
- F16/F19: one bounded lcl_project::read_file (8 MiB product limit) for
  documents, imports, manifest, lock, cache index, blobs and vendoring.
- F17: a declared cache that will not open is a project fault.
- F18: vendored URIs must meet the lexer's URI literal profile, so the
  cache index cannot be forged.
- F20/F21: lcl_project::locate_document is the one project rule for the
  CLI and the workspace; a CLI document path is resolved once.
- F22: package lock writes only inside the project; external locks may
  still be read.
- F23: uninstall never removes the shared icons/ and hicolor/ roots.
- F24: application tests stage private copies whose workspaces live under
  TMPDIR instead of /tmp/lcl-apps.
- F25: Address::parse handles IPv6 literals, query-only targets and scheme
  case; unreachable valid URIs are host limitations; stdlib grants reuse it.
- F26: root README names canonical/ as the only language authority.

F27 BLOCKED: the FALLBACK operation-identifier invocation site is not
checked (error.operation.parameter); recorded, not fixed. F28 not run.
```

No Git write operation (add, commit, push, tag, reset or any other) was performed.
