# Chapter 13. Missing values, errors and recovery

**In this chapter you will learn**

* the exact difference between MISSING, UNKNOWN and NULL;
* LCL's three-valued logic;
* how DEFAULT and ASSUME supply values, and their limits;
* how errors raise events, and how handlers respond to them;
* how to retry an action a bounded number of times.

Real work meets missing data, unavailable services and failed attempts. Many
systems quietly guess or carry on. LCL insists that every gap is named, and
that every recovery is declared in advance. The rules are in
`03_TYPES_AND_VALUES/09_MISSING_UNKNOWN_NULL_AND_OPTIONALITY.txt` and
`05_SEMANTICS/06_MISSING_UNKNOWN_NULL_DEFAULT_ASSUME_AND_HANDLER_RESOLUTION.txt`.

## 13.1 Three kinds of "nothing"

| Word | Means | Example | Can be stored? |
|---|---|---|---|
| `NULL` | known to be empty | a person has no middle name | yes, where the type is NULL |
| `MISSING` | no value exists here | an optional input nobody supplied; an output not yet produced | no |
| `UNKNOWN` | a value exists but cannot be determined | a sensor that is on, but whose reading cannot be trusted | no |

In many languages all three would be `null`, and a program would have to
guess which one was meant. In LCL they are different words, and the language
never converts one into another.

* **Reading something that does not exist gives MISSING**: an output no
  action has produced, an optional input with no value, a list index past the
  end, an object field that is absent.
* `EXISTS(x)` is FALSE only when `x` is MISSING. NULL and UNKNOWN both
  *exist*.
* `==` and `!=` work with all three: `MISSING == MISSING` is TRUE, and
  `MISSING != UNKNOWN`.
* Almost every other use of MISSING is an error: arithmetic, comparisons such
  as `<`, function arguments. The error is `error.required.missing`.

## 13.2 Three-valued logic

A condition in LCL can come out TRUE, FALSE or UNKNOWN. `AND`, `OR` and
`NOT` follow these tables:

| `A` | `B` | `A AND B` | `A OR B` |
|---|---|---|---|
| TRUE | UNKNOWN | UNKNOWN | TRUE |
| FALSE | UNKNOWN | FALSE | UNKNOWN |
| UNKNOWN | UNKNOWN | UNKNOWN | UNKNOWN |

`NOT UNKNOWN` is UNKNOWN. `ALL`, `ANY` and `NONE` work the same way: one
FALSE makes `ALL` FALSE whatever else is in the list, and one TRUE makes `ANY`
TRUE.

The rule of thumb: UNKNOWN stays UNKNOWN unless the other side settles the
answer. A **required** condition that ends UNKNOWN is an error
(`error.value.unknown`). It is never silently treated as FALSE.

This program checks all of the above on the real engine:

<!-- lcl: file=examples/13/special_values.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.special_values
    NAME: "MISSING, UNKNOWN and NULL"
    VERSION: "1.0.0"
    KIND: kind.task

DATA:
    ID: data.middle_name
    TYPE: NULL
    VALUE: NULL

DATA:
    ID: data.marks
    TYPE: LIST[INTEGER]
    VALUE: [7, 9]

OUTPUT:
    ID: output.done
    TYPE: BOOLEAN
    FORMAT: format.plain_text

OUTPUT:
    ID: output.never
    TYPE: INTEGER
    FORMAT: format.plain_text
    REQUIRED: FALSE

GOAL:
    ID: goal.all_true
    ASSERT: REF(output.done) == TRUE

ACTION:
    ID: action.done
    OPERATION: core.return
    TARGET: TRUE
    OUTPUT: REF(output.done)

VERIFY:
    ID: verify.not_unknown
    ASSERT: (NOT UNKNOWN) == UNKNOWN

VERIFY:
    ID: verify.and_false
    ASSERT: (FALSE AND UNKNOWN) == FALSE

VERIFY:
    ID: verify.and_true
    ASSERT: (TRUE AND UNKNOWN) == UNKNOWN

