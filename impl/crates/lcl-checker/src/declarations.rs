//! The walk: every declaration, every field, every expression.
//!
//! `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt`: "Every expression,
//! including a short-circuited or unreachable expression, has its names, arity,
//! operand families, and receiving contract checked before effects." So this
//! walk visits every block in every loaded unit — both arms of every `IF`, the
//! body of every `FOR EACH`, and every nested block — and hands each field's
//! value to the expression checker with the receiving contract its registered
//! value kind states.
//!
//! ## Where a receiving type comes from
//!
//! `field_signatures_v0.1.0.json` gives every field a `value_kind`. M2 already
//! judged each value's *shape* against that kind; what is left is the type
//! contract, and it is read from the same registry rather than listed here: a
//! block that declares `TYPE` as a `type_expression` types its own
//! `value_expression` and `value_or_object_expression` fields.

use crate::expr::{Check, Const, Expected};
use crate::schema::{Schema, SchemaField};
use crate::ty::{ObjectField, ObjectType, Type};
use crate::types::{self, TypeCatalog};
use crate::{DemandKind, FieldConstraints, StaticError};
use lcl_lexer::Span;
use lcl_parser::syntax::{
    Block, Body, Conditional, Executable, Expr, ForEach, Nested, Statement, TopLevel, Value,
};
use lcl_resolver::SourceId;
use std::collections::BTreeMap;

/// Resolve every declaration's declared type, then check every expression.
pub(crate) fn check_program(check: &mut Check<'_>) {
    declared_types(check);
    schemas(check);
    constants(check);
    report_type_cycles(check);
    walk(check);
}

/// Every declaration's `TYPE`, resolved through the type catalog.
fn declared_types(check: &mut Check<'_>) {
    let declarations: Vec<(usize, SourceId, String)> = check
        .resolved
        .declarations()
        .all()
        .iter()
        .enumerate()
        .map(|(index, declaration)| (index, declaration.source.clone(), declaration.block.clone()))
        .collect();

    for (index, source, block) in declarations {
        if check.contracts.value_kind(&block, "TYPE") != Some("type_expression") {
            continue;
        }
        let Some(syntax) = types::declaration_block(check.resolved, index) else {
            continue;
        };
        let Some(field) = syntax.field("TYPE") else {
            continue;
        };
        let Some(expr) = types::inline_expression(&field.body) else {
            continue;
        };
        match check.catalog.resolve(&source, expr) {
            Ok(ty) => {
                check.declaration_types.insert(index, ty);
            }
            Err(defect) => check.type_defect(&source, field.body.span(), defect),
        }
    }
}

/// Every object schema, resolved: each direct `BASE OBJECT` definition's own,
/// then each declaration's, whether a `SCHEMA` states it or its `TYPE` names a
/// defined object type.
fn schemas(check: &mut Check<'_>) {
    let declarations: Vec<(usize, SourceId)> = check
        .resolved
        .declarations()
        .all()
        .iter()
        .enumerate()
        .map(|(index, declaration)| (index, declaration.source.clone()))
        .collect();

    // "A direct BASE OBJECT requires one or more FIELD and defines their exact
    // object schema." Built silently: the walk judges every FIELD block, with
    // its diagnostics, where it is written.
    for (index, source) in &declarations {
        let Some(syntax) = types::declaration_block(check.resolved, *index) else {
            continue;
        };
        let direct_object = syntax
            .field("BASE")
            .and_then(|field| types::inline_expression(&field.body))
            .is_some_and(is_direct_object);
        if !direct_object {
            continue;
        }
        let silent = check.silent;
        check.silent = true;
        let schema = local_schema(check, source, &syntax.body);
        check.silent = silent;
        if let Some(schema) = schema {
            check.schemas.insert(*index, schema);
        }
    }

    for (index, source) in declarations {
        let Some(syntax) = types::declaration_block(check.resolved, index) else {
            continue;
        };
        // The defined object type this declaration's `TYPE` names, if any.
        let selected = syntax
            .field("TYPE")
            .and_then(|field| types::inline_expression(&field.body))
            .and_then(|expr| nominal_object(check, &source, expr))
            .and_then(|defining| check.schemas.get(&defining).cloned());
        let Some(field) = syntax.field("SCHEMA") else {
            if let Some(selected) = selected {
                check.schemas.insert(index, selected);
            }
            continue;
        };
        // "A schema is either REF(identifier) to a defined OBJECT type or a
        // local SCHEMA block containing FIELD blocks."
        let schema = match &field.body {
            Body::Nested(nested) => local_schema(check, &source, &nested.statements),
            Body::Inline(Value::Expression(expr)) => match check.catalog.resolve(&source, expr) {
                Ok(Type::Object(object)) => Some(
                    nominal_object(check, &source, expr)
                        .and_then(|defining| check.schemas.get(&defining).cloned())
                        .unwrap_or_else(|| from_object_type(&object, &source, field.key.span)),
                ),
                Ok(_) => {
                    check.emit(
                        StaticError::ObjectSchema,
                        &source,
                        field.body.span(),
                        "schema_reference",
                        "a SCHEMA reference names a defined OBJECT type".to_string(),
                    );
                    None
                }
                Err(defect) => {
                    check.type_defect(&source, field.body.span(), defect);
                    None
                }
            },
            Body::Inline(_) => None,
        };
        let Some(schema) = schema else { continue };

        // "When TYPE already selects an object schema, an additional SCHEMA must
        // have an identical field/type/requiredness map and identical defaults
        // and constraints after alias resolution; otherwise error.object.schema."
        if let Some(Type::Object(declared)) = check.declaration_types.get(&index).cloned() {
            let identical = match &selected {
                // Both sides are full schemas: compare defaults and constraints
                // as well as the field/type/requiredness map.
                Some(selected) => schema.identical_to(selected),
                // The declared type is an object whose defining schema is not
                // available here, so the identity half is what can be compared.
                None => schema.object_type() == declared,
            };
            if !identical {
                check.emit(
                    StaticError::ObjectSchema,
                    &source,
                    field.body.span(),
                    "combined_schema",
                    "TYPE already selects an object schema; an additional SCHEMA must be identical"
                        .to_string(),
                );
                continue;
            }
        } else {
            check
                .declaration_types
                .insert(index, Type::Object(schema.object_type()));
        }
        check.schemas.insert(index, schema);
    }
}

