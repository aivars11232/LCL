# Chapter 14. Projects and imports

**In this chapter you will learn**

* how to split definitions and rules into library documents;
* how IMPORT and namespaces work;
* how imported rules keep, or lose, their authority;
* what a project folder and `lcl.project.json` are;
* how a lock file pins exactly what was loaded.

## 14.1 Why split a document?

As tasks grow, some parts are shared: a school's safety rules, a company's
definitions of "customer" and "invoice", a set of constants. Copying them
into every task leads to copies that drift apart. Instead, put them in a
**library** (`KIND: kind.library`) and import it. Libraries hold
definitions, data and rules, but no actions and no EXECUTE (see Chapter 7).

## 14.2 A two-file project

<!-- lcl: file=examples/14/geometry/src/shapes.lcl expect=validate -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: lib.shapes
    NAME: "Shape constants"
    VERSION: "1.0.0"
    KIND: kind.library

DEFINE:
    ID: constant.sides_of_square
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 4
```

<!-- lcl: file=examples/14/geometry/src/main.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: app.perimeter
    NAME: "Perimeter of a square"
    VERSION: "1.0.0"
    KIND: kind.task

IMPORT:
    ID: import.shapes
    SOURCE: PATH("shapes.lcl")
    NAMESPACE: shapes
    VERSION: "1.0.0"

INPUT:
    ID: input.side
    TYPE: INTEGER
    VALUE: 5

OUTPUT:
    ID: output.perimeter
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.perimeter
    ASSERT: REF(output.perimeter) == 20

ACTION:
    ID: action.perimeter
    OPERATION: core.calculate
    TARGET: REF(input.side)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "target * REF(shapes.constant.sides_of_square)"
    OUTPUT: REF(output.perimeter)

VERIFY:
    ID: verify.perimeter
    ASSERT: REF(output.perimeter) == 20

SUCCESS:
    ID: success.perimeter
    ALL: [REF(verify.perimeter)]

TASK:
    ID: task.perimeter
    GOAL: REF(goal.perimeter)
    INPUT: REF(input.side)
    ACTION: REF(action.perimeter)
    OUTPUT: REF(output.perimeter)
    SUCCESS: REF(success.perimeter)

EXECUTE:
    REFERENCE: REF(task.perimeter)
```

An IMPORT block needs four fields:

* `ID`, as for any declaration;
* `SOURCE`, the document to load. `PATH("shapes.lcl")` is a relative path,
  measured from the importing document's own folder. This is the one place a
  single-string relative path is allowed. A web address (`URI(...)`) also
  needs a `CHECKSUM`, so the downloaded bytes are exactly the ones you
  checked.
* `NAMESPACE`, a new, unique prefix for everything the library declares;
* `VERSION`, the exact version of the library you expect. It must match the
  library's `SPECIFICATION.VERSION`.

Everything from the library is then named **with the prefix**:
`constant.sides_of_square` becomes `shapes.constant.sides_of_square`. Without
the prefix, the reference does not resolve:

```
error.reference.unresolved [resolution]: `REF(constant.sides_of_square)` in this fragment resolves to no declaration
```

Either file ending works on both sides of an import (see section 2.9). The
folder `examples/14/geometry_text/` is the same project with every document
saved as `.lcl.txt`: its entry is `src/main.lcl.txt`, which imports
`PATH("shapes.lcl.txt")`, and it gives exactly the same result.

<!-- lcl-run: file=examples/14/geometry_text/src/main.lcl.txt expect=run:succeeded -->
<!-- lcl-run: file=examples/14/geometry_text/src/shapes.lcl.txt expect=validate -->

This is deliberate: nothing from an import mixes silently into your own
names. The namespace rules:

* each import must have its own prefix, and no local ID may start with it;
* reserved names (`core`, `unit`, `error` and so on) cannot be prefixes, and
  `NAMESPACE: unit` is rejected with `error.namespace.invalid`;
* there are no wildcard imports, and import cycles (A imports B imports A)
  are invalid.

`lcl inspect` shows what was imported:

```
$ lcl inspect src/main.lcl
...
2 unit(s), 1 import(s), 11 declaration(s), 2 candidate node(s), 2 plan node(s)
  IMPORT PATH("shapes.lcl") as shapes -> loaded
  ...
```

## 14.3 Project folders

A **project** is a folder with a file `lcl.project.json` at its root:

```
geometry/
    lcl.project.json
    src/
        main.lcl
        shapes.lcl
```

```json
{
  "format": "lcl.project/1",
  "entry": "src/main.lcl"
}
```

* `format` must be exactly `"lcl.project/1"`.
* `entry` is the main document.
* An optional `"spec"` gives the path to the specification package, relative
  to the project root, so the project runs without `LCL_SPEC` or `--spec`.
  The examples here leave it out, because the right path depends on where you
  installed LCL. The repository's own `apps/` projects include it.

When you run a document, `lcl` looks for the nearest folder above it that
holds an `lcl.project.json`, and treats that as the project. You can also
name it with `--project <dir>`.

## 14.4 Imported rules and authority

Imported *rules* apply to the importing task too. This project imports a
school's safety rules, then tries to break one:

