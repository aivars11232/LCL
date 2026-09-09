//! Expression fragments: the STRING forms of `core.calculate`, `core.select`
//! and `core.filter`.
//!
//! Authority: `operations_v0.1.0.json#/expression_fragment_contract`.
//!
//! ## One expression, from the one grammar
//!
//! > After STRING decoding, consume exactly one EXPRESSION from
//! > `04_GRAMMAR/10_COMPLETE_EBNF.ebnf`, followed only by optional whitespace.
//! > No alternate expression language, declaration, statement, or operation
//! > invocation is admitted.
//!
//! So a fragment is lexed by the real lexer and parsed by the real parser
//! through [`lcl_parser::Parser::expression`], and evaluated by the real
//! evaluator. Nothing in this module interprets an expression; it arranges the
//! environment the contract describes and hands the work to the layers that
//! already own it.
//!
//! ## The fragment is its own source
//!
//! A fragment's spans index the fragment's own bytes, not the document's, so it
//! is evaluated under its own [`SourceId`]. That is not bookkeeping — it decides
//! meaning twice, and both times in the direction the contract states:
//!
//! * no static annotation matches a fragment span, so a bracket literal is a
//!   `LIST`, which is what `collection_expression/default_family` gives an
//!   expression with no immediate expected type; and
//! * no identity annotation matches either, so `REF(x)` inside a fragment
//!   *reads* `x` — "REF references resolve in the enclosing document and read
//!   values" — instead of retaining a reference identity.
//!
//! ## What a bare name may be
//!
//! > Bare names in fragments denote only declared local bindings, reserved
//! > target or item where provided, or contextual enum/qualified-identifier
//! > data. They never implicitly read a document declaration; use REF for that
//! > value read.
//!
//! The environment is therefore built by *removing* the document's loop locals
//! and adding only the fragment's own bindings and reserved names. Bound
//! OUTPUTs and written stores stay reachable, because `REF` must still read
//! them, and a bare name still cannot: a bare name only ever consults locals.

use lcl_lexer::Lexicon;
use lcl_parser::{FragmentError, Grammar, Parser};
use lcl_resolver::SourceId;
use lcl_runtime::diagnostic::RuntimeError;
use lcl_runtime::eval::{Evaluator, Fault};
use lcl_runtime::operations::Invocation;
use lcl_runtime::state::IterationPath;
use lcl_runtime::Value;
use std::collections::BTreeMap;

/// The identity a fragment's spans belong to.
///
/// Deliberately not a document name: nothing may match a document annotation by
/// accident, and a diagnostic that escapes with this identity is visibly about
/// a fragment rather than about a line of the source.
pub fn fragment_source(operation: &str, parameter: &str) -> SourceId {
    SourceId::new(format!("<{operation} {parameter}>"))
}

/// Why one fragment did not produce a value.
#[derive(Debug, Clone)]
pub enum FragmentFault {
    /// The fragment is not exactly one expression, or a binding name is
    /// invalid. "Malformed fragment syntax and invalid bindings produce
    /// error.operation.parameter."
    Malformed(String),
    /// The fragment is one expression, and evaluating it raised an ordinary
    /// expression diagnostic. "Valid fragments retain the normal expression
    /// diagnostics."
    Demand(Fault),
}

impl FragmentFault {
    /// The registered identifier this fault selects, within `contract`'s row.
    ///
    /// ## Why a malformed fragment has no single answer here
    ///
    /// `expression_fragment_contract/diagnostics` assigns malformed syntax and
    /// invalid bindings to `error.operation.parameter`, and that identifier's
    /// registered stage is `static_or_expression`, not `execution`:
    /// "Static checks cover the complete fragment; dynamic errors arise only
    /// from evaluated subexpressions." A fragment is a literal or a
    /// `kind.constant` STRING in every registered form, so its syntax is
    /// knowable statically and a malformed one should never reach execution.
    ///
    /// This runtime cannot emit a static-stage identifier without breaking
    /// stage monotonicity, so the malformed arm is totality rather than
    /// behavior: it selects an identifier the row itself lists — the execution
    /// stage's `error.operation.precondition` where the row admits it, and
    /// otherwise `error.operator.operand`, which every fragment-taking row
    /// admits. No row ever receives an identifier outside its own contract.
    pub fn error(&self, contract: &crate::contracts::OperationContract) -> RuntimeError {
        match self {
            FragmentFault::Malformed(_) => {
                if contract.admits_error("error.operation.precondition") {
                    RuntimeError::OperationPrecondition
                } else {
                    RuntimeError::OperatorOperand
                }
            }
            FragmentFault::Demand(fault) => fault.id,
        }
    }

