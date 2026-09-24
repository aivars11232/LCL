# Chapter 11. Files, workspaces and permissions

**In this chapter you will learn**

* how to name files safely with a WORKSPACE;
* how to read, create, replace and append to files;
* how the `lcl` tool decides what a document may touch;
* what happens when a document is not allowed to do something.

Until now every program only computed values. Real tasks change things:
files, services, messages. Changing things is where mistakes cost the most,
so LCL makes every change explicit, and the tool grants nothing that was not
asked for.

## 11.1 Workspaces

A **WORKSPACE** names a folder that relative paths are measured from:

<!-- lcl: expect=fragment -->
```lcl
WORKSPACE:
    ID: workspace.notes
    PATH: PATH("/tmp/lcl-manual/todo")
    MODE: mode.read_write
```

* `PATH` must be absolute. LCL has no "current directory", so a path never
  depends on where you happened to start the tool.
* Files inside it are named with `PATH(REF(workspace.notes), "todo.txt")`.
* A path may not leave its workspace. `PATH(REF(workspace.notes),
  "../escape.txt")` is rejected before anything runs (see 11.5).
* `MODE` states the intended access: `mode.read_only` or `mode.read_write`.

A TASK names the workspaces it uses in its `WORKSPACE` field.

## 11.2 File operations

| Operation | Does | Target must | Main parameters |
|---|---|---|---|
| `core.read` | reads a file's content | exist | optional `format`, `range` |
| `core.create` | makes a new file | **not** exist (by default) | `content`; `fail_if_exists` (default TRUE) |
| `core.write` | replaces the whole content | exist, unless `create_if_missing: TRUE` | `content` |
| `core.append` | adds to the end | exist | `content` |
| `core.copy`, `core.move`, `core.rename`, `core.delete` | what the names say | depends | see Appendix B |

These preconditions are strict on purpose. `core.create` refuses to
overwrite an existing file, and `core.write` refuses to create one. That way
an action never does something other than what its name says. If you run a
`core.create` twice, the second run fails with
`error.operation.precondition: ... already exists`.

## 11.3 A complete example

This task reads a to-do list, saves a backup copy, and appends a new item.
Before you run it, create the folder and the list:

```
$ mkdir -p /tmp/lcl-manual/todo
$ printf 'Buy milk\nWalk the dog\n' > /tmp/lcl-manual/todo/todo.txt
```

<!-- lcl: file=examples/11/todo.lcl expect=run:succeeded workspace=/tmp/lcl-manual/todo seed=examples/11/seed grant=rw produces=todo_backup.txt -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.todo
    NAME: "Back up and extend a to-do list"
    VERSION: "1.0.0"
    KIND: kind.task

WORKSPACE:
    ID: workspace.notes
    PATH: PATH("/tmp/lcl-manual/todo")
    MODE: mode.read_write

OUTPUT:
    ID: output.todo_text
    TYPE: STRING
    FORMAT: format.plain_text

OUTPUT:
    ID: output.backup
    TYPE: PATH
    FORMAT: format.plain_text
    TARGET: PATH(REF(workspace.notes), "todo_backup.txt")
    PROPERTY: target

OUTPUT:
    ID: output.appended
    TYPE: BOOLEAN
    FORMAT: format.plain_text

GOAL:
    ID: goal.todo
    ASSERT: EXISTS(REF(output.backup))

ACTION:
    ID: action.read
    OPERATION: core.read
    TARGET: PATH(REF(workspace.notes), "todo.txt")
    OUTPUT: REF(output.todo_text)

ACTION:
    ID: action.backup
    OPERATION: core.create
    TARGET: REF(output.backup).TARGET
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: REF(output.todo_text)
    OUTPUT: REF(output.backup)

ACTION:
    ID: action.append
    OPERATION: core.append
    TARGET: PATH(REF(workspace.notes), "todo.txt")
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "Learn LCL\n"
    OUTPUT: REF(output.appended)

VERIFY:
    ID: verify.read
    ASSERT: REF(output.todo_text) CONTAINS "Buy milk"

VERIFY:
    ID: verify.appended
    ASSERT: REF(output.appended) == TRUE

SUCCESS:
    ID: success.todo
    ALL: [REF(verify.read), REF(verify.appended)]

TASK:
    ID: task.todo
    GOAL: REF(goal.todo)
    WORKSPACE: REF(workspace.notes)
    ACTION: [REF(action.read), REF(action.backup), REF(action.append)]
    OUTPUT: [REF(output.todo_text), REF(output.backup), REF(output.appended)]
    SUCCESS: REF(success.todo)

EXECUTE:
    REFERENCE: REF(task.todo)