<!-- lcl: file=examples/14/register/src/safety.lcl expect=validate -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: lib.safety
    NAME: "School safety rules"
    VERSION: "1.0.0"
    KIND: kind.library
    AUTHORITY: 700

FORBID:
    ID: rule.no_overwrite_register
    OPERATION: core.write
    TARGET: PATH("/tmp/lcl-manual/imports/register.txt")
    AUTHORITY: 700
```

<!-- lcl: file=examples/14/register/src/main.lcl expect=reject:error.permission.denied -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: app.register
    NAME: "Try to overwrite the register"
    VERSION: "1.0.0"
    KIND: kind.task

IMPORT:
    ID: import.safety
    SOURCE: PATH("safety.lcl")
    NAMESPACE: safety
    VERSION: "1.0.0"
    AUTHORITY: 700

WORKSPACE:
    ID: workspace.school
    PATH: PATH("/tmp/lcl-manual/imports")
    MODE: mode.read_write

OUTPUT:
    ID: output.changed
    TYPE: BOOLEAN
    FORMAT: format.plain_text

GOAL:
    ID: goal.write
    ASSERT: REF(output.changed) == TRUE

ACTION:
    ID: action.write
    OPERATION: core.write
    TARGET: PATH("/tmp/lcl-manual/imports/register.txt")
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "empty"
    OUTPUT: REF(output.changed)

VERIFY:
    ID: verify.changed
    ASSERT: REF(output.changed) == TRUE

SUCCESS:
    ID: success.write
    ALL: [REF(verify.changed)]

TASK:
    ID: task.write
    GOAL: REF(goal.write)
    WORKSPACE: REF(workspace.school)
    ACTION: REF(action.write)
    OUTPUT: REF(output.changed)
    SUCCESS: REF(success.write)

EXECUTE:
    REFERENCE: REF(task.write)
```

```
$ lcl validate src/main.lcl
src/main.lcl:31:1: error.permission.denied [execution]: `action.write` invokes core.write, which `safety.rule.no_overwrite_register` forbids at authority 700 with no exact OVERRIDE naming a winner and a loser
```

The imported rule keeps its namespace (`safety.rule.no_overwrite_register`),
and the task is stopped before it touches the file.

**Authority is capped.** An imported rule's authority can never be higher
than the IMPORT's `AUTHORITY`, or the library's own. Change the IMPORT to
`AUTHORITY: 400` and the error reports the rule "at authority 400". A task
can choose to trust a library less than the library claims, but never more.

> **Tool note.** As in Chapter 12, the current `lcl` matches a FORBID's
> TARGET against an action's TARGET by the way the path is written. Here
> both use the same absolute path, so the rule is enforced. If the action
> wrote the same file as `PATH(REF(workspace.school), "register.txt")`,
> today's tool would not match it. Libraries cannot declare WORKSPACEs, so
> write shared path rules with absolute paths, and use the same absolute
> path in the task.

## 14.5 Lock files

Once a project works, you may want to be sure it keeps running *exactly*
the same documents: that nobody has edited the library since. A **lock file**
records the specification's identity and a fingerprint (SHA-256) of every
loaded document:

```
$ lcl package lock src/main.lcl
wrote .../geometry/lcl.lock
  spec 0.1.0 (00d648b1...)
  1aa4759f...  src/main.lcl
  aa918ee7...  src/shapes.lcl

$ lcl package verify src/main.lcl
.../geometry/lcl.lock matches what was loaded
```

Now edit `shapes.lcl`, for example changing the 4 to 5, and verify again:

```
$ lcl package verify src/main.lcl
.../geometry/lcl.lock does not describe what was loaded:
  src/shapes.lcl changed: locked aa918ee7..., now ffccf92c...
```

`lcl run --locked` refuses to run at all when the lock file disagrees
(exit code 4). Without `--locked`, the run goes ahead with the changed
library, and here the perimeter check fails. Commit `lcl.lock` along with your
documents, and use `--locked` wherever a run must be reproducible.

## Summary

* Libraries hold shared definitions and rules. IMPORT loads one under a
  mandatory NAMESPACE, and its declarations are named `prefix.id`.
* Import sources are relative to the importing document, and URI sources need
  a CHECKSUM.
* Imported rules apply to the importing task, with authority capped by the
  IMPORT.
* A project is a folder with `lcl.project.json`. `lcl package lock` /
  `verify` and `--locked` pin exactly what is loaded.

## Exercises

1. Add a second constant to `shapes.lcl`, `constant.sides_of_triangle`, and a
   second action to `main.lcl` that computes a triangle's perimeter with side
   7. Don't forget the checks.
2. Import `shapes.lcl` a second time under the namespace `more_shapes`. Is
   that allowed? What about importing it twice under the same namespace?
3. Change the IMPORT's `VERSION` to `"1.0.1"`. What happens, and why is that
   useful?
4. Lock the geometry project, change `main.lcl`'s input from 5 to 6 and its
   checks to match, then run `lcl run --locked`. What happens? What must you
   do before `--locked` accepts the change?

Solutions: [solutions/README.md](solutions/README.md#chapter-14).
