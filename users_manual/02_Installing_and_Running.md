# Chapter 2. Installing and running LCL

**In this chapter you will learn**

* how to install the `lcl` tools in your own home directory;
* what the *specification package* is and why every command needs it;
* the four commands you will use all the time: `check`, `validate`, `run` and
  `inspect`;
* how to read the tool's exit codes.

## 2.1 What you need

* A Linux computer (x86-64). This is the platform the release is built and
  tested on.
* A terminal and a plain-text editor. Any editor works, as long as it saves
  UTF-8 text and lets you indent with **spaces, not tabs**. Chapter 4 explains
  why that matters.
* The LCL release archive, `lcl-<version>-linux-x86_64.tar.gz`. In the LCL
  repository the current one is under `releases/candidates/`, in the directory
  whose provenance file names the newest commit.

## 2.2 Installing

Unpack the archive and run its installer:

```
$ tar -xzf lcl-0.2.0-linux-x86_64.tar.gz
$ cd lcl-0.2.0-linux-x86_64
$ ./install.sh
```

The installer needs no administrator rights and writes only under your home
directory. It prints every path it touches. The important ones are:

| Path | What it is |
|---|---|
| `~/.local/bin/lcl` | the command-line tool |
| `~/.local/bin/lcl-workspace` | the browser-based editor and debugger |
| `~/.local/share/lcl/LCL_Core_0.1.0` | the specification package for the language |

If the installer says `~/.local/bin is not on your PATH`, add this line to
your shell's start-up file (for example `~/.bashrc`) and open a new terminal:

```
export PATH="$HOME/.local/bin:$PATH"
```

To remove everything later, run `./uninstall.sh` from the same directory. It
keeps `~/.local/share/lcl/workspace`, because that folder holds your own work.

## 2.3 The specification package

Every `lcl` command needs to know where the **specification package** is. This
is the folder `LCL_Core_0.1.0`, which contains the complete, official
definition of the language: its grammar, its keyword list, its error codes and
so on. The tool checks your documents against it.

The tool never searches for the package, because a tool that picked its own
rulebook could give different answers on different machines. You have to name
it, in one of three ways, and the first one found wins:

1. per command: `lcl check --spec ~/.local/share/lcl/LCL_Core_0.1.0 file.lcl`
2. in your environment: `export LCL_SPEC=~/.local/share/lcl/LCL_Core_0.1.0`
3. in a project file, `lcl.project.json` (see
   [Chapter 14](14_Projects_and_Imports.md)).

The easiest option is to add the `export LCL_SPEC=...` line to your shell's
start-up file next to the `PATH` line. The rest of this manual assumes you
have done so.

Check that everything works:

```
$ lcl version
lcl 0.1.0
protocol lcl.engine/1
language 0.1.0

$ lcl spec
/home/you/.local/share/lcl/LCL_Core_0.1.0
  version   0.1.0
  identity  00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed
  authority authoritative
```

The long `identity` is a fingerprint of the whole specification package. If
yours shows exactly that value, you have the official Core 0.1.0 rules.
`authoritative` means the tool recognises the package as the real one.

## 2.4 Your first check

Make a folder for your course work, and in it create a file called
`hello.lcl` containing exactly this:

<!-- lcl: file=examples/02/hello.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.hello
    NAME: "Hello"
    VERSION: "1.0.0"
    KIND: kind.task

OUTPUT:
    ID: output.greeting
    TYPE: STRING
    FORMAT: format.plain_text

GOAL:
    ID: goal.greeting
    ASSERT: REF(output.greeting) == "Hello, world!"

ACTION:
    ID: action.greet
    OPERATION: core.return
    TARGET: "Hello, world!"
    OUTPUT: REF(output.greeting)

VERIFY:
    ID: verify.greeting
    ASSERT: REF(output.greeting) == "Hello, world!"

SUCCESS:
    ID: success.greeting
    ALL: [REF(verify.greeting)]

TASK:
    ID: task.hello
    GOAL: REF(goal.greeting)
    ACTION: REF(action.greet)
    OUTPUT: REF(output.greeting)
    SUCCESS: REF(success.greeting)

EXECUTE:
    REFERENCE: REF(task.hello)
```

Every indented line starts with exactly four or eight spaces. The file must
end with a line break after the last line.

## 2.5 The four everyday commands

LCL processes a document in a fixed order of thirteen steps
(`01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt`). The commands differ in
how far along those steps they go:

| Command | Steps | What it tells you |
|---|---|---|
| `lcl check` | 1–5 | The text is well formed: spelling, grammar, names and types are all correct. Nothing is executed. |
| `lcl validate` | 1–9 | Everything `check` does, plus every rule, permission and before-the-work check. Still nothing is executed. |
| `lcl run` | 1–13 | Everything, including the actual work, the after-the-work checks and a final status. |
| `lcl inspect` | 1–9 | Like `validate`, but also shows the imports, declarations and the planned order of execution. |

Try them in order:

```
$ lcl check hello.lcl
hello.lcl passed every stage through static_checking.
This is a statement about canonical steps 1 to 5, and not about execution.

