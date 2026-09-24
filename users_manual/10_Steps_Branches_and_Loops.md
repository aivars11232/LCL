# Chapter 10. Steps, branches and loops

**In this chapter you will learn**

* how to put actions in order with SEQUENCE and STEP;
* how to choose between actions with IF and ELSE;
* how to repeat an action for each member of a list with FOR EACH;
* how to group work into PHASEs, and when work may run in parallel;
* how to sort, filter and group lists.

So far every task listed its actions directly, and they ran in the listed
order. For anything more involved, LCL has execution structures. The rules
are in `04_GRAMMAR/04_CONDITIONS_BRANCHES_AND_BOUNDED_ITERATION.txt`,
`04_GRAMMAR/06_TASK_ACTION_PHASE_SEQUENCE_AND_STEP_FORM.txt` and
`05_SEMANTICS/08_PHASE_SEQUENCE_STEP_BRANCH_LOOP_RETRY_AND_CONCURRENCY.txt`.

## 10.1 The building blocks

| Block | Is | Contains |
|---|---|---|
| `STEP` | one unit of execution | exactly one of: an ACTION, a SEQUENCE, a PHASE or a TASK |
| `SEQUENCE` | steps in written order | STEPs, IFs, FOR EACHs |
| `PHASE` | a major stage of the work | STEPs, SEQUENCEs, IFs, FOR EACHs; may be ordered against other phases |
| `IF ... THEN:` / `ELSE:` | a choice | STEPs, nested IFs, FOR EACHs, COMMENTs |
| `FOR EACH x IN list:` | a bounded repetition | STEPs, nested IFs, FOR EACHs, COMMENTs |

A TASK refers to its sequences or phases with `SEQUENCE: REF(...)` or
`PHASE: [REF(...), REF(...)]`. It can never contain them inline.

## 10.2 Branches: IF and ELSE

<!-- lcl: file=examples/10/grade.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.grade
    NAME: "Choose a message by score"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.score
    TYPE: INTEGER
    REQUIRED: FALSE
    DEFAULT: 72

OUTPUT:
    ID: output.pass_message
    TYPE: STRING
    FORMAT: format.plain_text
    REQUIRED: FALSE

OUTPUT:
    ID: output.fail_message
    TYPE: STRING
    FORMAT: format.plain_text
    REQUIRED: FALSE

GOAL:
    ID: goal.message
    ASSERT: EXISTS(REF(output.pass_message)) OR EXISTS(REF(output.fail_message))

ACTION:
    ID: action.pass
    OPERATION: core.return
    TARGET: "passed"
    OUTPUT: REF(output.pass_message)

ACTION:
    ID: action.fail
    OPERATION: core.return
    TARGET: "try again"
    OUTPUT: REF(output.fail_message)

SEQUENCE:
    ID: sequence.grade
    IF (REF(input.score) >= 50) THEN:
        STEP:
            ID: step.pass
            ACTION: REF(action.pass)
    ELSE:
        STEP:
            ID: step.fail
            ACTION: REF(action.fail)

VERIFY:
    ID: verify.one_message
    ASSERT: EXISTS(REF(output.pass_message)) OR EXISTS(REF(output.fail_message))

SUCCESS:
    ID: success.message
    ALL: [REF(verify.one_message)]

TASK:
    ID: task.grade
    GOAL: REF(goal.message)
    INPUT: REF(input.score)
    SEQUENCE: REF(sequence.grade)
    OUTPUT: [REF(output.pass_message), REF(output.fail_message)]
    SUCCESS: REF(success.message)

EXECUTE:
    REFERENCE: REF(task.grade)
```

<!-- lcl-run: file=examples/10/grade.lcl expect=run:succeeded args="--input input.score=30" -->

```
$ lcl run grade.lcl
status.succeeded
  ...
  OUTPUT output.pass_message published = "passed"
  OUTPUT output.fail_message unbound

$ lcl run --input input.score=30 grade.lcl
status.succeeded
  ...
  OUTPUT output.pass_message unbound
  OUTPUT output.fail_message published = "try again"
