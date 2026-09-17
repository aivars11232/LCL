//! Invocation-site operation contracts.
//!
//! `06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt` is explicit
//! about both the rule and the stage:
//!
//! > Every operation lists error.operation.parameter; that error is emitted
//! > during static/expression checking when an ACTION, HANDLER, or FALLBACK
//! > invocation site omits TARGET while the selected operation marks it
//! > required and no handler-context binding supplies it, omits a named
//! > parameter that the operation marks required, supplies any positional
//! > argument, duplicates a named parameter, or supplies an unregistered named
//! > parameter.
//!
//! and about what is *not* remapped to it:
//!
//! > A declared parameter value rejected by its type or semantic constraint uses
//! > the general or row-specific error and is not remapped to
//! > error.operation.parameter. … A declared value outside a numeric bound such
//! > as core.inspect depth or core.retry limit uses error.value.out_of_range.
//!
//! ## The one permitted omission
//!
//! "Exactly one omission is permitted, the handler-context binding: when the
//! selected operation's registered target type admits REFERENCE[ACTION] or
//! REFERENCE[meta.execution_unit] and TARGET is omitted, the target binds to the
//! still-active invocation aggregate". That is read from the contract's own
//! target type, not from a list of operation names.

use crate::contracts::{ContractType, OperationContract};
use crate::declarations::field_in;
use crate::expr::Check;
use crate::ty::{EnumDomain, Type};
use crate::StaticError;
use lcl_parser::syntax::{Block, Expr, Statement, TypeExpr, Value};
use lcl_resolver::{FullId, SourceId};
use std::collections::BTreeMap;

/// The three invocation sites. "ACTION, HANDLER, and FALLBACK are the only
/// invocation sites; every required target and named parameter rule applies
/// identically on each of them." ACTION and HANDLER are blocks; FALLBACK is a
/// HANDLER field, judged by [`fallback`] with its handler.
const INVOCATION_BLOCKS: [&str; 2] = ["ACTION", "HANDLER"];

/// Check one block as an invocation site, when it is one.
pub(crate) fn invocation(
    check: &mut Check<'_>,
    source: &SourceId,
    block: &Block,
    name: &str,
) -> BTreeMap<lcl_lexer::Span, Type> {
    if !INVOCATION_BLOCKS.contains(&name) {
        return BTreeMap::new();
    }
    if name == "HANDLER" {
        fallback(check, source, block);
    }
    let Some(field) = block.field("OPERATION") else {
        return BTreeMap::new();
    };
    let Some(identifier) = operation_identifier(field) else {
        return BTreeMap::new();
    };
    // A custom `DEFINE kind.operation` declares its own contract in LCL; the
    // closed core contracts are the registry's.
    let Some(contract) = check.contracts.operation(&identifier).cloned() else {
        return BTreeMap::new();
    };

    let locus = block.key.span;
    parameters(check, source, block, &contract, locus);
    target(check, source, block, &contract, name, locus);
    enum_contexts(block, &contract)
}

/// 03_TYPES_AND_VALUES/05 permits bare ENUM only when this exact receiving
/// operation parameter supplies one domain. Key by the PARAMETER field's
/// locus so a sibling or nested value cannot accidentally inherit it.
fn enum_contexts(block: &Block, contract: &OperationContract) -> BTreeMap<lcl_lexer::Span, Type> {
    let mut contexts = BTreeMap::new();
    for statement in &block.body {
        let Statement::Field(field) = statement else {
            continue;
        };
        if field.key.text != "PARAMETER" {
            continue;
        }
        let Some(nested) = field.body.as_nested() else {
            continue;
        };
        let Some(name) = parameter_name(&nested.statements) else {
            continue;
        };
        let Some(spec) = contract.parameters.get(&name) else {
            continue;
        };
        let ContractType::Enum(items) = &spec.ty else {
            continue;
        };
        let Some(declared) = field_in(&nested.statements, "TYPE") else {
            continue;
        };
        if !matches!(declared.body.as_inline(),
            Some(Value::Expression(Expr::Type(TypeExpr::Scalar(word)))) if word.text == "ENUM")
        {
            continue;
        }
        // This internal identity is the registered slot, not a new source
        // declaration. The pointer separators cannot collide with a source ID.
        let id = FullId::local(format!(
            "operations_v0.1.0.json#/contracts/{}/parameters/{name}",
            contract.id
        ));
        contexts.insert(
            field.key.span,
            Type::Enum(EnumDomain {
                id,
                items: items.clone(),
            }),
        );
    }
    contexts
}

