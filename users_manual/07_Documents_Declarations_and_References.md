# Chapter 7. Documents, declarations and references

**In this chapter you will learn**

* the five kinds of document and which blocks each may contain;
* the rules for IDs;
* the two ways `REF` behaves: reading a value or pointing at a declaration;
* how to define constants and terms;
* how to declare your own operation.

## 7.1 Document kinds

`SPECIFICATION.KIND` decides what a document is for
(`01_FOUNDATION/04_DOCUMENT_KINDS_AND_BOUNDARIES.txt`):

| Kind | Purpose | EXECUTE |
|---|---|---|
| `kind.task` | does something | exactly one |
| `kind.test` | tests something (Chapter 9) | exactly one |
| `kind.library` | definitions and rules that other documents import | none |
| `kind.data` | typed data and definitions only | none |
| `kind.extension` | a versioned vocabulary of definitions | none |

Each kind allows only certain top-level blocks. A task or test document may
use every block in the language. The others are restricted:

| Kind | Allowed top-level blocks |
|---|---|
| `kind.library` | IMPORT, EXTENSION, DEFINE, DATA, SCOPE, ALLOW, FORBID, REQUIRE, PREFER, PRESERVE, OVERRIDE, VALIDATE, EVIDENCE, COMMENT, EXAMPLE |
| `kind.data` | IMPORT, EXTENSION, DEFINE, DATA, COMMENT, EXAMPLE |
| `kind.extension` | IMPORT, DEFINE, DATA, COMMENT, EXAMPLE |

So a library can hold rules and definitions but cannot do anything by itself.
An INPUT in a library is rejected:

```
error.block.context [grammar_or_schema]: `INPUT` is not a legal top-level block for kind.library
```

Every document, of every kind, starts with `LCL` and then `SPECIFICATION`.

## 7.2 IDs

* Every declaration that anything refers to has an `ID`.
* IDs are lowercase identifiers, usually qualified with dots: `input.price`.
* An ID must be **unique** in its document. Two declarations with the same ID
  give `error.id.duplicate`, even if they are different kinds of block.
* IDs cannot start with a reserved namespace (`core`, `encoding`, `error`,
  `event`, `format`, `kind`, `mode`, `status`, `unit`).
* `NAME` fields are labels for people, and a `REF` never looks at them.

## 7.3 Two ways `REF` behaves

`REF(id)` always finds exactly one declaration by its exact ID. What happens
next depends on **where** the `REF` is written
(`03_TYPES_AND_VALUES/04_TYPED_CONSTRUCTORS_AND_REFERENCES.txt`):