```

The rules for IF:

* The condition goes in parentheses: `IF (condition) THEN:`. It must be
  TRUE or FALSE.
* `ELSE:` is optional. When present, it sits directly under the IF, at the
  same indentation.
* IF may appear only inside a SEQUENCE, a PHASE, or another IF, ELSE or FOR
  EACH body, never directly in a TASK.
* The condition is evaluated once, when execution reaches it.

**Why two outputs?** Every OUTPUT has exactly one producing action, and
both branches cannot fill the same output. So each branch has its own output,
both marked `REQUIRED: FALSE`, because only one of them will exist. The VERIFY
then checks that one of them does.

## 10.3 Loops: FOR EACH

`FOR EACH name IN collection:` runs its body once for each member. Inside
the body, **`REF(name)` reads the current member**. A bare `name` would be
just an identifier.

LCL has no `WHILE`, no `UNTIL` and no recursion. Every loop walks a
collection whose size is known before it starts, so every loop ends.

This example squares only the numbers above 5:

<!-- lcl: file=examples/10/squares.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.squares
    NAME: "Square the large numbers"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.numbers
    TYPE: SET[INTEGER]
    VALUE: [4, 7, 10, 7]

OUTPUT:
    ID: output.square
    TYPE: INTEGER
    FORMAT: format.plain_text
    REQUIRED: FALSE

GOAL:
    ID: goal.loop
    ASSERT: COUNT(REF(input.numbers)) == 3

SEQUENCE:
    ID: sequence.each
    FOR EACH number IN REF(input.numbers):
        IF (REF(number) > 5) THEN:
            STEP:
                ID: step.square
                ACTION:
                    ID: action.square
                    OPERATION: core.calculate
                    TARGET: REF(number)
                    PARAMETER:
                        NAME: expression
                        TYPE: STRING
                        REQUIRED: TRUE
                        VALUE: "target * target"
                    OUTPUT: REF(output.square)
        ELSE:
            COMMENT:
                CONTENT: "Small numbers are left alone."

VERIFY:
    ID: verify.count
    ASSERT: COUNT(REF(input.numbers)) == 3

SUCCESS:
    ID: success.loop
    ALL: [REF(verify.count)]

TASK:
    ID: task.loop
    GOAL: REF(goal.loop)
    INPUT: REF(input.numbers)
    SEQUENCE: REF(sequence.each)
    SUCCESS: REF(success.loop)

EXECUTE:
    REFERENCE: REF(task.loop)
```

Things to notice:

* The input is a **SET**, so the duplicate 7 collapses and the set has three
  members. A set of numbers is walked in ascending order: 4, 7, 10. A LIST is
  walked in its written order.
* A STEP inside a loop may declare its ACTION **inline**, right there in the
  STEP, instead of referring to one declared at the top level.
* An ELSE body must contain something. A COMMENT is allowed, and it says
  plainly that doing nothing is intended.
* Each iteration has its own copy of `output.square`. That is why the output
  is not in the TASK's OUTPUT list, and why nothing outside the loop can read
  it: there is no single "the" square. A VERIFY outside the loop that reads
  `REF(output.square)` is rejected:

  ```
  error.reference.unresolved [resolution]: `output.square` has a loop-local producer; this value read selects no unique enclosing iteration
  ```

  To get a single result from a whole collection, use a list operation
  (next section) instead of a loop.
* A SET of values with no natural order, such as a `SET[BOOLEAN]`, cannot be
  walked directly (`error.type.mismatch`). Sort it into a list first with
  `core.sort`.

## 10.4 Working with whole lists

Many jobs that other languages do with a loop are single operations in LCL,
and give one result that later actions and checks can use:

<!-- lcl: file=examples/10/marks.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.marks
    NAME: "Sort, filter and loop"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.marks
    TYPE: LIST[INTEGER]
    VALUE: [72, 45, 90, 45, 61]

OUTPUT:
    ID: output.sorted
    TYPE: LIST[INTEGER]
    FORMAT: format.json

OUTPUT:
    ID: output.passing
    TYPE: LIST[INTEGER]
    FORMAT: format.json

GOAL:
    ID: goal.lists
    ASSERT: REF(output.sorted) == [45, 45, 61, 72, 90]

ACTION:
    ID: action.sort
    OPERATION: core.sort
    TARGET: REF(input.marks)
    OUTPUT: REF(output.sorted)

ACTION:
    ID: action.filter
    OPERATION: core.filter
    TARGET: REF(input.marks)
    PARAMETER:
        NAME: predicate
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "item >= 50"
    OUTPUT: REF(output.passing)

