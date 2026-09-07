//! The closed static-checking vocabulary of LCL Core 0.1.0, loaded as data.
//!
//! Nothing here transcribes a normative table. Every operator, function and
//! constructor signature, every operation's target and named-parameter
//! contract, every registered unit, format and encoding, the ordered-type
//! profile, the numeric promotion table and the three-valued logic tables are
//! read out of the verified package, and [`Contracts::load`] refuses any
//! package that is not the approved release — exactly as `Lexicon::load`,
//! `Grammar::load` and `Rules::load` do.
//!
//! ## The two closed notations
//!
//! Two different registry notations reach this crate, and conflating them would
//! be a language defect:
//!
//! * `operators_and_functions_v0.1.0.json#/signature_notation` writes operator,
//!   function and constructor signatures. Its designators — `numeric`,
//!   `ordered`, `equality_compatible`, `same_unit_MEASURE_collection` and the
//!   rest — are "specification terms, not source types". [`Designator`] models
//!   exactly that vocabulary.
//! * `operations_v0.1.0.json#/contract_type_notation` writes operation target,
//!   parameter and result-schema types. It "does not extend LCL source syntax";
//!   its unions, metatypes and schema names "do not become source forms".
//!   [`ContractType`] models exactly that grammar.
//!
//! Neither is [`crate::Type`], which is the *source* type system. A designator
//! or contract type is matched against a static type; it never becomes one.
//!
//! ## Fail closed
//!
//! A designator, contract type or registry row this build cannot read is an
//! error at load time, not a silently permissive check: "Unknown/unregistered
//! syntax, fields, types, operations, errors, statuses, or extensions fail
//! closed."

use crate::diagnostic::StaticError;
use crate::ty::Type;
use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Registry facts about one static-stage error identifier.
#[derive(Debug, Clone)]
pub struct RegisteredStaticError {
    pub id: StaticError,
    pub meaning: String,
    pub default_status: String,
    pub specificity_rank: u64,
    pub supersedes: BTreeSet<StaticError>,
}

#[derive(Debug)]
pub enum ContractsLoadError {
    /// The package did not establish authority. See [`lcl_spec::Authority`].
    UnverifiedPackage(lcl_spec::Authority),
    MissingRegistry(&'static str),
    Malformed(String),
    Diagnostics(lcl_diagnostics::DiagnosticsError),
    /// The registry's static-stage error set is not the set this build mirrors.
    StaticErrorSetMismatch {
        missing_from_build: Vec<String>,
        missing_from_registry: Vec<String>,
    },
    /// A signature designator this build cannot read. Unknown material fails
    /// closed rather than being treated as permissive.
    UnknownDesignator {
        owner: String,
        designator: String,
    },
    /// A contract type string that does not parse under
    /// `#/contract_type_notation`.
    UnreadableContractType {
        owner: String,
        text: String,
    },
    /// A registry sentence a derivation depends on is no longer present. The
    /// rule is not reinterpreted; the load refuses.
    RegistryTextChanged {
        pointer: &'static str,
        expected_phrase: &'static str,
    },
}

impl fmt::Display for ContractsLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContractsLoadError::UnverifiedPackage(a) => write!(
                f,
                "refusing to build static contracts from a {a} package: only the approved release is normative input"
            ),
            ContractsLoadError::MissingRegistry(n) => write!(f, "registry {n:?} not loaded"),
            ContractsLoadError::Malformed(m) => write!(f, "malformed registry data: {m}"),
            ContractsLoadError::Diagnostics(e) => write!(f, "diagnostic registry: {e}"),
            ContractsLoadError::StaticErrorSetMismatch {
                missing_from_build,
                missing_from_registry,
            } => write!(
                f,
                "static error set mismatch: registry has {missing_from_build:?} that this build does not mirror; this build has {missing_from_registry:?} that the registry does not register"
            ),
            ContractsLoadError::UnknownDesignator { owner, designator } => write!(
                f,
                "{owner} names operand designator {designator:?}, which this build cannot read: refusing to guess"
            ),
            ContractsLoadError::UnreadableContractType { owner, text } => write!(
                f,
                "{owner} declares contract type {text:?}, which does not parse under the closed contract type notation"
            ),
            ContractsLoadError::RegistryTextChanged {
                pointer,
                expected_phrase,
            } => write!(
                f,
                "{pointer} no longer states {expected_phrase:?}: refusing to apply a derivation the registry no longer supports"
            ),
        }
    }
}

impl std::error::Error for ContractsLoadError {}

// ---------------------------------------------------------------------------
// Signature notation
// ---------------------------------------------------------------------------

