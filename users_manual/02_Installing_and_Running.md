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
wants to touch files, it asks first.

Using it:

* **`+`** (next to "Project") creates a new document. A name without an
  ending gets the default file type from Settings: `.lcl`, the native
  default, unless you chose `.lcl.txt` there. If you type `.lcl` or `.lcl.txt`
  yourself, that ending is kept (see section 2.9).
* A new document's file is written to the project at once, but it only
  becomes a document you keep when you save it. If you close a new document
  you never saved, the workspace asks what to do: **Discard** removes the
  file it created, **Save** keeps it as an ordinary document, and **Cancel**
  takes you back to it.
* Closing a document that already existed, or one you have saved, asks only
  if it has unsaved edits. **Discard** then throws those edits away and leaves
  the file on disk exactly as it was.
* To delete a document, right-click it in the project tree and choose
  **Delete…**, or select it there and press the Delete key. The workspace
  always asks first, and says so if the document has unsaved edits, which
  are lost too. Deleting removes the file from the project for good. If the
  file changes on disk while the question is open, nothing is deleted. Only
  `.lcl` and `.lcl.txt` documents can be deleted this way.
* Until a document is open, the editor shows **No document open** and can't
  be typed into. Check, Inspect, Run, Save and Reload are unavailable until
  you create or select a document.
* The editor numbers every line in a **gutter** on the left. Click a line
  number to set a breakpoint for a run.
* **Settings** (the ⚙ button at the top right) has four parts.
  **Appearance** and **Editor** choose the theme (System, Dark or Light), the
  editor font size (11 to 20 px) and whether line numbers are shown. Your
  browser remembers these for the workspace's address, and that address
  changes every time the workspace starts unless you give it a fixed `--port`
  (see Chapter 17), so otherwise a new start begins with their defaults.
  **Files** chooses the **default file type** for new documents and the
  **default workspace location**: the folder that **LCL Workspace** opens
  when you start it from the desktop menu. These two are kept for your
  computer, in `~/.config/lcl/workspace-settings.json`, so they apply to every
  launch. Type the folder's full path, starting with `/`. **Check** tells you
  whether it exists, and a missing folder is only created if you press
  **Create this folder**. Leave the path empty to use the built-in folder,
  `~/.local/share/lcl/workspace`. A folder or document you open yourself,
  such as `lcl-workspace ~/lcl-course` or a file opened from your file
  manager, always wins over the default. **Android devices** pairs a phone
  or tablet running LCL for Android with this computer, through `lcl-remote`,
  which is installed separately (see `android/README.md` in the source):
  **Pair Android device** shows a one-time QR code. Scanning the QR code does
  not trust the phone: after you press Pair on the phone, it shows a
  verification code, and the same request waits here under **Pending pairing
  requests** until you approve the one whose code matches (**Approve…**, then
  **Approve device**) or deny it. Every paired device is listed with whether
  it is online and when it last connected, and can be revoked. None of these
  settings changes a document or how it runs.
* Indentation is always four spaces. The Tab key inserts four spaces, never a
  tab character, which LCL does not allow.

This manual uses the command line, since that shows exactly what the tool
reports, but everything works in the workspace too. See
[Chapter 17](17_Tools_Reference.md).

## 2.9 File names: `.lcl` and `.lcl.txt`

LCL source files normally use `.lcl`. You may alternatively use `.lcl.txt`
when a website, file-sharing service, editor, operating system or other tool
does not recognise `.lcl`. Both contain ordinary UTF-8 text and have identical
LCL meaning.

| Ending | Status |
|---|---|
| `.lcl` | the native, default LCL extension |
| `.lcl.txt` | an optional compatibility extension |

Everything in this manual works with either ending:

* every command: `lcl check task.lcl.txt`, `lcl validate task.lcl.txt`,
  `lcl inspect task.lcl.txt` and `lcl run task.lcl.txt`;
* a project's `entry`, and the `SOURCE` of an IMPORT (Chapter 14): a `.lcl`
  document may import a `.lcl.txt` one and the other way round;
* `lcl-workspace`, which lists and opens both. When you create a new
  document there, the ending you type is kept:

  | You type | The document is created as |
  |---|---|
  | `notes` | `notes.lcl` (the native default), or `notes.lcl.txt` if you chose that as the default file type in Settings |
  | `notes.lcl` | `notes.lcl` |
  | `notes.lcl.txt` | `notes.lcl.txt` |

  Saving an open document always keeps its name, whichever ending it has.

The ending decides nothing about meaning. The same text gives exactly the
same result under either name, and a mistake is reported with the same error
under either name: ending a file in `.txt` never relaxes a check. Only the
exact two-part ending `.lcl.txt` is recognised as LCL, so an ordinary text
file such as `notes.txt` is not treated as an LCL document. Upper-case endings
such as `.LCL` are not recognised either.

You never need to rename existing files. To try it, save a copy of
`hello.lcl` as `hello.lcl.txt` and run it:

<!-- lcl-run: file=examples/02/hello.lcl.txt expect=run:succeeded -->
<!-- lcl-run: file=examples/02/hello.lcl.txt expect=check -->

```
$ cp hello.lcl hello.lcl.txt
$ lcl run hello.lcl.txt
status.succeeded
  VERIFY verify.greeting = TRUE (required)
  SUCCESS success.greeting ALL = TRUE
  OUTPUT output.greeting published = "Hello, world!"
  because: SuccessSatisfied
  2 invocation(s), 0 event(s), 3 step(s)
```

This small document was written directly as a `.lcl.txt` file:

<!-- lcl: file=examples/02/shared.lcl.txt expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.shared
    NAME: "A document saved as .lcl.txt"
    VERSION: "1.0.0"
    KIND: kind.task

OUTPUT:
    ID: output.note
    TYPE: STRING
    FORMAT: format.plain_text

GOAL:
    ID: goal.note
    ASSERT: REF(output.note) == "Shared as plain text."

ACTION:
    ID: action.note
    OPERATION: core.return
    TARGET: "Shared as plain text."
    OUTPUT: REF(output.note)

VERIFY:
    ID: verify.note
    ASSERT: REF(output.note) == "Shared as plain text."

SUCCESS:
    ID: success.note
    ALL: [REF(verify.note)]

TASK:
    ID: task.shared
    GOAL: REF(goal.note)
    ACTION: REF(action.note)
    OUTPUT: REF(output.note)
    SUCCESS: REF(success.note)

EXECUTE:
    REFERENCE: REF(task.shared)
```

The rest of this manual uses `.lcl`, the native ending.

## Summary

* `install.sh` installs `lcl`, `lcl-workspace` and the specification package
  under `~/.local`.
* Every command needs the specification package: use `--spec`, `LCL_SPEC` or
  a project file.
* `check` checks the text, `validate` also checks the rules and preconditions,
  `run` does the work, and `inspect` shows the plan.
* Exit code 1 means the document is wrong. Exit code 2 means the work did not
  succeed.
* `.lcl` is the native ending. `.lcl.txt` is an optional compatibility ending
  with exactly the same meaning.

## Exercises

1. Run `lcl help` and find the option that prints JSON instead of text. Run
   `lcl check` on `hello.lcl` with it.
2. In `hello.lcl`, change the greeting in `TARGET` to `"Hello, LCL!"` but leave
   the `VERIFY` unchanged. Predict the exit code of `lcl check`, `lcl validate`
   and `lcl run`, then try them.
3. Replace the four spaces before `VERSION: "0.1.0"` with a tab character.
   What does `lcl check` report, and at which stage?

Solutions: [solutions/README.md](solutions/README.md#chapter-2).
