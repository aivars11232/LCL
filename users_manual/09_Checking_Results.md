# Chapter 9. Checking results

**In this chapter you will learn**

* the difference between checking *before* the work (VALIDATE) and *after*
  it (VERIFY);
* how SUCCESS combines checks, and how FAILURE names a specific failure;
* optional checks, and checks that apply only sometimes;
* how to record evidence;
* how to write a test document;
* every status a run can end with.

"How do we know it worked?" is the question LCL takes most seriously. A
document is not complete until it says how its own result is checked. The
rules are in `05_SEMANTICS/10_VERIFY_TEST_EVIDENCE_SUCCESS_FAILURE_AND_STATUS.txt`.

## 9.1 Before and after

| Block | When it runs | If a required one is FALSE |
|---|---|---|
| `VALIDATE` | before any action runs | the document is **rejected**: nothing runs, exit code 1 |
| `VERIFY` | after the actions | the task **fails**: exit code 2 |

Use VALIDATE for **preconditions**, things that must be true for the work to
make sense at all, such as "the price is positive". Use VERIFY for
**postconditions**, things that must be true once the work is done, such as
"the total equals price times quantity". The everyday word "check" is
ambiguous, which is why LCL has two words for it.

> **Tool note.** The current `lcl` evaluates only part of the expression
> language inside a VALIDATE: comparisons, `AND`/`OR`/`NOT`, `+`, `-`, `*`,
> `IN`, `CONTAINS`, `COUNT`, `EMPTY` and `EXISTS`. A VALIDATE that uses
> division, `ROUND`, `ABS`, `SUM`, `MIN`, `MAX`, `ALL`/`ANY`/`NONE`, `MATCHES`
> or a constructor such as `DATE(...)` is refused with a misleading
> `error.required.missing` ("cannot be evaluated before effects"). VERIFY
> checks support the whole language. Until the tool catches up, keep VALIDATE
> assertions simple, and put more complex conditions in a VERIFY.

## 9.2 A worked example

<!-- lcl: file=examples/09/order_checks.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.order_checks
    NAME: "Validate before, verify after"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.price
    TYPE: DECIMAL
    REQUIRED: FALSE
    DEFAULT: 19.99

INPUT:
    ID: input.quantity
    TYPE: INTEGER
    REQUIRED: FALSE
    DEFAULT: 3

OUTPUT:
    ID: output.total
    TYPE: DECIMAL
    FORMAT: format.plain_text

GOAL:
    ID: goal.total
    ASSERT: REF(output.total) > 0

VALIDATE:
    ID: validate.price
    ASSERT: REF(input.price) >= 0.01 AND REF(input.price) <= 1000.00

VALIDATE:
    ID: validate.quantity
    ASSERT: REF(input.quantity) >= 1

ACTION:
    ID: action.total
    OPERATION: core.calculate
    TARGET: REF(input.price)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(input.price) * REF(input.quantity)"
    OUTPUT: REF(output.total)

VERIFY:
    ID: verify.total
    ASSERT: REF(output.total) == REF(input.price) * REF(input.quantity)

FAILURE:
    ID: failure.too_large
    WHEN: REF(output.total) > 500.00
    STATUS: status.failed

SUCCESS:
    ID: success.total
    ALL: [REF(verify.total)]

TASK:
    ID: task.total
    GOAL: REF(goal.total)
    INPUT: [REF(input.price), REF(input.quantity)]
    ACTION: REF(action.total)
    OUTPUT: REF(output.total)
    SUCCESS: REF(success.total)

EXECUTE:
    REFERENCE: REF(task.total)
```

Run it four ways:

<!-- lcl-run: file=examples/09/order_checks.lcl expect=reject:error.validation.failed args="--input input.price=2000.00" -->
<!-- lcl-run: file=examples/09/order_checks.lcl expect=reject:error.validation.failed args="--input input.quantity=0" -->
<!-- lcl-run: file=examples/09/order_checks.lcl expect=run:failed args="--input input.quantity=30" -->

```
$ lcl run order_checks.lcl
status.succeeded
  VERIFY verify.total = TRUE (required)
  SUCCESS success.total ALL = TRUE
  OUTPUT output.total published = 59.97
  ...

$ lcl run --input input.price=2000.00 order_checks.lcl
order_checks.lcl:32:9: error.validation.failed [validation]: required check `validate.price` asserts a condition that is FALSE
    A required VALIDATE assertion is FALSE.

order_checks.lcl was rejected at the preflight stage.

$ lcl run --input input.quantity=0 order_checks.lcl
order_checks.lcl:36:9: error.validation.failed [validation]: required check `validate.quantity` asserts a condition that is FALSE
...

$ lcl run --input input.quantity=30 order_checks.lcl
status.failed
  VERIFY verify.total = TRUE (required)
  SUCCESS success.total ALL = TRUE
  FAILURE failure.too_large -> status.failed
  OUTPUT output.total published = 599.70
  because: DeclaredFailure("failure.too_large")
