# Solutions to the exercises

Try each exercise yourself before reading its solution. Many exercises have
more than one good answer. Where a solution is an LCL document, it is shown
in full, is also saved in this folder, and has been checked with `lcl`, like
every example in the manual.

Paths in the documents below (such as `solutions/03/triple.lcl`) are relative
to the `users_manual` folder.

## Chapter 1

1. **Questions an assistant would have to guess:** Which report (file name,
   location)? Where do "the latest numbers" come from, and as of when? What
   counts as "looks wrong", and wrong compared with what? Is the assistant
   allowed to change what looks wrong, or only report it? What does "break"
   mean: other files, formatting, formulas? How would anyone check that
   nothing broke? What counts as done?
2. **Report and location:** INPUT (or WORKSPACE and PATH). **Latest numbers:**
   INPUT with an exact SOURCE and PROVENANCE. **What is wrong:** VALIDATE /
   VERIFY conditions. **Allowed to change it:** ALLOW or FORBID, plus an
   ACTION. **Don't break anything:** FORBID and PRESERVE. **How to check:**
   VERIFY. **Done:** SUCCESS.
3. For example: "You may take one biscuit" leaves you free to take none;
   "you must take one biscuit" does not. A system that treats permission as
   an instruction does things nobody asked for.

## Chapter 2

1. `--machine`: `lcl check --machine hello.lcl` prints one JSON object.
2. `lcl check` exits 0 and `lcl validate` exits 0: the document is well
   formed, and changing a string does not make it invalid. `lcl run` exits 2
   with `error.verification.failed`, because the output is now
   `"Hello, LCL!"` and the VERIFY still expects `"Hello, world!"`.
3. `error.source.tab` at the `lexical` stage: tabs are forbidden anywhere
   outside strings.

## Chapter 3

**1. Triple.**

<!-- lcl: file=solutions/03/triple.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.triple
    NAME: "Triple one number"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.number
    TYPE: INTEGER
    VALUE: 14

OUTPUT:
    ID: output.tripled
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.tripled
    ASSERT: REF(output.tripled) == 42

ACTION:
    ID: action.triple
    OPERATION: core.calculate
    TARGET: REF(input.number)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(input.number) * 3"
    OUTPUT: REF(output.tripled)

VERIFY:
    ID: verify.tripled
    ASSERT: REF(output.tripled) == 42

SUCCESS:
    ID: success.tripled
    ALL: [REF(verify.tripled)]

TASK:
    ID: task.triple
    GOAL: REF(goal.tripled)
    INPUT: REF(input.number)
    ACTION: REF(action.triple)
    OUTPUT: REF(output.tripled)
    SUCCESS: REF(success.tripled)

EXECUTE:
    REFERENCE: REF(task.triple)
```

**2. Difference.** With two inputs, the calculation needs no TARGET: the
expression names both values with REF.

<!-- lcl: file=solutions/03/difference.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.difference
    NAME: "Difference of two numbers"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.larger
    TYPE: INTEGER
    VALUE: 100

INPUT:
    ID: input.smaller
    TYPE: INTEGER
    VALUE: 58

OUTPUT:
    ID: output.difference
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.difference
    ASSERT: REF(output.difference) == 42

ACTION:
    ID: action.subtract
    OPERATION: core.calculate
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(input.larger) - REF(input.smaller)"
    OUTPUT: REF(output.difference)

VERIFY:
    ID: verify.difference
    ASSERT: REF(output.difference) == 42

SUCCESS:
    ID: success.difference
    ALL: [REF(verify.difference)]

TASK:
    ID: task.difference
    GOAL: REF(goal.difference)
    INPUT: [REF(input.larger), REF(input.smaller)]
    ACTION: REF(action.subtract)
    OUTPUT: REF(output.difference)
    SUCCESS: REF(success.difference)

EXECUTE:
    REFERENCE: REF(task.difference)
```

**3.** Yes, it still works. After `LCL` and `SPECIFICATION`, block order does
not matter: the whole document is resolved before anything runs, and
references may point forwards.

<!-- lcl: file=solutions/03/task_first.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.task_first
    NAME: "Double one number"
    VERSION: "1.0.0"
    KIND: kind.task

TASK:
    ID: task.double
    GOAL: REF(goal.doubled)
    INPUT: REF(input.number)
    ACTION: REF(action.double)
    OUTPUT: REF(output.doubled)
    SUCCESS: REF(success.doubled)

INPUT:
    ID: input.number
    TYPE: INTEGER
    VALUE: 21

OUTPUT:
    ID: output.doubled
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.doubled
    ASSERT: REF(output.doubled) == 42

ACTION:
    ID: action.double
    OPERATION: core.calculate
    TARGET: REF(input.number)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(input.number) * 2"
    OUTPUT: REF(output.doubled)

VERIFY:
    ID: verify.doubled
    ASSERT: REF(output.doubled) == 42

SUCCESS:
    ID: success.doubled
    ALL: [REF(verify.doubled)]

EXECUTE:
    REFERENCE: REF(task.double)