/// `BASE: OBJECT`, written directly.
fn is_direct_object(expr: &Expr) -> bool {
    matches!(expr, Expr::Type(lcl_parser::syntax::TypeExpr::Scalar(word)) if word.text == "OBJECT")
}

/// The direct `BASE OBJECT` definition a type expression names, through
/// `OBJECT[...]` and any chain of transparent aliases.
///
/// Object types are structural, so two definitions with the same shape are the
/// same type and the type alone cannot say whose constraints apply. The written
/// reference can: "Transparent aliases preserve ... object schema".
fn nominal_object(check: &Check<'_>, source: &SourceId, expr: &Expr) -> Option<usize> {
    let mut source = source.clone();
    let mut expr = expr.clone();
    // Each step follows one BASE; a cyclic chain is the catalog's defect.
    for _ in 0..=check.resolved.declarations().all().len() {
        let reference = match &expr {
            Expr::Type(lcl_parser::syntax::TypeExpr::Object(bracket)) => bracket.argument.as_ref(),
            other => other,
        };
        let Expr::Call(call) = reference else {
            return None;
        };
        let identifier = call.reference_target()?;
        let declaration = check.catalog.binding(&source, identifier.span)?;
        let block = types::declaration_block(check.resolved, declaration)?;
        let base = types::inline_expression(&block.field("BASE")?.body)?;
        if is_direct_object(base) {
            return Some(declaration);
        }
        source = check
            .resolved
            .declarations()
            .get(declaration)?
            .source
            .clone();
        expr = base.clone();
    }
    None
}

/// A local `SCHEMA` block's `FIELD` children.
fn local_schema(
    check: &mut Check<'_>,
    source: &SourceId,
    statements: &[Statement],
) -> Option<Schema> {
    let mut fields: BTreeMap<String, SchemaField> = BTreeMap::new();
    for declared in crate::schema::schema_fields(statements) {
        let Some(name) = declared.name else { continue };
        let Some(type_expr) = declared.type_expr else {
            continue;
        };
        let ty = match check.catalog.resolve(source, type_expr) {
            Ok(ty) => ty,
            Err(defect) => {
                check.type_defect(source, type_expr.span(), defect);
                continue;
            }
        };
        // "FIELD.NAME is unique."
        if fields.contains_key(name) {
            check.emit(
                StaticError::ObjectSchema,
                source,
                declared.span,
                "duplicate_field",
                format!("`{name}` is declared more than once in this schema"),
            );
            continue;
        }
        fields.insert(
            name.to_string(),
            SchemaField {
                ty,
                required: declared.required.unwrap_or(true),
                default: declared.default.map(lcl_parser::syntax::render),
                constraints: declared.rendered_constraints(),
                declared: FieldConstraints {
                    source: source.clone(),
                    minimum: declared.minimum.cloned(),
                    maximum: declared.maximum.cloned(),
                    pattern: declared.pattern.cloned(),
                    schema: nominal_object(check, source, type_expr),
                },
                span: declared.span,
            },
        );
    }
    if fields.is_empty() {
        return None;
    }
    Some(Schema { fields })
}

fn from_object_type(object: &ObjectType, source: &SourceId, span: Span) -> Schema {
    Schema {
        fields: object
            .fields()
            .iter()
            .map(|(name, field)| {
                (
                    name.clone(),
                    SchemaField {
                        ty: field.ty.clone(),
                        required: field.required,
                        default: None,
                        constraints: Vec::new(),
                        declared: FieldConstraints {
                            source: source.clone(),
                            minimum: None,
                            maximum: None,
                            pattern: None,
                            schema: None,
                        },
                        span,
                    },
                )
            })
            .collect(),
    }
}