/// One operand designator of `#/signature_notation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Designator {
    /// One built-in type name with its `types_v0.1.0.json` meaning.
    Exact(Type),
    /// Bare `LIST`, `SET` or `OBJECT`: "accept any one material member type".
    AnyList,
    AnySet,
    AnyObject,
    /// Bare `REFERENCE`.
    AnyReference,
    /// `LIST[X]` / `SET[X]` with an inner designator.
    List(Box<Designator>),
    Set(Box<Designator>),
    /// `T` — "binds one identical static type throughout one signature
    /// application".
    Variable(char),
    /// The `UNKNOWN` sentinel, admitted explicitly by a row.
    Unknown,
    /// The `MISSING` sentinel, admitted explicitly by a row.
    Missing,
    Numeric,
    SameUnitMeasure,
    SameUnitMeasureCollection,
    Ordered,
    NonemptyOrderedCollection,
    EqualityCompatible,
    OrderCompatible,
    PropertyName,
    AnyExpressionOrReference,
    /// `LIST[BOOLEAN|UNKNOWN]`: "A material LIST[BOOLEAN], or the non-material
    /// immediate Boolean argument sequence defined by quantifier_argument".
    BooleanSequence,
    /// `qualified_identifier(unit)`.
    UnitIdentifier,
    /// `REFERENCE[WORKSPACE]`.
    WorkspaceReference,
}

/// One operand position: the alternatives admitted there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operand(pub Vec<Designator>);

impl Operand {
    pub fn alternatives(&self) -> &[Designator] {
        &self.0
    }

    pub fn admits_unknown(&self) -> bool {
        self.0.contains(&Designator::Unknown)
    }
}

/// One result designator of `#/signature_notation/result_designators`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultSpec {
    Exact(Type),
    SameNumericFamily,
    PromotedNumericOrMeasure,
    PromotedFamily,
    SameFamilyNonnegative,
    PromotedMemberFamily,
    MemberType,
    DeclaredPropertyType,
}

/// A result designator plus the non-material outcomes the row lists alongside
/// it. "Static material result families are lifted to these non-material
/// outcomes; this creates no storable UNKNOWN type."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultContract {
    pub spec: ResultSpec,
    pub unknown: bool,
    pub missing: bool,
}

/// One registered overload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overload {
    /// One entry per positional operand.
    pub parameters: Vec<Operand>,
    pub result: ResultContract,
    /// e.g. `same_exact_unit`.
    pub constraint: Option<String>,
    /// e.g. `preserve_left_exact_unit`, `cancel_identical_units`.
    pub unit: Option<String>,
    /// e.g. the `DECIMAL` numeric component of `MEASURE / INTEGER`.
    pub numeric_component: Option<Type>,
}

/// One registered operator row.
#[derive(Debug, Clone)]
pub struct OperatorRow {
    pub name: String,
    pub arity: usize,
    pub overloads: Vec<Overload>,
    pub precedence: u64,
}

/// One registered pure-function row.
#[derive(Debug, Clone)]
pub struct FunctionRow {
    pub name: String,
    pub overloads: Vec<Overload>,
    /// `minimum_count`: SUM, MIN and MAX "require at least one member".
    pub minimum_count: Option<u64>,
    /// `argument_contract`: `quantifier_argument` for ALL, ANY and NONE.
    pub argument_contract: Option<String>,
}

impl FunctionRow {
    /// Every arity this row registers.
    pub fn arities(&self) -> BTreeSet<usize> {
        self.overloads.iter().map(|o| o.parameters.len()).collect()
    }
}

/// One registered typed-constructor row.
#[derive(Debug, Clone)]
pub struct ConstructorRow {
    pub name: String,
    pub overloads: Vec<Overload>,
    /// The constructed type.
    pub result: Type,
    /// An inclusive numeric lower bound on the constructed value.
    pub minimum: Option<i64>,
    /// An inclusive numeric upper bound on the constructed value.
    pub maximum: Option<i64>,
    /// A unit category this constructor narrows to, e.g. `Time` for DURATION.
    pub unit_category: Option<String>,
}

impl ConstructorRow {
    pub fn arities(&self) -> BTreeSet<usize> {
        self.overloads.iter().map(|o| o.parameters.len()).collect()
    }
}

// ---------------------------------------------------------------------------
// Contract type notation
// ---------------------------------------------------------------------------

/// One type written in `operations_v0.1.0.json#/contract_type_notation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractType {
    Union(Vec<ContractType>),
    /// `ENUM[a|b]` — a closed set of lowercase identifiers.
    Enum(Vec<String>),
    List(Box<ContractType>),
    Set(Box<ContractType>),
    /// `REFERENCE` or `REFERENCE[TARGET|TARGET]`.
    Reference(Vec<String>),
    /// `qualified_identifier` or `qualified_identifier(DOMAIN)`.
    QualifiedIdentifier(Option<String>),
    /// A scalar type name, special value name, meta type, named value kind,
    /// result record name, or a one-letter type variable.
    Atom(String),
}

impl ContractType {
    /// Every alternative of a union, or this type alone.
    pub fn members(&self) -> Vec<&ContractType> {
        match self {
            ContractType::Union(members) => members.iter().collect(),
            other => vec![other],
        }
    }
}

/// One operation's target contract.
#[derive(Debug, Clone)]
pub struct TargetSpec {
    pub ty: ContractType,
    pub required: bool,
}

/// One operation's named parameter.
#[derive(Debug, Clone)]
pub struct ParameterSpec {
    pub name: String,
    pub ty: ContractType,
    pub required: bool,
    /// "In this registry only, JSON null in a parameter default field means no
    /// declared default and is not the LCL NULL value."
    pub has_default: bool,
    pub constraints: Vec<String>,
}