```

**4.** `error.id.duplicate`: IDs must be unique.

<!-- lcl: file=solutions/03/duplicate_id.invalid.lcl expect=reject:error.id.duplicate -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.duplicate_id
    NAME: "Double one number"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.number
    TYPE: INTEGER
    VALUE: 21

INPUT:
    ID: input.number
    TYPE: INTEGER
    VALUE: 22

OUTPUT:
    ID: output.doubled
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.doubled
    ASSERT: REF(output.doubled) == 42

ACTION:
    ID: action.double
    OPERATION: core.calculate
    TARGET: REF(input.number)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(input.number) * 2"
    OUTPUT: REF(output.doubled)

VERIFY:
    ID: verify.doubled
    ASSERT: REF(output.doubled) == 42

SUCCESS:
    ID: success.doubled
    ALL: [REF(verify.doubled)]

TASK:
    ID: task.double
    GOAL: REF(goal.doubled)
    INPUT: REF(input.number)
    ACTION: REF(action.double)
    OUTPUT: REF(output.doubled)
    SUCCESS: REF(success.doubled)

EXECUTE:
    REFERENCE: REF(task.double)
```

## Chapter 4

**1 and 2.**

<!-- lcl: file=solutions/04/me.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.about_me
    NAME: "About me"
    VERSION: "1.0.0"
    KIND: kind.data

DATA:
    ID: data.name
    TYPE: STRING
    VALUE: "Ada"

DATA:
    ID: data.age
    TYPE: INTEGER
    VALUE: 12

DATA:
    ID: data.height_in_metres
    TYPE: DECIMAL
    VALUE: 1.52

DATA:
    ID: data.likes_mathematics
    TYPE: BOOLEAN
    VALUE: TRUE

DATA:
    ID: data.poem
    TYPE: STRING
    VALUE: """
        The numbers line up,
          each one in its place,
        and the answer is exact.
    """
```

**3.**

* `VALUE: [1,2,3]`: exactly one space must follow each comma, so write
  `[1, 2, 3]`.
* `VALUE: 'yes'`: single quotes are excluded, so write `"yes"`.
* `VALUE: 1.5E3`: no exponent notation, so write `1500.0` (or `1500` for an
  INTEGER).
* `Value: 5`: keywords are uppercase, so write `VALUE: 5`.
* `VALUE: 10%`: `%` is excluded, so write `PERCENTAGE(10)`.

**4.** Accepting `true` would mean silently correcting what was written, and
the corrected document would no longer be exactly the one the author wrote.
Principle 11, *unsupported syntax fails closed*, says an unclear form is an
error, never a guess. Principle 3, *one keyword has one meaning*, is why the
lowercase `true` stays an ordinary identifier.

## Chapter 5

**1.**

* `TYPE: DECIMAL` with `VALUE: 10`: **rejected**, because 10 is an INTEGER.
  Write `10.0`.
* `TYPE: LIST[DECIMAL]` with `VALUE: [1.5, 2]`: **rejected**
  (`error.collection.heterogeneous`): 2 is an INTEGER.
* `DATE("2028-02-29")`: **accepted**, because 2028 is a leap year.
* `PERCENTAGE(100)`: **accepted**, because the range 0 to 100 includes 100.

**2 and 3.**

<!-- lcl: file=solutions/05/books.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.library_books
    NAME: "Books and seasons"
    VERSION: "1.0.0"
    KIND: kind.data

DEFINE:
    ID: type.book
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: title
        TYPE: STRING
        REQUIRED: TRUE
    FIELD:
        NAME: pages
        TYPE: INTEGER
        REQUIRED: TRUE
        MINIMUM: 1
    FIELD:
        NAME: subtitle
        TYPE: STRING
        REQUIRED: FALSE

DEFINE:
    ID: type.season
    KIND: kind.type
    BASE: ENUM
    ITEM: spring
    ITEM: summer
    ITEM: autumn
    ITEM: winter

DATA:
    ID: data.first_book
    TYPE: OBJECT[REF(type.book)]
    VALUE:
        title: "Flatland"
        pages: 96
        subtitle: "A Romance of Many Dimensions"

DATA:
    ID: data.second_book
    TYPE: OBJECT[REF(type.book)]
    VALUE:
        title: "The Number Devil"
        pages: 262

DATA:
    ID: data.shelf
    TYPE: LIST[OBJECT[REF(type.book)]]
    VALUE: [REF(data.first_book), REF(data.second_book)]

DATA:
    ID: data.favourite_season
    TYPE: REF(type.season)
    VALUE: autumn
```

**4.** Measures never convert units, so centimetres and metres are simply
different units, and the values are unequal. Durations are always measured
in exact nanoseconds, so 60 minutes and 1 hour are the same duration.

## Chapter 6

All the answers to exercises 1, 3 and 4, checked as VERIFY statements:

<!-- lcl: file=solutions/06/answers.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.chapter6_answers
    NAME: "Chapter 6 answers, checked"
    VERSION: "1.0.0"
    KIND: kind.task

OUTPUT:
    ID: output.done
    TYPE: BOOLEAN
    FORMAT: format.plain_text

GOAL:
    ID: goal.all_true
    ASSERT: REF(output.done) == TRUE

ACTION:
    ID: action.done
    OPERATION: core.return
    TARGET: TRUE
    OUTPUT: REF(output.done)

VERIFY:
    ID: verify.arithmetic
    ASSERT: 20 - 4 * 3 == 8 AND (20 - 4) * 3 == 48