/// Statically known `DEFINE kind.constant` values.
///
/// A constant may be written in terms of an earlier or later constant, so this
/// runs to a fixed point. It emits nothing: every one of these expressions is
/// checked again, with diagnostics, by the walk.
fn constants(check: &mut Check<'_>) {
    let candidates: Vec<(usize, SourceId)> = check
        .resolved
        .declarations()
        .all()
        .iter()
        .enumerate()
        .filter(|(_, declaration)| {
            declaration.block == "DEFINE"
                && declaration.definition_kind.as_deref() == Some("kind.constant")
        })
        .map(|(index, declaration)| (index, declaration.source.clone()))
        .collect();

    check.silent = true;
    for _ in 0..=candidates.len() {
        let mut learned = false;
        for (index, source) in &candidates {
            if check.constants.contains_key(index) {
                continue;
            }
            let Some(syntax) = types::declaration_block(check.resolved, *index) else {
                continue;
            };
            let Some(field) = syntax.field("VALUE") else {
                continue;
            };
            let Some(expr) = types::inline_expression(&field.body) else {
                continue;
            };
            let expected = check
                .declaration_types
                .get(index)
                .cloned()
                .map(Expected::Type)
                .unwrap_or(Expected::None);
            let judgement = check.expression(source, expr, &expected);
            if let Some(value) = judgement.value {
                check.constants.insert(*index, value);
                learned = true;
            }
        }
        if !learned {
            break;
        }
    }
    check.silent = false;
    check.values.clear();
    check.annotations.clear();
}

