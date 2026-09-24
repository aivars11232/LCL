# Chapter 16. Capstone project

**In this chapter you will**

* turn a plain-English request into a complete LCL task, step by step;
* use most of what the course has covered in one document;
* run it, prove it worked, and see its safety rules hold.

## 16.1 The request

A teacher writes:

> At the end of term, take the class results. Anyone with 50 or more has
> passed. Put the pupils who passed in order from highest mark to lowest, save
> that honour roll in the reports folder, and tell me who came top and what
> percentage of the class passed. Last term's report is in the same folder:
> never overwrite it or delete it. And tell me clearly whether it all worked.

## 16.2 Finding the parts

Before writing any LCL, go through the request phrase by phrase and decide
what each becomes. This is the most important step. Anything vague here will
be vague in the document.

| The request says | Question to settle | LCL |
|---|---|---|
| "the class results" | What exactly is a result? | `DEFINE type.result`: an object with `pupil` (STRING) and `mark` (INTEGER, 0 to 100) |
| | Where do they come from? | `INPUT input.results`, with PROVENANCE saying who entered them |
| "50 or more has passed" | Is 50 itself a pass? Yes: "or more". | `DEFINE term.pass` (the meaning) and `constant.pass_mark` = 50 |
| "put ... in order from highest to lowest" | Order by what? | `core.sort` with key `"mark"`, direction `descending` |
| "save that honour roll in the reports folder" | Which folder, which file? | `WORKSPACE workspace.reports`; `core.create` of `honour_roll.txt` |
| "who came top" | | `output.top_pupil`: the first entry of the honour roll |
| "what percentage passed" | Rounded how? | `output.pass_rate`: `ROUND(... * 100 / ..., 1)` |
| "never overwrite it or delete it" | Which operations exactly? | two FORBIDs: `core.write` and `core.delete` on `last_term.txt` |
| "tell me clearly whether it all worked" | What counts as "worked"? | VERIFY checks, a SUCCESS, and a FAILURE for a worrying pass rate |

Two gaps in the request need a decision. Write them down, don't guess
silently:

* *What if the class list is empty?* The pass rate would divide by zero.
  Decision: refuse to run, with a VALIDATE.
* *What if hardly anyone passes?* The teacher would want to know. Decision:
  a pass rate below 50% makes the run fail, with a FAILURE clause.

## 16.3 Building it

Build the document in the same order as the table, running `lcl check` after
each part. Chapter 15 showed how to read what it says.

1. **Header and identity**: `LCL`, `SPECIFICATION` with `DOMAIN:
   "education"`.
2. **Meaning**: the term, the constant and the type.
3. **Data and input**: one DATA per pupil, gathered into `input.results`.
   Because `type.result` limits marks to 0–100, a mark of 105 would be
   rejected by `lcl check` before anything runs, so no separate check is
   needed for it.
