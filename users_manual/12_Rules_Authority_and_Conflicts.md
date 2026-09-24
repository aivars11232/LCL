# Chapter 12. Rules, authority and conflicts

**In this chapter you will learn**

* the five kinds of rule: REQUIRE, ALLOW, FORBID, PREFER and PRESERVE;
* how AUTHORITY and PRIORITY decide between rules;
* how to make one exact exception to a rule with OVERRIDE;
* what the tool does when two rules contradict each other.

Chapter 1's principles 5 and 6 said that hard rules and soft preferences are
distinct, and that permission is distinct from requirement. This chapter is
where those principles become syntax. The rules are in
`05_SEMANTICS/03_REQUIRE_ALLOW_FORBID_PREFER_PRESERVE_AND_ACTION_AUTHORIZATION.txt`
and `05_SEMANTICS/04_AUTHORITY_PRIORITY_OVERRIDE_AND_CONFLICT_RESOLUTION.txt`.

## 12.1 The five rules

| Rule | Plain English | Strength | Contains |
|---|---|---|---|
| `REQUIRE` | must | hard | exactly one `ASSERT` or `ACTION` |
| `FORBID` | must not | hard | `OPERATION` and `TARGET` |
| `ALLOW` | may | permission only | `OPERATION` and `TARGET` |
| `PREFER` | should, ideally | soft | exactly one `ASSERT` or `ACTION` |
| `PRESERVE` | must stay exactly as it was | hard | `TARGET`, optional `PROPERTY` |

Every rule has an `ID`, and may have `WHEN` (a condition for the rule to
apply), `SCOPE` (what it applies to), `AUTHORITY`, `PRIORITY` and
`DESCRIPTION`.

Some consequences are worth stating plainly:

* **ALLOW never makes anything happen.** It only permits. An ALLOW with no
  action that uses it does nothing.
* **A goal or a preference never implies permission.** "Ideally the report
  is on the website" does not permit publishing it.
* **PREFER can never defeat a hard rule.** When a preference and a hard rule
  disagree, the hard rule wins, always.
* **FORBID beats an action that asks for the forbidden thing.** Even an
  action you wrote yourself is blocked, unless an OVERRIDE (12.3) resolves
  that exact conflict.

## 12.2 Authority and priority

Documents, imported documents and individual rules each carry an
**AUTHORITY** from 0 to 1000. Higher is stronger. A document's authority is
set in `SPECIFICATION` and defaults to 500. A rule declares its own with
`AUTHORITY:`. An imported rule can never have more authority than the
document it came from or the IMPORT that brought it in (Chapter 14).

When two rules that apply to the same thing conflict:

1. **Higher authority wins.** A FORBID at 600 beats an ALLOW at 500.
2. **At equal authority, higher PRIORITY wins**, from -1000 to 1000,
   default 0, except that a priority can never set aside a hard rule that
   is independently required.
3. **Two hard rules at equal authority that contradict each other** are an
   error, `error.conflict.hard`, unless an OVERRIDE names which one wins.

Rules that do not conflict all apply together.

## 12.3 OVERRIDE: one exact exception

A school might have a general rule that reports are not written to disk, and
one exception for the end-of-term summary. In LCL, that is a FORBID, an ALLOW
of equal authority, and an OVERRIDE that names the exception as the winner:

<!-- lcl: file=examples/12/override.lcl expect=run:succeeded workspace=/tmp/lcl-manual/rules grant=w produces=summary.txt -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.override
    NAME: "A narrow exception to a rule"
    VERSION: "1.0.0"
    KIND: kind.task
    AUTHORITY: 600

WORKSPACE:
    ID: workspace.reports
    PATH: PATH("/tmp/lcl-manual/rules")
    MODE: mode.read_write

FORBID:
    ID: rule.no_summary_file
    OPERATION: core.create
    TARGET: PATH(REF(workspace.reports), "summary.txt")
    AUTHORITY: 600
    DESCRIPTION: "Normally, reports are not written to disk."

ALLOW:
    ID: permission.summary_file
    OPERATION: core.create
    TARGET: PATH(REF(workspace.reports), "summary.txt")
    AUTHORITY: 600
    DESCRIPTION: "The end-of-term summary is the one exception."

OVERRIDE:
    ID: override.summary_file
    WINNER: REF(permission.summary_file)
    LOSER: REF(rule.no_summary_file)

FORBID:
    ID: rule.never_delete
    OPERATION: core.delete
    TARGET: PATH(REF(workspace.reports), "summary.txt")
    AUTHORITY: 600