/// `03_TYPES_AND_VALUES/01`: "cycles use error.reference.cycle".
fn report_type_cycles(check: &mut Check<'_>) {
    let cycles: Vec<(usize, Span)> = check.catalog.cycles().to_vec();
    for (declaration, span) in cycles {
        let Some(decl) = check.resolved.declarations().get(declaration) else {
            continue;
        };
        let source = decl.source.clone();
        let id = decl.id.clone();
        check.earlier_defect(
            &source,
            span,
            "error.reference.cycle",
            format!("the BASE chain of `{id}` resolves to itself"),
        );
    }
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

fn walk(check: &mut Check<'_>) {
    let units: Vec<SourceId> = check
        .resolved
        .units()
        .map(|unit| unit.id().clone())
        .collect();
    for source in units {
        let Some(document) = check
            .resolved
            .unit(&source)
            .and_then(|unit| unit.document())
        else {
            continue;
        };
        let items = document.items.clone();
        for item in &items {
            match item {
                TopLevel::Block(block) => block_of(check, &source, block),
                TopLevel::Conditional(conditional) => conditional_of(check, &source, conditional),
                TopLevel::ForEach(for_each) => for_each_of(check, &source, for_each),
            }
        }
    }
}

fn conditional_of(check: &mut Check<'_>, source: &SourceId, conditional: &Conditional) {
    // Both arms are checked; neither condition is evaluated.
    check.expression(source, &conditional.condition, &Expected::Boolean);
    for item in &conditional.then_body {
        executable_of(check, source, item);
    }
    if let Some(arm) = &conditional.else_body {
        for item in &arm.body {
            executable_of(check, source, item);
        }
    }
}

/// `FOR EACH`, including the direct-iteration order contract of a `SET`.
fn for_each_of(check: &mut Check<'_>, source: &SourceId, for_each: &ForEach) {
    let collection = check.expression(source, &for_each.collection, &Expected::None);
    let element = match collection.ty() {
        Some(Type::List(member)) => Some(member.as_ref().clone()),
        Some(Type::Set(member)) => {
            let member = member.as_ref().clone();
            // `03_TYPES_AND_VALUES/01`: "Direct FOR EACH over such a SET
            // produces error.type.mismatch before that iteration begins or
            // produces effects; core.sort may instead produce the LIST required
            // for iteration."
            if !check.contracts.is_ordered_family(member.family()) {
                check.emit(
                    StaticError::TypeMismatch,
                    source,
                    for_each.collection.span(),
                    "set_iteration_order",
                    format!(
                        "SET[{member}] has no registered total order, so direct FOR EACH is invalid; pass the SET to core.sort and iterate the returned LIST"
                    ),
                );
                None
            } else {
                // Members are order-compatible by family; whether the actual
                // members are mutually order-compatible is a demanded question.
                check.defer(
                    source,
                    for_each.collection.span(),
                    DemandKind::SetMemberOrder,
                    "direct SET iteration checks actual-member order compatibility".to_string(),
                );
                Some(member)
            }
        }
        Some(other) => {
            check.emit(
                StaticError::TypeMismatch,
                source,
                for_each.collection.span(),
                "iteration_family",
                format!("FOR EACH iterates a LIST or SET, not {other}"),
            );
            None
        }
        None => None,
    };

    let depth = check.locals.len();
    if let Some(element) = element {
        check.locals.push((for_each.binding.text.clone(), element));
    }
    for item in &for_each.body {
        executable_of(check, source, item);
    }
    check.locals.truncate(depth);
}

fn executable_of(check: &mut Check<'_>, source: &SourceId, item: &Executable) {
    match item {
        Executable::Block(block) => block_of(check, source, block),
        Executable::Conditional(conditional) => conditional_of(check, source, conditional),
        Executable::ForEach(for_each) => for_each_of(check, source, for_each),
    }
}

fn block_of(check: &mut Check<'_>, source: &SourceId, block: &Block) {
    let name = block.key.text.clone();
    let declaration = check
        .resolved
        .declarations()
        .by_span(source, block.key.span.start)
        .copied();
    let declared = declaration.and_then(|index| check.declaration_types.get(&index).cloned());

    let enum_contexts = crate::operation::invocation(check, source, block, &name);

    for statement in &block.body {
        match statement {
            Statement::Field(field) => {
                if let Some(context) = enum_contexts.get(&field.key.span) {
                    contextual_parameter(check, source, field, context);
                } else {
                    field_of(check, source, &name, field, declared.as_ref(), declaration);
                }
            }
            Statement::Property(property) => {
                // A lowercase key outside object data is `error.field.forbidden`
                // at the grammar stage; inside object data it is judged by its
                // receiving object type.
                if let Body::Inline(Value::Expression(expr)) = &property.body {
                    check.expression(source, expr, &Expected::None);
                }
            }
            Statement::Conditional(conditional) => conditional_of(check, source, conditional),
            Statement::ForEach(for_each) => for_each_of(check, source, for_each),
        }
    }

    // Constraints are judged after the values they constrain.
    declared_constraints(check, source, &block.body);
}

/// The operation has resolved this PARAMETER's written bare ENUM to one exact
/// registry domain. Record that TYPE and check every other field through the
/// ordinary receiving-type walk, including VALUE and declared constraints.
fn contextual_parameter(
    check: &mut Check<'_>,
    source: &SourceId,
    parameter: &lcl_parser::syntax::Field,
    context: &Type,
) {
    let Some(nested) = parameter.body.as_nested() else {
        return;
    };
    for statement in &nested.statements {
        match statement {
            Statement::Field(field) if field.key.text == "TYPE" => {
                if let Some(expr) = types::inline_expression(&field.body) {
                    check.record_designator(source, expr.span(), context.clone());
                }
            }
            Statement::Field(field) => {
                field_of(check, source, "PARAMETER", field, Some(context), None);
            }
            Statement::Property(_) => {}
            Statement::Conditional(conditional) => conditional_of(check, source, conditional),
            Statement::ForEach(for_each) => for_each_of(check, source, for_each),
        }
    }
    declared_constraints(check, source, &nested.statements);
}

fn field_of(
    check: &mut Check<'_>,
    source: &SourceId,
    block: &str,
    field: &lcl_parser::syntax::Field,
    declared: Option<&Type>,
    declaration: Option<usize>,
) {
    let key = field.key.text.clone();
    let kind = check
        .contracts
        .value_kind(block, &key)
        .unwrap_or_default()
        .to_string();

    // A nested body is a child block, a local schema, or object data.
    if let Body::Nested(nested) = &field.body {
        nested_of(
            check,
            source,
            block,
            &key,
            &kind,
            nested,
            declared,
            declaration,
        );
        return;
    }
    let Some(value) = field.body.as_inline() else {
        return;
    };

    // `SIDE_EFFECT` and `DEPENDENCY` declare closed axis classes, not material
    // values: "one LIST of one or more distinct concrete effect classes drawn
    // from …/enum_groups/effect_classes".
    if let Some(group) = axis_group(&kind) {
        axis_declaration(check, source, &key, &kind, group, field);
        return;
    }

    // A definition's own `BASE`. The catalog resolved it for `kind.type` —
    // including the direct `OBJECT` and `ENUM` bases, which are legal there and
    // nowhere else — and M3 resolved the alias domains. Judging it again as an
    // ordinary type expression would contradict both, so the walk records what
    // it resolves to and adds no verdict of its own.
    if kind == "type_or_format_base" {
        if let Some(expr) = types::inline_expression(&field.body) {
            if let Ok(ty) = check.catalog.resolve(source, expr) {
                check.record_designator(source, expr.span(), ty);
            }
        }
        return;
    }

    let expected = receiving_contract(check, block, &key, &kind, declared);
    let relative_path = matches!(
        (block, key.as_str()),
        ("IMPORT", "SOURCE") | ("EXTENSION", "SOURCE")
    );
    let previous = check.relative_path_allowed;
    check.relative_path_allowed = relative_path;

    match value {
        Value::Expression(expr) => {
            let judgement = check.expression(source, expr, &expected);
            constrain(check, source, &kind, &key, expr.span(), &judgement);
        }
        Value::MultilineCollection(collection) => {
            let expr = Expr::Collection(collection.clone());
            check.expression(source, &expr, &expected);
        }
    }
    check.relative_path_allowed = previous;
}

/// The receiving contract one field's registered value kind states.
fn receiving_contract(
    check: &Check<'_>,
    block: &str,
    key: &str,
    kind: &str,
    declared: Option<&Type>,
) -> Expected {
    // `DEFAULT` receives the declaring block's declared type, and admits the
    // literal MISSING it exists to replace.
    if key == "DEFAULT" {
        return match declared {
            Some(ty) => Expected::Default(ty.clone()),
            None => Expected::None,
        };
    }

    // A field whose own block declares a TYPE receives that exact type.
    let typed_by_declaration = matches!(kind, "value_expression" | "value_or_object_expression")
        && check.contracts.value_kind(block, "TYPE") == Some("type_expression");
    if typed_by_declaration {
        if let Some(ty) = declared {
            return Expected::Type(ty.clone());
        }
        return Expected::None;
    }
    if kind == "type_expression" {
        return Expected::TypeExpression;
    }
    // `type_or_format_base` is a definition's own BASE. For `kind.type` the
    // catalog already resolved it — including the direct `OBJECT` and `ENUM`
    // bases, which are legal there and nowhere else — and for the alias domains
    // M3 resolved it. Re-checking it here as an ordinary type expression would
    // contradict both.
    if kind == "type_or_format_base" {
        return Expected::None;
    }
    if kind == "boolean_expression" {
        return Expected::Boolean;
    }
    if kind == "boolean" {
        return Expected::Type(Type::Boolean);
    }
    if kind == "duration" {
        return Expected::Type(Type::Duration);
    }
    if kind.starts_with("reference") || kind == "operation_identifier_or_handler_reference" {
        return Expected::Identity;
    }
    if let Some(domain) = kind
        .strip_prefix("qualified_identifier(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return Expected::Identifier(domain.to_string());
    }
    // `DEFAULT` receives the declaring block's own declared type.
    Expected::None
}

/// Value kinds whose contract narrows a value beyond its shape.
fn constrain(
    check: &mut Check<'_>,
    source: &SourceId,
    kind: &str,
    key: &str,
    span: Span,
    judgement: &crate::expr::Judgement,
) {
    let Some(ty) = judgement.ty() else { return };
    match kind {
        // "ordered_value: INTEGER, DECIMAL, STRING, DATE, TIME, DATETIME,
        // DURATION, BYTES, PERCENTAGE, or same-unit MEASURE."
        "ordered_value" => {
            if !check.contracts.is_ordered_family(ty.family()) {
                check.emit(
                    StaticError::TypeMismatch,
                    source,
                    span,
                    "ordered_value",
                    format!("`{key}` takes a value of a registered ordered type, not {ty}"),
                );
            }
        }
        // "An INTEGER or DECIMAL greater than or equal to zero, or a MEASURE
        // whose numeric component is greater than or equal to zero."
        "nonnegative_numeric_or_measure" => {
            if !(ty.is_numeric() || matches!(ty, Type::Measure(_))) {
                check.emit(
                    StaticError::TypeMismatch,
                    source,
                    span,
                    "nonnegative_numeric",
                    format!("`{key}` takes a non-negative number or MEASURE, not {ty}"),
                );
                return;
            }
            match judgement.value.as_ref().and_then(Const::number) {
                Some(value) if value.is_negative() => check.emit(
                    StaticError::ValueOutOfRange,
                    source,
                    span,
                    "nonnegative_numeric",
                    format!("`{key}` takes a non-negative value"),
                ),
                Some(_) => {}
                None => check.defer(
                    source,
                    span,
                    DemandKind::DeclaredBound,
                    format!("`{key}` requires a non-negative value"),
                ),
            }
        }
        // "PATTERN is GLOB or REGEX and does not silently coerce the target to
        // STRING."
        "regex_or_glob" => {
            if !matches!(ty, Type::Regex | Type::Glob) {
                check.emit(
                    StaticError::TypeMismatch,
                    source,
                    span,
                    "pattern_value",
                    format!("`{key}` takes a REGEX or GLOB value, not {ty}"),
                );
            }
        }
        // "source_expression: PATH, URI, or REFERENCE to declared source data."
        "source_expression" if !matches!(ty, Type::Path | Type::Uri | Type::Reference(_)) => {
            check.emit(
                StaticError::TypeMismatch,
                source,
                span,
                "source_expression",
                format!("`{key}` takes a PATH, URI or reference, not {ty}"),
            );
        }
        _ => {}
    }
}

/// A block's declared value constraints, applied to a statically known value.
///
/// `03_TYPES_AND_VALUES/07`: "MINIMUM and MAXIMUM are inclusive. … PATTERN is
/// GLOB or REGEX and does not silently coerce the target to STRING." Only a
/// statically known value is judged; anything else is the demanding layer's.
/// The first direct field spelled `name`, among these statements.
///
/// The same lookup `Block::field` performs, over statements that are not
/// wrapped in a block. A nested body is a block's body without being a block,
/// and copying one into a synthetic `Block` to reuse that method costs a deep
/// copy of the whole subtree at every level.
pub(crate) fn field_in<'a>(
    statements: &'a [Statement],
    name: &str,
) -> Option<&'a lcl_parser::syntax::Field> {
    statements.iter().find_map(|statement| match statement {
        Statement::Field(field) if field.key.text == name => Some(field),
        _ => None,
    })
}