4. **Where**: the WORKSPACE, then the two FORBIDs, written with the same
   literal path style an action would use (Chapter 12's tool note).
5. **Results**: the five OUTPUTs and the EVIDENCE of where the marks came
   from.
6. **Work**: five actions, grouped into an *analyse* phase and a *publish*
   phase that must come after it.
7. **Proof**: the VALIDATE, the VERIFY checks, the FAILURE and the SUCCESS.
8. **Assembly**: the TASK and EXECUTE.

## 16.4 The complete document

<!-- lcl: file=examples/16/class_report.lcl expect=run:succeeded workspace=/tmp/lcl-manual/capstone seed=examples/16/seed grant=w produces=honour_roll.txt -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.class_report
    NAME: "End-of-term class report"
    VERSION: "1.0.0"
    KIND: kind.task
    DOMAIN: "education"
    DESCRIPTION: "Capstone project of the LCL users manual."

DEFINE:
    ID: term.pass
    KIND: kind.term
    MEANING: "A pupil passes the term with a mark of 50 or more out of 100."

DEFINE:
    ID: constant.pass_mark
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 50

DEFINE:
    ID: type.result
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: pupil
        TYPE: STRING
        REQUIRED: TRUE
    FIELD:
        NAME: mark
        TYPE: INTEGER
        REQUIRED: TRUE
        MINIMUM: 0
        MAXIMUM: 100

DATA:
    ID: data.ada
    TYPE: OBJECT[REF(type.result)]
    VALUE:
        pupil: "Ada"
        mark: 91

DATA:
    ID: data.alan
    TYPE: OBJECT[REF(type.result)]
    VALUE:
        pupil: "Alan"
        mark: 48

DATA:
    ID: data.grace
    TYPE: OBJECT[REF(type.result)]
    VALUE:
        pupil: "Grace"
        mark: 77

DATA:
    ID: data.katherine
    TYPE: OBJECT[REF(type.result)]
    VALUE:
        pupil: "Katherine"
        mark: 84

DATA:
    ID: data.tim
    TYPE: OBJECT[REF(type.result)]
    VALUE:
        pupil: "Tim"
        mark: 50

INPUT:
    ID: input.results
    TYPE: LIST[OBJECT[REF(type.result)]]
    VALUE: [REF(data.ada), REF(data.alan), REF(data.grace), REF(data.katherine), REF(data.tim)]
    PROVENANCE: "Marks entered by the class teacher on 2026-07-10."

WORKSPACE:
    ID: workspace.reports
    PATH: PATH("/tmp/lcl-manual/capstone")
    MODE: mode.read_write

FORBID:
    ID: rule.keep_last_term_write
    OPERATION: core.write
    TARGET: PATH(REF(workspace.reports), "last_term.txt")
    DESCRIPTION: "Last term's report must never be overwritten."

FORBID:
    ID: rule.keep_last_term_delete
    OPERATION: core.delete
    TARGET: PATH(REF(workspace.reports), "last_term.txt")
    DESCRIPTION: "Last term's report must never be deleted."

OUTPUT:
    ID: output.passed
    TYPE: LIST[OBJECT[REF(type.result)]]
    FORMAT: format.json

OUTPUT:
    ID: output.honour_roll
    TYPE: LIST[OBJECT[REF(type.result)]]
    FORMAT: format.json

OUTPUT:
    ID: output.top_pupil
    TYPE: STRING
    FORMAT: format.plain_text

OUTPUT:
    ID: output.pass_rate
    TYPE: DECIMAL
    FORMAT: format.plain_text

OUTPUT:
    ID: output.saved
    TYPE: BOOLEAN
    FORMAT: format.plain_text

EVIDENCE:
    ID: evidence.marks_source
    TYPE: STRING
    VALUE: "Marks entered by the class teacher on 2026-07-10."
    PROVENANCE: "INPUT input.results."
    REQUIRED: TRUE

GOAL:
    ID: goal.report
    ASSERT: REF(output.saved) == TRUE AND COUNT(REF(output.honour_roll)) == COUNT(REF(output.passed))

VALIDATE:
    ID: validate.class_not_empty
    ASSERT: COUNT(REF(input.results)) >= 1

ACTION:
    ID: action.passed
    OPERATION: core.filter
    TARGET: REF(input.results)
    PARAMETER:
        NAME: predicate
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "(item).mark >= REF(constant.pass_mark)"
    OUTPUT: REF(output.passed)

ACTION:
    ID: action.rank
    OPERATION: core.sort
    TARGET: REF(output.passed)
    PARAMETER:
        NAME: key
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "mark"
    PARAMETER:
        NAME: direction
        TYPE: ENUM
        REQUIRED: TRUE
        VALUE: descending
    OUTPUT: REF(output.honour_roll)

ACTION:
    ID: action.top_pupil
    OPERATION: core.calculate
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(output.honour_roll)[0].pupil"
    OUTPUT: REF(output.top_pupil)

ACTION:
    ID: action.pass_rate
    OPERATION: core.calculate
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "ROUND(COUNT(REF(output.passed)) * 100 / COUNT(REF(input.results)), 1)"
    OUTPUT: REF(output.pass_rate)

ACTION:
    ID: action.save
    OPERATION: core.create
    TARGET: PATH(REF(workspace.reports), "honour_roll.txt")
    PARAMETER:
        NAME: content
        TYPE: LIST[OBJECT[REF(type.result)]]
        REQUIRED: TRUE
        VALUE: REF(output.honour_roll)
    OUTPUT: REF(output.saved)

PHASE:
    ID: phase.analyse
    STEP:
        ID: step.passed
        ACTION: REF(action.passed)
    STEP:
        ID: step.rank
        ACTION: REF(action.rank)
    STEP:
        ID: step.top_pupil
        ACTION: REF(action.top_pupil)
    STEP:
        ID: step.pass_rate
        ACTION: REF(action.pass_rate)

PHASE:
    ID: phase.publish
    AFTER: REF(phase.analyse)
    STEP:
        ID: step.save
        ACTION: REF(action.save)

VERIFY:
    ID: verify.only_passes
    ASSERT: COUNT(REF(output.passed)) == 4
    EVIDENCE: REF(evidence.marks_source)

VERIFY:
    ID: verify.ranked
    ASSERT: REF(output.honour_roll)[0].mark >= REF(output.honour_roll)[3].mark

VERIFY:
    ID: verify.top_pupil
    ASSERT: REF(output.top_pupil) == "Ada"

VERIFY:
    ID: verify.pass_rate
    ASSERT: REF(output.pass_rate) == 80.0

VERIFY:
    ID: verify.saved
    ASSERT: REF(output.saved) == TRUE

FAILURE:
    ID: failure.low_pass_rate
    WHEN: REF(output.pass_rate) < 50.0
    STATUS: status.failed

SUCCESS:
    ID: success.report
    ALL: [REF(verify.only_passes), REF(verify.ranked), REF(verify.top_pupil), REF(verify.pass_rate), REF(verify.saved)]

TASK:
    ID: task.class_report
    GOAL: REF(goal.report)
    INPUT: REF(input.results)
    WORKSPACE: REF(workspace.reports)
    PHASE: [REF(phase.analyse), REF(phase.publish)]
    OUTPUT: [REF(output.passed), REF(output.honour_roll), REF(output.top_pupil), REF(output.pass_rate), REF(output.saved)]
    SUCCESS: REF(success.report)

EXECUTE:
    REFERENCE: REF(task.class_report)
```

A few details worth a second look:

* **`(item).mark`** in the filter's predicate. Written as `item.mark`, it
  would be read as one dotted identifier (Chapter 6). The parentheses make
  `item` a value, and `.mark` a field of it.
* **The pass mark lives in one place.** The predicate says
  `REF(constant.pass_mark)`, not `50`. If the school changes the pass mark,
  one line changes.
* **The sort direction** uses the ENUM form (`TYPE: ENUM`,
  `VALUE: descending`), as Chapter 10 recommends.
* **The phases.** `phase.publish` is `AFTER: REF(phase.analyse)`, so the file
  is written only once the honour roll exists.
* **The checks test the goal.** The VERIFY checks confirm exactly what the
  teacher asked for: only passes, correctly ranked, the right top pupil, the
  right percentage, and the file saved.

## 16.5 Running it

Prepare the reports folder with last term's file:

```
$ mkdir -p /tmp/lcl-manual/capstone
$ printf 'Last term: Ada 88, Grace 80\n' > /tmp/lcl-manual/capstone/last_term.txt
```

Validate first, then run with the one permission the task needs:

```
$ lcl validate class_report.lcl
class_report.lcl passed every stage through preflight.
...

$ lcl run --allow-write /tmp/lcl-manual/capstone class_report.lcl
status.succeeded
  VERIFY verify.only_passes = TRUE (required)
  VERIFY verify.ranked = TRUE (required)
  VERIFY verify.top_pupil = TRUE (required)
  VERIFY verify.pass_rate = TRUE (required)
  VERIFY verify.saved = TRUE (required)
  SUCCESS success.report ALL = TRUE
  EVIDENCE evidence.marks_source satisfied "Marks entered by the class teacher on 2026-07-10."
  OUTPUT output.passed published = [{mark: 91, pupil: "Ada"}, {mark: 77, pupil: "Grace"}, {mark: 84, pupil: "Katherine"}, {mark: 50, pupil: "Tim"}]
  OUTPUT output.honour_roll published = [{mark: 91, pupil: "Ada"}, {mark: 84, pupil: "Katherine"}, {mark: 77, pupil: "Grace"}, {mark: 50, pupil: "Tim"}]
  OUTPUT output.top_pupil published = "Ada"
  OUTPUT output.pass_rate published = 80.0
  OUTPUT output.saved published = TRUE
  because: SuccessSatisfied
  13 invocation(s), 0 event(s), 21 step(s)

$ cat /tmp/lcl-manual/capstone/honour_roll.txt
[{mark: 91, pupil: "Ada"}, {mark: 84, pupil: "Katherine"}, {mark: 77, pupil: "Grace"}, {mark: 50, pupil: "Tim"}]
```

Tim, with exactly 50, is on the honour roll: "50 or more". Alan, with 48, is
not. The file holds the list in LCL's own value notation. That is how the
reference tool writes a list value, so it is not strictly JSON, even though
the OUTPUT's FORMAT says what the value *is*.

`lcl inspect class_report.lcl` shows the plan: the task, the analyse phase
and its four steps, then the publish phase and its one step.

## 16.6 The safety rules at work

Suppose someone later edits the save action carelessly, to write to
`last_term.txt` with `core.write`. The FORBID stops it before anything runs:

<!-- lcl-run: file=examples/16/class_report_mistake.lcl expect=reject:error.permission.denied -->

```
$ lcl validate class_report_mistake.lcl
class_report_mistake.lcl:183:1: error.permission.denied [execution]: `action.save` invokes core.write, which `rule.keep_last_term_write` forbids at authority 500 with no exact OVERRIDE naming a winner and a loser
```

The rule was written once, near the top of the document, and it protects the
file from every action in it, including ones added later by someone who
never read the rule.

## 16.7 Where to go from here

Suggested projects, roughly in order of difficulty:

1. **Grade bands.** Add outputs listing the pupils in each band (70+, 50–69,
   below 50), with checks that every pupil appears in exactly one band.
2. **Shared school rules.** Move the FORBIDs and the pass-mark constant into a
   library, `school_rules.lcl`, and import it (Chapter 14). Lock the project.
3. **A test document.** Write `class_report_test.lcl` (`kind.test`) that runs
   `action.rank` on a small list and compares the result with the expected
   order (Chapter 9).
4. **Your own domain.** Take a real request from your own life, a shopping
   list, a training plan or a revision timetable, and write it as LCL. Start
   with the table in 16.2.

## Summary

* Start from the request, not from the syntax. Settle every vague phrase
  (what, where, which, how exact) and write down the decisions for gaps.
* Build in layers, checking as you go: identity, meaning, data, place and
  rules, results, work, proof, assembly.
* Put important numbers and meanings in one place, as constants and terms.
* Make the checks test exactly what was asked, and let FORBIDs protect what
  must not change.

Solutions to the suggested projects are not given: they are yours. Check them
the way this manual checks its own examples, by running them.
