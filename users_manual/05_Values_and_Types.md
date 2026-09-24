# Chapter 5. Values and types

**In this chapter you will learn**

* the built-in types and how to write a value of each;
* the typed constructors for paths, dates, times, durations, measures and
  more;
* lists, sets and objects, and how to describe an object's shape with a schema;
* how to define your own types and enumerations;
* why LCL never converts one type into another behind your back.

The rules are in the specification's `03_TYPES_AND_VALUES/` folder.

## 5.1 Every value has exactly one type

In LCL every value has exactly one type, and the type is checked before
anything runs. There is **no automatic conversion**, with one small exception
covered in 5.3. A string is never quietly turned into a number, and a number
is never treated as true or false.

This is stricter than you may be used to. For example, `5` is an INTEGER, so
it cannot be stored where a DECIMAL is declared. Write `5.0` instead:

| You declare | You write | Result |
|---|---|---|
| `TYPE: DECIMAL` | `VALUE: 5` | rejected: `this value is INTEGER, not the declared type DECIMAL` |
| `TYPE: DECIMAL` | `VALUE: 5.0` | accepted |
| `TYPE: INTEGER` | `VALUE: 5.0` | rejected: DECIMAL is not INTEGER |
| `TYPE: BOOLEAN` | `VALUE: 1` | rejected: numbers are not true or false |

## 5.2 The scalar types

| Type | Holds | Example value |
|---|---|---|
| `STRING` | Unicode text | `"Ada Lovelace"` |
| `INTEGER` | whole numbers of any size | `42`, `-7` |
| `DECIMAL` | exact decimal numbers | `3.14`, `0.1` |
| `BOOLEAN` | `TRUE` or `FALSE` | `TRUE` |
| `NULL` | "known to be empty" | `NULL` |

INTEGER has no maximum and never overflows. DECIMAL is **exact**: `0.1` is
exactly one tenth, not the nearest binary fraction as in many languages, so
`0.1 + 0.2 == 0.3` is TRUE in LCL.

## 5.3 Numbers mixing

The one automatic conversion is that an INTEGER combined with a DECIMAL in
arithmetic or comparison is treated as a DECIMAL. So `2 + 0.5` is `2.5`, and
`5 == 5.0` is TRUE. Nothing else converts.

## 5.4 Typed constructors

Many kinds of value are written with a **constructor**: an uppercase name
with arguments in parentheses. The constructor checks the value, so a
malformed date or a relative web address is caught by `lcl check`.

| Constructor | Makes | Example |
|---|---|---|
| `PATH("/abs/path")` | an absolute file path | `PATH("/home/ada/notes.txt")` |
| `PATH(REF(workspace.x), "rel/path")` | a path inside a workspace (Chapter 11) | `PATH(REF(workspace.project), "src/main.py")` |
| `URI("scheme:...")` | an absolute web or other address | `URI("https://example.org/data")` |
| `DATE("YYYY-MM-DD")` | a calendar date | `DATE("2026-09-24")` |
| `TIME("HH:MM:SS")` | a clock time, UTC unless an offset is given | `TIME("09:15:00+02:00")` |
| `DATETIME("...T...")` | a date and time | `DATETIME("2026-09-24T09:15:00Z")` |
| `DURATION(n, unit)` | a length of time (never negative) | `DURATION(90, unit.second)` |
| `PERCENTAGE(n)` | a percentage from 0 to 100 | `PERCENTAGE(12.5)` |
| `BYTES(n)` | a count of bytes | `BYTES(1024)` |
| `MEASURE(n, unit)` | a number with a unit | `MEASURE(2.5, unit.kilometer)` |
| `GLOB("pattern")` | a file-name pattern | `GLOB("src/**/*.py")` |
| `REGEX("pattern")` | a text pattern | `REGEX("[A-Z][a-z]+")` |

Arguments are always positional (in a fixed order) and never named. The
checks are real:

* `DATE("2026-02-30")` is rejected: February 2026 has no 30th.
* `TIME("24:00:00")` is rejected: hours run from 00 to 23.
* `PERCENTAGE(150)` is rejected: the maximum is 100.
* `DURATION(5, unit.meter)` is rejected: a duration needs a time unit.
* `URI("/relative")` is rejected: a URI needs a scheme such as `https:`.
* `PATH("relative/file.txt")` is rejected: outside imports, a single-string
  path must be absolute.

### Units

`MEASURE` and `DURATION` take a unit from a fixed list. You cannot invent
units.

| Category | Units |
|---|---|
| Time | `unit.nanosecond`, `unit.microsecond`, `unit.millisecond`, `unit.second`, `unit.minute`, `unit.hour`, `unit.day`, `unit.week` |
| Storage | `unit.byte`, `unit.kibibyte`, `unit.mebibyte`, `unit.gibibyte` |
| Length | `unit.pixel`, `unit.point`, `unit.millimeter`, `unit.centimeter`, `unit.meter`, `unit.kilometer` |
| Angle | `unit.degree`, `unit.radian` |
| Frequency | `unit.hertz`, `unit.kilohertz`, `unit.megahertz` |
| Media | `unit.frame`, `unit.frame_per_second`, `unit.bit_per_second` |

