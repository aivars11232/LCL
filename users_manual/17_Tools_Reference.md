# Chapter 17. Tools reference

This chapter is a reference for the two programs that come with LCL, `lcl`
and `lcl-workspace`. Everything here matches the tools' own `help` output.

## 17.1 `lcl` commands

```
lcl <command> [options] [document]
```

| Command | Stages | What it does |
|---|---|---|
| `lcl check <doc>` | 1–5 | lexical, grammar, resolution and static (type) checking |
| `lcl validate <doc>` | 1–9 | everything in `check`, plus rules, permissions, conflicts and VALIDATE checks; no effects |
| `lcl run <doc>` | 1–13 | everything, including execution, VERIFY/TEST, evidence and one terminal status |
| `lcl inspect <doc>` | 1–9 | like `validate`, and also reports units, imports, declarations and the ordered execution plan |
| `lcl package lock <doc>` | | writes `lcl.lock`: the specification identity and the SHA-256 of every loaded document |
| `lcl package verify <doc>` | | compares `lcl.lock` with what is loaded now |
| `lcl package vendor <uri> <file>` | | puts a local file into the project's package cache under a URI, for imports by URI |
| `lcl package list` | | lists what the package cache holds |
| `lcl spec` | | reports the specification package's path, version, identity and authority |
| `lcl syntax` | | prints syntax facts for editors and tools: the recognised endings (`.lcl.txt`, `.lcl`), encoding, word counts |
| `lcl version` | | prints the tool, protocol and language versions |
| `lcl help` | | prints the built-in help |

The package cache used by `vendor` and `list` must be declared as `"cache"`
in `lcl.project.json`.

## 17.2 `lcl` options

| Option | Used with | Effect |
|---|---|---|
| `--spec <path>` | all | the specification package root (overrides `LCL_SPEC` and the project file) |
| `--localized-spec <path>` | all | the Core 0.2.0 package, for localized documents (not covered in this manual) |
| `--profile <file>` | all | a locale profile for localized documents; repeatable |
| `--project <dir>` | all | the project root; by default the nearest folder above the document holding `lcl.project.json`, else the document's own folder |
| `--input <id>=<expr>` | `run` | supply one declared INPUT, STATE, MEMORY or CONTEXT (Chapter 8) |
| `--machine` | all | print the JSON record instead of text |
| `--locked` | all | refuse to proceed when `lcl.lock` disagrees with what is loaded |
| `--allow-read <path>` | `run` | grant reading at or below a path |
| `--allow-write <path>` | `run` | grant writing at or below a path |
| `--allow-run <program>` | `run` | grant running one program |
| `--allow-net <host>` | `run` | grant network access to one host |

A run grants nothing unless an `--allow-*` option says so. An operation that
needs a capability that was not granted reports `error.host.constraint`, and
the run ends `status.blocked` (Chapter 11).

Every command accepts a document ending in `.lcl` (the native ending) or
`.lcl.txt` (the optional compatibility ending), with identical results. See
section 2.9.

## 17.3 Exit codes

| Code | Meaning |
|---|---|
| 0 | the requested work completed; for `run`, the terminal status was `status.succeeded` |
| 1 | the document was rejected by a diagnostic |
| 2 | the document ran and its terminal status was not `status.succeeded` |
| 3 | the command line or a supplied input was not usable |
| 4 | the specification, project, lock file or document could not be read or did not match |

## 17.4 Environment variables

| Variable | Meaning |
|---|---|
| `LCL_SPEC` | the specification package, used when `--spec` is not given |
| `LCL_LOCALIZED_SPEC` | the Core 0.2.0 package, used when `--localized-spec` is not given |

The specification package is found from `--spec`, then `LCL_SPEC`, then the
project file's `"spec"`, and **never** by searching.

## 17.5 `lcl.project.json`

```json
{
  "format": "lcl.project/1",
  "spec": "../../canonical/LCL_Core_0.1.0",
  "entry": "src/main.lcl"
}
```

| Field | Meaning |
|---|---|
| `format` | must be `"lcl.project/1"` |
| `entry` | the main document, relative to the project root |
| `spec` | optional: the specification package, relative to the project root |
| `cache` | optional: the package cache folder for URI imports |
| `localized_spec`, `profiles` | optional: for localized (Core 0.2.0) documents |

## 17.6 Reading `lcl run` output

```
status.succeeded                                   <- the terminal status
  VERIFY verify.x = TRUE (required)                <- each selected check and its result
  VERIFY verify.y skipped: when_false              <- a check whose WHEN was FALSE
  TEST test.z = TRUE (required)                    <- a TEST root's result
  SUCCESS success.s ALL = TRUE                     <- the success condition
  FAILURE failure.f -> status.failed               <- a FAILURE clause that matched
  EVIDENCE evidence.e satisfied "..."              <- evidence and its value
  OUTPUT output.o published = 42                   <- an output and its value
  OUTPUT output.p unbound                          <- an output no action produced
  because: SuccessSatisfied                        <- why this status
  2 invocation(s), 0 event(s), 3 step(s)           <- what the engine did
```

When the run did not succeed, the diagnostics come first, and the `because:`
line names the primary diagnostic or the declared failure that decided the
status.

## 17.7 `--machine` output

With `--machine`, every command prints one JSON object. Its top-level keys
include:

| Key | Holds |
|---|---|
| `protocol` | `"lcl.engine/1"` |
| `command` | the command that ran |
| `spec` | the package root, version, identity digest and authority |
| `units` | every loaded document, with its SHA-256 digest |
| `reached` | the last stage reached |
| `outcome` | `accepted` or `rejected` |
| `diagnostics` | each with `id`, `stage`, `position` (`line`, `column`, `offset`), `detail`, `meaning`, `default_status` and `primary` |
| `completion` (for `run`) | the checks, evidence, verdict, outputs and `terminal_status` |

## 17.8 `lcl-workspace`

```
lcl-workspace [PROJECT] [OPTIONS]
```

`lcl-workspace` is an editor, inspector and debugger that runs in your web
browser.

| Option | Effect |
|---|---|
| `PROJECT` | the project folder (default: the current folder) |
| `--document <PATH>` | open one document; its project is found as for `lcl` |
| `--spec <PATH>` | the specification package (else `LCL_SPEC`, else the project file) |
| `--localized-spec <PATH>`, `--profile <FILE>` | for localized documents |
| `--create` | create a project here before opening it |
| `--port <PORT>` | use this port instead of a random one |
| `--open` | open the printed address in the browser |
| `--log` | print one line per request (never the secret token) |

The server listens only on `127.0.0.1`, your own computer, and every
request needs the secret token included in the printed address. It grants no
capabilities: when a run wants an effect, the workspace asks you first.

In the page, `+` creates a document (`.lcl` unless you type `.lcl.txt`), and
the ⚙ **Settings** button sets the theme, the editor font size and line
numbers. These preferences are stored in your browser, not in the project.

After `install.sh`, the desktop menu has an **LCL Workspace** entry, and
`.lcl` files can be opened with it. Menu launches write any problem to
`~/.local/state/lcl/launch.log`.

## 17.9 Installing and uninstalling

| Command (in the unpacked release folder) | Effect |
|---|---|
| `./install.sh` | installs everything under `~/.local`; needs no administrator rights; prints every path |
| `./uninstall.sh` | removes everything the installer installed, but keeps `~/.local/share/lcl/workspace` (your own documents) |

Installing registers the `text/x-lcl` file type and an application for it,
but it does not make LCL your default application for anything.
