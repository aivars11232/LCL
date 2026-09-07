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
use crate::expr::Check;
use crate::StaticError;
use lcl_parser::syntax::{Block, Statement, Value};
use lcl_resolver::SourceId;
use std::collections::BTreeMap;

/// The three invocation sites. "ACTION, HANDLER, and FALLBACK are the only
/// invocation sites; every required target and named parameter rule applies
/// identically on each of them."
const INVOCATION_BLOCKS: [&str; 2] = ["ACTION", "HANDLER"];

/// Check one block as an invocation site, when it is one.
pub(crate) fn invocation(check: &mut Check<'_>, source: &SourceId, block: &Block, name: &str) {
    if !INVOCATION_BLOCKS.contains(&name) {
        return;
    }
    let Some(field) = block.field("OPERATION") else {
        return;
    };
    let Some(identifier) = operation_identifier(field) else {
        return;
    };
    // A custom `DEFINE kind.operation` declares its own contract in LCL; the
    // closed core contracts are the registry's.
    let Some(contract) = check.contracts.operation(&identifier).cloned() else {
        return;
    };

    let locus = block.key.span;
    parameters(check, source, block, &contract, locus);
    target(check, source, block, &contract, name, locus);
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