PREFER:
    ID: preference.short
    ASSERT: COUNT(REF(output.summary_text)) <= 80

OUTPUT:
    ID: output.summary_text
    TYPE: STRING
    FORMAT: format.plain_text

OUTPUT:
    ID: output.summary_file
    TYPE: PATH
    FORMAT: format.plain_text
    PROPERTY: target

GOAL:
    ID: goal.summary
    ASSERT: EXISTS(REF(output.summary_file))

ACTION:
    ID: action.text
    OPERATION: core.return
    TARGET: "Term report: 28 pupils, average mark 71."
    OUTPUT: REF(output.summary_text)

ACTION:
    ID: action.save
    OPERATION: core.create
    TARGET: PATH(REF(workspace.reports), "summary.txt")
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: REF(output.summary_text)
    OUTPUT: REF(output.summary_file)

VERIFY:
    ID: verify.saved
    ASSERT: EXISTS(REF(output.summary_file))

SUCCESS:
    ID: success.summary
    ALL: [REF(verify.saved)]

TASK:
    ID: task.summary
    GOAL: REF(goal.summary)
    WORKSPACE: REF(workspace.reports)
    ACTION: [REF(action.text), REF(action.save)]
    OUTPUT: [REF(output.summary_text), REF(output.summary_file)]
    SUCCESS: REF(success.summary)

EXECUTE:
    REFERENCE: REF(task.summary)
```

```
$ mkdir -p /tmp/lcl-manual/rules
$ lcl run --allow-write /tmp/lcl-manual/rules override.lcl
status.succeeded
  ...
  OUTPUT output.summary_file published = PATH("/tmp/lcl-manual/rules/summary.txt")
```

Now experiment. Each variation below is in `examples/12/` and has been
checked:

<!-- lcl-run: file=examples/12/no_override.lcl expect=reject:error.permission.denied -->
<!-- lcl-run: file=examples/12/low_winner.lcl expect=reject:error.override.invalid -->

**Remove the OVERRIDE** (`no_override.lcl`). The ALLOW alone does not defeat
the FORBID, so `lcl validate` stops the action before anything runs:

```
error.permission.denied [execution]: `action.save` invokes core.create, which `rule.no_summary_file` forbids at authority 600 with no exact OVERRIDE naming a winner and a loser
```

**Give the ALLOW a lower authority, 400** (`low_winner.lcl`). An OVERRIDE may
not make a weaker rule beat a stronger one:

```
error.override.invalid [resolution]: `override.summary_file` names `permission.summary_file` at authority 400 as winner over `rule.no_summary_file` at authority 600; a lower-authority winner is invalid
```

The rules for OVERRIDE:

* It names exactly one `WINNER` and one `LOSER`. It never applies "to rules
  like this one" or to a whole category.
* The winner must not have lower authority than the loser.
* It may have a `WHEN`, so the exception applies only in a stated situation.

> **Tool note.** The current `lcl` does not yet evaluate `WHEN` on rule
> clauses (OVERRIDE, FORBID, ALLOW): the rule applies as if WHEN were absent,
> even when it is FALSE. Until this is supported, do not rely on WHEN to limit
> an OVERRIDE. Write a separate, narrower ALLOW and OVERRIDE for each
> situation instead.

`rule.never_delete` shows one more thing: FORBIDs can sit in a document
purely as protection, whatever the task does. Here they guarantee that this
task can never delete the summary.

> **Tool note.** The current `lcl` enforces a FORBID when the rule's TARGET
> and the action's TARGET are written as the same **literal path**, as in
> this example. Two forms that the specification treats as matching are not
> yet enforced: a FORBID whose TARGET is a SCOPE built from a GLOB
> (such as `GLOB("**")`), and an action whose TARGET is given indirectly as
> `REF(output.x).TARGET`. Until the tool catches up, write safety-critical
> FORBIDs with the exact literal path the action uses. Grant only the
> narrowest `--allow-write` paths (Chapter 11) as a second line of defence.

## 12.4 Contradicting hard rules

Two REQUIREs that cannot both be true, at the same authority, with no
OVERRIDE, make the document invalid:

<!-- lcl: file=examples/12/conflict.invalid.lcl expect=reject:error.conflict.hard -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.conflict
    NAME: "Two rules that cannot both hold"
    VERSION: "1.0.0"
    KIND: kind.library

DATA:
    ID: data.uniform_required
    TYPE: BOOLEAN
    VALUE: TRUE

REQUIRE:
    ID: rule.wear_uniform
    ASSERT: REF(data.uniform_required) == TRUE

REQUIRE:
    ID: rule.no_uniform
    ASSERT: REF(data.uniform_required) == FALSE
```