VERIFY:
    ID: verify.or_false
    ASSERT: (FALSE OR UNKNOWN) == UNKNOWN

VERIFY:
    ID: verify.or_true
    ASSERT: (TRUE OR UNKNOWN) == TRUE

VERIFY:
    ID: verify.all_unknown
    ASSERT: ALL([TRUE, UNKNOWN]) == UNKNOWN AND ALL([FALSE, UNKNOWN]) == FALSE

VERIFY:
    ID: verify.any_unknown
    ASSERT: ANY([TRUE, UNKNOWN]) == TRUE AND ANY([FALSE, UNKNOWN]) == UNKNOWN

VERIFY:
    ID: verify.sentinels
    ASSERT: MISSING == MISSING AND UNKNOWN == UNKNOWN AND MISSING != UNKNOWN

VERIFY:
    ID: verify.null_is_not_missing
    ASSERT: REF(data.middle_name) == NULL AND REF(data.middle_name) != MISSING AND EXISTS(REF(data.middle_name))

VERIFY:
    ID: verify.missing_output
    ASSERT: REF(output.never) == MISSING AND NOT EXISTS(REF(output.never))

VERIFY:
    ID: verify.missing_index
    ASSERT: REF(data.marks)[10] == MISSING

SUCCESS:
    ID: success.all_true
    ALL: [REF(verify.not_unknown), REF(verify.and_false), REF(verify.and_true), REF(verify.or_false), REF(verify.or_true), REF(verify.all_unknown), REF(verify.any_unknown), REF(verify.sentinels), REF(verify.null_is_not_missing), REF(verify.missing_output), REF(verify.missing_index)]

TASK:
    ID: task.special_values
    GOAL: REF(goal.all_true)
    ACTION: REF(action.done)
    OUTPUT: [REF(output.done), REF(output.never)]
    SUCCESS: REF(success.all_true)

EXECUTE:
    REFERENCE: REF(task.special_values)
```

## 13.3 When a value is missing

<!-- lcl: file=examples/13/missing_rate.lcl expect=run:blocked primary=error.required.missing -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.missing_rate
    NAME: "A value that may be missing"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.rate
    TYPE: INTEGER
    REQUIRED: FALSE
    DEFAULT: MISSING

OUTPUT:
    ID: output.rate_doubled
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.rate
    ASSERT: EXISTS(REF(output.rate_doubled))

ACTION:
    ID: action.double
    OPERATION: core.calculate
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(input.rate) * 2"
    OUTPUT: REF(output.rate_doubled)

VERIFY:
    ID: verify.rate
    ASSERT: EXISTS(REF(output.rate_doubled))

SUCCESS:
    ID: success.rate
    ALL: [REF(verify.rate)]

TASK:
    ID: task.rate
    GOAL: REF(goal.rate)
    INPUT: REF(input.rate)
    ACTION: REF(action.double)
    OUTPUT: REF(output.rate_doubled)
    SUCCESS: REF(success.rate)

EXECUTE:
    REFERENCE: REF(task.rate)
```

<!-- lcl-run: file=examples/13/missing_rate.lcl expect=run:succeeded args="--input input.rate=7" -->

`DEFAULT: MISSING` says explicitly: "if nobody supplies this, there is no
value". Run it without an input:

```
$ lcl run missing_rate.lcl
missing_rate.lcl:25:1: error.required.missing [execution]: a required left operand yielded MISSING at its demand point
  ...
status.blocked
  ...
  2 invocation(s), 1 event(s), 3 step(s)
```

The calculation needed the rate, the rate did not exist, so the task is
**blocked**: it could not proceed because data was unavailable. It did not
guess a rate, and it did not pretend to succeed. Supply the value and it
works:

```
$ lcl run --input input.rate=7 missing_rate.lcl
status.succeeded
  ...
  OUTPUT output.rate_doubled published = 14
```

Notice `1 event(s)` in the blocked run. The missing value raised an
**event**, which a handler could have responded to (13.5).

## 13.4 Supplying values: DEFAULT and ASSUME