VERIFY:
    ID: verify.division
    ASSERT: 9 / 4 == 2.25 AND ROUND(2 / 3, 3) == 0.667

VERIFY:
    ID: verify.nested_count
    ASSERT: COUNT([[1, 2], [3]]) == 2

VERIFY:
    ID: verify.rounded_division_allowed
    ASSERT: ROUND(2 / 3, 1) == 0.7

VERIFY:
    ID: verify.postcode_matches
    ASSERT: "SW1A" MATCHES REGEX("[A-Z]{1,2}[0-9][A-Z0-9]?") AND "M1" MATCHES REGEX("[A-Z]{1,2}[0-9][A-Z0-9]?")

VERIFY:
    ID: verify.postcode_rejects
    ASSERT: NOT ("sw1a" MATCHES REGEX("[A-Z]{1,2}[0-9][A-Z0-9]?")) AND NOT ("SW1AB" MATCHES REGEX("[A-Z]{1,2}[0-9][A-Z0-9]?"))

VERIFY:
    ID: verify.average
    ASSERT: ROUND(SUM([12, 15, 9, 18]) / COUNT([12, 15, 9, 18]), 1) == 13.5

SUCCESS:
    ID: success.all_true
    ALL: [REF(verify.arithmetic), REF(verify.division), REF(verify.nested_count), REF(verify.rounded_division_allowed), REF(verify.postcode_matches), REF(verify.postcode_rejects), REF(verify.average)]

TASK:
    ID: task.answers
    GOAL: REF(goal.all_true)
    ACTION: REF(action.done)
    OUTPUT: REF(output.done)
    SUCCESS: REF(success.all_true)

EXECUTE:
    REFERENCE: REF(task.answers)
```

**1.** `20 - 4 * 3` is 8; `(20 - 4) * 3` is 48; `9 / 4` is 2.25;
`ROUND(2 / 3, 3)` is 0.667; `COUNT([[1, 2], [3]])` is 2 (two members, each a
list).

**2.**

* `7 / 0` is rejected (`error.numeric.division_by_zero`).
* `2 / 3` is rejected (`error.numeric.non_terminating`).
* `ROUND(2 / 3, 1)` is accepted, and is 0.7.
* `NOT 5 > 3` is rejected (`error.operator.operand`): it means
  `(NOT 5) > 3`. Write `NOT (5 > 3)`.
* `"total: " + "5"` is rejected (`error.operator.operand`): `+` does not join
  strings.

**3.** `REGEX("[A-Z]{1,2}[0-9][A-Z0-9]?")`: see `verify.postcode_matches` and
`verify.postcode_rejects` above.

**4.** `ROUND(SUM([12, 15, 9, 18]) / COUNT([12, 15, 9, 18]), 1)`, which is
13.5.

## Chapter 7

**1.** A constant can be computed from other constants:

<!-- lcl: file=solutions/07/time_constants.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.time_constants
    NAME: "Time constants"
    VERSION: "1.0.0"
    KIND: kind.data

DEFINE:
    ID: constant.days_per_week
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 7

DEFINE:
    ID: constant.hours_per_day
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 24

DEFINE:
    ID: constant.minutes_per_hour
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 60

DEFINE:
    ID: constant.minutes_per_week
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: REF(constant.days_per_week) * REF(constant.hours_per_day) * REF(constant.minutes_per_hour)
```

**2.** `error.block.context`: an ACTION is not a legal top-level block in a
data document.

<!-- lcl: file=solutions/07/time_constants_with_action.invalid.lcl expect=reject:error.block.context -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.time_constants_with_action
    NAME: "Time constants"
    VERSION: "1.0.0"
    KIND: kind.data

DEFINE:
    ID: constant.days_per_week
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 7

DEFINE:
    ID: constant.hours_per_day
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 24

DEFINE:
    ID: constant.minutes_per_hour
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 60

DEFINE:
    ID: constant.minutes_per_week
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: REF(constant.days_per_week) * REF(constant.hours_per_day) * REF(constant.minutes_per_hour)

ACTION:
    ID: action.nothing
    OPERATION: core.return
    TARGET: 1
```

**3.** The formula gains one more factor, and the checks change from 1500 to
18000:

<!-- lcl: file=solutions/07/definitions_per_term.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.definitions_per_term
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
    ID: constant.weeks_per_term
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 12

DEFINE:
    ID: constant.weekly_formula
    KIND: kind.constant
    TYPE: STRING
    VALUE: "target * REF(constant.days_per_week) * REF(constant.lesson_minutes) * REF(constant.weeks_per_term)"

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
    ASSERT: REF(output.minutes) == 18000

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
    ASSERT: REF(output.minutes) == 18000

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

**4.** `DETERMINISTIC: FALSE`, because a translation can be worded in more than
one correct way, and two runs may differ. `DEPENDENCY: [model]`, because the
translation is produced by a language model. `SIDE_EFFECT: FALSE`, because
translating changes nothing.

<!-- lcl: file=solutions/07/translate.lcl expect=validate -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.translate_operation
    NAME: "A translation operation"
    VERSION: "1.0.0"
    KIND: kind.data
    DOMAIN: "teaching"

DEFINE:
    ID: teaching.translate
    KIND: kind.operation
    MEANING: "Translate one text into the named language."
    SIDE_EFFECT: FALSE
    DEPENDENCY: [model]
    DETERMINISTIC: FALSE
    PARAMETER:
        NAME: text
        TYPE: STRING
        REQUIRED: TRUE
    PARAMETER:
        NAME: language
        TYPE: STRING
        REQUIRED: TRUE
    RESULT:
        TYPE: STRING
```