Measures never convert units. `MEASURE(1000, unit.meter)` and
`MEASURE(1, unit.kilometer)` are **not** equal, because they have different
units. Durations are different: every duration is measured in exact
nanoseconds, so `DURATION(90, unit.second) == DURATION(1.5, unit.minute)` is
TRUE.

## 5.5 Collections: LIST and SET

A **LIST** is an ordered sequence of values of one type. It may contain
duplicates. Its type is written `LIST[member type]`:

<!-- lcl: expect=fragment -->
```lcl
    TYPE: LIST[INTEGER]
    VALUE: [3, 1, 3]
```

A **SET** is written the same way, but its type is `SET[member type]`. A set
has no order, and equal members collapse into one. `[3, 1, 3]` as a
`SET[INTEGER]` has two members.

All members must have the same type. `[1, 2.5]` is rejected even though both
are numbers. Write `[1.0, 2.5]`. An empty list `[]` is allowed only where its
type is known, for example as the VALUE of a `LIST[INTEGER]`.

Lists can be nested: `LIST[LIST[INTEGER]]` holds values such as
`[[1, 2], [3]]`.

## 5.6 Objects

An **OBJECT** is a group of named fields, written as an indented block with
lowercase field names:

<!-- lcl: expect=fragment -->
```lcl
DATA:
    ID: data.pupil
    TYPE: OBJECT
    VALUE:
        name: "Ada"
        year: 7
```

Plain `TYPE: OBJECT` accepts any fields. Usually you want to say exactly
which fields an object must have. For that, define an object type with a
**schema**, which is covered next.

## 5.7 Defining your own types

`DEFINE` creates a named, unchangeable definition. With `KIND: kind.type` it
defines a type. You refer to your type as `REF(type.id)`, never by its bare
name.

**An object type** lists its fields with `FIELD` blocks. Each field has a
`NAME`, a `TYPE` and `REQUIRED`, and may add bounds such as `MINIMUM` and
`MAXIMUM`:

<!-- lcl: file=examples/05/pupils.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.pupils
    NAME: "A class register"
    VERSION: "1.0.0"
    KIND: kind.data

DEFINE:
    ID: type.pupil
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: name
        TYPE: STRING
        REQUIRED: TRUE
    FIELD:
        NAME: year
        TYPE: INTEGER
        REQUIRED: TRUE
        MINIMUM: 1
        MAXIMUM: 13
    FIELD:
        NAME: nickname
        TYPE: STRING
        REQUIRED: FALSE

DEFINE:
    ID: type.weekday
    KIND: kind.type
    BASE: ENUM
    ITEM: monday
    ITEM: tuesday
    ITEM: wednesday
    ITEM: thursday
    ITEM: friday

DEFINE:
    ID: type.mark
    KIND: kind.type
    BASE: INTEGER

DATA:
    ID: data.ada
    TYPE: OBJECT[REF(type.pupil)]
    VALUE:
        name: "Ada"
        year: 7

DATA:
    ID: data.grace
    TYPE: OBJECT[REF(type.pupil)]
    VALUE:
        name: "Grace"
        year: 8
        nickname: "Amazing Grace"

DATA:
    ID: data.register
    TYPE: LIST[OBJECT[REF(type.pupil)]]
    VALUE: [REF(data.ada), REF(data.grace)]

DATA:
    ID: data.club_day
    TYPE: REF(type.weekday)
    VALUE: wednesday

DATA:
    ID: data.best_mark
    TYPE: REF(type.mark)
    VALUE: 10