A missing value can be filled from several places, in this fixed order
(`05_SEMANTICS/06_...`):

1. an explicit `VALUE`;
2. a resolved `SOURCE`, INPUT, STATE, MEMORY or CONTEXT;
3. a `DEFAULT`, which applies **only to MISSING**, never to NULL or UNKNOWN;
4. an applicable `ASSUME`;
5. a matching HANDLER;
6. otherwise a required item blocks or fails, and an optional one stays
   absent.

**ASSUME** is a declared, conditional, temporary value, used when a value is
missing and a sensible assumption is acceptable, *provided it is recorded*:

<!-- lcl: expect=fragment -->
```lcl
EVIDENCE:
    ID: evidence.rate_assumed
    TYPE: STRING
    VALUE: "Rate assumed to be 20 because none was supplied."
    PROVENANCE: "Declared in this document."

ASSUME:
    ID: assumption.rate
    TARGET: REF(input.rate)
    VALUE: 20
    WHEN: REF(input.rate) == MISSING
    EVIDENCE: REF(evidence.rate_assumed)
```

An ASSUME never overrides a value that was actually given, applies only when
its WHEN is TRUE, and must leave evidence behind, so anyone reading the
result can see that an assumption was made. This is the opposite of silent
guessing.

> **Tool note.** The current `lcl` records the assumption's evidence but does
> not yet apply the ASSUME's value, so the example above still ends blocked
> on the missing rate. The specification's own example
> `08_EXAMPLES/VALID/09_EXPLICIT_ASSUMPTION_AND_EVIDENCE.lcl` behaves the
> same way with this tool. Until it is supported, use a `DEFAULT` for values
> that have a safe standard, and keep ASSUME for documents meant for an AI
> interpreter.

## 13.5 Events and handlers

Some errors are **recoverable**: when one of them happens, it raises an
**event**, and a HANDLER declared for that event can respond. There are
exactly five:

| Event | Raised by | Without a handler, the run ends |
|---|---|---|
| `event.missing` | `error.required.missing` | blocked |
| `event.unknown` | `error.value.unknown` | blocked |
| `event.dependency_failure` | `error.dependency.unsatisfied` | blocked |
| `event.host_constraint` | `error.host.constraint` | blocked |
| `event.execution_error` | `error.retry.exhausted` | failed |

All other errors, such as a syntax error, a type mismatch or a forbidden
action, raise no event and cannot be "handled". A broken document has to be
fixed, not recovered.

A **HANDLER** is declared at the top level and *attached* where it should
apply: in a TASK's, a STEP's, a DEPENDENCY's or a FAILURE's `HANDLER` field,
or in an action's RETRY block. The closest attachment is tried first. A
handler has:

* `EVENT`: the event it responds to;
* `OPERATION`: what to do, for example `core.retry`, `core.stop`,
  `core.continue` or `core.ask`;
* optionally `WHEN`, `FALLBACK` (what to do if the operation itself fails),
  and `LIMIT`.

A handler that is declared but never attached is never selected.

## 13.6 Retrying

Some failures are temporary: a server is busy, or a file is being written.
An action may declare a **RETRY** block. It is a budget of extra attempts,
used by a handler whose operation is `core.retry`:

<!-- lcl: file=examples/13/retry.lcl expect=run:succeeded workspace=/tmp/lcl-manual/retry seed=examples/13/seed grant=r -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.retry
    NAME: "Read with a bounded retry"
    VERSION: "1.0.0"
    KIND: kind.task

WORKSPACE:
    ID: workspace.lab
    PATH: PATH("/tmp/lcl-manual/retry")
    MODE: mode.read_only

HANDLER:
    ID: handler.retry_read
    EVENT: event.host_constraint
    OPERATION: core.retry
    LIMIT: 2

OUTPUT:
    ID: output.text
    TYPE: STRING
    FORMAT: format.plain_text

GOAL:
    ID: goal.read
    ASSERT: EXISTS(REF(output.text))