## Chapter 8

All four exercises in one document. The cheap limit is 3.00, there is a most
expensive price and a highest-first list, and delivery is an optional input.
Note that the calculation's binding is called `postage`: `delivery` would
clash with the input `input.delivery` (section 8.5).

<!-- lcl: file=solutions/08/shop_solution.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.shop_solution
    NAME: "A small shop order, extended"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.prices
    TYPE: LIST[DECIMAL]
    VALUE: [2.50, 0.99, 12.00, 4.75]

INPUT:
    ID: input.delivery
    TYPE: DECIMAL
    REQUIRED: FALSE
    DEFAULT: 3.00

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
    ID: output.most_expensive
    TYPE: DECIMAL
    FORMAT: format.plain_text

OUTPUT:
    ID: output.highest_first
    TYPE: LIST[DECIMAL]
    FORMAT: format.json

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
        VALUE: "item < 3.00"
    OUTPUT: REF(output.cheap_items)

ACTION:
    ID: action.cheap_count
    OPERATION: core.filter
    TARGET: REF(input.prices)
    PARAMETER:
        NAME: predicate
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "item < 3.00"
    OUTPUT: REF(output.cheap_count)

ACTION:
    ID: action.total
    OPERATION: core.calculate
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "SUM(amounts) + postage"
    PARAMETER:
        NAME: bindings
        TYPE: OBJECT
        REQUIRED: TRUE
        VALUE:
            amounts: REF(input.prices)
            postage: REF(input.delivery)
    OUTPUT: REF(output.total)

ACTION:
    ID: action.most_expensive
    OPERATION: core.calculate
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "MAX(REF(input.prices))"
    OUTPUT: REF(output.most_expensive)

ACTION:
    ID: action.highest_first
    OPERATION: core.sort
    TARGET: REF(input.prices)
    PARAMETER:
        NAME: direction
        TYPE: ENUM
        REQUIRED: TRUE
        VALUE: descending
    OUTPUT: REF(output.highest_first)

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
    ASSERT: REF(output.cheap_items) == [2.50, 0.99] AND REF(output.cheap_count) == 2

VERIFY:
    ID: verify.total
    ASSERT: REF(output.total) == 20.24 + REF(input.delivery)

VERIFY:
    ID: verify.budget
    ASSERT: REF(output.within_budget) == TRUE

VERIFY:
    ID: verify.most_expensive
    ASSERT: REF(output.most_expensive) == 12.00

VERIFY:
    ID: verify.highest_first
    ASSERT: REF(output.highest_first) == [12.00, 4.75, 2.50, 0.99]

SUCCESS:
    ID: success.order
    ALL: [REF(verify.cheap), REF(verify.total), REF(verify.budget), REF(verify.most_expensive), REF(verify.highest_first)]

TASK:
    ID: task.order
    GOAL: REF(goal.order)
    INPUT: [REF(input.prices), REF(input.delivery), REF(input.budget)]
    ACTION: [REF(action.cheap_items), REF(action.cheap_count), REF(action.total), REF(action.most_expensive), REF(action.highest_first), REF(action.within_budget)]
    OUTPUT: [REF(output.cheap_items), REF(output.cheap_count), REF(output.total), REF(output.most_expensive), REF(output.highest_first), REF(output.within_budget)]
    SUCCESS: REF(success.order)

EXECUTE:
    REFERENCE: REF(task.order)
```

<!-- lcl-run: file=solutions/08/shop_solution.lcl expect=run:succeeded args="--input input.delivery=0.00" -->

With free delivery, `lcl run --input input.delivery=0.00 shop_solution.lcl`
also succeeds, because `verify.total` now compares with
`20.24 + REF(input.delivery)` instead of a fixed number.

## Chapter 9

**1.**

<!-- lcl: file=solutions/09/order_checks_solution.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.order_checks_solution
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

VALIDATE:
    ID: validate.quantity_limit
    ASSERT: REF(input.quantity) <= 100

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

<!-- lcl-run: file=solutions/09/order_checks_solution.lcl expect=reject:error.validation.failed args="--input input.quantity=101" -->

`lcl run --input input.quantity=101 order_checks_solution.lcl` is rejected
by `validate.quantity_limit` before anything runs.

**2.** With `verify.comfortable` required, the status changes for 30.0 and
-5.0: both become `status.failed`, because neither is between 18 and 24. The
other rows do not change.

<!-- lcl: file=solutions/09/sensor_solution.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.sensor_solution
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

<!-- lcl-run: file=solutions/09/sensor_solution.lcl expect=run:failed args="--input input.temperature=30.0" -->
<!-- lcl-run: file=solutions/09/sensor_solution.lcl expect=run:failed args="--input input.temperature=-5.0" -->

**3.**

<!-- lcl: file=solutions/09/test_filter.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.test_filter
    NAME: "Test core.filter"
    VERSION: "1.0.0"
    KIND: kind.test

INPUT:
    ID: input.numbers
    TYPE: LIST[INTEGER]
    VALUE: [3, 8, 1, 9]

OUTPUT:
    ID: output.kept
    TYPE: LIST[INTEGER]
    FORMAT: format.json

ACTION:
    ID: action.keep_large
    OPERATION: core.filter
    TARGET: REF(input.numbers)
    PARAMETER:
        NAME: predicate
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "item > 2"
    OUTPUT: REF(output.kept)

TEST:
    ID: test.keep_large
    ACTION: REF(action.keep_large)
    EXPECTED: [3, 8, 9]
    ACTUAL: REF(output.kept)

EXECUTE:
    REFERENCE: REF(test.keep_large)
```