/// The `OPERATION` field's identifier, as written.
fn operation_identifier(field: &lcl_parser::syntax::Field) -> Option<String> {
    match field.body.as_inline()? {
        Value::Expression(lcl_parser::syntax::Expr::Identifier(ident)) => Some(ident.text.clone()),
        _ => None,
    }
}

/// Every `PARAMETER` child of an invocation site, judged against the contract.
fn parameters(
    check: &mut Check<'_>,
    source: &SourceId,
    block: &Block,
    contract: &OperationContract,
    locus: lcl_lexer::Span,
) {
    let mut supplied: BTreeMap<String, usize> = BTreeMap::new();

    // A registered invocation-site field can *be* a named parameter:
    // `field_signatures#/blocks/HANDLER/conditional_requirements` states "LIMIT
    // is exactly the limit named parameter of the selected operation and is
    // legal only when that operation registers it." The mapping is the field
    // key lowercased, so it is read rather than listed.
    for statement in &block.body {
        let Statement::Field(field) = statement else {
            continue;
        };
        let lowered = field.key.text.to_lowercase();
        if field.key.text == "PARAMETER" || !contract.parameters.contains_key(&lowered) {
            continue;
        }
        *supplied.entry(lowered).or_default() += 1;
    }

    for statement in &block.body {
        let Statement::Field(field) = statement else {
            continue;
        };
        if field.key.text != "PARAMETER" {
            continue;
        }
        let Some(nested) = field.body.as_nested() else {
            continue;
        };
        let Some(name) = parameter_name(&nested.statements) else {
            continue;
        };
        let count = supplied.entry(name.clone()).or_default();
        *count += 1;

        // "duplicates a named parameter" — including one already supplied by
        // the invocation site's own registered field.
        if *count > 1 {
            check.emit(
                StaticError::OperationParameter,
                source,
                field.key.span,
                "duplicate_parameter",
                format!(
                    "`{}` supplies the named parameter `{name}` more than once",
                    contract.id
                ),
            );
            continue;
        }
        // "Static checks cover the complete fragment."
        if let Some(spec) = contract.parameters.get(&name) {
            fragment_parameter(check, source, contract, spec, &nested.statements);
            declared_family(check, source, contract, spec, &name, &nested.statements);
            closed_object_parameter(check, source, contract, spec, &name, &nested.statements);
        }
        // "supplies an unregistered named parameter"
        if !contract.parameters.contains_key(&name) {
            let registered: Vec<&str> = contract.parameters.keys().map(String::as_str).collect();
            check.emit(
                StaticError::OperationParameter,
                source,
                field.key.span,
                "unregistered_parameter",
                format!(
                    "`{}` registers {}; `{name}` is not one of them",
                    contract.id,
                    if registered.is_empty() {
                        "no named parameter".to_string()
                    } else {
                        format!("exactly {}", registered.join(" and "))
                    }
                ),
            );
        }
    }

    // "omits a named parameter that the operation marks required"
    for (name, spec) in &contract.parameters {
        if spec.required && !supplied.contains_key(name) {
            check.emit(
                StaticError::OperationParameter,
                source,
                locus,
                "missing_parameter",
                format!("`{}` requires the named parameter `{name}`", contract.id),
            );
        }
    }
}

/// A `PARAMETER` block's `NAME`.
fn parameter_name(statements: &[Statement]) -> Option<String> {
    for statement in statements {
        let Statement::Field(field) = statement else {
            continue;
        };
        if field.key.text != "NAME" {
            continue;
        }
        if let Some(Value::Expression(lcl_parser::syntax::Expr::Identifier(ident))) =
            field.body.as_inline()
        {
            return Some(ident.text.clone());
        }
    }
    None
}

