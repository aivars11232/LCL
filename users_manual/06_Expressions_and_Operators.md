# Chapter 6. Expressions and operators

**In this chapter you will learn**

* how to write calculations, comparisons and logical conditions;
* why LCL division is exact, and how to round;
* the built-in functions;
* how to read parts of lists and objects;
* how to test text against patterns.

An **expression** is a formula that produces a value: `2 + 3`,
`REF(input.age) >= 18`, `COUNT(REF(input.names))`. Expressions appear in
`VALUE` fields, in `ASSERT` fields of checks, in conditions, and inside the
`expression` parameter of `core.calculate`. Every expression is free of side
effects: evaluating it never changes anything.

The full rules are in `05_SEMANTICS/12_OPERATOR_FUNCTION_AND_SPECIAL_VALUE_SEMANTICS.txt`
and `03_TYPES_AND_VALUES/06_NUMERIC_ARITHMETIC_COMPARISON_AND_ROUNDING.txt`.

## 6.1 The operators

| Operator | Meaning | Works on |
|---|---|---|
| `+` `-` `*` | add, subtract, multiply | numbers; `+`/`-` also on durations and on measures with the same unit |
| `/` | exact division | numbers, and measures |
| `-x` | negation | numbers |
| `==` `!=` | equal, not equal | any two values |
| `<` `<=` `>` `>=` | ordering | numbers, strings, dates, times, durations, same-unit measures |
| `AND` `OR` `NOT` | logic | `TRUE`/`FALSE` only |
| `IN` | "is a member of" | value `IN` list or set |
| `CONTAINS` | "has as a member / substring / key" | list, set, string or object on the left |
| `MATCHES` | "the whole text fits the pattern" | string `MATCHES` REGEX, path or string `MATCHES` GLOB |

There is no string concatenation with `+`: `"a" + "b"` is rejected. There is
no power operator and no remainder operator in Core 0.1.0.

## 6.2 Precedence

When an expression has several operators, they are applied in this order,
from first to last:

1. unary minus (`-x`);
2. `*` and `/`;
3. `+` and `-`;
4. comparisons (`==`, `<`, `IN`, `MATCHES`, ...);
5. `AND`;
6. `OR`.

Operators of equal rank are applied left to right, so `10 - 3 - 2` is `5`.
Parentheses override everything: `(2 + 3) * 4` is `20`.

**`NOT` applies only to the single thing immediately after it.** So
`NOT 1 == 2` means `(NOT 1) == 2`, which is an error because `NOT` needs a
Boolean. Always put parentheses after `NOT`: `NOT (1 == 2)`.

## 6.3 Exact division

Most languages divide approximately: `1 / 3` becomes `0.3333333333333333`,
which is wrong in the last digit. LCL never approximates.

* **Every division gives a DECIMAL**, even `4 / 2`, which is `2.0`. You
  cannot store `7 / 2` in an INTEGER.
* If the answer has a finite number of decimal places, you get it exactly:
  `7 / 2` is `3.5` and `1 / 8` is `0.125`.
* If it would go on forever, like `1 / 3`, the expression is **rejected**
  (`error.numeric.non_terminating`). LCL will not pick a number of digits for
  you.
* To get a rounded answer, put the division **directly** inside `ROUND`:
  `ROUND(1 / 3, 2)` is `0.33`. The second argument is the number of decimal
  places.