**4.**

<!-- lcl: expect=fragment -->
```lcl
SUCCESS:
    ID: success.comfortable
    NONE: [REF(verify.too_hot), REF(verify.too_cold)]
```

## Chapter 10

**1.** An IF inside the ELSE gives three branches, each with its own optional
output:

<!-- lcl: file=solutions/10/grade_three_ways.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.grade_three_ways
    NAME: "Three outcomes by score"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.score
    TYPE: INTEGER
    REQUIRED: FALSE
    DEFAULT: 72

OUTPUT:
    ID: output.distinction_message
    TYPE: STRING
    FORMAT: format.plain_text
    REQUIRED: FALSE

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
    ASSERT: EXISTS(REF(output.distinction_message)) OR EXISTS(REF(output.pass_message)) OR EXISTS(REF(output.fail_message))

ACTION:
    ID: action.distinction
    OPERATION: core.return
    TARGET: "distinction"
    OUTPUT: REF(output.distinction_message)

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
    IF (REF(input.score) >= 70) THEN:
        STEP:
            ID: step.distinction
            ACTION: REF(action.distinction)
    ELSE:
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
    ASSERT: EXISTS(REF(output.distinction_message)) OR EXISTS(REF(output.pass_message)) OR EXISTS(REF(output.fail_message))

SUCCESS:
    ID: success.message
    ALL: [REF(verify.one_message)]

TASK:
    ID: task.grade
    GOAL: REF(goal.message)
    INPUT: REF(input.score)
    SEQUENCE: REF(sequence.grade)
    OUTPUT: [REF(output.distinction_message), REF(output.pass_message), REF(output.fail_message)]
    SUCCESS: REF(success.message)

EXECUTE:
    REFERENCE: REF(task.grade)
```

<!-- lcl-run: file=solutions/10/grade_three_ways.lcl expect=run:succeeded args="--input input.score=60" -->
<!-- lcl-run: file=solutions/10/grade_three_ways.lcl expect=run:succeeded args="--input input.score=30" -->

**2.** Filter the failing marks, then sort that list, descending:

<!-- lcl: file=solutions/10/marks_failing.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.marks_failing
    NAME: "Failing marks, highest first"
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
    ID: output.failing
    TYPE: LIST[INTEGER]
    FORMAT: format.json

OUTPUT:
    ID: output.failing_highest_first
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

ACTION:
    ID: action.failing
    OPERATION: core.filter
    TARGET: REF(input.marks)
    PARAMETER:
        NAME: predicate
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "item < 50"
    OUTPUT: REF(output.failing)

ACTION:
    ID: action.failing_sorted
    OPERATION: core.sort
    TARGET: REF(output.failing)
    PARAMETER:
        NAME: direction
        TYPE: ENUM
        REQUIRED: TRUE
        VALUE: descending
    OUTPUT: REF(output.failing_highest_first)

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

VERIFY:
    ID: verify.failing
    ASSERT: REF(output.failing_highest_first) == [45, 45]

SUCCESS:
    ID: success.lists
    ALL: [REF(verify.sorted), REF(verify.passing), REF(verify.failing)]

TASK:
    ID: task.lists
    GOAL: REF(goal.lists)
    INPUT: REF(input.marks)
    ACTION: [REF(action.sort), REF(action.filter), REF(action.failing), REF(action.failing_sorted)]
    SEQUENCE: REF(sequence.each)
    OUTPUT: [REF(output.sorted), REF(output.passing), REF(output.failing), REF(output.failing_highest_first)]
    SUCCESS: REF(success.lists)

EXECUTE:
    REFERENCE: REF(task.lists)
```

**3.** Each loop iteration has its own copy of an output declared inside the
loop, and one OUTPUT can only have one producer, so there is nothing that
could "collect" values across iterations. Use whole-list operations instead.
`core.filter` needs a LIST, so first turn the SET into a list with
`core.sort`:

<!-- lcl: file=solutions/10/large_numbers.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.large_numbers
    NAME: "The numbers above 5, as one list"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.numbers
    TYPE: SET[INTEGER]
    VALUE: [4, 7, 10, 7]

OUTPUT:
    ID: output.as_list
    TYPE: LIST[INTEGER]
    FORMAT: format.json

OUTPUT:
    ID: output.large
    TYPE: LIST[INTEGER]
    FORMAT: format.json

GOAL:
    ID: goal.large
    ASSERT: REF(output.large) == [7, 10]