/// `TARGET`, with the one permitted handler-context omission.
fn target(
    check: &mut Check<'_>,
    source: &SourceId,
    block: &Block,
    contract: &OperationContract,
    block_name: &str,
    locus: lcl_lexer::Span,
) {
    if block.field("TARGET").is_some() || !contract.target.required {
        return;
    }
    if block_name == "HANDLER" && admits_handler_context(&contract.target.ty) {
        return;
    }
    check.emit(
        StaticError::OperationParameter,
        source,
        locus,
        "missing_target",
        format!("`{}` marks TARGET required", contract.id),
    );
}

/// A HANDLER's operation-identifier `FALLBACK`, the third invocation site.
///
/// `05_SEMANTICS/06`: "It is a second invocation site with no target or
/// parameter surface of its own ... One operation identifier is legal only when
/// the operation registers no required named parameter and its required target,
/// if any, is supplied by the original handler-context binding above. An
/// operation identifier that cannot satisfy its contract under those limits
/// uses error.operation.parameter". The `REF` form carries the referenced
/// handler's own invocation data, which is judged where that handler is
/// written; a custom operation's contract is its own, exactly as at ACTION.
fn fallback(check: &mut Check<'_>, source: &SourceId, block: &Block) {
    let Some(field) = block.field("FALLBACK") else {
        return;
    };
    let Some(identifier) = operation_identifier(field) else {
        return;
    };
    let Some(contract) = check.contracts.operation(&identifier).cloned() else {
        return;
    };
    let span = field
        .body
        .as_inline()
        .map_or(field.key.span, |value| match value {
            Value::Expression(expr) => expr.span(),
            _ => field.key.span,
        });
    for (name, spec) in &contract.parameters {
        if spec.required {
            check.emit(
                StaticError::OperationParameter,
                source,
                span,
                "fallback_parameter",
                format!(
                    "FALLBACK `{}` requires the named parameter `{name}`, and a FALLBACK supplies none",
                    contract.id
                ),
            );
        }
    }
    if contract.target.required && !admits_handler_context(&contract.target.ty) {
        check.emit(
            StaticError::OperationParameter,
            source,
            span,
            "fallback_target",
            format!(
                "FALLBACK `{}` marks TARGET required, and no handler-context binding supplies it",
                contract.id
            ),
        );
    }
}

/// True when the contract's target type "admits REFERENCE[ACTION] or
/// REFERENCE[meta.execution_unit]".
fn admits_handler_context(ty: &ContractType) -> bool {
    ty.members().into_iter().any(|member| match member {
        ContractType::Reference(domain) => domain
            .iter()
            .any(|target| target == "ACTION" || target == "meta.execution_unit"),
        _ => false,
    })
}

/// One expression-fragment parameter, checked as a fragment.
///
/// `expression_fragment_contract/evaluation`: "Static checks cover the complete
/// fragment; dynamic errors arise only from evaluated subexpressions."
/// `#/diagnostics`: "Malformed fragment syntax and invalid bindings produce
/// error.operation.parameter."
///
/// ## Why this is the stage that can say it
///
/// `error.operation.parameter` is registered `static_or_expression`, and it is
/// not in `expression_demand_resolution`'s eligible map, whose `exclusion_rule`
/// puts every "source structure, token, name resolution" defect outside demand
/// resolution. A fragment that is not one expression is a source-structure
/// defect. The runtime therefore cannot raise this identifier without
/// relabelling a stage, and before this check it substituted another
/// identifier from the row. Here the fragment is a written STRING, so the
/// defect is knowable exactly where the registry says it is checked.
///
/// ## Which parameters are fragments
///
/// The ones whose registered constraints say so. The rule is read from the
/// contract rather than from a list of operation names, so a row that gains a
/// fragment parameter is covered without a code change, and one that loses it
/// stops being checked for the same reason.
fn fragment_parameter(
    check: &mut Check<'_>,
    source: &SourceId,
    contract: &OperationContract,
    spec: &crate::contracts::ParameterSpec,
    statements: &[Statement],
) {
    if !spec
        .constraints
        .iter()
        .any(|c| c.contains("expression_fragment_contract"))
    {
        return;
    }
    // Only a written STRING is knowable here. "or REF to a DEFINE kind.constant
    // whose declared value is such a STRING" is a reference this stage does not
    // dereference, and a value that arrives at demand is the demanding layer's.
    let Some((text, span)) = fragment_text(statements) else {
        return;
    };
    let Err(error) = check.contracts.expression_fragment(&text) else {
        return;
    };
    check.emit(
        StaticError::OperationParameter,
        source,
        span,
        "malformed_fragment",
        format!(
            "`{}` takes {:?} as one expression fragment, and {}",
            contract.id,
            text,
            describe(&error, &text)
        ),
    );
}