fn declared_constraints(check: &mut Check<'_>, source: &SourceId, block: &[Statement]) {
    let value = field_in(block, "VALUE").and_then(|field| types::inline_expression(&field.body));
    let (subject, demanded) = match value {
        Some(value) => (value, true),
        None => {
            let Some(default) =
                field_in(block, "DEFAULT").and_then(|field| types::inline_expression(&field.body))
            else {
                return;
            };
            (default, false)
        }
    };
    let bound = |key: &str| field_in(block, key).and_then(|f| types::inline_expression(&f.body));
    let declared = FieldConstraints {
        source: source.clone(),
        minimum: bound("MINIMUM").cloned(),
        maximum: bound("MAXIMUM").cloned(),
        pattern: bound("PATTERN").cloned(),
        schema: None,
    };
    judge_constraints(check, source, subject.span(), &declared, demanded);
}

/// Judge one value against its declared constraints when the value and the
/// constraint are statically known, and hand the obligation to the demanding
/// layer otherwise.
///
/// `demanded` says whether a later layer constructs this value and so consumes
/// an obligation for a value this stage cannot know. A written `DEFAULT` is
/// not constructed by any later layer, so an unknown default is not deferred.
fn judge_constraints(
    check: &mut Check<'_>,
    source: &SourceId,
    subject: Span,
    declared: &FieldConstraints,
    demanded: bool,
) {
    let Some(value) = check.value_at(source, subject) else {
        if demanded {
            if declared.minimum.is_some() || declared.maximum.is_some() {
                check.defer(
                    source,
                    subject,
                    DemandKind::DeclaredBound,
                    "a declared bound constrains this value".to_string(),
                );
            }
            if declared.pattern.is_some() {
                check.defer(
                    source,
                    subject,
                    DemandKind::DeclaredPattern,
                    "a declared PATTERN constrains this value".to_string(),
                );
            }
        }
        return;
    };

    for (key, bound_expr, minimum) in [
        ("MINIMUM", &declared.minimum, true),
        ("MAXIMUM", &declared.maximum, false),
    ] {
        let Some(bound_expr) = bound_expr else {
            continue;
        };
        let (Some(actual), Some(bound)) = (
            value.number(),
            constraint_value(check, &declared.source, bound_expr)
                .as_ref()
                .and_then(Const::number)
                .cloned(),
        ) else {
            check.defer(
                source,
                subject,
                DemandKind::DeclaredBound,
                format!("`{key}` bounds this value"),
            );
            continue;
        };
        let ordering = actual.compare(&bound);
        let violated = if minimum {
            ordering == std::cmp::Ordering::Less
        } else {
            ordering == std::cmp::Ordering::Greater
        };
        if violated {
            check.emit(
                StaticError::ValueOutOfRange,
                source,
                subject,
                "declared_bound",
                format!(
                    "this value violates the inclusive declared {}",
                    key.to_lowercase()
                ),
            );
        }
    }

    // "PATTERN is GLOB or REGEX and does not silently coerce the target to
    // STRING."
    let Some(pattern_expr) = &declared.pattern else {
        return;
    };
    let Some(Const::Pattern {
        kind,
        pattern,
        flags,
    }) = constraint_value(check, &declared.source, pattern_expr)
    else {
        check.defer(
            source,
            subject,
            DemandKind::DeclaredPattern,
            "a declared PATTERN constrains this value".to_string(),
        );
        return;
    };
    let Some(text) = value.text() else {
        check.defer(
            source,
            subject,
            DemandKind::DeclaredPattern,
            "a declared PATTERN constrains this value".to_string(),
        );
        return;
    };
    match crate::pattern::compile(kind, &pattern, &flags)
        .and_then(|compiled| compiled.matches(text))
    {
        Ok(true) => {}
        Ok(false) => check.emit(
            StaticError::PatternMismatch,
            source,
            subject,
            "declared_pattern",
            "this value does not match its declared pattern".to_string(),
        ),
        Err(crate::pattern::PatternError::ResourceLimit) => check.emit(
            StaticError::PatternResourceLimit,
            &declared.source,
            pattern_expr.span(),
            "pattern_resource_limit",
            "this pattern exhausts the declared finite pattern-resource limit".to_string(),
        ),
        // Malformed pattern text is `error.literal.invalid` at the lexical
        // stage, which M1 already judged for every source literal.
        Err(crate::pattern::PatternError::Malformed) => {}
    }
}