ACTION:
    ID: action.read
    OPERATION: core.read
    TARGET: PATH(REF(workspace.lab), "in.txt")
    OUTPUT: REF(output.text)
    RETRY:
        LIMIT: 2
        DELAY: DURATION(0, unit.second)
        HANDLER: REF(handler.retry_read)

VERIFY:
    ID: verify.read
    ASSERT: REF(output.text) == "data\n"

SUCCESS:
    ID: success.read
    ALL: [REF(verify.read)]

TASK:
    ID: task.read
    GOAL: REF(goal.read)
    WORKSPACE: REF(workspace.lab)
    ACTION: REF(action.read)
    OUTPUT: REF(output.text)
    SUCCESS: REF(success.read)

EXECUTE:
    REFERENCE: REF(task.read)
```

<!-- lcl-run: file=examples/13/retry.lcl expect=run:blocked primary=error.host.constraint workspace=/tmp/lcl-manual/retry seed=examples/13/seed -->

To try it, create the file first:

```
$ mkdir -p /tmp/lcl-manual/retry
$ printf 'data\n' > /tmp/lcl-manual/retry/in.txt
```

With `--allow-read /tmp/lcl-manual/retry` it succeeds at the first attempt.
Without the grant, every attempt fails the same way, and you can watch the
budget being spent:

```
$ lcl run retry.lcl
retry.lcl:30:1: error.host.constraint [execution]: no filesystem capability is installed
  retry.lcl:30:1: error.host.constraint [execution]: no filesystem capability is installed
  retry.lcl:30:1: error.host.constraint [execution]: no filesystem capability is installed
  retry.lcl:30:1: error.retry.exhausted [execution]: 3 attempts were made and all failed
  ...
status.blocked
  ...
  4 invocation(s), 3 event(s), 3 step(s)
```

The rules for RETRY:

* RETRY sits inside an ACTION. It has no ID and cannot be referenced.
* `LIMIT` is the number of **extra** attempts, from 0 to 100. The total is
  at most `1 + LIMIT`: here 3.
* A handler's `LIMIT` for `core.retry` must equal the RETRY's `LIMIT`. The
  RETRY block is the only budget, and a handler cannot add attempts.
* `WHEN` (default TRUE) is checked before each extra attempt, and `DELAY`
  (default 0 seconds) is waited before it.
* The first successful attempt ends the retrying.
* `error.retry.exhausted` is reported only when exactly `1 + LIMIT` attempts
  were made and all failed.
* An attempt that may already have changed something (for example, half a
  file written) is retried only when it is provably safe to repeat.

Retrying cannot fix a failure that will happen every time. Here, no amount of
retrying grants a missing permission. RETRY is for failures that can go away
by themselves.

## Summary

* NULL is a stored "empty". MISSING means no value. UNKNOWN means a value
  that cannot be determined. They never convert into each other.
* Logic is three-valued. Required conditions that end UNKNOWN or MISSING are
  errors, never silently FALSE.
* DEFAULT fills MISSING only. ASSUME is a recorded, conditional assumption.
* Five recoverable errors raise events. HANDLERs attached to tasks, steps or
  retries respond to them.
* RETRY gives an action at most `1 + LIMIT` attempts, used through a
  `core.retry` handler.

## Exercises

1. For each, say whether the result is TRUE, FALSE, UNKNOWN or an error:
   `FALSE AND UNKNOWN`, `NOT (TRUE OR UNKNOWN)`, `ANY([UNKNOWN, FALSE])`,
   `MISSING + 1`, `EXISTS(NULL)`.
2. Change `missing_rate.lcl` so a missing rate is treated as 10. Which line
   do you change, and what does the run print now?
3. In `retry.lcl`, change the RETRY's `LIMIT` to 4 but leave the handler's
   `LIMIT` at 2. Predict what `lcl run` reports, then try it.
4. A weather service sometimes times out. Explain why RETRY suits that, but
   would not help if the service's address were wrong.

Solutions: [solutions/README.md](solutions/README.md#chapter-13).