/// A `PARAMETER` block's `VALUE`, when it is written as one STRING literal.
fn fragment_text(statements: &[Statement]) -> Option<(String, lcl_lexer::Span)> {
    for statement in statements {
        let Statement::Field(field) = statement else {
            continue;
        };
        if field.key.text != "VALUE" {
            continue;
        }
        let Some(Value::Expression(lcl_parser::syntax::Expr::Literal(literal))) =
            field.body.as_inline()
        else {
            return None;
        };
        if !matches!(
            literal.kind,
            lcl_parser::syntax::LiteralKind::String
                | lcl_parser::syntax::LiteralKind::MultilineString
        ) {
            return None;
        }
        return Some((literal.text.clone(), literal.span));
    }
    None
}

/// Why one fragment is not exactly one expression, in the author's terms.
fn describe(error: &lcl_parser::FragmentError, fragment: &str) -> String {
    use lcl_parser::FragmentError;
    match error {
        FragmentError::Trailing(span) => format!(
            "input remains after the first one at byte {}",
            span.start
                .saturating_sub(lcl_parser::FRAGMENT_CONTEXT.len())
        ),
        FragmentError::Empty(_) => format!("{fragment:?} holds no expression"),
        other => format!("it is not one expression: {other}"),
    }
}

/// One supplied parameter whose declared `TYPE` names a family the row does not
/// register.
///
/// `06_STANDARD_LIBRARY/10`, on `error.operation.parameter`, covers an
/// invocation site that gets the *parameter contract* wrong, as distinct from a
/// value that violates its own declared type: "A declared parameter value
/// rejected by its type or semantic constraint uses the general or row-specific
/// error and is not remapped to error.operation.parameter." A site that
/// declares `TYPE: BYTES` for a parameter the registry types `STRING|LIST[T]`
/// has not supplied a value of the wrong type; it has declared a parameter the
/// row does not have, which is what the two witnesses name:
///
/// > `core.append` content BYTES(4) — error.operation.parameter; a byte count
/// > specifies no content to append.
///
/// > `core.validate` schema supplied as an arbitrary OBJECT —
/// > error.operation.parameter; schema requires a reference to a resolved
/// > object-schema type.
///
/// ## Deliberately narrow
///
/// Only a plain scalar type word is judged, and only against a row whose every
/// alternative is a concrete form. A row that admits a metatype or a type
/// variable is skipped entirely, because a scalar word may legitimately satisfy
/// one and this check does not model the whole contract-type notation. Nothing
/// uncertain is refused here; a value defect the later layers own stays theirs.
fn declared_family(
    check: &mut Check<'_>,
    source: &SourceId,
    contract: &OperationContract,
    spec: &crate::contracts::ParameterSpec,
    name: &str,
    statements: &[Statement],
) {
    let Some((declared, span)) = declared_scalar(statements) else {
        return;
    };
    let members = spec.ty.members();
    // `#/contract_type_notation`: "registry-only unions, metatypes, and schema
    // names do not become source forms". A row alternative with no source
    // spelling cannot be compared to a written type word at all, so a row that
    // has one is left alone entirely: the site had to write *something* else,
    // and deciding what would be inventing a mapping the registry withholds.
    // A type variable is skipped for the same reason, and a metatype because a
    // scalar word may legitimately satisfy one.
    if members.iter().any(|member| match member {
        ContractType::Enum(_) | ContractType::QualifiedIdentifier(_) => true,
        // A source `TYPE:` field spells one `SCALAR_TYPE`, and every one of
        // those is an uppercase word. An atom that is not one is a registry-only
        // name -- a metatype, a named value kind such as `boolean_expression`, a
        // result record -- with no source spelling at all. A type variable is
        // uppercase but names no family either.
        ContractType::Atom(atom) => {
            !atom.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                || (atom.len() == 1 && atom.chars().all(|c| c.is_ascii_uppercase()))
        }
        _ => false,
    }) {
        return;
    }
    let admitted = members.iter().any(|member| match member {
        ContractType::Atom(atom) => atom == &declared,
        _ => false,
    });
    if admitted {
        return;
    }
    let registered: Vec<String> = members.iter().map(|m| m.to_string()).collect();
    check.emit(
        StaticError::OperationParameter,
        source,
        span,
        "parameter_family",
        format!(
            "`{}` types the named parameter `{name}` as {}; `{declared}` is not one of them",
            contract.id,
            registered.join(" or ")
        ),
    );
}