```

Once the schema exists, the tool checks every object against it:

| Mistake | Error |
|---|---|
| a required field left out | `error.object.schema`: omits the required schema field `year` |
| a field the schema does not declare | `error.object.schema`: `age` is not declared by this object schema |
| a field of the wrong type | `error.type.mismatch` |
| a value outside `MINIMUM`/`MAXIMUM` | `error.value.out_of_range` |

Schemas are **closed**: an object may contain only the fields its schema
declares.

**An enumeration** (`BASE: ENUM`) is a type whose values are a fixed list of
names, given by `ITEM` fields. A value of type `REF(type.weekday)` must be
one of those names, written as a bare lowercase word: `wednesday`. `sunday`
is rejected, because it is not in the list.

**An alias** (`BASE: INTEGER` in `type.mark`) gives an existing type a new
name. It adds no new rules: a `REF(type.mark)` value is simply an INTEGER.

## 5.8 Type expressions at a glance

Wherever a `TYPE` is required, you may write:

| Form | Meaning |
|---|---|
| `INTEGER`, `STRING`, `PATH`, ... | a built-in type |
| `REF(type.id)` | a type you defined |
| `LIST[T]`, `SET[T]` | a list or set of T |
| `OBJECT[REF(type.id)]` | an object following that schema |
| `REFERENCE[REF(id)]` | a reference to a declaration (advanced; see Chapter 7) |

A bare identifier is never a type. `TYPE: type.weekday` is an error; write
`TYPE: REF(type.weekday)`.

## 5.9 Equality

`==` compares two values exactly:

* different types are never equal, except INTEGER against DECIMAL;
* lists are equal when they have the same members in the same order;
* sets are equal when they have the same members, in any order;
* objects are equal when they have the same fields with equal values;
* measures are equal only with the same unit and the same number;
* strings are equal only when every character is the same (`"Ada"` is not
  equal to `"ada"`).

This program checks those rules on the real engine. Every VERIFY must be TRUE
for the run to succeed:

<!-- lcl: file=examples/05/equality.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.equality
    NAME: "How equality works"
    VERSION: "1.0.0"
    KIND: kind.task

DATA:
    ID: data.numbers
    TYPE: SET[INTEGER]
    VALUE: [3, 1, 3]

OUTPUT:
    ID: output.done
    TYPE: BOOLEAN
    FORMAT: format.plain_text

GOAL:
    ID: goal.rules_hold
    ASSERT: REF(output.done) == TRUE

ACTION:
    ID: action.done
    OPERATION: core.return
    TARGET: TRUE
    OUTPUT: REF(output.done)

VERIFY:
    ID: verify.set_collapses_duplicates
    ASSERT: COUNT(REF(data.numbers)) == 2

VERIFY:
    ID: verify.integer_meets_decimal
    ASSERT: 5 == 5.0 AND 0.1 + 0.2 == 0.3

VERIFY:
    ID: verify.list_order_matters
    ASSERT: [1, 2] != [2, 1]

VERIFY:
    ID: verify.case_matters
    ASSERT: "Ada" != "ada"

VERIFY:
    ID: verify.units_never_convert
    ASSERT: MEASURE(1000, unit.meter) != MEASURE(1, unit.kilometer)

VERIFY:
    ID: verify.durations_normalise
    ASSERT: DURATION(90, unit.second) == DURATION(1.5, unit.minute)

VERIFY:
    ID: verify.types_never_convert
    ASSERT: "5" != 5

SUCCESS:
    ID: success.rules_hold
    ALL: [REF(verify.set_collapses_duplicates), REF(verify.integer_meets_decimal), REF(verify.list_order_matters), REF(verify.case_matters), REF(verify.units_never_convert), REF(verify.durations_normalise), REF(verify.types_never_convert)]

TASK:
    ID: task.equality
    GOAL: REF(goal.rules_hold)
    ACTION: REF(action.done)
    OUTPUT: REF(output.done)
    SUCCESS: REF(success.rules_hold)

EXECUTE:
    REFERENCE: REF(task.equality)
```

The one action only returns `TRUE`, because a task must do at least one
thing. All the interest is in the VERIFY checks.

## 5.10 NULL

`NULL` means "known to be empty": for example, a middle name that the person
does not have. A NULL can be stored only where the type is `NULL` itself, so
`TYPE: INTEGER` with `VALUE: NULL` is rejected. Do not use NULL to mean
"unknown" or "not given yet". LCL has separate words for those, covered in
[Chapter 13](13_Missing_Values_Errors_and_Recovery.md).

## Summary

* Every value has one type, checked before running, and nothing converts
  automatically except INTEGER to DECIMAL in arithmetic and comparison.
* Constructors such as `DATE(...)`, `DURATION(...)` and `MEASURE(...)` check
  their values, and units come from a fixed list.
* A LIST is ordered and allows duplicates. A SET is unordered with no
  duplicates. All members share one type.
* `DEFINE` with `KIND: kind.type` creates object types (closed schemas),
  enumerations and aliases, used as `REF(type.id)`.

## Exercises

1. Which of these are accepted? Check your answers with `lcl check`.
   * `TYPE: DECIMAL` with `VALUE: 10`
   * `TYPE: LIST[DECIMAL]` with `VALUE: [1.5, 2]`
   * `TYPE: DATE` with `VALUE: DATE("2028-02-29")`
   * `TYPE: PERCENTAGE` with `VALUE: PERCENTAGE(100)`
2. Define an object type `type.book` with a required `title` (STRING), a
   required `pages` (INTEGER, at least 1) and an optional `subtitle`. Declare
   two books as DATA and a `LIST` holding both.
3. Define an enumeration `type.season` with four members. Declare one DATA
   value of that type.
4. Explain why `MEASURE(100, unit.centimeter) == MEASURE(1, unit.meter)` is
   FALSE, while `DURATION(60, unit.minute) == DURATION(1, unit.hour)` is TRUE.

Solutions: [solutions/README.md](solutions/README.md#chapter-5).