```

Points to notice:

* `output.backup` declares its destination in `TARGET`. `action.backup` uses
  `REF(output.backup).TARGET` as its own target, so the path is written only
  once. `PROPERTY: target` makes the output hold the created file's path.
* `output.appended` has no PROPERTY, so it holds the default field of
  `core.append`'s result, `changed`: TRUE if the file changed.
* The actions run in the listed order: read, then create the backup from what
  was read, then append.

## 11.4 Granting permission

Run it with no extra options and it does **not** succeed:

<!-- lcl-run: file=examples/11/todo.lcl expect=run:blocked primary=error.host.constraint workspace=/tmp/lcl-manual/todo seed=examples/11/seed -->

```
$ lcl run todo.lcl
todo.lcl:36:1: error.host.constraint [execution]: no filesystem capability is installed
...
status.blocked
```

The document is fine, but `lcl run` **grants the host nothing by default**.
Reading and writing files are capabilities of the host, and you give them
explicitly with command-line options:

| Option | Grants |
|---|---|
| `--allow-read <path>` | reading files at or below `<path>` |
| `--allow-write <path>` | writing files at or below `<path>` |
| `--allow-run <program>` | running that program (`core.execute`) |
| `--allow-net <host>` | network access to that host |

Grant only reading, and the write is refused:

<!-- lcl-run: file=examples/11/todo.lcl expect=run:failed primary=error.permission.denied workspace=/tmp/lcl-manual/todo seed=examples/11/seed grant=r -->

```
$ lcl run --allow-read /tmp/lcl-manual/todo todo.lcl
todo.lcl:42:1: error.permission.denied [execution]: /tmp/lcl-manual/todo/todo_backup.txt is inside a read-only granted scope
...
status.failed
```

Grant both, and the task succeeds:

```
$ lcl run --allow-read /tmp/lcl-manual/todo --allow-write /tmp/lcl-manual/todo todo.lcl
status.succeeded
  VERIFY verify.read = TRUE (required)
  VERIFY verify.appended = TRUE (required)
  SUCCESS success.todo ALL = TRUE
  OUTPUT output.todo_text published = "Buy milk\nWalk the dog\n"
  OUTPUT output.backup published = PATH("/tmp/lcl-manual/todo/todo_backup.txt")
  OUTPUT output.appended published = TRUE
  ...

$ cat /tmp/lcl-manual/todo/todo.txt
Buy milk
Walk the dog
Learn LCL
```

To run the example again, delete `todo_backup.txt` first, because
`core.create` will not overwrite it.

Look at the difference between the two refusals:

* **No capability at all** gives `error.host.constraint` and `status.blocked`:
  the host *could not* do it.
* **A capability that does not cover the target** gives
  `error.permission.denied` and `status.failed`: the host *would not* do it.

Neither one is the tool being unhelpful. A document that asks for more than
it was granted gets a clear refusal, not a surprise.

> **Tool note.** The current `lcl` does not enforce a workspace's `MODE`: a
> `core.write` into a `mode.read_only` workspace still succeeds when
> `--allow-write` covers it. The specification says the mode states the
> intended access. Until the tool enforces it, protect files with the grants
> you give, and with FORBID rules (Chapter 12).

## 11.5 Paths cannot escape

<!-- lcl: file=examples/11/escape.invalid.lcl expect=reject:error.value.out_of_range -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.escape
    NAME: "A path that leaves its workspace"
    VERSION: "1.0.0"
    KIND: kind.task

WORKSPACE:
    ID: workspace.notes
    PATH: PATH("/tmp/lcl-manual/todo")
    MODE: mode.read_write

OUTPUT:
    ID: output.todo_text
    TYPE: STRING
    FORMAT: format.plain_text

OUTPUT:
    ID: output.backup
    TYPE: PATH
    FORMAT: format.plain_text
    TARGET: PATH(REF(workspace.notes), "../escape.txt")
    PROPERTY: target

OUTPUT:
    ID: output.appended
    TYPE: BOOLEAN
    FORMAT: format.plain_text

GOAL:
    ID: goal.todo
    ASSERT: EXISTS(REF(output.backup))

ACTION:
    ID: action.read
    OPERATION: core.read
    TARGET: PATH(REF(workspace.notes), "todo.txt")
    OUTPUT: REF(output.todo_text)

ACTION:
    ID: action.backup
    OPERATION: core.create
    TARGET: REF(output.backup).TARGET
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: REF(output.todo_text)
    OUTPUT: REF(output.backup)

ACTION:
    ID: action.append
    OPERATION: core.append
    TARGET: PATH(REF(workspace.notes), "todo.txt")
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "Learn LCL\n"
    OUTPUT: REF(output.appended)

VERIFY:
    ID: verify.read
    ASSERT: REF(output.todo_text) CONTAINS "Buy milk"

VERIFY:
    ID: verify.appended
    ASSERT: REF(output.appended) == TRUE

SUCCESS:
    ID: success.todo
    ALL: [REF(verify.read), REF(verify.appended)]

TASK:
    ID: task.todo
    GOAL: REF(goal.todo)
    WORKSPACE: REF(workspace.notes)
    ACTION: [REF(action.read), REF(action.backup), REF(action.append)]
    OUTPUT: [REF(output.todo_text), REF(output.backup), REF(output.appended)]
    SUCCESS: REF(success.todo)

EXECUTE:
    REFERENCE: REF(task.todo)
```