/// The statically known value of one constraint expression.
///
/// A block's own constraints were judged just before, so their value is
/// recorded. An object field's constraints are written in the schema's
/// definition, which the walk may not have reached yet; they are judged
/// silently here, and judged again with diagnostics where they are written.
fn constraint_value(check: &mut Check<'_>, source: &SourceId, expr: &Expr) -> Option<Const> {
    if let Some(value) = check.value_at(source, expr.span()) {
        return Some(value);
    }
    let silent = check.silent;
    check.silent = true;
    let judgement = check.expression(source, expr, &Expected::None);
    check.silent = silent;
    judgement.value
}

/// The closed enum group one axis-declaration value kind draws from.
fn axis_group(kind: &str) -> Option<&'static str> {
    match kind {
        "side_effect_declaration" => Some("effect_classes"),
        "dependency_class_list" => Some("dependency_classes"),
        _ => None,
    }
}

/// `SIDE_EFFECT` and `DEPENDENCY`: a closed list of distinct registered
/// classes, or — for `SIDE_EFFECT` alone — exactly the BOOLEAN `FALSE`.
fn axis_declaration(
    check: &mut Check<'_>,
    source: &SourceId,
    key: &str,
    kind: &str,
    group: &'static str,
    field: &lcl_parser::syntax::Field,
) {
    let members: Vec<(String, Span)> = match field.body.as_inline() {
        // "SIDE_EFFECT FALSE declares possible effects exactly {none}."
        Some(Value::Expression(Expr::Literal(literal)))
            if literal.kind == lcl_parser::syntax::LiteralKind::False
                && kind == "side_effect_declaration" =>
        {
            return
        }
        Some(Value::Expression(Expr::Collection(collection))) => collection
            .members
            .iter()
            .map(|member| (identifier_text(member), member.span()))
            .collect(),
        Some(Value::MultilineCollection(collection)) => collection
            .members
            .iter()
            .map(|member| (identifier_text(member), member.span()))
            .collect(),
        _ => return,
    };

    if members.is_empty() {
        check.emit(
            StaticError::TypeMismatch,
            source,
            field.body.span(),
            "axis_declaration",
            format!("`{key}` takes one or more distinct registered classes"),
        );
        return;
    }
    let mut seen: Vec<String> = Vec::new();
    for (name, span) in members {
        if name.is_empty() {
            continue;
        }
        if !check.contracts.is_enum_group_member(group, &name) {
            check.emit(
                StaticError::TypeMismatch,
                source,
                span,
                "axis_declaration",
                format!("`{name}` is not a registered {group} member"),
            );
            continue;
        }
        // "one LIST of one or more distinct … classes"
        if seen.contains(&name) {
            check.emit(
                StaticError::TypeMismatch,
                source,
                span,
                "axis_declaration",
                format!("`{key}` lists `{name}` more than once"),
            );
            continue;
        }
        seen.push(name);
    }
}