ACTION:
    ID: action.as_list
    OPERATION: core.sort
    TARGET: REF(input.numbers)
    OUTPUT: REF(output.as_list)

ACTION:
    ID: action.large
    OPERATION: core.filter
    TARGET: REF(output.as_list)
    PARAMETER:
        NAME: predicate
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "item > 5"
    OUTPUT: REF(output.large)

VERIFY:
    ID: verify.large
    ASSERT: REF(output.large) == [7, 10]

SUCCESS:
    ID: success.large
    ALL: [REF(verify.large)]

TASK:
    ID: task.large
    GOAL: REF(goal.large)
    INPUT: REF(input.numbers)
    ACTION: [REF(action.as_list), REF(action.large)]
    OUTPUT: [REF(output.as_list), REF(output.large)]
    SUCCESS: REF(success.large)

EXECUTE:
    REFERENCE: REF(task.large)
```

**4.** No. `step.leader` reads `output.table`, which `step.sort` writes. Steps
in a parallel group must be independent, and a step that reads another's
output is not. That is why `phase.report` exists and comes `AFTER`
`phase.arrange`.

## Chapter 11

**1.** Only `--allow-write` is needed, because the task writes but never reads:
`lcl run --allow-write /tmp/lcl-manual/diary diary.lcl`.

<!-- lcl: file=solutions/11/diary.lcl expect=run:succeeded workspace=/tmp/lcl-manual/diary grant=w produces=today.txt -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.diary
    NAME: "Write today's diary line"
    VERSION: "1.0.0"
    KIND: kind.task

WORKSPACE:
    ID: workspace.diary
    PATH: PATH("/tmp/lcl-manual/diary")
    MODE: mode.read_write

OUTPUT:
    ID: output.entry
    TYPE: PATH
    FORMAT: format.plain_text
    PROPERTY: target

GOAL:
    ID: goal.entry
    ASSERT: EXISTS(REF(output.entry))

ACTION:
    ID: action.write_entry
    OPERATION: core.create
    TARGET: PATH(REF(workspace.diary), "today.txt")
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "Today I wrote my first LCL file.\n"
    OUTPUT: REF(output.entry)

VERIFY:
    ID: verify.entry
    ASSERT: EXISTS(REF(output.entry))

SUCCESS:
    ID: success.entry
    ALL: [REF(verify.entry)]

TASK:
    ID: task.diary
    GOAL: REF(goal.entry)
    WORKSPACE: REF(workspace.diary)
    ACTION: REF(action.write_entry)
    OUTPUT: REF(output.entry)
    SUCCESS: REF(success.entry)

EXECUTE:
    REFERENCE: REF(task.diary)
```

**2.** With `core.write` and `create_if_missing: TRUE`, a second run no
longer fails: it replaces the existing backup. That is convenient, but it
means an older backup is silently lost. Choose `core.create` when an
existing file must never be replaced.

<!-- lcl: file=solutions/11/todo_write.lcl expect=run:succeeded workspace=/tmp/lcl-manual/todo seed=solutions/11/seed_existing grant=rw produces=todo_backup.txt -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.todo_write
    NAME: "Back up with core.write"
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
    OPERATION: core.write
    TARGET: REF(output.backup).TARGET
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: REF(output.todo_text)
    PARAMETER:
        NAME: create_if_missing
        TYPE: BOOLEAN
        REQUIRED: TRUE
        VALUE: TRUE
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

(The check above starts with an old `todo_backup.txt` already present, and
the run still succeeds.)

**3.** If `action.append` runs first, the read sees three lines, so the backup
contains `Learn LCL` as well. The order of the TASK's ACTION list is the order
of execution, and it changes the result.

**4.** A "current directory" is wherever the program happened to be started.
A document that depended on it would mean different things on different
machines or in different terminals. LCL paths are absolute, or relative to a
declared WORKSPACE, so the document alone says exactly which files are meant.

## Chapter 12

**1.** Yes, it still succeeds. A FORBID applies only to the operation it
names. The task uses `core.create`, not `core.write`, so the new rule does
not conflict with anything. It adds protection against a later edit that
overwrites the summary.

<!-- lcl: file=solutions/12/forbid_write.lcl expect=run:succeeded workspace=/tmp/lcl-manual/rules grant=w produces=summary.txt -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.override_forbid_write
    NAME: "An extra FORBID that does not apply"
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

FORBID:
    ID: rule.no_overwrite_summary
    OPERATION: core.write
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

**2.** With only the new FORBID, `lcl validate` stops the backup action:

<!-- lcl: file=solutions/12/backup_blocked.lcl expect=reject:error.permission.denied -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.backup_blocked
    NAME: "A second file, not yet permitted"
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

FORBID:
    ID: rule.no_backup_file
    OPERATION: core.create
    TARGET: PATH(REF(workspace.reports), "backup.txt")
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

OUTPUT:
    ID: output.backup_file
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

ACTION:
    ID: action.backup
    OPERATION: core.create
    TARGET: PATH(REF(workspace.reports), "backup.txt")
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: REF(output.summary_text)
    OUTPUT: REF(output.backup_file)

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
    ACTION: [REF(action.text), REF(action.save), REF(action.backup)]
    OUTPUT: [REF(output.summary_text), REF(output.summary_file), REF(output.backup_file)]
    SUCCESS: REF(success.summary)