```
$ lcl check escape.invalid.lcl
escape.invalid.lcl:24:13: error.value.out_of_range [static_or_expression]: PATH(REF(workspace.notes), "../escape.txt") resolves outside WORKSPACE `/tmp/lcl-manual/todo`
```

The check is made on where the path actually leads, not on how it is
spelled, so detours through links are caught too. Nothing runs, and no grant
can make an escaping path legal.

## 11.6 What an action is allowed to do

A reachable ACTION authorises **exactly** what it declares: its one
operation, on its one target, with its parameters, plus the direct effects
needed to do that. An AI carrying out the document may not decide to "also
tidy up" another file. An extra effect needs its own ALLOW rule (next
chapter), and a FORBID blocks an action even though the action asks for it.

## 11.7 Scopes

A **SCOPE** names a set of things that a rule or action applies to. It
`INCLUDE`s entities and may `EXCLUDE` some:

<!-- lcl: expect=fragment -->
```lcl
SCOPE:
    ID: scope.python_sources
    INCLUDE: GLOB("src/**/*.py")
    EXCLUDE: GLOB("src/generated/**")
```

EXCLUDE always wins inside the same scope. A GLOB in a scope is measured
from the workspace and cannot reach outside it. Scopes are how you say
"these files and no others", which replaces vague phrases like "the relevant
files". Chapter 12 uses scopes with rules.

## 11.8 Running programs

`core.execute` runs a program, which is how an automated coding task runs a
test suite, for example. Its TARGET is the program, and its parameters give
the `arguments` as a list of strings, and optionally a `working_directory`, a
`timeout` and `environment` additions:

<!-- lcl: expect=fragment -->
```lcl
ACTION:
    ID: action.test
    OPERATION: core.execute
    TARGET: PATH("/usr/bin/python3")
    PARAMETER:
        NAME: arguments
        TYPE: LIST[STRING]
        REQUIRED: TRUE
        VALUE: ["-m", "pytest", "-q"]
    PARAMETER:
        NAME: timeout
        TYPE: DURATION
        REQUIRED: TRUE
        VALUE: DURATION(5, unit.minute)
    OUTPUT: REF(output.test_log)
```

The result record holds `stdout` (the default field), `stderr` and
`exit_code`. A program that runs and exits with a non-zero code has still
*completed*: whether that counts as success is for your VERIFY to decide, for
example `ASSERT: REF(output.exit_code) == 0` on an output with
`PROPERTY: exit_code`. The specification's example
`08_EXAMPLES/VALID/04_AUTOMATED_CODING_TASK.lcl` shows a whole coding task
built this way.

Running a program needs `--allow-run <program>`.

> **Tool note.** The current `lcl` also requires the program's own path to
> be covered by `--allow-write`, so it will not run a program in a system
> folder such as `/usr/bin` unless that folder is write-granted. **Do not**
> grant write access to system folders to get around this. This manual
> therefore has no runnable `core.execute` example.

## Summary

* A WORKSPACE gives an absolute base folder, and `PATH(REF(ws), "rel")` names
  files inside it. Paths cannot escape it.
* `core.read`, `core.create`, `core.write` and `core.append` have strict
  preconditions: create never overwrites, write never creates by default.
* `lcl run` grants nothing by default. Use `--allow-read`, `--allow-write`,
  `--allow-run` and `--allow-net`, as narrowly as possible.
* With no capability the result is *blocked*; with a capability that does not
  cover the target it is *failed* with `error.permission.denied`.
* An action authorises exactly its own operation and target, nothing more.

## Exercises

1. Write a task that creates `/tmp/lcl-manual/diary/today.txt` containing one
   line of text, and verifies that the file was created. Run it with the
   narrowest grant that works.
2. Change the to-do task so that it uses `core.write` with
   `create_if_missing: TRUE` for the backup instead of `core.create`. What
   changes when you run it twice?
3. What would happen if `action.append` came *before* `action.read` in the
   TASK's list? Predict the backup file's content, then try it.
4. Explain in your own words why LCL has no "current directory".

Solutions: [solutions/README.md](solutions/README.md#chapter-11).
