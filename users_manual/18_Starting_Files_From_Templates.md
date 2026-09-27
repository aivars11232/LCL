# Chapter 18. Starting files from roles and templates

**In this chapter you will learn**

* what a file *role* is in a Core 0.3.0 project;
* what the Minimal and Guided starting structures contain;
* how to fill in a *slot*, and why an unfilled slot stops the file;
* what Master templates and project Masters are, and where they live;
* which LCL version a template works with.

> **Tool note.** This chapter describes LCL Core 0.3.0, which is still a
> candidate, not the released Core 0.1.0 the rest of this manual teaches. The
> reference tools make these starting structures today; the workspace menus
> that offer them arrive in a later update.

## 18.1 Files have roles

A Core 0.3.0 project is one specification split over several files. The
project entry (usually `main.lcl`) lists every other file in a `PART` block,
and each of those files says what it is with its `SPECIFICATION KIND`. That
kind is the file's *role*. There are eight:

| Role | `KIND` | What goes in it |
|---|---|---|
| Task | `kind.part.task` | the work: goals, actions, tasks, checks of the task |
| Description | `kind.part.description` | plain explanation for people; it has no effect |
| Rules | `kind.part.rules` | `REQUIRE`, `ALLOW`, `FORBID` and the other rules |
| Context | `kind.part.context` | `WORKSPACE`, `CONTEXT`, `MEMORY`, `STATE` |
| Data | `kind.part.data` | `DATA`, `INPUT`, `ASSUME` |
| Output | `kind.part.output` | `OUTPUT` declarations |
| Checks | `kind.part.checks` | `VALIDATE`, `VERIFY`, `TEST`, `SUCCESS` and the like |
| Definitions | `kind.part.definitions` | `DEFINE` declarations |

A file of one role may only contain the blocks that role allows; the
specification lists them, and a file that breaks the list is rejected. When
you create a file by role, LCL starts it with the structure of that role
instead of a blank page. A blank file is still possible for advanced use.

## 18.2 Slots

LCL has no comment symbol, so a starting file cannot hold a greyed-out
example value. Instead, a place you still have to fill is a **slot**: a field
line with nothing after its colon.

<!-- lcl: expect=fragment -->
```lcl
    NAME:
```

A slot is not a value. The tool rejects a file that still has one
(`error.indentation.empty_block`, at the line after the slot), so an unfilled
file can never be checked, validated or run by mistake. Fill the slot by
typing the value after the colon:

<!-- lcl: expect=fragment -->
```lcl
    NAME: "Double one number"
```

Some slots are **required** (the file needs that field) and some are
**optional** (fill it in or delete the whole line). Editors can show which is
which. Two more kinds of line appear in a new file:

* **generated IDs**, such as `ID: goal.build`. LCL makes them from the file
  name without its ending (`build.lcl` gives `build`), so the file name must
  be a valid ID segment: lowercase letters, digits and `_`, starting with a
  letter. You may rename them.
* **guidance**, a `COMMENT` block explaining the file. A `COMMENT` has no
  effect on what the program does. Delete it whenever you like.

## 18.3 Minimal and Guided

Every role has two starting structures.

**Minimal** holds only what the specification requires of a file of that
role: the `LCL` line and the `SPECIFICATION` block. Here is a Minimal task
file named `task.lcl`:

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.3.0"

SPECIFICATION:
    ID: specification.task
    NAME:
    VERSION:
    KIND: kind.part.task
```

The project entry is the one file with more: it must list its parts and it
must have exactly one `EXECUTE`. A Minimal project is `main.lcl` and one task
file:

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.3.0"

SPECIFICATION:
    ID: specification.main
    NAME:
    VERSION:
    KIND: kind.project

PART:
    ID: part.task
    SOURCE: PATH("task.lcl")
    KIND: kind.part.task

EXECUTE:
    REFERENCE:
```

**Guided** adds an optional `DESCRIPTION` slot, a guidance `COMMENT` that
lists the blocks the role allows, and the blocks most files of that role
need, with their required fields as slots. A Guided project has `main.lcl`,
`description.lcl`, `rules.lcl` and `task.lcl`.

Every part declares the same `SPECIFICATION VERSION` as the entry, so fill in
the same version everywhere.

## 18.4 Guided examples, role by role

### Task

`TASK` needs a goal, a success condition and at least one `PHASE`,
`SEQUENCE` or `ACTION`. The Guided task file declares a goal, an action and
a success condition, and already wires the `TASK` to them with `REF`:

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.3.0"