**Pointing.** In a field that expects a declaration, `REF` *points at* it:
`ACTION: REF(action.double)` in a TASK, `EXECUTE`'s `REFERENCE:
REF(task.double)`, `SUCCESS`'s `ALL: [REF(verify.doubled)]`. Nothing is read
or run by the `REF` itself.

**Reading.** In an ordinary expression, `REF` *reads one value*:

* an INPUT, DATA, CONTEXT, MEMORY, STATE or OUTPUT gives its current value;
* a DEFINE of kind `kind.constant` gives the constant's value;
* a VALIDATE or VERIFY gives its TRUE/FALSE result;
* a FOR EACH variable gives the current member (Chapter 10).

Reading an OUTPUT that no action has produced yet gives `MISSING`. It does
not start the action that would produce it. Reading never runs anything.

**Metadata.** An **uppercase** property straight after `REF` reads a field of
the declaration itself, not its value: `REF(input.lessons_per_day).DESCRIPTION`
is the input's description text, and `REF(output.copy).TARGET` is the
destination declared on an OUTPUT, available even before the output exists.
A **lowercase** property reads a field of the value:
`REF(data.pupil).name`.

## 7.4 Order does not matter

After `LCL` and `SPECIFICATION`, blocks may appear in any order, and a `REF`
may point to something declared further down. The whole document is read and
resolved before anything is evaluated. Most people still write documents in a
readable order: definitions, then data, then the work, then the checks, then
the task and `EXECUTE`.

References must not go round in a circle. A type defined in terms of itself,
for example, gives `error.reference.cycle`.

## 7.5 Definitions: DEFINE

`DEFINE` creates something named that never changes. Its `KIND` says what:

| DEFINE kind | Creates | Required fields |
|---|---|---|
| `kind.term` | the exact meaning of a word in your domain | `MEANING` |
| `kind.constant` | a named, fixed value | `TYPE`, `VALUE` |
| `kind.type` | a type (Chapter 5) | `BASE` |
| `kind.operation` | a new operation (section 7.6) | `MEANING`, `SIDE_EFFECT`, `DETERMINISTIC` |
| `kind.format` | a named data format | `MEANING` |
| `kind.event`, `kind.status`, `kind.error` | an alias of a built-in event, status or error | `MEANING`, `BASE` |

**Constants** give names to fixed numbers and texts. A constant can even hold
the calculation for `core.calculate`, which keeps a formula in one place:

<!-- lcl: file=examples/07/definitions.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.definitions
    NAME: "Constants, terms and metadata"
    VERSION: "1.0.0"
    KIND: kind.task

DEFINE:
    ID: term.school_week
    KIND: kind.term
    MEANING: "The five school days from Monday to Friday."

DEFINE:
    ID: constant.days_per_week
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 5

DEFINE:
    ID: constant.lesson_minutes
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 50

DEFINE:
    ID: constant.weekly_formula
    KIND: kind.constant
    TYPE: STRING
    VALUE: "target * REF(constant.days_per_week) * REF(constant.lesson_minutes)"

INPUT:
    ID: input.lessons_per_day
    TYPE: INTEGER
    VALUE: 6
    DESCRIPTION: "Lessons on one school day."

OUTPUT:
    ID: output.minutes
    TYPE: INTEGER
    FORMAT: format.plain_text
    DESCRIPTION: "Teaching minutes in one school week."

GOAL:
    ID: goal.minutes
    ASSERT: REF(output.minutes) == 1500

ACTION:
    ID: action.minutes
    OPERATION: core.calculate
    TARGET: REF(input.lessons_per_day)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: REF(constant.weekly_formula)
    OUTPUT: REF(output.minutes)

VERIFY:
    ID: verify.minutes
    ASSERT: REF(output.minutes) == 1500

VERIFY:
    ID: verify.metadata
    ASSERT: REF(input.lessons_per_day).DESCRIPTION == "Lessons on one school day."

SUCCESS:
    ID: success.minutes
    ALL: [REF(verify.minutes), REF(verify.metadata)]

TASK:
    ID: task.minutes
    GOAL: REF(goal.minutes)
    INPUT: REF(input.lessons_per_day)
    ACTION: REF(action.minutes)
    OUTPUT: REF(output.minutes)
    SUCCESS: REF(success.minutes)

EXECUTE:
    REFERENCE: REF(task.minutes)
```

Six lessons a day, five days a week, fifty minutes each: 1500 minutes. The
`kind.term` does not change the calculation. It records, exactly and in one
place, what "school week" means, for every person or AI reading the document.

### Reference values

Occasionally you want to *store* a pointer to a declaration rather than its
value. The type `REFERENCE[REF(id)]` does this:

<!-- lcl: expect=fragment -->
```lcl
DEFINE:
    ID: constant.counter_reference
    KIND: kind.constant
    TYPE: REFERENCE[REF(constant.counter)]
    VALUE: REF(constant.counter)
```

Here the constant stores *which* declaration is meant, not the value of
`constant.counter`. You will rarely need this in everyday documents. The
specification's own example `08_EXAMPLES/VALID/13_TYPES_AND_REFERENCE_VALUES.lcl`
shows it in full.

## 7.6 Declaring your own operation

The `core.*` operations cover common work: calculating, reading, writing,
sorting and so on (see [Appendix B](Appendix_B_Operation_Reference.md)). For
anything else, a document may declare its own operation with
`KIND: kind.operation`. The declaration is a **contract**: what the operation
means, what it may change, what it depends on, whether it always gives the
same answer, its parameters and its result.

<!-- lcl: file=examples/07/custom_operation.lcl expect=run:blocked primary=error.host.constraint -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.custom_operation
    NAME: "A custom operation"
    VERSION: "1.0.0"
    KIND: kind.task
    DOMAIN: "teaching"