$ lcl validate hello.lcl
hello.lcl passed every stage through preflight.
This is a statement about canonical steps 1 to 9, before any effect, and not about execution.

$ lcl run hello.lcl
status.succeeded
  VERIFY verify.greeting = TRUE (required)
  SUCCESS success.greeting ALL = TRUE
  OUTPUT output.greeting published = "Hello, world!"
  because: SuccessSatisfied
  2 invocation(s), 0 event(s), 3 step(s)
```

Read the `run` output from the top:

* `status.succeeded` is the **terminal status**: the one final result of the
  run.
* `VERIFY verify.greeting = TRUE (required)`: the after-the-work check passed.
* `SUCCESS success.greeting ALL = TRUE`: the success condition held.
* `OUTPUT output.greeting published = "Hello, world!"`: the value the task
  produced.
* `because: SuccessSatisfied` gives the reason for the status.
* The last line counts what the engine did.

Now `inspect`:

```
$ lcl inspect hello.lcl
hello.lcl passed every stage through preflight.
This is a statement about canonical steps 1 to 9, before any effect, and not about execution.

1 unit(s), 0 import(s), 7 declaration(s), 2 candidate node(s), 2 plan node(s)
  execution order:
      0. TASK task.hello
      1. ACTION action.greet [core.return]
```

## 2.6 Making a mistake on purpose

Change `KIND: kind.task` to `Kind: kind.task` and run `check` again:

```
$ lcl check hello.lcl
hello.lcl:8:5: error.keyword.case [lexical]: `Kind` is a mixed-case spelling of the registered word `KIND`
    A case-insensitive match to a registered word has incorrect case in ...

hello.lcl was rejected at the lexical stage.
```

Every error message starts with **file:line:column**, then the **error code**
(`error.keyword.case`), then in square brackets the **stage** where it was
found (`lexical`), then an explanation. [Chapter 15](15_Reading_Error_Messages.md)
covers error messages in depth. Undo the change before you continue.

## 2.7 Exit codes

Every command ends with an exit code, which scripts can test. In a shell,
`echo $?` straight after a command prints it.

| Code | Meaning |
|---|---|
| 0 | The requested work completed. For `run`, the final status was `status.succeeded`. |
| 1 | The document was rejected by a diagnostic before running. |
| 2 | The document ran, but its final status was not `status.succeeded`. |
| 3 | The command line, or a supplied input, was not usable. |
| 4 | The specification, project or document could not be read. |

The difference between 1 and 2 matters. Exit code 1 means *your document is
wrong*. Exit code 2 means *your document is fine, but the work it describes
did not succeed*. A task whose after-the-work check fails is a correctly
written task that failed, not a broken one.

## 2.8 The workspace editor

`lcl-workspace` is a small editor and debugger that runs in your web browser.
Open a folder with it:

```
$ lcl-workspace ~/lcl-course --open
```

It serves only your own computer (address `127.0.0.1`), protects the page with
a secret token in the printed URL, and grants no capabilities. When a run
wants to touch files, it asks first. This manual uses the command line, since
that shows exactly what the tool reports, but everything works in the
workspace too. See [Chapter 17](17_Tools_Reference.md).

## 2.9 File names

LCL documents end in `.lcl`. The tools also accept `.lcl.txt`, which some
systems open more easily as plain text. The ending never changes the rules.
Documents with either ending are judged the same way.

## Summary

* `install.sh` installs `lcl`, `lcl-workspace` and the specification package
  under `~/.local`.
* Every command needs the specification package: use `--spec`, `LCL_SPEC` or
  a project file.
* `check` checks the text, `validate` also checks the rules and preconditions,
  `run` does the work, and `inspect` shows the plan.
* Exit code 1 means the document is wrong. Exit code 2 means the work did not
  succeed.

## Exercises

1. Run `lcl help` and find the option that prints JSON instead of text. Run
   `lcl check` on `hello.lcl` with it.
2. In `hello.lcl`, change the greeting in `TARGET` to `"Hello, LCL!"` but leave
   the `VERIFY` unchanged. Predict the exit code of `lcl check`, `lcl validate`
   and `lcl run`, then try them.
3. Replace the four spaces before `VERSION: "0.1.0"` with a tab character.
   What does `lcl check` report, and at which stage?

Solutions: [solutions/README.md](solutions/README.md#chapter-2).