/// One of the 39 closed core operation contracts.
#[derive(Debug, Clone)]
pub struct OperationContract {
    pub id: String,
    pub target: TargetSpec,
    /// Named parameters, by name. "No positional parameters exist."
    pub parameters: BTreeMap<String, ParameterSpec>,
    pub positional_parameters: bool,
    pub result_schema: String,
}

// ---------------------------------------------------------------------------
// The loaded contracts
// ---------------------------------------------------------------------------

/// A registry sentence each derivation depends on, verified at load.
const REGISTRY_ANCHORS: &[(&str, &str, &str)] = &[
    (
        "operators_and_functions",
        "evaluation_contract/static_validation",
        "Every subexpression is statically checked, including a branch that Boolean short-circuit will skip.",
    ),
    (
        "operators_and_functions",
        "collection_expression/default_family",
        "An ordinary bracket literal denotes LIST unless its immediate expected type is one exact SET[T]",
    ),
    (
        "operators_and_functions",
        "signature_notation/operand_designators/numeric",
        "INTEGER or DECIMAL only.",
    ),
    (
        "types",
        "set_iteration_profile/direct_for_each/legal_when",
        "Every pair of actual members is mutually order-compatible",
    ),
    (
        "types",
        "object_type_contract/identity",
        "recursively equal field-name/type/requiredness maps identify the same object type",
    ),
    (
        "types",
        "source_type_contract/defined_type_form",
        "REF(identifier), resolving exactly once to DEFINE KIND kind.type",
    ),
    (
        "operators_and_functions",
        "signature_notation/structure",
        "A single compatibility designator denotes the whole binary tuple.",
    ),
    ("operations", "parameter_binding", "named_only"),
];

/// The closed static-checking vocabulary of LCL Core 0.1.0.
pub struct Contracts {
    /// The registered diagnostic model, kept so an identifier outside this
    /// stage can still be reported with its own registered stage and status.
    diagnostics: DiagnosticRegistry,
    errors: BTreeMap<StaticError, RegisteredStaticError>,
    supersedes: BTreeMap<StaticError, BTreeSet<StaticError>>,
    type_rows: BTreeSet<String>,
    ordered_families: BTreeSet<String>,
    units: BTreeMap<String, String>,
    formats: BTreeSet<String>,
    encodings: BTreeSet<String>,
    operators: BTreeMap<String, OperatorRow>,
    functions: BTreeMap<String, FunctionRow>,
    constructors: BTreeMap<String, ConstructorRow>,
    operations: BTreeMap<String, OperationContract>,
    /// `(block, field)` -> registered `value_kind`, verbatim.
    value_kinds: BTreeMap<(String, String), String>,
    /// `(block, field)` for every field the signature marks required.
    required_fields: BTreeSet<(String, String)>,
    /// `#/unknown_logic`, e.g. `TRUE AND UNKNOWN` -> `UNKNOWN`.
    unknown_logic: BTreeMap<String, String>,
    /// `#/numeric_promotion`, e.g. `INTEGER+DECIMAL` -> `DECIMAL`.
    numeric_promotion: BTreeMap<String, String>,
    /// `built_in_groups_and_results_v0.1.0.json#/enum_groups`, each closed.
    enum_groups: BTreeMap<String, BTreeSet<String>>,
}

impl fmt::Debug for Contracts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Contracts")
            .field("static_errors", &self.errors.len())
            .field("operators", &self.operators.len())
            .field("functions", &self.functions.len())
            .field("constructors", &self.constructors.len())
            .field("operations", &self.operations.len())
            .field("units", &self.units.len())
            .finish()
    }
}

