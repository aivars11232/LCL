# Chapter 8. Inputs, outputs and actions

**In this chapter you will learn**

* the difference between INPUT and DATA;
* how to make an input that can be supplied from the command line;
* how actions, operations, targets and parameters fit together;
* how an action's result becomes an OUTPUT, and how to pick one field of it;
* several useful built-in operations.

## 8.1 INPUT and DATA

Both hold typed values, but they mean different things:

* **DATA** is fixed material that belongs to the document: a price list, a
  table of rates. It always has a `VALUE`.
* **INPUT** is what the task works *on*. It says where its value comes from:
  * `VALUE:`, a value written in the document;
  * `SOURCE:`, a file or address the value is read from; or
  * nothing, if the input is optional (`REQUIRED: FALSE`) with a `DEFAULT`.

An INPUT must have exactly one of `VALUE` or `SOURCE`, unless it is optional
with a `DEFAULT`. Otherwise `lcl check` reports
`error.block.conditional_requirement`.

## 8.2 Supplying an input from the command line

`lcl run --input <id>=<expression>` supplies a value for an input. A value
written in the document with `VALUE:` always wins, so to make an input that
the command line can set, declare it **optional with a default**:

<!-- lcl: expect=fragment -->
```lcl
INPUT:
    ID: input.budget
    TYPE: DECIMAL
    REQUIRED: FALSE
    DEFAULT: 25.00
```

Now `lcl run shop.lcl` uses 25.00, and `lcl run --input input.budget=20.00
shop.lcl` uses 20.00. The part after `=` is an LCL expression, so strings
need quotes and lists need brackets. Wrap the whole argument in single quotes
so the shell passes it through unchanged:

```
$ lcl run --input 'input.name="Ada"' greet.lcl
$ lcl run --input 'input.marks=[7, 9, 10]' report.lcl
```

> **Tool note.** The current `lcl` does not warn when the ID after
> `--input` matches no declared input. A misspelled ID is silently ignored
> and the default is used. If a run seems to ignore your input, check the
> spelling first.

## 8.3 Actions

An ACTION is one invocation of one **operation**:

<!-- lcl: expect=fragment -->
```lcl
ACTION:
    ID: action.cheap_items
    OPERATION: core.filter
    TARGET: REF(input.prices)
    PARAMETER:
        NAME: predicate
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "item < 5.00"
    OUTPUT: REF(output.cheap_items)
```

* `OPERATION` names exactly one operation. The built-in ones start with
  `core.` (the full list is in
  [Appendix B](Appendix_B_Operation_Reference.md)).
* `TARGET` is the thing the operation works on. Each operation says whether
  it needs one and of what type.
* Each `PARAMETER` block passes one **named** value: `NAME`, `TYPE`,
  `REQUIRED` and `VALUE`. The names must be the ones the operation defines.
  A misspelled or unknown parameter name gives `error.operation.parameter`,
  and so does leaving out a required one.
* `OUTPUT` names the single OUTPUT that receives the result. An action has at
  most one output.

An action also **authorises** exactly what it declares: its operation, on its
target, with its parameters. Chapters 11 and 12 build on this.

## 8.4 Outputs and results

An OUTPUT declares a result before it exists:

* `TYPE` and `FORMAT` are required. The formats are `format.plain_text`,
  `format.json`, `format.csv`, `format.markdown` and others (see Appendix B).
* `REQUIRED` defaults to TRUE. A required output that no action filled makes
  the task fail. Mark an output `REQUIRED: FALSE` if it is produced only
  sometimes (Chapter 10 has an example).
* `TARGET` (optional) is where the output should end up, for example a file
  path. It is a destination, not the value.

Every OUTPUT has **exactly one** producing action. If two actions name the
same output, the document is rejected before anything runs:

```
error.execution.order [execution]: OUTPUT `output.cheap_items` is produced by two ACTION declarations, ...
```

### Result records and PROPERTY

An operation produces a **result record** with several fields, and the
OUTPUT receives one of them. Each operation has a default field. For
`core.filter` it is `items`, the list of members that passed. The record also
holds `count`, the number of those members. Add `PROPERTY` to the OUTPUT to
choose a different field:

<!-- lcl: expect=fragment -->
```lcl
OUTPUT:
    ID: output.cheap_count
    TYPE: INTEGER
    FORMAT: format.plain_text
    PROPERTY: count
```

The default fields of the common operations:

| Operation | Default output | Other fields you can choose |
|---|---|---|
| `core.calculate`, `core.return`, `core.read`, `core.compare` | `value` | |
| `core.filter`, `core.sort` | `items` | `count` |
| `core.create`, `core.write` | `changed` (TRUE/FALSE) | `target` (the path) |

## 8.5 Some useful operations

These operations only compute. They touch nothing outside the document, so
they run anywhere:

| Operation | Does | Needs |
|---|---|---|
| `core.return` | produces its TARGET value unchanged | TARGET |
| `core.calculate` | evaluates an expression | parameter `expression`; optional TARGET and `bindings` |
| `core.compare` | compares TARGET with `against` | parameter `against`; optional `criteria` such as `"<="` (default `==`) |
| `core.filter` | keeps list members for which a condition is TRUE | TARGET list; parameter `predicate`, which uses `item` for the member |
| `core.sort` | sorts a list or set into a list | TARGET; optional `direction` (`ascending`/`descending`) and `key` |
| `core.group` | groups list members by a key | TARGET list; parameter `key` |

**`core.calculate` bindings.** Besides `REF(...)` and `target`, a calculation
can use short local names that you define in a `bindings` object:

<!-- lcl: expect=fragment -->
```lcl
    PARAMETER:
        NAME: bindings
        TYPE: OBJECT
        REQUIRED: TRUE
        VALUE:
            amounts: REF(input.prices)
            delivery: 3.00
```

The expression can then say `"SUM(amounts) + delivery"`. A binding name
must not be the same as the last part of any ID in the document: `prices`
would clash with `input.prices`, and the tool refuses the ambiguity.

## 8.6 A complete example

This task filters a price list, counts the cheap items, adds up an order
including delivery, and compares it with a budget:

<!-- lcl: file=examples/08/shop.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.shop
    NAME: "A small shop order"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.prices
    TYPE: LIST[DECIMAL]
    VALUE: [2.50, 0.99, 12.00, 4.75]

INPUT:
    ID: input.budget
    TYPE: DECIMAL
    REQUIRED: FALSE
    DEFAULT: 25.00

OUTPUT:
    ID: output.cheap_items
    TYPE: LIST[DECIMAL]
    FORMAT: format.json

OUTPUT:
    ID: output.cheap_count
    TYPE: INTEGER
    FORMAT: format.plain_text
    PROPERTY: count

OUTPUT:
    ID: output.total
    TYPE: DECIMAL
    FORMAT: format.plain_text

OUTPUT:
    ID: output.within_budget
    TYPE: BOOLEAN
    FORMAT: format.plain_text

GOAL:
    ID: goal.order
    ASSERT: REF(output.within_budget) == TRUE

ACTION:
    ID: action.cheap_items
    OPERATION: core.filter
    TARGET: REF(input.prices)
    PARAMETER:
        NAME: predicate
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "item < 5.00"
    OUTPUT: REF(output.cheap_items)

ACTION:
    ID: action.cheap_count
    OPERATION: core.filter
    TARGET: REF(input.prices)
    PARAMETER:
        NAME: predicate
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "item < 5.00"
    OUTPUT: REF(output.cheap_count)

ACTION:
    ID: action.total
    OPERATION: core.calculate
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "SUM(amounts) + delivery"
    PARAMETER:
        NAME: bindings
        TYPE: OBJECT
        REQUIRED: TRUE
        VALUE:
            amounts: REF(input.prices)
            delivery: 3.00
    OUTPUT: REF(output.total)

ACTION:
    ID: action.within_budget
    OPERATION: core.compare
    TARGET: REF(output.total)
    PARAMETER:
        NAME: against
        TYPE: DECIMAL
        REQUIRED: TRUE
        VALUE: REF(input.budget)
    PARAMETER:
        NAME: criteria
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "<="
    OUTPUT: REF(output.within_budget)

VERIFY:
    ID: verify.cheap
    ASSERT: REF(output.cheap_items) == [2.50, 0.99, 4.75] AND REF(output.cheap_count) == 3

VERIFY:
    ID: verify.total
    ASSERT: REF(output.total) == 23.24

VERIFY:
    ID: verify.budget
    ASSERT: REF(output.within_budget) == TRUE

SUCCESS:
    ID: success.order
    ALL: [REF(verify.cheap), REF(verify.total), REF(verify.budget)]

TASK:
    ID: task.order
    GOAL: REF(goal.order)
    INPUT: [REF(input.prices), REF(input.budget)]
    ACTION: [REF(action.cheap_items), REF(action.cheap_count), REF(action.total), REF(action.within_budget)]
    OUTPUT: [REF(output.cheap_items), REF(output.cheap_count), REF(output.total), REF(output.within_budget)]
    SUCCESS: REF(success.order)

EXECUTE:
    REFERENCE: REF(task.order)
```

```
$ lcl run shop.lcl
status.succeeded
  VERIFY verify.cheap = TRUE (required)
  VERIFY verify.total = TRUE (required)
  VERIFY verify.budget = TRUE (required)
  SUCCESS success.order ALL = TRUE
  OUTPUT output.cheap_items published = [2.50, 0.99, 4.75]
  OUTPUT output.cheap_count published = 3
  OUTPUT output.total published = 23.24
  OUTPUT output.within_budget published = TRUE
  because: SuccessSatisfied
  5 invocation(s), 0 event(s), 6 step(s)
```

Now lower the budget from the command line:

<!-- lcl-run: file=examples/08/shop.lcl expect=run:failed primary=error.verification.failed args="--input input.budget=20.00" -->

```
$ lcl run --input input.budget=20.00 shop.lcl
shop.lcl:110:9: error.verification.failed [verification_or_completion]: required VERIFY `verify.budget` asserted FALSE
...
  OUTPUT output.within_budget published = FALSE