SEQUENCE:
    ID: sequence.each
    FOR EACH mark IN REF(input.marks):
        STEP:
            ID: step.show
            ACTION:
                ID: action.show
                OPERATION: core.return
                TARGET: REF(mark)

VERIFY:
    ID: verify.sorted
    ASSERT: REF(output.sorted) == [45, 45, 61, 72, 90]

VERIFY:
    ID: verify.passing
    ASSERT: REF(output.passing) == [72, 90, 61]

SUCCESS:
    ID: success.lists
    ALL: [REF(verify.sorted), REF(verify.passing)]

TASK:
    ID: task.lists
    GOAL: REF(goal.lists)
    INPUT: REF(input.marks)
    ACTION: [REF(action.sort), REF(action.filter)]
    SEQUENCE: REF(sequence.each)
    OUTPUT: [REF(output.sorted), REF(output.passing)]
    SUCCESS: REF(success.lists)

EXECUTE:
    REFERENCE: REF(task.lists)
```

* `core.sort` returns a sorted **list** (from a list or a set). Equal values
  keep their original order.
* `core.filter` returns every member for which its `predicate` is TRUE, in the
  original order. In the predicate, the bare name **`item`** is the member
  being tested.
* The TASK lists an ACTION field and a SEQUENCE field. They run in that
  order: the actions, then the sequence.

## 10.5 Phases, parallel work and ordering

A **PHASE** is a major stage. Phases in a task's `PHASE:` list run in that
order, and `BEFORE:` and `AFTER:` can state an ordering explicitly. Inside a
phase or sequence, `MODE` decides how its steps run:

* `mode.sequential` (the default): one after another, in written order;
* `mode.parallel`: in any order, possibly at the same time. This is allowed
  only when the steps are provably independent: no step may write what
  another reads or writes. Otherwise the document is rejected. Results are
  still reported in written order.

> **Tool note.** The specification requires validation to fail when steps in
> a parallel phase depend on each other. The current `lcl` does not yet
> detect this: a parallel phase in which one step reads another's output
> still passes `lcl validate`. Make sure every step in a parallel group only
> reads data that exists before the group starts.

This example sorts a league table and groups the teams by division in one
parallel phase, then reports the leader in a second phase:

<!-- lcl: file=examples/10/league.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.league
    NAME: "League table"
    VERSION: "1.0.0"
    KIND: kind.task

DEFINE:
    ID: type.team
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: name
        TYPE: STRING
        REQUIRED: TRUE
    FIELD:
        NAME: points
        TYPE: INTEGER
        REQUIRED: TRUE
    FIELD:
        NAME: division
        TYPE: STRING
        REQUIRED: TRUE

DATA:
    ID: data.lions
    TYPE: OBJECT[REF(type.team)]
    VALUE:
        name: "Lions"
        points: 12
        division: "north"

DATA:
    ID: data.otters
    TYPE: OBJECT[REF(type.team)]
    VALUE:
        name: "Otters"
        points: 18
        division: "south"

DATA:
    ID: data.hawks
    TYPE: OBJECT[REF(type.team)]
    VALUE:
        name: "Hawks"
        points: 15
        division: "north"

INPUT:
    ID: input.teams
    TYPE: LIST[OBJECT[REF(type.team)]]
    VALUE: [REF(data.lions), REF(data.otters), REF(data.hawks)]

OUTPUT:
    ID: output.table
    TYPE: LIST[OBJECT[REF(type.team)]]
    FORMAT: format.json

OUTPUT:
    ID: output.by_division
    TYPE: LIST[OBJECT]
    FORMAT: format.json

OUTPUT:
    ID: output.leader_points
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.table
    ASSERT: REF(output.leader_points) == 18

ACTION:
    ID: action.sort
    OPERATION: core.sort
    TARGET: REF(input.teams)
    PARAMETER:
        NAME: key
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "points"
    PARAMETER:
        NAME: direction
        TYPE: ENUM
        REQUIRED: TRUE
        VALUE: descending
    OUTPUT: REF(output.table)

ACTION:
    ID: action.group
    OPERATION: core.group
    TARGET: REF(input.teams)
    PARAMETER:
        NAME: key
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "division"
    OUTPUT: REF(output.by_division)

ACTION:
    ID: action.leader
    OPERATION: core.calculate
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(output.table)[0].points"
    OUTPUT: REF(output.leader_points)

PHASE:
    ID: phase.arrange
    MODE: mode.parallel
    STEP:
        ID: step.sort
        ACTION: REF(action.sort)
    STEP:
        ID: step.group
        ACTION: REF(action.group)

PHASE:
    ID: phase.report
    AFTER: REF(phase.arrange)
    STEP:
        ID: step.leader
        ACTION: REF(action.leader)

VERIFY:
    ID: verify.order
    ASSERT: REF(output.table)[0].name == "Otters" AND REF(output.table)[2].name == "Lions"

VERIFY:
    ID: verify.groups
    ASSERT: COUNT(REF(output.by_division)) == 2

SUCCESS:
    ID: success.table
    ALL: [REF(verify.order), REF(verify.groups)]

TASK:
    ID: task.table
    GOAL: REF(goal.table)
    INPUT: REF(input.teams)
    PHASE: [REF(phase.arrange), REF(phase.report)]
    OUTPUT: [REF(output.table), REF(output.by_division), REF(output.leader_points)]
    SUCCESS: REF(success.table)

EXECUTE:
    REFERENCE: REF(task.table)
```