```

Notice:

* A failed VALIDATE stops everything **before** any action. The rejection
  happens at the *preflight* stage, the same stage `lcl validate` reaches,
  so `lcl validate` with the same input shows it without running anything.
* In the last run, everything worked and SUCCESS was TRUE, yet the task
  failed, because a FAILURE clause matched. FAILURE lets you name a specific
  situation that must count as failure.

## 9.3 SUCCESS

SUCCESS contains exactly one of three fields, each holding a list of
TRUE/FALSE conditions, usually references to checks:

| Field | TRUE when |
|---|---|
| `ALL: [...]` | every member is TRUE |
| `ANY: [...]` | at least one member is TRUE |
| `NONE: [...]` | no member is TRUE |

<!-- lcl: expect=fragment -->
```lcl
SUCCESS:
    ID: success.any_route
    ANY: [REF(verify.by_train), REF(verify.by_bus)]
```

A task succeeds only when **all** of these hold:

1. its SUCCESS is TRUE;
2. every **required** VERIFY that applies is TRUE, even one not listed in
   SUCCESS;
3. every required OUTPUT was produced;
4. required EVIDENCE exists;
5. no FAILURE clause matched, and no error occurred.

## 9.4 FAILURE

<!-- lcl: expect=fragment -->
```lcl
FAILURE:
    ID: failure.too_large
    WHEN: REF(output.total) > 500.00
    STATUS: status.failed
```

A FAILURE clause has a condition (`WHEN`) and the non-success `STATUS` to end
with when the condition is TRUE. If several clauses are TRUE, the first one
in the document wins. A FAILURE never overrides an error that has already
decided the outcome. You can also add `ERROR:` with an error identifier, to
classify the failure for whoever reads the result.

## 9.5 Optional checks and conditional checks

* `REQUIRED: FALSE` makes a check **optional**. Its result is still computed
  and shown, but a FALSE result does not stop the task.
* `WHEN:` makes a check apply only when its condition is TRUE. When the
  condition is FALSE the check is **skipped**, which is not the same as
  passing.

<!-- lcl: file=examples/09/sensor.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.checks
    NAME: "Optional checks and quantifiers"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.temperature
    TYPE: DECIMAL
    REQUIRED: FALSE
    DEFAULT: 21.5

OUTPUT:
    ID: output.reading
    TYPE: DECIMAL
    FORMAT: format.plain_text

EVIDENCE:
    ID: evidence.sensor
    TYPE: STRING
    VALUE: "Reading taken from classroom sensor 3."
    PROVENANCE: "Declared in this document for teaching."
    REQUIRED: TRUE

GOAL:
    ID: goal.reading
    ASSERT: EXISTS(REF(output.reading))

ACTION:
    ID: action.read
    OPERATION: core.return
    TARGET: REF(input.temperature)
    OUTPUT: REF(output.reading)

VERIFY:
    ID: verify.plausible
    ASSERT: REF(output.reading) > -50.0 AND REF(output.reading) < 60.0
    EVIDENCE: REF(evidence.sensor)

VERIFY:
    ID: verify.comfortable
    REQUIRED: FALSE
    ASSERT: REF(output.reading) >= 18.0 AND REF(output.reading) <= 24.0

VERIFY:
    ID: verify.freezing_warning
    WHEN: REF(output.reading) < 0.0
    ASSERT: REF(output.reading) > -20.0

SUCCESS:
    ID: success.reading
    ALL: [REF(verify.plausible)]

TASK:
    ID: task.reading
    GOAL: REF(goal.reading)
    INPUT: REF(input.temperature)
    ACTION: REF(action.read)
    OUTPUT: REF(output.reading)
    SUCCESS: REF(success.reading)

EXECUTE:
    REFERENCE: REF(task.reading)
```

<!-- lcl-run: file=examples/09/sensor.lcl expect=run:succeeded args="--input input.temperature=30.0" -->
<!-- lcl-run: file=examples/09/sensor.lcl expect=run:succeeded args="--input input.temperature=-5.0" -->
<!-- lcl-run: file=examples/09/sensor.lcl expect=run:failed primary=error.verification.failed args="--input input.temperature=-30.0" -->
<!-- lcl-run: file=examples/09/sensor.lcl expect=run:failed primary=error.verification.failed args="--input input.temperature=99.0" -->

| Temperature | `plausible` | `comfortable` (optional) | `freezing_warning` (when below 0) | Status |
|---|---|---|---|---|
| 21.5 | TRUE | TRUE | skipped | succeeded |
| 30.0 | TRUE | FALSE | skipped | succeeded |
| -5.0 | TRUE | FALSE | TRUE | succeeded |
| -30.0 | TRUE | FALSE | FALSE | **failed** |
| 99.0 | FALSE | FALSE | skipped | **failed** |

Look at the -30.0 row: `freezing_warning` is not listed in SUCCESS, but it is
required and it applied, so its FALSE result still fails the task. The run
output shows this plainly:

```
$ lcl run --input input.temperature=-30.0 sensor.lcl
sensor.lcl:49:9: error.verification.failed [verification_or_completion]: required VERIFY `verify.freezing_warning` asserted FALSE
...
status.failed
  VERIFY verify.plausible = TRUE (required)
  VERIFY verify.comfortable = FALSE
  VERIFY verify.freezing_warning = FALSE (required)
  SUCCESS success.reading ALL = TRUE
  ...
```

## 9.6 Evidence

EVIDENCE records **what supports a result**: an observation, a source, a
document. It has a type, and a `VALUE` or a `SOURCE`. `PROVENANCE` says where
it came from. A check can point to the evidence it relies on with its
`EVIDENCE` field. Required evidence must exist for the task to succeed, and
the run output lists it:

```
  EVIDENCE evidence.sensor satisfied "Reading taken from classroom sensor 3."
```

Evidence matters most when an AI does the work. "The tests pass" is a claim;
the test output, attached as evidence, is proof.

## 9.7 Test documents

A **test document** (`KIND: kind.test`) exists to test something. Its
EXECUTE points at a `TEST` block, which may run one action or task and then
compares an `EXPECTED` value with an `ACTUAL` one, or checks an `ASSERT`:

<!-- lcl: file=examples/09/test_double.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.test_double
    NAME: "Test that doubling works"
    VERSION: "1.0.0"
    KIND: kind.test

INPUT:
    ID: input.value
    TYPE: INTEGER
    VALUE: 21

OUTPUT:
    ID: output.value
    TYPE: INTEGER
    FORMAT: format.plain_text

ACTION:
    ID: action.double
    OPERATION: core.calculate
    TARGET: REF(input.value)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "target * 2"
    OUTPUT: REF(output.value)

TEST:
    ID: test.double
    ACTION: REF(action.double)
    EXPECTED: 42
    ACTUAL: REF(output.value)

EXECUTE:
    REFERENCE: REF(test.double)
```

```
$ lcl run test_double.lcl
status.succeeded
  TEST test.double = TRUE (required)
  because: SuccessSatisfied
```

Change `EXPECTED: 42` to `EXPECTED: 43` and the run ends
`status.failed` with `error.verification.failed`. A test document needs no
GOAL, TASK or SUCCESS: the TEST is its own success condition.

## 9.8 Statuses

Every run ends in exactly one **terminal status**
(`06_STANDARD_LIBRARY/05_CORE_STATUS_IDENTIFIERS.txt`):

| Status | Means | Exit code |
|---|---|---|
| `status.succeeded` | everything required was done and checked | 0 |
| `status.invalid` | the document itself is wrong; nothing ran | 1 |
| `status.failed` | a required action, check or rule failed | 2 |
| `status.blocked` | something needed was not available: data, permission, a capability | 2 |
| `status.stopped` | execution stopped on a declared stop path | 2 |
| `status.cancelled` | whoever started the run cancelled it | 2 |
| `status.partial` | only part of the required work was completed | 2 |

`status.skipped` is used for parts of a run that did not apply, never for a
whole run. The in-between states `status.not_started`, `status.validating`,
`status.ready` and `status.running` appear only while a run is in progress.

Two distinctions are worth remembering:

* **failed or blocked?** *Failed* means the work was attempted and went wrong,
  or produced the wrong result. *Blocked* means it could not proceed:
  something outside the document was missing. Chapter 7's custom operation
  was blocked because the tool has no summariser. Retrying later with the
  missing thing available might succeed; retrying a failure unchanged will
  fail again.
* **An operation's own result is separate from the task's status.** A
  comparison that answers FALSE, as in Chapter 8, *succeeded*: it
  correctly found FALSE. What decides the task is whether your checks accept
  that answer.

## Summary

* VALIDATE checks preconditions before anything runs; a required FALSE
  rejects the document. VERIFY checks postconditions afterwards; a required
  FALSE fails the task.
* SUCCESS uses ALL, ANY or NONE. Every applicable required check must also
  pass, listed in SUCCESS or not.
* FAILURE maps a named situation to a non-success status.
* `REQUIRED: FALSE` makes a check optional. `WHEN` makes it conditional, and
  a skipped check is not a passed one.
* EVIDENCE records what supports a result.
* `kind.test` documents run a TEST comparing EXPECTED with ACTUAL.

## Exercises

1. Add a VALIDATE to `order_checks.lcl` that rejects an order of more than
   100 items. Test it from the command line.
2. In `sensor.lcl`, make `verify.comfortable` required. For which
   temperatures in the table does the status change?
3. Write a test document that tests `core.filter`: given `[3, 8, 1, 9]` and the
   predicate `"item > 2"`, the result must be `[3, 8, 9]`.
4. Write a SUCCESS that is TRUE when neither of two checks,
   `verify.too_hot` and `verify.too_cold`, is TRUE.

Solutions: [solutions/README.md](solutions/README.md#chapter-9).
