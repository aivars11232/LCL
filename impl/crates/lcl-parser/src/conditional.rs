//! The structurally decidable `conditional_requirements`.
//!
//! `field_signatures_v0.1.0.json` records 88 conditional requirements as prose.
//! Most state semantics — authority, defaults, execution behaviour, handler
//! contracts — that later milestones own. A minority are pure structure:
//! whether a field is present, how many of a group occur, whether a value is a
//! reference rather than an inline block. Those are grammar-stage questions and
//! are enforced here as `error.block.conditional_requirement`.
//!
//! Each entry below quotes the registry string **verbatim**. A test asserts
//! every quoted string is still present in the registry for that block, so a
//! reworded or withdrawn requirement fails the build rather than silently
//! ceasing to be enforced. A requirement not listed here is deliberately
//! deferred, and the deferral is named in `DEFERRED`.

use crate::diagnostic::GrammarError;
use crate::grammar::BlockSchema;
use crate::schema::SchemaChecker;
use crate::syntax::*;
use lcl_lexer::Span;
use std::collections::BTreeMap;

/// What a listed requirement checks.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Rule {
    /// Exactly one of the named fields occurs.
    ExactlyOneOf(&'static [&'static str]),
    /// Exactly one of the named fields occurs, unless `DEFAULT` is present.
    ExactlyOneOfUnlessDefault(&'static [&'static str]),
    /// At least one of the named fields occurs.
    AtLeastOneOf(&'static [&'static str]),
    /// At most one of the named fields occurs.
    AtMostOneOf(&'static [&'static str]),
    /// The named fields admit a reference or reference list, never an inline
    /// child block.
    ReferenceOnly(&'static [&'static str]),
    /// Each occurrence of the named repeatable field carries a distinct value.
    UniqueValues(&'static str),
    /// A `URI(...)` SOURCE requires a CHECKSUM sibling.
    UriRequiresChecksum,
    /// `ITEM` is legal only for `KIND kind.type` with `BASE ENUM`.
    ItemRequiresEnumBase,
    /// `FIELD` is legal only for `KIND kind.type` with a direct `BASE OBJECT`.
    FieldRequiresObjectBase,
}