SPECIFICATION:
    ID: specification.task
    NAME:
    VERSION:
    KIND: kind.part.task
    DESCRIPTION:

COMMENT:
    CONTENT: "This file is the task part of a project. The blocks allowed here are ACTION, ALLOW, ASSUME, COMMENT, CONTEXT, DATA, DEFINE, DEPENDENCY, EVIDENCE, EXAMPLE, FAILURE, FORBID, GOAL, HANDLER, INPUT, MEMORY, OUTPUT, OVERRIDE, PHASE, PREFER, PRESERVE, REQUIRE, SCOPE, SEQUENCE, STATE, SUCCESS, TASK, TEST, VALIDATE, VERIFY, WORKSPACE. A field with nothing after its colon is a slot to fill in; an optional slot you do not need can be deleted."

GOAL:
    ID: goal.task
    ASSERT:

ACTION:
    ID: action.task
    OPERATION:

SUCCESS:
    ID: success.task
    ALL:

TASK:
    ID: task.task
    GOAL: REF(goal.task)
    ACTION: REF(action.task)
    SUCCESS: REF(success.task)
```

### Description

The only content of a description file is text for people:

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.3.0"

SPECIFICATION:
    ID: specification.description
    NAME:
    VERSION:
    KIND: kind.part.description
    DESCRIPTION:

COMMENT:
    CONTENT: "This file is the description part of a project. The blocks allowed here are COMMENT, EXAMPLE. A field with nothing after its colon is a slot to fill in; an optional slot you do not need can be deleted."

COMMENT:
    CONTENT:
```

### Rules

A hard requirement and a hard prohibition, the two most common rules:

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.3.0"

SPECIFICATION:
    ID: specification.rules
    NAME:
    VERSION:
    KIND: kind.part.rules
    DESCRIPTION:

COMMENT:
    CONTENT: "This file is the rules part of a project. The blocks allowed here are ALLOW, COMMENT, EXAMPLE, FORBID, OVERRIDE, PREFER, PRESERVE, REQUIRE, SCOPE. A field with nothing after its colon is a slot to fill in; an optional slot you do not need can be deleted."

REQUIRE:
    ID: require.rules
    ASSERT:

FORBID:
    ID: forbid.rules
    OPERATION:
    TARGET:
```

### Context

`CONTEXT` needs a type, a scope, and a `VALUE` or a `SOURCE`. Its `SCOPE`
names a `SCOPE` block, which can live in a rules file:

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.3.0"

SPECIFICATION:
    ID: specification.context
    NAME:
    VERSION:
    KIND: kind.part.context
    DESCRIPTION:

COMMENT:
    CONTENT: "This file is the context part of a project. The blocks allowed here are COMMENT, CONTEXT, EXAMPLE, MEMORY, STATE, WORKSPACE. A field with nothing after its colon is a slot to fill in; an optional slot you do not need can be deleted."

CONTEXT:
    ID: context.context
    TYPE:
    SCOPE:
    VALUE:
```

### Data

An input and a piece of typed data:

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.3.0"

SPECIFICATION:
    ID: specification.data
    NAME:
    VERSION:
    KIND: kind.part.data
    DESCRIPTION:

COMMENT:
    CONTENT: "This file is the data part of a project. The blocks allowed here are ASSUME, COMMENT, DATA, EXAMPLE, INPUT. A field with nothing after its colon is a slot to fill in; an optional slot you do not need can be deleted."

INPUT:
    ID: input.data
    TYPE:
    VALUE:

DATA:
    ID: data.data
    TYPE:
    VALUE:
```

### Output

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.3.0"

SPECIFICATION:
    ID: specification.output
    NAME:
    VERSION:
    KIND: kind.part.output
    DESCRIPTION:

COMMENT:
    CONTENT: "This file is the output part of a project. The blocks allowed here are COMMENT, EXAMPLE, OUTPUT. A field with nothing after its colon is a slot to fill in; an optional slot you do not need can be deleted."

OUTPUT:
    ID: output.output
    TYPE:
    FORMAT:
```

### Checks

`VALIDATE` runs before the first side effect; `VERIFY` runs once what it
checks exists:

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.3.0"

SPECIFICATION:
    ID: specification.checks
    NAME:
    VERSION:
    KIND: kind.part.checks
    DESCRIPTION:

COMMENT:
    CONTENT: "This file is the checks part of a project. The blocks allowed here are COMMENT, EVIDENCE, EXAMPLE, FAILURE, SUCCESS, TEST, VALIDATE, VERIFY. A field with nothing after its colon is a slot to fill in; an optional slot you do not need can be deleted."