impl Contracts {
    /// Build the static-checking vocabulary from a verified package.
    pub fn load(spec: &SpecPackage) -> Result<Self, ContractsLoadError> {
        if !spec.is_authoritative() {
            return Err(ContractsLoadError::UnverifiedPackage(spec.authority()));
        }

        for (reg, pointer, phrase) in REGISTRY_ANCHORS {
            let mut node = registry(spec, reg)?;
            for segment in pointer.split('/') {
                node = node
                    .get(segment)
                    .ok_or(ContractsLoadError::RegistryTextChanged {
                        pointer,
                        expected_phrase: phrase,
                    })?;
            }
            if !node.as_str().unwrap_or_default().contains(phrase) {
                return Err(ContractsLoadError::RegistryTextChanged {
                    pointer,
                    expected_phrase: phrase,
                });
            }
        }

        let types = registry(spec, "types")?;
        let opfn = registry(spec, "operators_and_functions")?;
        let field_signatures = registry(spec, "field_signatures")?;
        let operations_reg = registry(spec, "operations")?;
        let units_reg = registry(spec, "formats_encodings_units")?;
        let statuses = registry(spec, "statuses_and_errors")?;

        let diagnostics =
            DiagnosticRegistry::load(spec).map_err(ContractsLoadError::Diagnostics)?;
        let (errors, supersedes) = static_errors(&diagnostics, statuses)?;

        let type_rows: BTreeSet<String> = types
            .get("types")
            .and_then(Json::as_object)
            .ok_or_else(|| ContractsLoadError::Malformed("types.types missing".into()))?
            .iter()
            .map(|(k, _)| k.clone())
            .collect();

        // `#/ordered_types` lists ten rows; `MEASURE[same unit]` names the
        // MEASURE family, whose same-unit requirement each overload applies
        // separately.
        let ordered_families: BTreeSet<String> = opfn
            .get("ordered_types")
            .and_then(Json::as_array)
            .ok_or_else(|| ContractsLoadError::Malformed("ordered_types missing".into()))?
            .iter()
            .filter_map(Json::as_str)
            .map(|t| t.split('[').next().unwrap_or(t).trim().to_string())
            .collect();

        let units = object_map(units_reg, "units")?;
        let formats: BTreeSet<String> = object_map(units_reg, "formats")?.into_keys().collect();
        let encodings: BTreeSet<String> = object_map(units_reg, "encodings")?.into_keys().collect();

        let mut operators = BTreeMap::new();
        for (name, row) in members(opfn, "operators")? {
            operators.insert(name.clone(), operator_row(name, row)?);
        }

        let mut functions = BTreeMap::new();
        for (name, row) in members(opfn, "functions")? {
            functions.insert(name.clone(), function_row(name, row)?);
        }

        let mut constructors = BTreeMap::new();
        for (name, row) in members(opfn, "constructors")? {
            constructors.insert(name.clone(), constructor_row(name, row)?);
        }

        let positional_default = operations_reg
            .get("parameter_binding")
            .and_then(Json::as_str)
            .map(|binding| binding != "named_only")
            .unwrap_or(true);
        let mut operations = BTreeMap::new();
        for (id, contract) in members(operations_reg, "contracts")? {
            operations.insert(
                id.clone(),
                operation_contract(id, contract, positional_default)?,
            );
        }

        let mut value_kinds = BTreeMap::new();
        let mut required_fields = BTreeSet::new();
        for (block, schema) in members(field_signatures, "blocks")? {
            let Some(fields) = schema.get("fields").and_then(Json::as_object) else {
                continue;
            };
            for (field, signature) in fields {
                let kind = signature
                    .get("value_kind")
                    .and_then(Json::as_str)
                    .ok_or_else(|| {
                        ContractsLoadError::Malformed(format!(
                            "{block}.{field}: value_kind missing"
                        ))
                    })?;
                value_kinds.insert((block.clone(), field.clone()), kind.to_string());
                if signature
                    .get("required")
                    .and_then(Json::as_bool)
                    .unwrap_or(false)
                {
                    required_fields.insert((block.clone(), field.clone()));
                }
            }
        }

        let groups = registry(spec, "built_in_groups_and_results")?;
        let mut enum_groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (name, values) in members(groups, "enum_groups")? {
            let Some(entries) = values.as_array() else {
                continue;
            };
            enum_groups.insert(
                name.clone(),
                entries
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect(),
            );
        }

        let unknown_logic = object_map(opfn, "unknown_logic")?;
        let numeric_promotion = object_map(opfn, "numeric_promotion")?;

        Ok(Contracts {
            diagnostics,
            errors,
            supersedes,
            type_rows,
            ordered_families,
            units,
            formats,
            encodings,
            operators,
            functions,
            constructors,
            operations,
            value_kinds,
            required_fields,
            unknown_logic,
            numeric_promotion,
            enum_groups,
        })
    }

    pub fn error(&self, id: StaticError) -> &RegisteredStaticError {
        self.errors
            .get(&id)
            .expect("every mirrored identifier is loaded")
    }

    /// The registered diagnostic model, for identifiers outside this stage.
    pub fn diagnostics(&self) -> &DiagnosticRegistry {
        &self.diagnostics
    }

    pub(crate) fn supersedes(&self) -> &BTreeMap<StaticError, BTreeSet<StaticError>> {
        &self.supersedes
    }

    pub fn type_row_count(&self) -> usize {
        self.type_rows.len()
    }

    pub fn is_type_row(&self, row: &str) -> bool {
        self.type_rows.contains(row)
    }

    /// True when this family has a registered total order.
    pub fn is_ordered_family(&self, family: &str) -> bool {
        self.ordered_families.contains(family)
    }

    pub fn ordered_families(&self) -> impl Iterator<Item = &str> {
        self.ordered_families.iter().map(String::as_str)
    }

    /// True when `id` is a registered unit identifier.
    pub fn is_unit(&self, id: &str) -> bool {
        self.units.contains_key(id)
    }

    /// The registered category text of one unit, e.g. `Category: Time`.
    pub fn unit_category(&self, id: &str) -> Option<&str> {
        self.units.get(id).map(String::as_str)
    }

    /// True when the unit belongs to the named category, e.g. `Time`.
    pub fn unit_in_category(&self, id: &str, category: &str) -> bool {
        self.unit_category(id)
            .and_then(|text| text.split(':').next_back())
            .map(|c| c.trim() == category)
            .unwrap_or(false)
    }

    pub fn unit_count(&self) -> usize {
        self.units.len()
    }

    pub fn is_format(&self, id: &str) -> bool {
        self.formats.contains(id)
    }

    pub fn is_encoding(&self, id: &str) -> bool {
        self.encodings.contains(id)
    }

    pub fn operator(&self, name: &str) -> Option<&OperatorRow> {
        self.operators.get(name)
    }

    pub fn operators(&self) -> impl Iterator<Item = &OperatorRow> {
        self.operators.values()
    }

    pub fn function(&self, name: &str) -> Option<&FunctionRow> {
        self.functions.get(name)
    }