/// A bare lowercase identifier's text, or the empty string.
fn identifier_text(expr: &Expr) -> String {
    match expr {
        Expr::Identifier(ident) => ident.text.clone(),
        _ => String::new(),
    }
}

/// A nested body: a child block, a local schema, or object data.
#[allow(clippy::too_many_arguments)]
fn nested_of(
    check: &mut Check<'_>,
    source: &SourceId,
    block: &str,
    key: &str,
    kind: &str,
    nested: &Nested,
    declared: Option<&Type>,
    declaration: Option<usize>,
) {
    // `nested_block(NAME)` names a child block; M2 already validated it as one.
    if let Some(child) = kind
        .strip_prefix("nested_block(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        // The child's own declared type, when it declares one. The statements
        // are read where they are: building a `Block` around a copy of them
        // would deep-copy the whole subtree at every level of a nested body,
        // which is quadratic in the document and recursive in the stack.
        let child_declared = child_declared_type(check, source, child, &nested.statements);
        for statement in &nested.statements {
            match statement {
                Statement::Field(field) => {
                    field_of(check, source, child, field, child_declared.as_ref(), None)
                }
                Statement::Property(_) => {}
                Statement::Conditional(conditional) => conditional_of(check, source, conditional),
                Statement::ForEach(for_each) => for_each_of(check, source, for_each),
            }
        }
        // A child block declares its own constraints — `FIELD` and `PARAMETER`
        // both carry MINIMUM, MAXIMUM and PATTERN — so they are judged here too.
        declared_constraints(check, source, &nested.statements);
        return;
    }

    // A local `SCHEMA` was already resolved with its declaration.
    if kind == "schema_reference_or_nested_schema" {
        for declared_field in crate::schema::schema_fields(&nested.statements) {
            if let Some(type_expr) = declared_field.type_expr {
                check.expression(source, type_expr, &Expected::TypeExpression);
            }
        }
        return;
    }

    // Otherwise this is object data.
    let schema = declaration.and_then(|index| check.schemas.get(&index).cloned());
    object_data(check, source, nested, declared, schema.as_ref(), block, key);
}

fn child_declared_type(
    check: &mut Check<'_>,
    source: &SourceId,
    child: &str,
    block: &[Statement],
) -> Option<Type> {
    if check.contracts.value_kind(child, "TYPE") != Some("type_expression") {
        return None;
    }
    let field = field_in(block, "TYPE")?;
    let expr = types::inline_expression(&field.body)?;
    check.catalog.resolve(source, expr).ok()
}

/// One indented object body still to be judged, and what it is judged against.
///
/// A frame rather than a stack frame: see [`object_data`].
struct ObjectFrame<'a> {
    nested: &'a Nested,
    declared: Option<Type>,
    schema: Option<Schema>,
    key: String,
}