/// `(block, exact registry string, rule)`.
pub(crate) const IMPLEMENTED: &[(&str, &str, Rule)] = &[
    (
        "INPUT",
        "Exactly one VALUE or SOURCE unless optional with DEFAULT.",
        Rule::ExactlyOneOfUnlessDefault(&["VALUE", "SOURCE"]),
    ),
    (
        "CONTEXT",
        "Exactly one VALUE or SOURCE.",
        Rule::ExactlyOneOf(&["VALUE", "SOURCE"]),
    ),
    (
        "MEMORY",
        "Exactly one VALUE or SOURCE.",
        Rule::ExactlyOneOf(&["VALUE", "SOURCE"]),
    ),
    (
        "STATE",
        "Exactly one VALUE or SOURCE.",
        Rule::ExactlyOneOf(&["VALUE", "SOURCE"]),
    ),
    (
        "GOAL",
        "At least ASSERT or RESULT.",
        Rule::AtLeastOneOf(&["ASSERT", "RESULT"]),
    ),
    (
        "TASK",
        "At least PHASE, SEQUENCE, or ACTION.",
        Rule::AtLeastOneOf(&["PHASE", "SEQUENCE", "ACTION"]),
    ),
    (
        "TASK",
        "PHASE, SEQUENCE, and ACTION fields are reference/list-only; inline child blocks are forbidden.",
        Rule::ReferenceOnly(&["PHASE", "SEQUENCE", "ACTION"]),
    ),
    (
        "PHASE",
        "At least STEP, SEQUENCE, IF, or FOR EACH.",
        Rule::AtLeastOneOf(&["STEP", "SEQUENCE"]),
    ),
    (
        "SEQUENCE",
        "At least STEP, IF, or FOR EACH.",
        Rule::AtLeastOneOf(&["STEP"]),
    ),
    (
        "STEP",
        "Exactly one ACTION, SEQUENCE, PHASE, or TASK.",
        Rule::ExactlyOneOf(&["ACTION", "SEQUENCE", "PHASE", "TASK"]),
    ),
    (
        "STEP",
        "SEQUENCE and PHASE fields are reference/list-only; inline SEQUENCE or PHASE blocks are forbidden.",
        Rule::ReferenceOnly(&["SEQUENCE", "PHASE"]),
    ),
    (
        "REQUIRE",
        "Exactly one ASSERT or ACTION.",
        Rule::ExactlyOneOf(&["ASSERT", "ACTION"]),
    ),
    (
        "REQUIRE",
        "ACTION is reference/list-only; an inline ACTION block is forbidden.",
        Rule::ReferenceOnly(&["ACTION"]),
    ),
    (
        "PREFER",
        "Exactly one ASSERT or ACTION.",
        Rule::ExactlyOneOf(&["ASSERT", "ACTION"]),
    ),
    (
        "PREFER",
        "ACTION is reference/list-only; an inline ACTION block is forbidden.",
        Rule::ReferenceOnly(&["ACTION"]),
    ),
    (
        "SUCCESS",
        "Exactly one ALL, ANY, or NONE.",
        Rule::ExactlyOneOf(&["ALL", "ANY", "NONE"]),
    ),
    (
        "TEST",
        "At most one TASK/ACTION.",
        Rule::AtMostOneOf(&["TASK", "ACTION"]),
    ),
    (
        "OUTPUT",
        "Every PROPERTY is unique and must name an available projectable result field.",
        Rule::UniqueValues("PROPERTY"),
    ),
    (
        "IMPORT",
        "URI source requires CHECKSUM.",
        Rule::UriRequiresChecksum,
    ),
    (
        "DEFINE",
        "ITEM is legal only for KIND kind.type with BASE ENUM and occurs one or more times there.",
        Rule::ItemRequiresEnumBase,
    ),
    (
        "DEFINE",
        "FIELD is legal only for KIND kind.type with direct BASE OBJECT. An alias adds no FIELD or ITEM and preserves the resolved base schema or enum domain.",
        Rule::FieldRequiresObjectBase,
    ),
];

/// Requirements this milestone deliberately does not enforce, with the layer
/// that owns each. Listed so the boundary is explicit rather than accidental.
pub(crate) const DEFERRED: &[(&str, &str)] = &[
    ("authority, priority and override outcomes", "M5 semantic preflight"),
    ("declared defaults and their application", "M5 semantic preflight"),
    ("execution ordering, retry budgets and handler selection", "M6 runtime"),
    ("operation parameter and result contracts", "M7 standard library"),
    ("evidence and status lifecycle", "M8 completion"),
    ("value domains, ranges and reference targets", "M3 resolution and M4 static checking"),
];

impl SchemaChecker<'_, '_> {
    /// Apply every implemented conditional requirement for one block.
    pub(crate) fn conditional_requirements(
        &mut self,
        block: &Block,
        schema: &BlockSchema,
        counts: &BTreeMap<&str, Vec<&Field>>,
    ) {
        for (owner, text, rule) in IMPLEMENTED {
            if *owner != schema.name {
                continue;
            }
            // Only enforce what this release still declares.
            if !schema.conditional_requirements.iter().any(|c| c == text) {
                continue;
            }
            self.apply(block, schema, counts, text, *rule);
        }
    }