VALIDATE:
    ID: validate.checks
    ASSERT:

VERIFY:
    ID: verify.checks
    ASSERT:
```

### Definitions

`DEFINE` needs a `KIND`; `MEANING` is optional:

<!-- lcl: expect=fragment -->
```lcl
LCL:
    VERSION: "0.3.0"

SPECIFICATION:
    ID: specification.definitions
    NAME:
    VERSION:
    KIND: kind.part.definitions
    DESCRIPTION:

COMMENT:
    CONTENT: "This file is the definitions part of a project. The blocks allowed here are COMMENT, DEFINE, EXAMPLE. A field with nothing after its colon is a slot to fill in; an optional slot you do not need can be deleted."

DEFINE:
    ID: define.definitions
    KIND:
    MEANING:
```

## 18.5 Master templates

If you start many files the same way, save that start as a **Master
template**: your own starting text for one role and one LCL version. A new
file of that role then starts as an exact copy of your Master.

A Master is a small JSON file:

```json
{"format": 1, "id": "my-task", "name": "My task", "core": "0.3.0",
 "role": "kind.part.task", "text": "LCL:\n    VERSION: \"0.3.0\"\n..."}
```

* `id` names the file: lowercase letters, digits, `_` and `-`.
* `name` is what you see in lists.
* `core` is the one LCL version the Master is for.
* `role` is one of the eight roles.
* `text` is the new file's exact text. It may contain slots.

Masters live in `~/.config/lcl/masters/` (or `$XDG_CONFIG_HOME/lcl/masters/`),
one `<id>.json` each. `defaults.json` in the same folder names the default
Master of a role.

A Master is checked before it is saved and whenever it is used. It is
refused when the JSON has an unknown or missing key, when `core` is not the
version the tools use, when its text does not declare exactly that
`LCL VERSION` and its `role` as `SPECIFICATION KIND`, when it uses a block
the role does not allow or a field the block does not have, when a slot is
anything but a field line of a top-level block, or when the text with its
slots filled would not be structurally valid. An invalid Master can never
become a default.

**Which start a new file gets.** A Master you pick explicitly comes first,
then the default Master for the role, then the built-in Minimal or Guided
structure. If the Master you picked, or the default, is missing or invalid,
creating the file fails with a message; LCL never quietly uses something
else.

**Copies, not links.** A file made from a Master gets the Master's text once,
when it is created. The file does not remember the Master. Editing or
deleting a Master changes only files you create later, and editing your file
never changes the Master.

## 18.6 Project Masters

A project Master chooses the files a new project starts with:

```json
{"format": 1, "id": "web", "name": "Web project", "core": "0.3.0",
 "role": "kind.project", "mode": "guided", "entry": "main.lcl",
 "parts": [
   {"path": "description.lcl", "role": "kind.part.description"},
   {"path": "task/build.lcl", "role": "kind.part.task", "master": "my-task"},
   {"path": "notes.lcl", "role": "kind.part.description", "required": false}
 ]}
```

The entry sits in the project folder itself and is always made from this
list: one `PART` per file, in this order, with `REQUIRED: FALSE` for an
optional part. A part with `master` starts from that role Master; the others
start from the role's default Master, or the built-in structure in `mode`.
Paths are relative, use `/`, and each file name must be a different ID
segment, because the generated IDs end with it.

Before anything is written, you see every file exactly as it will be
created. Creating then writes all of them or none: it never replaces an
existing file, and if anything fails part-way it removes what it made.

## 18.7 Versions

A Master targets exactly one LCL version. The tools refuse a Master for
another version instead of reading it under different rules, so a Master
made for 0.3.0 never silently changes meaning. File roles and projects
exist from Core 0.3.0; documents for 0.1.0 and 0.2.0 keep working as
before, as single files.

Localized projects work file by file. A starting structure is made in a
locale only when that locale's profile spells every keyword it needs, since
one file cannot mix spellings; otherwise LCL says which words are missing.

## Summary

* A project file's role is its `SPECIFICATION KIND`; each role allows its
  own blocks.
* A slot is a field line with nothing after its colon. The file cannot run
  until every slot is filled or, if optional, deleted.
* Minimal is only what the role requires; Guided adds the common blocks and
  a guidance `COMMENT`.
* Masters are your own starting texts: explicit choice first, then the
  default, then the built-in structure. They are copied, never linked.
* A project Master lists the files of a new project; you see them before
  they are written, and they are written all or nothing.
