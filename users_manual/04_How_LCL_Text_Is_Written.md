# Chapter 4. How LCL text is written

**In this chapter you will learn**

* the exact rules for characters, lines and indentation;
* how keywords and identifiers are spelled;
* how to write strings, numbers, true/false and collections;
* which symbols LCL does not allow, and what to write instead.

LCL is strict about how text is written. Some languages quietly repair small
mistakes; LCL rejects them instead, because a repaired document is not quite
the one you wrote. The rules are few, and once you know them the tool's
messages become easy to act on. They are defined in the specification's
`02_LEXICAL/` folder.

## 4.1 A practice document

The examples in this chapter use a **data document**, `KIND: kind.data`. A
data document holds typed values and nothing else. It has no task and nothing
to execute, which makes it ideal for practising how values are written. (A
data document still "runs": it is checked and then succeeds immediately, with
nothing to do.)

<!-- lcl: file=examples/04/literals.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.literals
    NAME: "Every kind of literal"
    VERSION: "1.0.0"
    KIND: kind.data

DATA:
    ID: data.whole_number
    TYPE: INTEGER
    VALUE: 42

DATA:
    ID: data.negative_number
    TYPE: INTEGER
    VALUE: -7

DATA:
    ID: data.exact_decimal
    TYPE: DECIMAL
    VALUE: 3.14

DATA:
    ID: data.flag
    TYPE: BOOLEAN
    VALUE: TRUE

DATA:
    ID: data.greeting
    TYPE: STRING
    VALUE: "Hello, world!"

DATA:
    ID: data.with_escapes
    TYPE: STRING
    VALUE: "Tab:\t Quote:\" Backslash:\\ Snowman:\u2603"

DATA:
    ID: data.unicode_text
    TYPE: STRING
    VALUE: "Grüße, 世界"

DATA:
    ID: data.poem
    TYPE: STRING
    VALUE: """
        Roses are red,
          this line keeps two extra spaces,

        and a blank line is kept too.
    """

DATA:
    ID: data.short_list
    TYPE: LIST[INTEGER]
    VALUE: [1, 2, 3]

DATA:
    ID: data.long_list
    TYPE: LIST[STRING]
    VALUE: [
        "first",
        "second",
        "third"
    ]

DATA:
    ID: data.person
    TYPE: OBJECT
    VALUE:
        name: "Ada"
        age: 12
        status: "student"