    fn apply(
        &mut self,
        block: &Block,
        schema: &BlockSchema,
        counts: &BTreeMap<&str, Vec<&Field>>,
        text: &str,
        rule: Rule,
    ) {
        let present = |names: &[&str]| -> usize {
            names.iter().filter(|n| counts.contains_key(**n)).count()
        };
        match rule {
            Rule::ExactlyOneOf(names) => {
                if present(names) != 1 {
                    self.requirement(block, schema, text);
                }
            }
            Rule::ExactlyOneOfUnlessDefault(names) => {
                let relaxed = counts.contains_key("DEFAULT");
                let count = present(names);
                if (relaxed && count > 1) || (!relaxed && count != 1) {
                    self.requirement(block, schema, text);
                }
            }
            Rule::AtLeastOneOf(names) => {
                // A control form satisfies the PHASE and SEQUENCE variants, so
                // an IF or FOR EACH child counts as a present member.
                let control = block.body.iter().any(|s| {
                    matches!(s, Statement::Conditional(_) | Statement::ForEach(_))
                });
                if present(names) == 0 && !control {
                    self.requirement(block, schema, text);
                }
            }
            Rule::AtMostOneOf(names) => {
                if present(names) > 1 {
                    self.requirement(block, schema, text);
                }
            }
            Rule::ReferenceOnly(names) => {
                for name in names {
                    for field in counts.get(*name).into_iter().flatten() {
                        if field.body.as_nested().is_some() {
                            let span = field.key.span;
                            let owner = schema.name.clone();
                            let field_name = (*name).to_string();
                            self.emit_requirement(
                                span,
                                format!(
                                    "`{owner}.{field_name}` is reference/list-only; an inline block is forbidden"
                                ),
                            );
                        }
                    }
                }
            }
            Rule::UniqueValues(name) => {
                let mut seen: Vec<(String, Span)> = Vec::new();
                for field in counts.get(name).into_iter().flatten() {
                    let Some(text) = field.body.as_inline().map(render_value) else {
                        continue;
                    };
                    if seen.iter().any(|(prior, _)| prior == &text) {
                        let span = field.key.span;
                        let owner = schema.name.clone();
                        self.emit_requirement(
                            span,
                            format!("`{owner}.{name}` occurrences must each be distinct"),
                        );
                    }
                    seen.push((text, field.key.span));
                }
            }
            Rule::UriRequiresChecksum => {
                let uri_source = counts
                    .get("SOURCE")
                    .into_iter()
                    .flatten()
                    .any(|f| is_call_to(f, "URI"));
                if uri_source && !counts.contains_key("CHECKSUM") {
                    self.requirement(block, schema, text);
                }
            }
            Rule::ItemRequiresEnumBase => {
                if counts.contains_key("ITEM")
                    && !(has_inline_identifier(counts, "KIND", "kind.type")
                        && has_inline_word(counts, "BASE", "ENUM"))
                {
                    self.requirement(block, schema, text);
                }
            }
            Rule::FieldRequiresObjectBase => {
                if counts.contains_key("FIELD")
                    && !(has_inline_identifier(counts, "KIND", "kind.type")
                        && has_inline_word(counts, "BASE", "OBJECT"))
                {
                    self.requirement(block, schema, text);
                }
            }
        }
    }

    fn requirement(&mut self, block: &Block, schema: &BlockSchema, text: &str) {
        let span = block.key.span;
        let owner = schema.name.clone();
        self.emit_requirement(span, format!("`{owner}`: {text}"));
    }

    fn emit_requirement(&mut self, span: Span, detail: String) {
        self.emitter.emit(
            GrammarError::BlockConditionalRequirement,
            span,
            "conditional_requirement",
            detail,
        );
    }
}

/// A stable rendering of an inline value, for uniqueness comparison only.
fn render_value(value: &Value) -> String {
    match value {
        Value::Expression(Expr::Identifier(id)) => id.text.clone(),
        Value::Expression(Expr::Literal(l)) => l.text.clone(),
        Value::Expression(e) => format!("{:?}", e.span()),
        Value::MultilineCollection(c) => format!("{:?}", c.span),
    }
}

fn is_call_to(field: &Field, callable: &str) -> bool {
    matches!(
        field.body.as_inline().and_then(Value::as_expression),
        Some(Expr::Call(c)) if c.callable.text == callable
    )
}

fn has_inline_identifier(
    counts: &BTreeMap<&str, Vec<&Field>>,
    field: &str,
    expected: &str,
) -> bool {
    counts.get(field).into_iter().flatten().any(|f| {
        matches!(
            f.body.as_inline().and_then(Value::as_expression),
            Some(Expr::Identifier(id)) if id.text == expected
        )
    })
}

fn has_inline_word(counts: &BTreeMap<&str, Vec<&Field>>, field: &str, expected: &str) -> bool {
    counts.get(field).into_iter().flatten().any(|f| {
        match f.body.as_inline().and_then(Value::as_expression) {
            Some(Expr::Type(t)) => t.word().text == expected,
            Some(Expr::Identifier(id)) => id.text == expected,
            _ => false,
        }
    })
}