/// An indented object value, against its schema when it has one.
///
/// ## Why this is a worklist and not a recursive walk
///
/// `04_GRAMMAR/12` puts an object in an indented `VALUE` body, and an object
/// field's value may be another indented body, with no declared depth limit.
/// Judging one by calling this function again cost a native frame per level,
/// and past roughly a thousand levels on a small stack the process aborted
/// instead of returning a diagnostic. That is the defect
/// `LCL_RELEASE_REPORT.md` section 10 recorded as a 512-level bound.
///
/// So a nested field's body is pushed onto an explicit worklist and judged on
/// the next turn of this loop. Depth costs heap, exactly as it does in M2's
/// parser and in the expression walk beside this one.
///
/// Emission order changes: a parent's own diagnostics are all emitted before
/// its children's, where recursion interleaved them. That is not observable.
/// `stable_order` sorts every static diagnostic by source then byte offset
/// before anything reads them, which `crate::diagnostic` applies to the whole
/// run.
fn object_data(
    check: &mut Check<'_>,
    source: &SourceId,
    nested: &Nested,
    declared: Option<&Type>,
    schema: Option<&Schema>,
    block: &str,
    key: &str,
) {
    let mut pending = vec![ObjectFrame {
        nested,
        declared: declared.cloned(),
        schema: schema.cloned(),
        key: key.to_string(),
    }];
    while let Some(frame) = pending.pop() {
        object_body(check, source, &frame, block, &mut pending);
    }
}

/// Judge one object body, queueing the indented values inside it.
fn object_body<'a>(
    check: &mut Check<'_>,
    source: &SourceId,
    frame: &ObjectFrame<'a>,
    block: &str,
    pending: &mut Vec<ObjectFrame<'a>>,
) {
    let key = frame.key.as_str();
    let expected_object = match frame.declared.as_ref() {
        Some(Type::Object(object)) => Some(object.clone()),
        _ => None,
    };
    let mut present: BTreeMap<String, Type> = BTreeMap::new();
    let mut children: Vec<ObjectFrame<'a>> = Vec::new();

    for statement in &frame.nested.statements {
        let Statement::Property(property) = statement else {
            continue;
        };
        let name = property.key.text.clone();
        let field_type = expected_object
            .as_ref()
            .and_then(|object| object.field(&name).map(|field| field.ty.clone()));

        // "A field outside a closed schema uses error.operator.operand."
        if expected_object.is_some() && field_type.is_none() {
            check.emit(
                StaticError::ObjectSchema,
                source,
                property.key.span,
                "undeclared_field",
                format!("`{name}` is not declared by this object schema"),
            );
            continue;
        }

        let expectation = field_type
            .clone()
            .map(Expected::Type)
            .unwrap_or(Expected::None);
        let declared = frame
            .schema
            .as_ref()
            .and_then(|schema| schema.fields.get(&name))
            .map(|field| field.declared.clone());
        match &property.body {
            Body::Inline(Value::Expression(expr)) => {
                let judgement = check.expression(source, expr, &expectation);
                // "Defaults and constraints govern construction/validation".
                if let Some(declared) = declared.as_ref().filter(|d| !d.is_empty()) {
                    if judgement.ty().is_some() {
                        judge_constraints(check, source, expr.span(), declared, true);
                    }
                }
                // A field the source wrote is present whatever its value's type
                // turned out to be: reporting it absent as well would report one
                // defect twice.
                present.insert(name, judgement.ty().cloned().unwrap_or(Type::ObjectFamily));
            }
            Body::Inline(Value::MultilineCollection(collection)) => {
                let expr = Expr::Collection(collection.clone());
                let judgement = check.expression(source, &expr, &expectation);
                present.insert(name, judgement.ty().cloned().unwrap_or(Type::ObjectFamily));
            }
            Body::Nested(inner) => {
                let inner_declared = field_type.clone();
                children.push(ObjectFrame {
                    nested: inner,
                    declared: inner_declared.clone(),
                    schema: declared
                        .and_then(|declared| declared.schema)
                        .and_then(|defining| check.schemas.get(&defining).cloned()),
                    key: name.clone(),
                });
                if let Some(ty) = inner_declared {
                    present.insert(name, ty);
                }
            }
        }
    }

    // "When SCHEMA is present, every required schema field must occur exactly
    // once and no undeclared field is legal."
    if let Some(object) = &expected_object {
        for (name, field) in object.fields() {
            if field.required && !present.contains_key(name) {
                check.emit(
                    StaticError::ObjectSchema,
                    source,
                    frame.nested.span,
                    "required_field",
                    format!("`{block}.{key}` omits the required schema field `{name}`"),
                );
            }
        }
    } else if frame.schema.is_none() {
        // "A schema-free object infers each present field's exact static type
        // from its value", all required.
        let inferred = ObjectType::new(
            present
                .into_iter()
                .map(|(name, ty)| (name, ObjectField { ty, required: true }))
                .collect(),
        );
        let _ = inferred;
    }

    // Popped, so pushed in reverse to keep source order.
    pending.extend(children.into_iter().rev());
}

/// The catalog every pass shares.
pub(crate) fn catalog(resolved: &lcl_resolver::Resolved) -> TypeCatalog {
    TypeCatalog::build(resolved)
}