```

Save it as `literals.lcl` and check it with `lcl check literals.lcl`. Then
work through the rules below, changing the document to see each one enforced.

## 4.2 Characters and lines

* Files are **UTF-8** text, with no byte-order mark.
* Lines end with a single **line feed** (the Unix line ending). Windows-style
  carriage-return-plus-line-feed endings are rejected
  (`error.newline.invalid`). If you use Windows, set your editor to "LF"
  line endings.
* The file must **end with a line feed** (`error.source.final_line_feed`).
* A line may not end with a space (`error.source.trailing_space`). Many
  editors can highlight or remove trailing spaces automatically.
* **Tabs are forbidden** everywhere outside strings (`error.source.tab`).
* Outside strings, only plain ASCII letters, digits, spaces and LCL's own
  symbols may appear. Any other character, such as `ö`, is rejected
  (`error.source.non_ascii_outside_string`). Inside strings, any Unicode
  text is fine: `"Grüße, 世界"` is a legal string.

## 4.3 Indentation

* One level of indentation is **exactly four spaces**. Two spaces is an error
  (`error.indentation.width`).
* Indentation increases by one level only after a line ending in a colon
  (which opens a block) or after an opening `[` of a list written over
  several lines.
* Going back to a smaller indentation closes the deeper blocks.
* A block cannot be empty. `DATA:` with nothing under it is an error
  (`error.indentation.empty_block`).
* You cannot continue an expression onto the next line. `VALUE: 1 +` followed
  by `2` on the next line is rejected
  (`error.indentation.invalid`). Keep each expression on one line.

## 4.4 Keys, colons and values

* `KEY: value` (a colon, then **exactly one** space, then the value) is an
  inline field.
* `KEY:` followed immediately by the end of the line opens a block.
* There is no `=` for assignment. `VALUE = 5` is rejected
  (`error.symbol.invalid`). Use `:` to declare and `==` to compare.
* There are no semicolons.

## 4.5 Keywords

LCL's own words are **uppercase**: `INPUT`, `TYPE`, `REF`, `TRUE`, `AND`,
`FOR`, `EACH` and so on. There are 141 of them (see
[Appendix A](Appendix_A_Keyword_Reference.md)). The list is closed:

* An uppercase word that is not on the list is an error:
  `MUST: 5` gives `error.keyword.unknown`. If you meant "must", use
  `REQUIRE`. If you meant a loop, note that `WHILE` does not exist.
* A keyword in the wrong case is an error: `id:` gives `error.keyword.case`,
  and so does `Name:`. The tool never corrects the case for you.

## 4.6 Identifiers

Identifiers are the names *you* choose, for example in `ID:` fields.

* A simple identifier is lowercase letters, digits and underscores, starting
  with a letter: `total`, `line_2`, `tax_rate`.
* A qualified identifier is simple identifiers joined by dots:
  `input.tax_rate`, `step.check_totals`.
* No uppercase letters: `Data.X` is rejected.
* Some first parts are reserved for the language itself: `core`, `encoding`,
  `error`, `event`, `format`, `kind`, `mode`, `status` and `unit`. An ID such
  as `core.mine` is rejected (`error.namespace.invalid`).
* A lowercase word is always an identifier, even if it spells a keyword:
  `status` and `count` are fine as object field names, and `true` is an
  identifier, **not** the Boolean `TRUE`. Writing `VALUE: true` for a
  BOOLEAN is therefore an error (`error.type.mismatch`).

## 4.7 Strings

* Strings use **double quotes**: `"like this"`. Single quotes are not allowed
  (`error.symbol.invalid`).
* Inside a string, a backslash starts an escape:

  | Escape | Means |
  |---|---|
  | `\"` | a double quote |
  | `\\` | a backslash |
  | `\n` | a line feed |
  | `\r` | a carriage return |
  | `\t` | a tab |
  | `\u2603` | the Unicode character with hexadecimal code 2603 (☃) |

  Any other escape, such as `\q`, is an error (`error.literal.escape`).
* There is no interpolation: `"$name"` is just those five characters.

**Multiline strings** start with `"""` at the end of the value line. The
content goes on the following lines, indented **one level deeper than the
field**. The closing `"""` sits alone on its own line, at the **same
indentation as the field** (here `VALUE:`):

<!-- lcl: expect=fragment -->
```lcl
    VALUE: """
        Roses are red,
          this line keeps two extra spaces,

        and a blank line is kept too.
    """
```

The content's own indentation is removed. Extra spaces beyond it are kept,
and every line, including the last, ends with a line feed. Putting the
closing `"""` anywhere else is an error (`error.literal.invalid`).

## 4.8 Numbers

| Write | Type | Notes |
|---|---|---|
| `0`, `42`, `1000000` | INTEGER | No leading zeros (`007` is an error) and no separators (`1_000` is an error). |
| `3.14`, `0.5`, `2.0` | DECIMAL | Digits on both sides of the point: `.5` is an error, write `0.5`. |
| `-7`, `-0.25` | INTEGER or DECIMAL | The minus is an operator applied to the number. |

There is no leading `+`, no exponent notation (`1.5e3` is an error), and no
infinity or "not a number". Integers have no size limit, and decimals are
exact. Chapter 6 explains what "exact" means for division.

## 4.9 True, false and the special values

`TRUE` and `FALSE` are the two Boolean values. `NULL`, `MISSING` and
`UNKNOWN` are special values with precise meanings, covered in
[Chapter 13](13_Missing_Values_Errors_and_Recovery.md).

## 4.10 Lists and objects

A list on one line uses brackets, with **exactly one space after each
comma**: `[1, 2, 3]`. Both `[1,2]` and `[1,  2]` are errors.

A long list may be written one member per line. Indent the members one level,
put a comma after every member **except the last**, and put the closing `]`
on its own line, aligned with the field:

<!-- lcl: expect=fragment -->
```lcl
    VALUE: [
        "first",
        "second",
        "third"
    ]
```

A comma after the last member is an error.

An **object** is written as an indented block under `VALUE:`, with lowercase
field names:

<!-- lcl: expect=fragment -->
```lcl
    VALUE:
        name: "Ada"
        age: 12
```

Braces `{ }` are not used anywhere in LCL.

## 4.11 Symbols LCL does not use

LCL deliberately rejects symbols that mean different things in different
languages (`02_LEXICAL/10_EXCLUDED_SYMBOLS_AND_NOTATION.txt`). Each one has a
clear replacement:

| Instead of | Write |
|---|---|
| `=` | `:` to declare, `==` to compare |
| `{ }` | indentation |
| `;` | a new line |
| `'text'` | `"text"` |
| `# comment`, `// comment`, `/* */` | a `COMMENT` block (see below) |
| `&`, `&&` | `AND` |
| `\|`, `\|\|` | `OR` |
| `!` | `NOT`, or `FORBID` for rules |
| `$` | nothing: there is no interpolation |
| `50%` | `PERCENTAGE(50)` |
| `~` | `MINIMUM`/`MAXIMUM` or `TOLERANCE` |
| `^` | there is no power operator in Core 0.1.0 |
| `->` | `IF (...) THEN:` or an exact reference |
| `?` | `REQUIRED: FALSE` for optional things |
| `...` | the actual content: nothing may be left out |
| `` ` `` | nothing: Markdown code marks are not LCL |

All of these give `error.symbol.invalid`. Inside a string they are ordinary
text, so `"50% off!"` is fine.

## 4.12 Comments

There are no comment symbols. To leave a note in a document, write a
`COMMENT` block:

<!-- lcl: file=examples/04/comment.lcl expect=run:succeeded -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.comment
    NAME: "A document with a comment"
    VERSION: "1.0.0"
    KIND: kind.data
    DESCRIPTION: "Descriptions are labels for people, too."

COMMENT:
    CONTENT: "The rate below comes from the 2026 price list."

DATA:
    ID: data.rate
    TYPE: DECIMAL
    VALUE: 0.2
    DESCRIPTION: "Standard rate as a fraction, not a percentage."
```

`COMMENT` and `DESCRIPTION` are for people. They never change what a document
does, and a tool or AI must never look inside them for instructions. If
something matters, it has to be written as a real field or rule.

## 4.13 Three rejected documents

Each of these is a complete document with exactly one mistake. Predict the
error before you run `lcl check`.

A mathematician's habit:

<!-- lcl: file=examples/04/bare_equals.invalid.lcl expect=reject:error.symbol.invalid -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.bare_equals
    NAME: "Bare equals"
    VERSION: "1.0.0"
    KIND: kind.data

DATA:
    ID: data.x
    TYPE: INTEGER
    VALUE = 5
```

A Python habit:

<!-- lcl: file=examples/04/lowercase_true.invalid.lcl expect=reject:error.type.mismatch -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.lowercase_true
    NAME: "Lowercase true"
    VERSION: "1.0.0"
    KIND: kind.data

DATA:
    ID: data.ready
    TYPE: BOOLEAN
    VALUE: true
```

A C or JavaScript habit:

<!-- lcl: file=examples/04/slash_comment.invalid.lcl expect=reject:error.symbol.invalid -->
```lcl
LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: course.slash_comment
    NAME: "Slash comment"
    VERSION: "1.0.0"
    KIND: kind.data

DATA:
    ID: data.x
    TYPE: INTEGER
    VALUE: 5 // five
```

The tool tells you the line and column of each mistake:

```
$ lcl check bare_equals.invalid.lcl
bare_equals.invalid.lcl:13:11: error.symbol.invalid [lexical]: `=` is an excluded symbol
```

## Summary

* UTF-8, LF line endings, a final line feed, no trailing spaces, no tabs.
* Indent by exactly four spaces, and only after a colon or an opening `[`.
* Keywords are uppercase and come from a closed list. Identifiers are
  lowercase, dotted names.
* Strings use double quotes and a small, fixed set of escapes. Multiline
  strings use `"""` with exact alignment.
* Numbers are plain: no `+`, no separators, no exponents, no leading zeros.
* Many familiar symbols are excluded on purpose, and each has a written
  replacement.
* Notes go in `COMMENT` blocks and `DESCRIPTION` fields, never in hidden
  syntax.

## Exercises

1. Write a data document `me.lcl` with DATA values for your name (STRING),
   your age (INTEGER), your height in metres (DECIMAL) and whether you like
   mathematics (BOOLEAN). Check it.
2. Add a multiline string holding a three-line poem, with the middle line
   indented by two extra spaces. Check it.
3. Each line below breaks one rule. Name the rule and fix the line.
   * `VALUE: [1,2,3]`
   * `VALUE: 'yes'`
   * `VALUE: 1.5E3`
   * `Value: 5`
   * `VALUE: 10%`
4. Why does LCL reject `VALUE: true` for a BOOLEAN instead of accepting it as
   `TRUE`? Use a design principle from Chapter 1 in your answer.

Solutions: [solutions/README.md](solutions/README.md#chapter-4).