EXECUTE:
    REFERENCE: REF(task.summary)
```

It needs its own ALLOW and its own OVERRIDE. Each exception is exact, so the
summary's OVERRIDE does not cover the backup:

<!-- lcl: file=solutions/12/backup_permitted.lcl expect=run:succeeded workspace=/tmp/lcl-manual/rules grant=w produces=backup.txt -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.backup_permitted
    NAME: "Two narrow exceptions"
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

FORBID:
    ID: rule.no_backup_file
    OPERATION: core.create
    TARGET: PATH(REF(workspace.reports), "backup.txt")
    AUTHORITY: 600

ALLOW:
    ID: permission.backup_file
    OPERATION: core.create
    TARGET: PATH(REF(workspace.reports), "backup.txt")
    AUTHORITY: 600

OVERRIDE:
    ID: override.backup_file
    WINNER: REF(permission.backup_file)
    LOSER: REF(rule.no_backup_file)

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

OUTPUT:
    ID: output.backup_file
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

ACTION:
    ID: action.backup
    OPERATION: core.create
    TARGET: PATH(REF(workspace.reports), "backup.txt")
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: REF(output.summary_text)
    OUTPUT: REF(output.backup_file)

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
    ACTION: [REF(action.text), REF(action.save), REF(action.backup)]
    OUTPUT: [REF(output.summary_text), REF(output.summary_file), REF(output.backup_file)]
    SUCCESS: REF(success.summary)

EXECUTE:
    REFERENCE: REF(task.summary)
```

**3.**

<!-- lcl: file=solutions/12/archive_rules.lcl expect=validate -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.archive_rules
    NAME: "Archive rules"
    VERSION: "1.0.0"
    KIND: kind.library

FORBID:
    ID: rule.never_delete_archive
    OPERATION: core.delete
    TARGET: PATH("/srv/archive")

FORBID:
    ID: rule.never_move_archive
    OPERATION: core.move
    TARGET: PATH("/srv/archive")

ALLOW:
    ID: permission.incoming
    OPERATION: core.create
    TARGET: PATH("/srv/archive/incoming")

PRESERVE:
    ID: rule.keep_index
    TARGET: PATH("/srv/archive/index.csv")
```

"Prefer file names without spaces" cannot be written as a rule on its own,
because a PREFER needs something to assert about. Once a task has an output
holding the new file's name, it becomes:

<!-- lcl: expect=fragment -->
```lcl
PREFER:
    ID: preference.no_spaces
    ASSERT: NOT (REF(output.file_name) CONTAINS " ")
```

**4.** An ALLOW at higher authority is a deliberate decision by a stronger
source to permit something, so letting it win is reasonable. If an ALLOW
won at *equal* authority, any document could cancel any rule of the same
standing just by adding an ALLOW, and "must not" would stop meaning anything.
Requiring an explicit OVERRIDE makes every such exception visible, exact and
reviewable.

## Chapter 13

**1.** `FALSE AND UNKNOWN` is FALSE. `NOT (TRUE OR UNKNOWN)` is FALSE.
`ANY([UNKNOWN, FALSE])` is UNKNOWN. `EXISTS(NULL)` is TRUE. `MISSING + 1` is
an error. Written literally, it is rejected by `lcl check`
(`error.type.mismatch`: `+` does not admit MISSING). A value that only turns
out to be MISSING while running gives `error.required.missing`, as in
`missing_rate.lcl`.

<!-- lcl: file=solutions/13/answers.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.chapter13_answers
    NAME: "Chapter 13 answers, checked"
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
    ID: verify.false_and_unknown
    ASSERT: (FALSE AND UNKNOWN) == FALSE

VERIFY:
    ID: verify.not_true_or_unknown
    ASSERT: (NOT (TRUE OR UNKNOWN)) == FALSE

VERIFY:
    ID: verify.any_unknown_false
    ASSERT: ANY([UNKNOWN, FALSE]) == UNKNOWN

VERIFY:
    ID: verify.exists_null
    ASSERT: EXISTS(REF(data.middle_name)) == TRUE

SUCCESS:
    ID: success.all_true
    ALL: [REF(verify.false_and_unknown), REF(verify.not_true_or_unknown), REF(verify.any_unknown_false), REF(verify.exists_null)]

TASK:
    ID: task.special_values
    GOAL: REF(goal.all_true)
    ACTION: REF(action.done)
    OUTPUT: [REF(output.done), REF(output.never)]
    SUCCESS: REF(success.all_true)

EXECUTE:
    REFERENCE: REF(task.special_values)
```

**2.** Change `DEFAULT: MISSING` to `DEFAULT: 10`. The run then succeeds and
prints `OUTPUT output.rate_doubled published = 20`.

<!-- lcl: file=solutions/13/default_rate.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.default_rate
    NAME: "A missing rate treated as 10"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.rate
    TYPE: INTEGER
    REQUIRED: FALSE
    DEFAULT: 10

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

**3.** The first attempt fails as before. Instead of retrying, the handler's
`core.retry` refuses, because its limit (2) does not equal the RETRY's
(4): `error.operation.precondition: core.retry resolved limit 2 does not
equal the declared RETRY LIMIT 4`. The RETRY block is the only budget, and a
handler cannot silently change it.