/// A `PARAMETER` block's `TYPE`, when it is written as one scalar type word.
fn declared_scalar(statements: &[Statement]) -> Option<(String, lcl_lexer::Span)> {
    for statement in statements {
        let Statement::Field(field) = statement else {
            continue;
        };
        if field.key.text != "TYPE" {
            continue;
        }
        let Some(Value::Expression(lcl_parser::syntax::Expr::Type(
            lcl_parser::syntax::TypeExpr::Scalar(word),
        ))) = field.body.as_inline()
        else {
            return None;
        };
        return Some((word.text.clone(), word.span));
    }
    None
}

// ---------------------------------------------------------------------------
// A parameter whose registered constraint closes its object shape
// ---------------------------------------------------------------------------

/// The closed shape one parameter's registered constraints declare.
///
/// Read from the row, not written here. `core.read`'s `range` is the only row
/// that currently carries such a constraint; a row that gained one would be
/// covered without a code change, and one that lost it would stop being
/// checked for the same reason.
struct ClosedShape {
    /// Admitted keys with the type word each is declared as, in source order.
    fields: Vec<(String, String)>,
    /// The closed set of words the `unit` key admits.
    units: Vec<String>,
}

/// The marker phrase that says a parameter's object shape is closed.
const CLOSED_KEYS: &str = "No other keys or defaults are admitted";

fn closed_shape(spec: &crate::contracts::ParameterSpec) -> Option<ClosedShape> {
    let constraint = spec.constraints.iter().find(|c| c.contains(CLOSED_KEYS))?;

    // "... is an OBJECT with exactly unit: STRING, start: INTEGER, and end:
    // INTEGER. No other keys or defaults are admitted."
    let listed = constraint
        .split("with exactly ")
        .nth(1)?
        .split(". ")
        .next()?;
    let mut fields = Vec::new();
    for piece in listed.split(',') {
        let piece = piece.trim().trim_start_matches("and ").trim();
        let (key, word) = piece.split_once(':')?;
        fields.push((key.trim().to_string(), word.trim().to_string()));
    }
    if fields.is_empty() {
        return None;
    }

    // "unit is exactly scalar, line, item, or byte."
    let units = match constraint.split(" is exactly ").nth(1) {
        Some(rest) => rest
            .split('.')
            .next()
            .unwrap_or_default()
            .split(',')
            .map(|word| word.trim().trim_start_matches("or ").trim().to_string())
            .filter(|word| !word.is_empty())
            .collect(),
        None => Vec::new(),
    };
    Some(ClosedShape { fields, units })
}