* `ROUND` uses "round half to even" (banker's rounding): `ROUND(2.5, 0)` is
  `2.0` and `ROUND(3.5, 0)` is `4.0`. This avoids a bias upwards when many
  values are rounded.
* Dividing by zero is always rejected (`error.numeric.division_by_zero`).

Because of these rules, a document either gives the exact answer or tells
you plainly why it cannot. It never gives a slightly wrong one.

## 6.4 Logic: AND, OR, NOT

`AND` and `OR` accept only `TRUE` and `FALSE`: `TRUE AND 1` is an error. When
the left side already decides the answer (`FALSE AND ...` or `TRUE OR ...`),
the right side is not evaluated. It is still **checked**, though: an
expression such as `TRUE OR 1 / 0 == 1` is rejected, because the tool checks
every part of every expression before running anything.

## 6.5 Built-in functions

| Function | Returns | Example | Result |
|---|---|---|---|
| `ABS(x)` | absolute value | `ABS(-2.5)` | `2.5` |
| `COUNT(x)` | number of members, fields or characters | `COUNT("héllo")` | `5` |
| `SUM(list)` | total of a non-empty list | `SUM([4, 2, 9])` | `15` |
| `MIN(list)`, `MAX(list)` | smallest, largest | `MAX([4, 2, 9])` | `9` |
| `ROUND(x, n)` | x rounded to n places (half to even) | `ROUND(2.675, 2)` | `2.68` |
| `EMPTY(x)` | TRUE if it has nothing in it | `EMPTY("")` | `TRUE` |
| `EXISTS(x)` | FALSE only if x is MISSING | `EXISTS(REF(output.total))` | depends |
| `ALL([...])` | TRUE if every member is TRUE | `ALL([TRUE, TRUE])` | `TRUE` |
| `ANY([...])` | TRUE if at least one member is TRUE | `ANY([FALSE, TRUE])` | `TRUE` |
| `NONE([...])` | TRUE if no member is TRUE | `NONE([FALSE, FALSE])` | `TRUE` |

Arguments are positional, in a fixed order. `ALL([])` and `NONE([])` are
TRUE, and `ANY([])` is FALSE. `COUNT` counts characters, not bytes, so the
accented `é` counts as one. `SUM`, `MIN` and `MAX` refuse an empty list,
because there is nothing sensible to return.

## 6.6 Reading values: REF, properties and indexes

`REF(id)` reads the value of a declaration. From there:

* `.name` (lowercase) reads a field of an object: `REF(data.pupil).name`.
* `[n]` reads member number `n` of a list, **counting from 0**:
  `REF(data.marks)[0]` is the first member.
* An index past the end gives `MISSING` instead of crashing, and so does a
  negative index. Negative indexes never count from the end.
* Sets and strings cannot be indexed, because a set has no order.

> **Careful:** a bare dotted name such as `target.marks` is read as **one
> identifier**, not as "the `marks` field of `target`". To read a field,
> start from a `REF(...)` (or another complete value) and add the field after
> it: `REF(data.pupil).marks`.

## 6.7 Patterns: REGEX and GLOB

`MATCHES` tests whether **the whole** text fits a pattern. There is no partial
match. `"Ada1" MATCHES REGEX("[A-Z][a-z]+")` is FALSE, because the `1` is
left over.

A **REGEX** is a text pattern:

| Pattern | Matches |
|---|---|
| `a` | the letter a |
| `.` | any one character (except a line break) |
| `[a-z]` | one character from the range |
| `[^0-9]` | one character that is not a digit |
| `\d`, `\w`, `\s` | a digit, a word character, a space character |
| `x*`, `x+`, `x?` | zero or more, one or more, zero or one x |
| `x{3}`, `x{2,5}` | exactly 3, between 2 and 5 x |
| `a\|b` | a or b |
| `(...)` | grouping |

The optional second argument holds flags: `i` ignores upper/lower case, `m`
lets `^` and `$` match at line breaks, and `s` lets `.` match line breaks.
Flags must be written in that order: `"is"` is fine, but `"si"` is rejected.
Advanced features from other languages (look-ahead, back-references, named
groups) do not exist, and using them is an error.

A **GLOB** is a file-name pattern, used on workspace-relative paths:

| Pattern | Matches |
|---|---|
| `*` | any characters within one folder name |
| `?` | exactly one character |
| `**` | any number of whole folders, including none |
| `[abc]` | one of the listed characters |

So `src/**/*.py` matches `src/main.py` and `src/a/b/main.py`, and `src/*`
matches `src/notes.txt` but not `src/a/notes.txt`.

## 6.8 All of it, checked

This program states twenty facts about expressions as VERIFY checks. The run
succeeds only if every one is TRUE, and it does. Read each assertion and
convince yourself why it holds.

<!-- lcl: file=examples/06/expressions.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.expressions
    NAME: "Expressions, checked"
    VERSION: "1.0.0"
    KIND: kind.task

DATA:
    ID: data.pupil
    TYPE: OBJECT
    VALUE:
        name: "Ada"
        year: 7

DATA:
    ID: data.marks
    TYPE: LIST[INTEGER]
    VALUE: [7, 9, 10]

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
    ID: verify.precedence
    ASSERT: 2 + 3 * 4 == 14 AND (2 + 3) * 4 == 20

VERIFY:
    ID: verify.left_to_right
    ASSERT: 10 - 3 - 2 == 5

VERIFY:
    ID: verify.unary_minus
    ASSERT: -3 + 5 == 2

VERIFY:
    ID: verify.exact_division
    ASSERT: 7 / 2 == 3.5 AND 1 / 8 == 0.125

VERIFY:
    ID: verify.rounding
    ASSERT: ROUND(1 / 3, 2) == 0.33 AND ROUND(2.5, 0) == 2.0 AND ROUND(3.5, 0) == 4.0

VERIFY:
    ID: verify.comparison
    ASSERT: 3 < 5 AND 5 <= 5 AND "apple" < "banana" AND DATE("2026-01-02") > DATE("2026-01-01")

VERIFY:
    ID: verify.not_needs_parentheses
    ASSERT: NOT (1 == 2)

VERIFY:
    ID: verify.logic
    ASSERT: (TRUE OR FALSE) AND NOT (TRUE AND FALSE)

VERIFY:
    ID: verify.functions
    ASSERT: ABS(-2.5) == 2.5 AND MIN([4, 2, 9]) == 2 AND MAX([4, 2, 9]) == 9 AND SUM([4, 2, 9]) == 15

VERIFY:
    ID: verify.counting
    ASSERT: COUNT([4, 2, 9]) == 3 AND COUNT("héllo") == 5 AND EMPTY("") AND NOT EMPTY([1])

VERIFY:
    ID: verify.quantifiers
    ASSERT: ALL([TRUE, TRUE]) AND ANY([FALSE, TRUE]) AND NONE([FALSE, FALSE])

VERIFY:
    ID: verify.membership
    ASSERT: "b" IN ["a", "b"] AND ["a", "b"] CONTAINS "a" AND "timetable" CONTAINS "table"

VERIFY:
    ID: verify.object_access
    ASSERT: REF(data.pupil).name == "Ada" AND REF(data.pupil) CONTAINS "year"

VERIFY:
    ID: verify.list_index
    ASSERT: REF(data.marks)[0] == 7 AND REF(data.marks)[2] == 10

VERIFY:
    ID: verify.index_out_of_range
    ASSERT: REF(data.marks)[3] == MISSING AND REF(data.marks)[-1] == MISSING

VERIFY:
    ID: verify.regex_whole_string
    ASSERT: "Ada" MATCHES REGEX("[A-Z][a-z]+") AND NOT ("Ada1" MATCHES REGEX("[A-Z][a-z]+"))

VERIFY:
    ID: verify.regex_ignore_case
    ASSERT: "HELLO" MATCHES REGEX("hello", "i")

VERIFY:
    ID: verify.glob
    ASSERT: "src/a/b/main.py" MATCHES GLOB("src/**/*.py") AND NOT ("src/a/notes.txt" MATCHES GLOB("src/*"))

VERIFY:
    ID: verify.durations
    ASSERT: DURATION(1, unit.hour) + DURATION(30, unit.minute) == DURATION(90, unit.minute)

VERIFY:
    ID: verify.measures
    ASSERT: MEASURE(2, unit.meter) + MEASURE(3, unit.meter) == MEASURE(5, unit.meter)

SUCCESS:
    ID: success.all_true
    ALL: [REF(verify.precedence), REF(verify.left_to_right), REF(verify.unary_minus), REF(verify.exact_division), REF(verify.rounding), REF(verify.comparison), REF(verify.not_needs_parentheses), REF(verify.logic), REF(verify.functions), REF(verify.counting), REF(verify.quantifiers), REF(verify.membership), REF(verify.object_access), REF(verify.list_index), REF(verify.index_out_of_range), REF(verify.regex_whole_string), REF(verify.regex_ignore_case), REF(verify.glob), REF(verify.durations), REF(verify.measures)]

TASK:
    ID: task.expressions
    GOAL: REF(goal.all_true)
    ACTION: REF(action.done)
    OUTPUT: REF(output.done)
    SUCCESS: REF(success.all_true)

EXECUTE:
    REFERENCE: REF(task.expressions)
```

## 6.9 Expressions inside `core.calculate`

In Chapter 3 you saw `core.calculate`, whose `expression` parameter is an
expression written **inside a string**:

<!-- lcl: expect=fragment -->
```lcl
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "REF(input.price) * REF(input.quantity)"
```

The string holds exactly one expression, following all the rules in this
chapter. Inside it, the bare name `target` means the action's TARGET value,
so `"target * 2"` doubles whatever the TARGET is. Only a bare `target`
works. To read a field of the target, use `REF(...)` on the declaration
itself.

## Summary

* Operators follow a fixed precedence. Use parentheses whenever it helps, and
  always after `NOT`.
* Division is exact and always gives a DECIMAL. Endless decimals are
  rejected unless the division is the first argument of `ROUND`, which rounds
  half to even.
* `AND`, `OR` and `NOT` work only on `TRUE` and `FALSE`.
* List indexes start at 0, and out-of-range indexes give `MISSING`.
* `MATCHES` tests the whole text against a REGEX or GLOB pattern.

## Exercises

1. Work out the value of each expression by hand, then check your answers by
   adding them as VERIFY checks to a copy of `expressions.lcl`:
   `20 - 4 * 3`, `(20 - 4) * 3`, `9 / 4`, `ROUND(2 / 3, 3)`,
   `COUNT([[1, 2], [3]])`.
2. Which of these are rejected, and why? `7 / 0`, `2 / 3`, `ROUND(2 / 3, 1)`,
   `NOT 5 > 3`, `"total: " + "5"`.
3. Write a REGEX that matches a UK-style postcode district such as `SW1A` or
   `M1`: one or two capital letters, one digit, then optionally one more
   letter or digit. Test it with `MATCHES` in VERIFY checks, including one
   string that must *not* match.
4. A class has marks `[12, 15, 9, 18]`. Write one expression for the average
   mark rounded to one decimal place.

Solutions: [solutions/README.md](solutions/README.md#chapter-6).