    pub fn detail(&self) -> String {
        match self {
            FragmentFault::Malformed(detail) => detail.clone(),
            FragmentFault::Demand(fault) => fault.detail.clone(),
        }
    }

    pub fn cause(&self) -> String {
        match self {
            FragmentFault::Malformed(_) => "fragment".to_string(),
            FragmentFault::Demand(fault) => fault.cause.clone(),
        }
    }
}

/// One fragment's evaluation environment.
#[derive(Debug, Default)]
pub struct Environment<'a> {
    /// The reserved bindings this call site supplies, such as `target` for
    /// `core.calculate` or `item` for a predicate.
    pub reserved: Vec<(&'a str, Value)>,
    /// The declared `bindings` OBJECT, when the operation takes one.
    pub bindings: BTreeMap<String, Value>,
}

impl Environment<'_> {
    /// An environment with no reserved names and no declared bindings.
    pub fn new() -> Environment<'static> {
        Environment::default()
    }
}

/// Every name a fragment may not bind, beyond the reserved ones.
///
/// > keys must not collide with any visible declaration ID or its simple name,
/// > any registered reserved word, or target.
pub fn invalid_binding_name(
    cx: &Invocation<'_>,
    name: &str,
    lexicon: &Lexicon,
    reserved: &[&str],
) -> Option<String> {
    if reserved.contains(&name) {
        return Some(format!("{name} is a reserved fragment binding"));
    }
    if lexicon.is_reserved_word(name) {
        return Some(format!("{name} is a registered reserved word"));
    }
    for declaration in cx.resolved.declarations().all() {
        let qualified = declaration.id.qualified();
        if qualified == name {
            return Some(format!("{name} is a visible declaration ID"));
        }
        if let Some((_, simple)) = qualified.rsplit_once('.') {
            if simple == name {
                return Some(format!(
                    "{name} is the simple name of the visible declaration {qualified}"
                ));
            }
        }
    }
    None
}

/// Parse and evaluate one fragment in one environment.
pub fn evaluate(
    cx: &Invocation<'_>,
    lexicon: &Lexicon,
    grammar: &Grammar,
    source: &SourceId,
    fragment: &str,
    environment: &Environment<'_>,
) -> Result<Value, FragmentFault> {
    let expression = Parser::new(grammar)
        .expression_fragment(lexicon, fragment)
        .map_err(|error| FragmentFault::Malformed(describe(&error, fragment)))?;

    // The document's loop locals are not visible to a bare name. Bound OUTPUTs
    // and written stores stay, because REF must still read them.
    let mut bindings = cx.bindings.clone();
    bindings.release_locals(&IterationPath::root());
    for (name, value) in &environment.reserved {
        bindings.bind_local(name, &cx.iteration, value.clone());
    }
    for (name, value) in &environment.bindings {
        bindings.bind_local(name, &cx.iteration, value.clone());
    }

    let evaluator = Evaluator {
        contracts: cx.contracts,
        resolved: cx.resolved,
        checked: cx.checked,
        plan: cx.plan,
        bindings: &bindings,
        source: source.clone(),
        iteration: cx.iteration.clone(),
    };
    evaluator.demand(&expression).map_err(FragmentFault::Demand)
}

fn describe(error: &FragmentError, fragment: &str) -> String {
    match error {
        FragmentError::Trailing(span) => format!(
            "{fragment:?} holds more than one expression; input remains at byte {}",
            span.start
                .saturating_sub(lcl_parser::FRAGMENT_CONTEXT.len())
        ),
        FragmentError::Empty(_) => format!("{fragment:?} holds no expression"),
        other => format!("{fragment:?} is not one expression: {other}"),
    }
}