    pub fn functions(&self) -> impl Iterator<Item = &FunctionRow> {
        self.functions.values()
    }

    pub fn constructor(&self, name: &str) -> Option<&ConstructorRow> {
        self.constructors.get(name)
    }

    pub fn constructors(&self) -> impl Iterator<Item = &ConstructorRow> {
        self.constructors.values()
    }

    pub fn operation(&self, id: &str) -> Option<&OperationContract> {
        self.operations.get(id)
    }

    pub fn operations(&self) -> impl Iterator<Item = &OperationContract> {
        self.operations.values()
    }

    /// The registered `value_kind` of one block field, verbatim.
    pub fn value_kind(&self, block: &str, field: &str) -> Option<&str> {
        self.value_kinds
            .get(&(block.to_string(), field.to_string()))
            .map(String::as_str)
    }

    pub fn field_is_required(&self, block: &str, field: &str) -> bool {
        self.required_fields
            .contains(&(block.to_string(), field.to_string()))
    }

    /// One row of `#/unknown_logic`, e.g. `TRUE AND UNKNOWN`.
    pub fn unknown_logic(&self, expression: &str) -> Option<&str> {
        self.unknown_logic.get(expression).map(String::as_str)
    }

    pub fn unknown_logic_rows(&self) -> usize {
        self.unknown_logic.len()
    }

    /// True when `id` is a member of one closed enum group, e.g. an
    /// `effect_classes` or `dependency_classes` identifier.
    pub fn is_enum_group_member(&self, group: &str, id: &str) -> bool {
        self.enum_groups
            .get(group)
            .is_some_and(|members| members.contains(id))
    }

    /// The members of one closed enum group, in registry order.
    pub fn enum_group(&self, group: &str) -> Option<impl Iterator<Item = &str>> {
        self.enum_groups
            .get(group)
            .map(|members| members.iter().map(String::as_str))
    }

    /// One row of `#/numeric_promotion`, e.g. `INTEGER+DECIMAL`.
    pub fn numeric_promotion(&self, left: &str, right: &str) -> Option<&str> {
        self.numeric_promotion
            .get(&format!("{left}+{right}"))
            .map(String::as_str)
    }
}

// ---------------------------------------------------------------------------
// Row readers
// ---------------------------------------------------------------------------

fn registry<'a>(spec: &'a SpecPackage, name: &'static str) -> Result<&'a Json, ContractsLoadError> {
    spec.registry(name)
        .ok_or(ContractsLoadError::MissingRegistry(name))
}

fn members<'a>(node: &'a Json, key: &str) -> Result<&'a [(String, Json)], ContractsLoadError> {
    node.get(key)
        .and_then(Json::as_object)
        .ok_or_else(|| ContractsLoadError::Malformed(format!("{key} missing or not an object")))
}

fn object_map(node: &Json, key: &str) -> Result<BTreeMap<String, String>, ContractsLoadError> {
    Ok(members(node, key)?
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
        .collect())
}

type StaticErrorTables = (
    BTreeMap<StaticError, RegisteredStaticError>,
    BTreeMap<StaticError, BTreeSet<StaticError>>,
);

fn static_errors(
    registry_errors: &DiagnosticRegistry,
    statuses: &Json,
) -> Result<StaticErrorTables, ContractsLoadError> {
    let registered: BTreeSet<String> = registry_errors
        .errors_by_stage(Stage::StaticOrExpression)
        .into_iter()
        .map(|e| e.id.clone())
        .collect();
    let mirrored: BTreeSet<String> = StaticError::ALL
        .into_iter()
        .map(|e| e.as_registry_str().to_string())
        .collect();
    if registered != mirrored {
        return Err(ContractsLoadError::StaticErrorSetMismatch {
            missing_from_build: registered.difference(&mirrored).cloned().collect(),
            missing_from_registry: mirrored.difference(&registered).cloned().collect(),
        });
    }

    let selection = statuses
        .get("diagnostic_selection")
        .ok_or_else(|| ContractsLoadError::Malformed("diagnostic_selection missing".into()))?;
    let default_rank = selection
        .get("specificity_rank")
        .and_then(|r| r.get("default_for_every_error"))
        .and_then(Json::as_u64)
        .ok_or_else(|| {
            ContractsLoadError::Malformed("specificity_rank.default_for_every_error missing".into())
        })?;
    let rank_overrides = selection
        .get("specificity_rank")
        .and_then(|r| r.get("overrides"))
        .and_then(Json::as_object)
        .unwrap_or(&[]);
    let supersede_overrides = selection
        .get("supersedes")
        .and_then(|s| s.get("overrides"))
        .and_then(Json::as_object)
        .unwrap_or(&[]);

    let mut errors = BTreeMap::new();
    let mut supersedes: BTreeMap<StaticError, BTreeSet<StaticError>> = BTreeMap::new();
    for id in StaticError::ALL {
        let def = registry_errors
            .error(id.as_registry_str())
            .ok_or_else(|| ContractsLoadError::Malformed(format!("{id} not registered")))?;
        let rank = rank_overrides
            .iter()
            .find(|(k, _)| k == id.as_registry_str())
            .and_then(|(_, v)| v.as_u64())
            .unwrap_or(default_rank);
        let targets: BTreeSet<StaticError> = supersede_overrides
            .iter()
            .find(|(k, _)| k == id.as_registry_str())
            .and_then(|(_, v)| v.as_array())
            .unwrap_or(&[])
            .iter()
            .filter_map(|t| t.as_str().and_then(StaticError::from_registry_str))
            .collect();
        if !targets.is_empty() {
            supersedes.insert(id, targets.clone());
        }
        errors.insert(
            id,
            RegisteredStaticError {
                id,
                meaning: def.meaning.clone(),
                default_status: def.default_status.clone(),
                specificity_rank: rank,
                supersedes: targets,
            },
        );
    }
    Ok((errors, supersedes))
}