<!-- lcl: file=solutions/13/retry_mismatch.lcl expect=run:blocked primary=error.host.constraint workspace=/tmp/lcl-manual/retry seed=examples/13/seed -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.retry_mismatch
    NAME: "A retry budget that does not match"
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
        LIMIT: 4
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

**4.** A timeout is often temporary: the same request may succeed a moment
later, so a bounded RETRY with a DELAY makes sense. A wrong address fails
identically every time. Retrying only wastes attempts, and the real fix is to
correct the document.

## Chapter 14

**1.**

<!-- lcl: file=solutions/14/geometry/src/shapes.lcl expect=validate -->
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

DEFINE:
    ID: constant.sides_of_triangle
    KIND: kind.constant
    TYPE: INTEGER
    VALUE: 3
```

<!-- lcl: file=solutions/14/geometry/src/main.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: app.perimeters
    NAME: "Perimeters of a square and a triangle"
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

INPUT:
    ID: input.triangle_side
    TYPE: INTEGER
    VALUE: 7

OUTPUT:
    ID: output.triangle_perimeter
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

ACTION:
    ID: action.triangle_perimeter
    OPERATION: core.calculate
    TARGET: REF(input.triangle_side)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "target * REF(shapes.constant.sides_of_triangle)"
    OUTPUT: REF(output.triangle_perimeter)

VERIFY:
    ID: verify.triangle_perimeter
    ASSERT: REF(output.triangle_perimeter) == 21

VERIFY:
    ID: verify.perimeter
    ASSERT: REF(output.perimeter) == 20

SUCCESS:
    ID: success.perimeter
    ALL: [REF(verify.perimeter), REF(verify.triangle_perimeter)]

TASK:
    ID: task.perimeter
    GOAL: REF(goal.perimeter)
    INPUT: [REF(input.side), REF(input.triangle_side)]
    ACTION: [REF(action.perimeter), REF(action.triangle_perimeter)]
    OUTPUT: [REF(output.perimeter), REF(output.triangle_perimeter)]
    SUCCESS: REF(success.perimeter)

EXECUTE:
    REFERENCE: REF(task.perimeter)
```

**2.** Importing the same library twice under **different** namespaces is
allowed. Its declarations then exist under both prefixes:

<!-- lcl: file=solutions/14/geometry/src/import_twice.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: app.import_twice
    NAME: "Perimeter of a square"
    VERSION: "1.0.0"
    KIND: kind.task

IMPORT:
    ID: import.shapes
    SOURCE: PATH("shapes.lcl")
    NAMESPACE: shapes
    VERSION: "1.0.0"

IMPORT:
    ID: import.more_shapes
    SOURCE: PATH("shapes.lcl")
    NAMESPACE: more_shapes
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

Under the **same** namespace it is rejected, because one prefix can belong to
only one import:

<!-- lcl: file=solutions/14/geometry/src/same_namespace.invalid.lcl expect=reject:error.id.duplicate -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: app.same_namespace
    NAME: "Perimeter of a square"
    VERSION: "1.0.0"
    KIND: kind.task

IMPORT:
    ID: import.shapes
    SOURCE: PATH("shapes.lcl")
    NAMESPACE: shapes
    VERSION: "1.0.0"

IMPORT:
    ID: import.shapes_again
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

**3.** `error.version.mismatch`: the library says it is version 1.0.0, and the
import asked for 1.0.1. This protects you from silently running against a
different version of a library than the one you wrote and tested against.

<!-- lcl: file=solutions/14/geometry/src/wrong_version.invalid.lcl expect=reject:error.version.mismatch -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: app.wrong_version
    NAME: "Perimeter of a square"
    VERSION: "1.0.0"
    KIND: kind.task

IMPORT:
    ID: import.shapes
    SOURCE: PATH("shapes.lcl")
    NAMESPACE: shapes
    VERSION: "1.0.1"

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

**4.** `lcl run --locked` refuses with exit code 4, because `main.lcl`'s
fingerprint no longer matches `lcl.lock`. After checking the change is
intended, run `lcl package lock src/main.lcl` again to record the new
fingerprint. `--locked` then accepts it.

## Chapter 15

**1.** For example:

* **lexical:** a lowercase `type:` (`error.keyword.case`), or an uppercase
  word LCL does not have, such as `COLOUR:` (`error.keyword.unknown`);
* **grammar:** a real keyword in a block that does not allow it, such as
  `NAME:` inside an INPUT (`error.field.forbidden`);
* **resolution:** a misspelled REF (`error.reference.unresolved`);
* **static:** a STRING value for an INTEGER (`error.type.mismatch`).

**2.** Only `08_HARD_CONFLICT.invalid.lcl` passes `lcl check`. Its two REQUIREs
are well formed. The contradiction between them is found only at the rules
stage, which `lcl validate` reaches.

**3.** `core.sort` has exactly two parameters, `key` and `direction`. The fix
is to rename `order` to `direction`, written as `TYPE: ENUM` with the value
`ascending` or `descending` (Chapter 10).

**4.** `lcl` exits with code 1 for a rejected document, so the shell can test
the exit code directly:

```
for f in *.lcl; do
    lcl check --machine "$f" > /dev/null || echo "rejected: $f"
done
```