DEFINE:
    ID: teaching.summarise
    KIND: kind.operation
    MEANING: "Summarise one text in at most the given number of words."
    SIDE_EFFECT: FALSE
    DEPENDENCY: [model]
    DETERMINISTIC: FALSE
    PARAMETER:
        NAME: text
        TYPE: STRING
        REQUIRED: TRUE
    PARAMETER:
        NAME: maximum_words
        TYPE: INTEGER
        REQUIRED: TRUE
        MINIMUM: 1
    RESULT:
        TYPE: STRING

INPUT:
    ID: input.article
    TYPE: STRING
    VALUE: "LCL is a declarative language for stating tasks exactly. It separates goals, rules, actions and checks."

OUTPUT:
    ID: output.summary
    TYPE: STRING
    FORMAT: format.plain_text

GOAL:
    ID: goal.summary
    ASSERT: COUNT(REF(output.summary)) <= COUNT(REF(input.article))

ACTION:
    ID: action.summarise
    OPERATION: teaching.summarise
    PARAMETER:
        NAME: text
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: REF(input.article)
    PARAMETER:
        NAME: maximum_words
        TYPE: INTEGER
        REQUIRED: TRUE
        VALUE: 12
    OUTPUT: REF(output.summary)

VERIFY:
    ID: verify.shorter
    ASSERT: COUNT(REF(output.summary)) <= COUNT(REF(input.article))

SUCCESS:
    ID: success.summary
    ALL: [REF(verify.shorter)]

TASK:
    ID: task.summarise
    GOAL: REF(goal.summary)
    INPUT: REF(input.article)
    ACTION: REF(action.summarise)
    OUTPUT: REF(output.summary)
    SUCCESS: REF(success.summary)

EXECUTE:
    REFERENCE: REF(task.summarise)
```

The contract fields:

* `SIDE_EFFECT: FALSE` promises that the operation changes nothing. An
  operation that does change things lists the kinds of change instead, for
  example `[filesystem]` or `[network]`.
* `DEPENDENCY: [model]` says the result depends on an AI model. Other classes
  include `host` and `network`. Leaving the field out means the operation
  depends only on declared values.
* `DETERMINISTIC: FALSE` admits that two runs may give different wording.
  `TRUE` would be a promise, and a promise the contract cannot keep is
  rejected.

The operation's ID must not start with `core.`. A domain prefix such as
`teaching.` is usual.

`lcl validate` accepts this document: every name, type and parameter checks
out. `lcl run` stops with:

```
error.host.constraint [execution]: this host installs no capability for teaching.summarise
...
status.blocked
```

That is the honest answer. The document says *what* the operation must do;
something must actually *do* it. An AI system acting as the host would
provide the summary. The deterministic reference tool has no summariser, so
it reports that it cannot, rather than inventing a result. `status.blocked`
means "could not proceed because something outside the document is
unavailable" (Chapter 9 lists all the statuses).

## Summary

* Tasks and tests execute; libraries, data and extensions only declare. Each
  kind allows its own set of blocks.
* IDs are unique, lowercase and never start with a reserved namespace.
* `REF` points at a declaration in reference fields and reads one value in
  expressions. An uppercase property reads declaration metadata.
* `DEFINE` creates immutable terms, constants, types, operations, formats and
  aliases.
* A custom operation is a contract. The host must supply its implementation.

## Exercises

1. Write a `kind.data` document with constants for the number of days in a
   week, hours in a day, and minutes in an hour, plus a constant computed from
   the three: minutes in a week.
2. Try adding an `ACTION` to that data document. What does `lcl check` report?
3. Change `definitions.lcl` so the formula also multiplies by a new constant,
   `constant.weeks_per_term`, with value 12. Update the checks so the run still
   succeeds.
4. Declare a custom operation `teaching.translate` with parameters `text` and
   `language` (both STRING) and a STRING result. What should its
   `DETERMINISTIC` and `DEPENDENCY` be, and why?

Solutions: [solutions/README.md](solutions/README.md#chapter-7).