fn operator_row(name: &str, row: &Json) -> Result<OperatorRow, ContractsLoadError> {
    let arity =
        row.get("arity").and_then(Json::as_u64).ok_or_else(|| {
            ContractsLoadError::Malformed(format!("operator {name}: arity missing"))
        })? as usize;
    let precedence = row.get("precedence").and_then(Json::as_u64).unwrap_or(0);
    let mut overloads = Vec::new();

    if let Some(rows) = row.get("overloads").and_then(Json::as_array) {
        for entry in rows {
            let left = entry.get("left").and_then(Json::as_str).ok_or_else(|| {
                ContractsLoadError::Malformed(format!("operator {name}: overload without left"))
            })?;
            let mut parameters = vec![operand(name, left)?];
            if let Some(right) = entry.get("right").and_then(Json::as_str) {
                parameters.push(operand(name, right)?);
            }
            overloads.push(Overload {
                parameters,
                result: result_contract(
                    name,
                    entry.get("result").and_then(Json::as_str).unwrap_or(""),
                )?,
                constraint: entry
                    .get("constraint")
                    .and_then(Json::as_str)
                    .map(str::to_string),
                unit: entry.get("unit").and_then(Json::as_str).map(str::to_string),
                numeric_component: entry
                    .get("numeric_component")
                    .and_then(Json::as_str)
                    .and_then(Type::scalar),
            });
        }
    } else {
        let result = result_contract(name, row.get("result").and_then(Json::as_str).unwrap_or(""))?;
        let tuples = row
            .get("operands")
            .and_then(Json::as_array)
            .ok_or_else(|| {
                ContractsLoadError::Malformed(format!("operator {name}: operands missing"))
            })?;
        for tuple in tuples {
            let text = tuple.as_str().unwrap_or("");
            let mut parameters = Vec::new();
            for part in split_tuple(text) {
                parameters.push(operand(name, &part)?);
            }
            // `#/signature_notation/structure`: "A single compatibility
            // designator denotes the whole binary tuple." So `equality_compatible`
            // and `order_compatible` describe both operands at once; every other
            // shortfall is a registry shape this build must not guess at.
            if parameters.len() == 1 && arity > 1 && is_whole_tuple(&parameters[0]) {
                let designator = parameters[0].clone();
                parameters = vec![designator; arity];
            }
            if parameters.len() != arity {
                return Err(ContractsLoadError::Malformed(format!(
                    "operator {name}: operand tuple {text:?} has {} operands for arity {arity}",
                    parameters.len()
                )));
            }
            overloads.push(Overload {
                parameters,
                result: result.clone(),
                constraint: None,
                unit: None,
                numeric_component: None,
            });
        }
    }

    Ok(OperatorRow {
        name: name.to_string(),
        arity,
        overloads,
        precedence,
    })
}

/// True for a designator that describes a whole binary tuple rather than one
/// operand position.
fn is_whole_tuple(operand: &Operand) -> bool {
    operand.alternatives().iter().all(|d| {
        matches!(
            d,
            Designator::EqualityCompatible | Designator::OrderCompatible
        )
    })
}

fn function_row(name: &str, row: &Json) -> Result<FunctionRow, ContractsLoadError> {
    let mut overloads = Vec::new();
    if let Some(rows) = row.get("overloads").and_then(Json::as_array) {
        for entry in rows {
            overloads.push(Overload {
                parameters: parameter_list(name, entry)?,
                result: result_contract(
                    name,
                    entry.get("result").and_then(Json::as_str).unwrap_or(""),
                )?,
                constraint: entry
                    .get("constraint")
                    .and_then(Json::as_str)
                    .map(str::to_string),
                unit: entry.get("unit").and_then(Json::as_str).map(str::to_string),
                numeric_component: None,
            });
        }
    } else {
        overloads.push(Overload {
            parameters: parameter_list(name, row)?,
            result: result_contract(name, row.get("result").and_then(Json::as_str).unwrap_or(""))?,
            constraint: None,
            unit: None,
            numeric_component: None,
        });
    }
    Ok(FunctionRow {
        name: name.to_string(),
        overloads,
        minimum_count: row.get("minimum_count").and_then(Json::as_u64),
        argument_contract: row
            .get("argument_contract")
            .and_then(Json::as_str)
            .map(str::to_string),
    })
}