/// One written object literal, against the closed shape its row declares.
///
/// ## Why this belongs here rather than at demand
///
/// The row names `error.operation.parameter` for "an incompatible
/// unit/representation or wrong key/type", and that identifier is registered at
/// the `static_or_expression` stage. `expression_demand_resolution` does not
/// make it eligible, and its `exclusion_rule` puts every "source structure,
/// token, name resolution, type-family" defect outside the map, so a wrong key
/// or a wrong value type never becomes an execution-stage failure.
///
/// A range written as a literal OBJECT is knowable exactly here, which is where
/// `earliest_stage_rule` requires it to be decided. Before this check the
/// runtime met these cases at demand and substituted another identifier from
/// the row, which `LCL_RELEASE_REPORT.md` recorded as a known limitation.
///
/// ## Deliberately narrow
///
/// Only a written literal is judged. A property whose value is a reference, an
/// expression or anything else this stage does not evaluate is left alone, and
/// so is a `VALUE` that is not an indented object at all. Nothing uncertain is
/// refused here.
fn closed_object_parameter(
    check: &mut Check<'_>,
    source: &SourceId,
    contract: &OperationContract,
    spec: &crate::contracts::ParameterSpec,
    name: &str,
    statements: &[Statement],
) {
    let Some(shape) = closed_shape(spec) else {
        return;
    };
    let Some(value) = field_in(statements, "VALUE") else {
        return;
    };
    let Some(nested) = value.body.as_nested() else {
        return;
    };

    let mut written: Vec<(&str, &lcl_parser::syntax::Property)> = Vec::new();
    for statement in &nested.statements {
        if let Statement::Property(property) = statement {
            written.push((property.key.text.as_str(), property));
        }
    }

    // "No other keys or defaults are admitted": the written set must be the
    // registered set, neither short nor long.
    let admitted: Vec<&str> = shape.fields.iter().map(|(key, _)| key.as_str()).collect();
    let mut present: Vec<&str> = written.iter().map(|(key, _)| *key).collect();
    present.sort_unstable();
    present.dedup();
    let mut expected: Vec<&str> = admitted.clone();
    expected.sort_unstable();
    if present != expected {
        check.emit(
            StaticError::OperationParameter,
            source,
            nested.span,
            "closed_object_keys",
            format!(
                "`{}` admits exactly {} in the named parameter `{name}`; this one holds {}",
                contract.id,
                admitted.join(", "),
                match present.is_empty() {
                    true => "no key".to_string(),
                    false => present.join(", "),
                }
            ),
        );
        return;
    }

    for (key, declared) in &shape.fields {
        let Some((_, property)) = written.iter().find(|(written, _)| written == key) else {
            continue;
        };
        let Some(Value::Expression(lcl_parser::syntax::Expr::Literal(literal))) =
            property.body.as_inline()
        else {
            // Not a written literal, so not knowable at this stage.
            continue;
        };
        if !literal_is(literal.kind, declared) {
            check.emit(
                StaticError::OperationParameter,
                source,
                literal.span,
                "closed_object_type",
                format!("`{}` types `{name}.{key}` as {declared}", contract.id),
            );
            continue;
        }
        // A closed word list applies to the key the constraint names.
        if key == "unit" && !shape.units.is_empty() && !shape.units.contains(&literal.text) {
            check.emit(
                StaticError::OperationParameter,
                source,
                literal.span,
                "closed_object_word",
                format!(
                    "`{}` admits exactly {} as `{name}.{key}`; found {:?}",
                    contract.id,
                    shape.units.join(", "),
                    literal.text
                ),
            );
        }
    }
}

/// Whether one written literal satisfies a registered scalar type word.
fn literal_is(kind: lcl_parser::syntax::LiteralKind, declared: &str) -> bool {
    use lcl_parser::syntax::LiteralKind;
    match declared {
        "STRING" => matches!(kind, LiteralKind::String | LiteralKind::MultilineString),
        "INTEGER" => matches!(kind, LiteralKind::Integer),
        "DECIMAL" => matches!(kind, LiteralKind::Integer | LiteralKind::Decimal),
        "BOOLEAN" => matches!(kind, LiteralKind::True | LiteralKind::False),
        // A word this check does not model is not a word it refuses on.
        _ => true,
    }
}