Three new things appear here:

* **Sorting objects by a field.** `key` is the name of the field to sort by
  (`"points"`).
* **Sorting direction** is an enumeration parameter. Declare it with
  `TYPE: ENUM` and the bare value `descending` or `ascending`.

  > **Tool note.** Writing the direction as a string
  > (`TYPE: STRING`, `VALUE: "descending"`) is not rejected by the current
  > tool, but it is ignored and the list comes out ascending. Always use the
  > ENUM form shown here, and keep a VERIFY on the order.

* **`core.group`** splits a list into groups that share the value of `key`.
  Its result is a list of objects, each with a `key` and the `items` in that
  group, in order of first appearance:

  ```
  [{items: [Lions, Hawks], key: "north"}, {items: [Otters], key: "south"}]
  ```

  (shown here with the team objects shortened to their names).

`step.sort` and `step.group` only read `input.teams` and write different
outputs, so they are independent and may run in parallel.
`phase.report` reads `output.table`, so it must come after `phase.arrange`.
Its `AFTER:` says so explicitly, in addition to the order of the task's
PHASE list.

## 10.6 How the order of work is decided

The engine builds an **execution graph** before it runs anything:

1. It starts from `EXECUTE` and follows the task's `PHASE`, `SEQUENCE` and
   `ACTION` fields, in that written order.
2. Inside each sequence or phase, it follows the steps in written order,
   including every branch of every IF and the body of every FOR EACH.
3. It adds the extra edges from `BEFORE`, `AFTER` and the phase order.

Anything that could run twice by two routes (for example, one action
reachable from two different steps) is rejected with `error.execution.order`
before anything happens. So is an ordering cycle, such as A after B and B
after A. Run `lcl inspect` to see the planned order.

## Summary

* STEP holds exactly one execution unit. SEQUENCE runs steps in written
  order. PHASE groups work into stages.
* `IF (condition) THEN:` with optional `ELSE:` chooses between branches. Each
  branch fills its own outputs.
* `FOR EACH x IN collection:` repeats a body a known number of times, with the
  member read as `REF(x)`. Outputs inside a loop exist per iteration.
* `core.sort`, `core.filter` and `core.group` process whole lists and give one
  result.
* `mode.parallel` is allowed only for provably independent steps.

## Exercises

1. Extend `grade.lcl` to three outcomes: 70 and above gives `"distinction"`,
   50 to 69 gives `"passed"`, below 50 gives `"try again"`. (Hint: an IF
   inside an ELSE.)
2. Change `marks.lcl` so that it also produces the **failing** marks, in
   descending order.
3. Why can't `squares.lcl` collect its squares into one output inside the
   loop? Write a version that produces, as one output, the list of the numbers
   above 5. (Hint: `core.filter` needs a LIST, and the input is a SET.)
4. In `league.lcl`, could `step.leader` be moved into `phase.arrange`, next to
   `step.sort` and `step.group`? Explain using the rule for `mode.parallel`.

Solutions: [solutions/README.md](solutions/README.md#chapter-10).