fn constructor_row(name: &str, row: &Json) -> Result<ConstructorRow, ContractsLoadError> {
    let result_name = row.get("result").and_then(Json::as_str).ok_or_else(|| {
        ContractsLoadError::Malformed(format!("constructor {name}: result missing"))
    })?;
    let result =
        Type::scalar(result_name).ok_or_else(|| ContractsLoadError::UnknownDesignator {
            owner: format!("constructor {name}"),
            designator: result_name.to_string(),
        })?;
    let rows = row
        .get("overloads")
        .and_then(Json::as_array)
        .ok_or_else(|| {
            ContractsLoadError::Malformed(format!("constructor {name}: overloads missing"))
        })?;
    let mut overloads = Vec::new();
    for entry in rows {
        overloads.push(Overload {
            parameters: parameter_list(name, entry)?,
            result: ResultContract {
                spec: ResultSpec::Exact(result.clone()),
                unknown: false,
                missing: false,
            },
            constraint: entry
                .get("constraint")
                .and_then(Json::as_str)
                .map(str::to_string),
            unit: None,
            numeric_component: None,
        });
    }
    Ok(ConstructorRow {
        name: name.to_string(),
        overloads,
        result,
        minimum: row.get("minimum").and_then(json_i64),
        maximum: row.get("maximum").and_then(json_i64),
        unit_category: row
            .get("unit_category")
            .and_then(Json::as_str)
            .map(str::to_string),
    })
}

fn json_i64(node: &Json) -> Option<i64> {
    match node {
        Json::Number(n) if n.fract() == 0.0 => Some(*n as i64),
        _ => None,
    }
}

fn parameter_list(owner: &str, node: &Json) -> Result<Vec<Operand>, ContractsLoadError> {
    let entries = node
        .get("parameters")
        .and_then(Json::as_array)
        .ok_or_else(|| ContractsLoadError::Malformed(format!("{owner}: parameters missing")))?;
    let mut out = Vec::new();
    for entry in entries {
        out.push(operand(owner, entry.as_str().unwrap_or(""))?);
    }
    Ok(out)
}

/// Split one operands-array entry into its comma-separated operand slots,
/// respecting the brackets that bind a collection element domain.
fn split_tuple(text: &str) -> Vec<String> {
    split_on(text, ',')
}

/// Split on `|` at bracket depth zero.
fn split_alternatives(text: &str) -> Vec<String> {
    split_on(text, '|')
}