```

Everything ran correctly, but the order is over budget, so the task did not
succeed, and the exit code is 2.

Notice in this example:

* The four actions run in the order the TASK lists them.
* `action.total` has no TARGET: `core.calculate` does not need one when the
  expression names everything it uses.
* `action.within_budget` uses the output of `action.total` as its TARGET. An
  action may use any output that an earlier action produced.

## 8.7 CONTEXT, MEMORY and STATE

Three more kinds of data block describe information that comes from outside
the task itself. Each needs a `SCOPE` saying which part of the work it
applies to:

| Block | Holds | Example |
|---|---|---|
| `CONTEXT` | supporting background information; it can never create rules | the class is year 8, the subject is mathematics |
| `MEMORY` | retained information, such as preferences from earlier work | this teacher's lessons last 50 minutes |
| `STATE` | the current value of something outside that can change | 11 lessons have been taught so far |

MEMORY and STATE also need a `MODE`, such as `mode.read_only`. Writing to
them requires the operations `core.memory_write` and `core.state_update`,
which must be reachable, scoped and authorised like any other effect.

Why not just use DATA? Because these blocks make an important promise
explicit: an AI working on the task may use **only** the context, memory and
state that the document declares. Something from an earlier conversation, or
a note in the assistant's own memory, has no effect unless the document
brings it in. This is design principle 1 from Chapter 1.

<!-- lcl: file=examples/08/lesson_plan.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.lesson_plan
    NAME: "Context, memory and state"
    VERSION: "1.0.0"
    KIND: kind.task

SCOPE:
    ID: scope.lesson
    INCLUDE: REF(task.plan)

CONTEXT:
    ID: context.class
    TYPE: OBJECT
    SCOPE: REF(scope.lesson)
    VALUE:
        year_group: 8
        subject: "mathematics"

MEMORY:
    ID: memory.teacher_preferences
    TYPE: OBJECT
    SCOPE: REF(scope.lesson)
    MODE: mode.read_only
    VALUE:
        lesson_minutes: 50

STATE:
    ID: state.lessons_taught
    TYPE: INTEGER
    SCOPE: REF(scope.lesson)
    MODE: mode.read_only
    VALUE: 11

OUTPUT:
    ID: output.next_lesson
    TYPE: INTEGER
    FORMAT: format.plain_text

OUTPUT:
    ID: output.minutes_so_far
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.plan
    ASSERT: REF(output.next_lesson) == 12

ACTION:
    ID: action.next_lesson
    OPERATION: core.calculate
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(state.lessons_taught) + 1"
    OUTPUT: REF(output.next_lesson)

ACTION:
    ID: action.minutes_so_far
    OPERATION: core.calculate
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(state.lessons_taught) * REF(memory.teacher_preferences).lesson_minutes"
    OUTPUT: REF(output.minutes_so_far)

VERIFY:
    ID: verify.next_lesson
    ASSERT: REF(output.next_lesson) == 12

VERIFY:
    ID: verify.minutes
    ASSERT: REF(output.minutes_so_far) == 550

VERIFY:
    ID: verify.context_used
    ASSERT: REF(context.class).subject == "mathematics"

SUCCESS:
    ID: success.plan
    ALL: [REF(verify.next_lesson), REF(verify.minutes), REF(verify.context_used)]

TASK:
    ID: task.plan
    GOAL: REF(goal.plan)
    ACTION: [REF(action.next_lesson), REF(action.minutes_so_far)]
    OUTPUT: [REF(output.next_lesson), REF(output.minutes_so_far)]
    SUCCESS: REF(success.plan)

EXECUTE:
    REFERENCE: REF(task.plan)
```

Each block is read like any other value: `REF(state.lessons_taught)`, and
`REF(memory.teacher_preferences).lesson_minutes` for a field. The SCOPE
`scope.lesson` includes the task, so all three apply to it.

## Summary

* DATA is fixed material. INPUT is what the task works on, from `VALUE`,
  `SOURCE` or an optional `DEFAULT`.
* `--input id=expression` sets an optional input from the command line.
* An ACTION runs one operation on one target with named parameters and fills
  at most one OUTPUT. Every OUTPUT has exactly one producer.
* `PROPERTY` picks a field of the operation's result record.
* CONTEXT, MEMORY and STATE declare, and so limit, the outside information a
  task may use.

## Exercises

1. Change `shop.lcl` so that "cheap" means under 3.00. Which checks must
   change, and to what?
2. Add an output `output.most_expensive` holding the highest price, produced
   by `core.calculate` with `MAX`. Add a check for it.
3. Add a `core.sort` action producing the prices from highest to lowest.
   (Hint: the `direction` parameter.)
4. Make the delivery charge an optional input with default 3.00, and run the
   task with free delivery from the command line.

Solutions: [solutions/README.md](solutions/README.md#chapter-8).