```
$ lcl validate conflict.invalid.lcl
conflict.invalid.lcl:17:13: error.conflict.hard [resolution]: `rule.wear_uniform` asserts REF(data.uniform_required) == TRUE and `rule.no_uniform` asserts REF(data.uniform_required) == FALSE at the same authority 500; no exact OVERRIDE names a winner and a loser
```

`lcl check` does not report this. Conflicts are decided in the rules stage,
which `validate` reaches and `check` does not. Get into the habit of running
`lcl validate` before `lcl run`.

## 12.5 REQUIRE and PRESERVE

A **REQUIRE** with an `ASSERT` states a condition that must hold. It is a
hard rule, so by the specification a task cannot succeed while an applicable
REQUIRE is FALSE. A REQUIRE with an `ACTION` makes that action mandatory.

<!-- lcl: expect=fragment -->
```lcl
REQUIRE:
    ID: rule.positive_quantity
    ASSERT: REF(input.quantity) > 0
```

A **PRESERVE** says that a target, or selected properties of it, must be
exactly the same after the task as before:

<!-- lcl: expect=fragment -->
```lcl
PRESERVE:
    ID: rule.keep_original
    TARGET: PATH(REF(workspace.project), "data/original.csv")
```

> **Tool note.** The current `lcl` detects contradictions *between*
> REQUIREs (12.4), but it does not yet evaluate a REQUIRE's ASSERT, or check
> a PRESERVE, when a task runs. A run can end `status.succeeded` even though
> a REQUIRE was FALSE or a preserved file changed. The specification is
> clear that both must hold, and an AI interpreter must honour them. When
> you need the reference tool to *enforce* a condition, also write it as a
> VALIDATE (before the work) or a VERIFY (after the work). Those are always
> checked (Chapter 9).

## 12.6 PREFER

A **PREFER** records a soft objective:

<!-- lcl: expect=fragment -->
```lcl
PREFER:
    ID: preference.short
    ASSERT: COUNT(REF(output.summary_text)) <= 80
```

It guides a choice where there is one, such as the wording an AI chooses. It
never blocks success, never permits an action, and always gives way to a
hard rule. If "short" is essential, it is not a preference: make it a
VERIFY.

## 12.7 Choosing the right rule

| You want to say | Use |
|---|---|
| "The quantity must be positive." | `VALIDATE` (checked before), and `REQUIRE` to state it as a rule |
| "The output must mention the date." | `VERIFY` |
| "Never delete anything in `data/`." | `FORBID` with `core.delete` |
| "You may create files in `out/`." | `ALLOW` with `core.create` |
| "Keep the original data unchanged." | `PRESERVE` |
| "Shorter is better." | `PREFER` |
| "Just this once, the rule does not apply." | `ALLOW` plus `OVERRIDE` |

## Summary

* REQUIRE, FORBID and PRESERVE are hard. ALLOW only permits. PREFER is soft.
* Higher AUTHORITY wins a conflict. PRIORITY breaks ties at equal authority.
* An OVERRIDE names one exact WINNER and LOSER; a lower-authority winner is
  invalid.
* Contradicting hard rules at equal authority are `error.conflict.hard`,
  found by `lcl validate`.
* With the current tool, enforce important conditions with VALIDATE and
  VERIFY, and write FORBIDs on literal paths.

## Exercises

1. Add a FORBID to `override.lcl` that forbids `core.write` on the summary
   file. Does the task still succeed? Why?
2. Add a second action to `override.lcl` that creates `backup.txt` in the
   same folder, and a FORBID at authority 600 of `core.create` on that file.
   What does `lcl validate` report? Then add what is needed so the backup is
   permitted too.
3. Rewrite this request as LCL rules, without writing the actions: "You must
   not delete or move anything in `/srv/archive`. You may create new files in
   `/srv/archive/incoming`. Keep `/srv/archive/index.csv` unchanged. Prefer
   file names without spaces."
4. Why is "an ALLOW at authority 600 beats a FORBID at authority 500" a
   reasonable rule, but "an ALLOW beats a FORBID at the same authority" a
   dangerous one?

Solutions: [solutions/README.md](solutions/README.md#chapter-12).