fn split_on(text: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in text.chars() {
        match ch {
            '[' | '(' => {
                depth = depth.saturating_add(1);
                current.push(ch);
            }
            ']' | ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            c if c == separator && depth == 0 => {
                parts.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts.retain(|p| !p.is_empty());
    parts
}

fn operand(owner: &str, text: &str) -> Result<Operand, ContractsLoadError> {
    // `LIST[BOOLEAN|UNKNOWN]` is one registered designator, not a union, so it
    // is recognized before the alternatives split.
    if text == "LIST[BOOLEAN|UNKNOWN]" {
        return Ok(Operand(vec![Designator::BooleanSequence]));
    }
    let mut out = Vec::new();
    for alternative in split_alternatives(text) {
        out.push(designator(owner, &alternative)?);
    }
    if out.is_empty() {
        return Err(ContractsLoadError::UnknownDesignator {
            owner: owner.to_string(),
            designator: text.to_string(),
        });
    }
    Ok(Operand(out))
}

fn designator(owner: &str, text: &str) -> Result<Designator, ContractsLoadError> {
    if let Some(inner) = text.strip_prefix("LIST[").and_then(|t| t.strip_suffix(']')) {
        return Ok(Designator::List(Box::new(designator(owner, inner)?)));
    }
    if let Some(inner) = text.strip_prefix("SET[").and_then(|t| t.strip_suffix(']')) {
        return Ok(Designator::Set(Box::new(designator(owner, inner)?)));
    }
    if let Some(ty) = Type::scalar(text) {
        return Ok(Designator::Exact(ty));
    }
    Ok(match text {
        "LIST" => Designator::AnyList,
        "SET" => Designator::AnySet,
        "OBJECT" => Designator::AnyObject,
        "REFERENCE" => Designator::AnyReference,
        "UNKNOWN" => Designator::Unknown,
        "MISSING" => Designator::Missing,
        "numeric" => Designator::Numeric,
        "same_unit_MEASURE" => Designator::SameUnitMeasure,
        "same_unit_MEASURE_collection" => Designator::SameUnitMeasureCollection,
        "ordered" => Designator::Ordered,
        "nonempty_collection[ordered]" => Designator::NonemptyOrderedCollection,
        "equality_compatible" => Designator::EqualityCompatible,
        "order_compatible" => Designator::OrderCompatible,
        "property_name" => Designator::PropertyName,
        "any_expression_or_reference" => Designator::AnyExpressionOrReference,
        "qualified_identifier(unit)" => Designator::UnitIdentifier,
        "REFERENCE[WORKSPACE]" => Designator::WorkspaceReference,
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_uppercase() => Designator::Variable(c),
                _ => {
                    return Err(ContractsLoadError::UnknownDesignator {
                        owner: owner.to_string(),
                        designator: other.to_string(),
                    })
                }
            }
        }
    })
}

fn result_contract(owner: &str, text: &str) -> Result<ResultContract, ContractsLoadError> {
    let mut unknown = false;
    let mut missing = false;
    let mut spec = None;
    for alternative in split_alternatives(text) {
        match alternative.as_str() {
            "UNKNOWN" => unknown = true,
            "MISSING" => missing = true,
            "same_numeric_family" => spec = Some(ResultSpec::SameNumericFamily),
            "promoted_numeric_or_measure" => spec = Some(ResultSpec::PromotedNumericOrMeasure),
            "promoted_family" => spec = Some(ResultSpec::PromotedFamily),
            "same_family_nonnegative" => spec = Some(ResultSpec::SameFamilyNonnegative),
            "promoted_member_family" => spec = Some(ResultSpec::PromotedMemberFamily),
            "member_type" => spec = Some(ResultSpec::MemberType),
            "declared_property_type" => spec = Some(ResultSpec::DeclaredPropertyType),
            other => match Type::scalar(other) {
                Some(ty) => spec = Some(ResultSpec::Exact(ty)),
                None => {
                    return Err(ContractsLoadError::UnknownDesignator {
                        owner: owner.to_string(),
                        designator: other.to_string(),
                    })
                }
            },
        }
    }
    let spec = spec.ok_or_else(|| ContractsLoadError::UnknownDesignator {
        owner: owner.to_string(),
        designator: text.to_string(),
    })?;
    Ok(ResultContract {
        spec,
        unknown,
        missing,
    })
}

fn operation_contract(
    id: &str,
    contract: &Json,
    positional_default: bool,
) -> Result<OperationContract, ContractsLoadError> {
    let target_node = contract
        .get("target")
        .ok_or_else(|| ContractsLoadError::Malformed(format!("operation {id}: target missing")))?;
    let target_type = target_node
        .get("type")
        .and_then(Json::as_str)
        .ok_or_else(|| {
            ContractsLoadError::Malformed(format!("operation {id}: target type missing"))
        })?;
    let target = TargetSpec {
        ty: contract_type(&format!("operation {id} target"), target_type)?,
        required: target_node
            .get("required")
            .and_then(Json::as_bool)
            .unwrap_or(false),
    };

    let mut parameters = BTreeMap::new();
    for (name, spec) in contract
        .get("parameters")
        .and_then(Json::as_object)
        .unwrap_or(&[])
    {
        let text = spec.get("type").and_then(Json::as_str).ok_or_else(|| {
            ContractsLoadError::Malformed(format!("operation {id}.{name}: type missing"))
        })?;
        parameters.insert(
            name.clone(),
            ParameterSpec {
                name: name.clone(),
                ty: contract_type(&format!("operation {id} parameter {name}"), text)?,
                required: spec
                    .get("required")
                    .and_then(Json::as_bool)
                    .unwrap_or(false),
                has_default: !matches!(spec.get("default"), None | Some(Json::Null)),
                constraints: spec
                    .get("constraints")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect(),
            },
        );
    }

    Ok(OperationContract {
        id: id.to_string(),
        target,
        parameters,
        positional_parameters: contract
            .get("positional_parameters")
            .and_then(Json::as_bool)
            .unwrap_or(positional_default),
        result_schema: contract
            .get("result_schema")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

/// Parse one string of `operations_v0.1.0.json#/contract_type_notation`.
fn contract_type(owner: &str, text: &str) -> Result<ContractType, ContractsLoadError> {
    let members = split_alternatives(text);
    if members.is_empty() {
        return Err(ContractsLoadError::UnreadableContractType {
            owner: owner.to_string(),
            text: text.to_string(),
        });
    }
    let mut parsed = Vec::new();
    for member in &members {
        parsed.push(contract_member(owner, member)?);
    }
    Ok(if parsed.len() == 1 {
        parsed.remove(0)
    } else {
        ContractType::Union(parsed)
    })
}

fn contract_member(owner: &str, text: &str) -> Result<ContractType, ContractsLoadError> {
    if let Some(inner) = text.strip_prefix("ENUM[").and_then(|t| t.strip_suffix(']')) {
        return Ok(ContractType::Enum(split_alternatives(inner)));
    }
    if let Some(inner) = text.strip_prefix("LIST[").and_then(|t| t.strip_suffix(']')) {
        return Ok(ContractType::List(Box::new(contract_type(owner, inner)?)));
    }
    if let Some(inner) = text.strip_prefix("SET[").and_then(|t| t.strip_suffix(']')) {
        return Ok(ContractType::Set(Box::new(contract_type(owner, inner)?)));
    }
    if let Some(inner) = text
        .strip_prefix("REFERENCE[")
        .and_then(|t| t.strip_suffix(']'))
    {
        return Ok(ContractType::Reference(split_alternatives(inner)));
    }
    if text == "REFERENCE" {
        return Ok(ContractType::Reference(Vec::new()));
    }
    if text == "qualified_identifier" {
        return Ok(ContractType::QualifiedIdentifier(None));
    }
    if let Some(inner) = text
        .strip_prefix("qualified_identifier(")
        .and_then(|t| t.strip_suffix(')'))
    {
        return Ok(ContractType::QualifiedIdentifier(Some(inner.to_string())));
    }
    if text.is_empty() || text.contains(['[', ']', '(', ')', ' ']) {
        return Err(ContractsLoadError::UnreadableContractType {
            owner: owner.to_string(),
            text: text.to_string(),
        });
    }
    Ok(ContractType::Atom(text.to_string()))
}
