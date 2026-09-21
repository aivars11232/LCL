//! Effective authority, priority and scope records.
//!
//! Authority: `05_SEMANTICS/04_AUTHORITY_PRIORITY_OVERRIDE_AND_CONFLICT_RESOLUTION.txt`
//! and `05_SEMANTICS/03_REQUIRE_ALLOW_FORBID_PREFER_PRESERVE_AND_ACTION_AUTHORIZATION.txt`.
//!
//! > Effective AUTHORITY is 0..1000; local default 500. Imported declarations
//! > cannot exceed the lower of their source authority and IMPORT authority.
//! > PRIORITY is -1000..1000 as a strict INTEGER and compares only clauses of
//! > equal authority. When PRIORITY is optional and MISSING, its value is 0.
//! > PRIORITY never inherits from an enclosing or referenced clause.
//!
//! This module owns the *records*; `crate::conflict` owns the decisions taken
//! over them.

use lcl_lexer::Span;
use lcl_resolver::SourceId;
use std::collections::BTreeMap;
use std::fmt;

/// Which of the five rule blocks a clause is.
///
/// `05_SEMANTICS/03`: "REQUIRE is hard. ALLOW grants optional capability.
/// FORBID is hard prohibition. PREFER is soft and cannot authorize an action or
/// defeat hard rules. PRESERVE is hard pre-state equals post-state for selected
/// target/properties."
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleKind {
    Require,
    Allow,
    Forbid,
    Prefer,
    Preserve,
}

impl RuleKind {
    pub fn from_block(block: &str) -> Option<RuleKind> {
        Some(match block {
            "REQUIRE" => RuleKind::Require,
            "ALLOW" => RuleKind::Allow,
            "FORBID" => RuleKind::Forbid,
            "PREFER" => RuleKind::Prefer,
            "PRESERVE" => RuleKind::Preserve,
            _ => return None,
        })
    }

    pub fn as_block(self) -> &'static str {
        match self {
            RuleKind::Require => "REQUIRE",
            RuleKind::Allow => "ALLOW",
            RuleKind::Forbid => "FORBID",
            RuleKind::Prefer => "PREFER",
            RuleKind::Preserve => "PRESERVE",
        }
    }

    /// True for a rule the language calls hard.
    ///
    /// `ALLOW` is not hard: it "grants optional capability" and "never causes
    /// execution". `PREFER` is explicitly soft.
    pub fn is_hard(self) -> bool {
        matches!(
            self,
            RuleKind::Require | RuleKind::Forbid | RuleKind::Preserve
        )
    }

    /// True for a rule that can authorize an effect.
    ///
    /// `05_SEMANTICS/03`: "A goal or preference never implies permission."
    pub fn authorizes(self) -> bool {
        matches!(self, RuleKind::Require | RuleKind::Allow)
    }

    /// True for a rule that prohibits.
    pub fn prohibits(self) -> bool {
        matches!(self, RuleKind::Forbid | RuleKind::Preserve)
    }
}

impl fmt::Display for RuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_block())
    }
}

/// Whether a clause's applicability condition was decided, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Applicability {
    /// No `WHEN` field, or a `WHEN` that evaluated TRUE.
    Applicable,
    /// A `WHEN` that evaluated FALSE. The clause takes no part in conflicts.
    Inapplicable,
    /// A `WHEN` whose value is not determinable before effects.
    ///
    /// The clause is retained and reported rather than silently dropped: a
    /// clause whose applicability is unknown cannot be assumed away.
    Undetermined,
}

/// One applicable rule clause with its effective authority and priority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityRecord {
    /// Index into the resolver's declaration index.
    pub declaration: usize,
    pub id: String,
    pub kind: RuleKind,
    pub source: SourceId,
    pub span: Span,
    /// Effective authority after the import ceiling is applied.
    pub authority: u32,
    /// The authority the clause itself declared, before any ceiling.
    pub declared_authority: Option<u32>,
    /// The ceiling an import imposed, when this declaration is imported:
    /// "Imported declarations cannot exceed the lower of their source authority
    /// and IMPORT authority."
    pub import_ceiling: Option<u32>,
    /// Effective priority. `0` when the optional field is absent.
    pub priority: i32,
    /// Whether `PRIORITY` was declared explicitly.
    pub priority_declared: bool,
    pub applicability: Applicability,
    /// The operation this clause names, for `ALLOW` and `FORBID`.
    pub operation: Option<String>,
    /// The target this clause names.
    pub target: Option<String>,
    /// The scope declaration this clause names.
    pub scope: Option<String>,
}

impl AuthorityRecord {
    /// True when this clause takes part in conflict resolution.
    pub fn is_applicable(&self) -> bool {
        !matches!(self.applicability, Applicability::Inapplicable)
    }

    pub fn is_hard(&self) -> bool {
        self.kind.is_hard()
    }
}

/// One resolved `SCOPE` declaration.
///
/// `05_SEMANTICS/02`: "SCOPE is computed as INCLUDE minus EXCLUDE. Exact
/// references/paths identify one entity. GLOB/REGEX select a finite set
/// resolved before affected execution. EXCLUDE wins within the same SCOPE."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeRecord {
    pub declaration: usize,
    pub id: String,
    pub source: SourceId,
    pub span: Span,
    /// Exact selectors the scope includes, in declaration order.
    pub include: Vec<Selector>,
    /// Exact selectors the scope excludes, in declaration order.
    pub exclude: Vec<Selector>,
    /// The operation this scope is restricted to, when it names one.
    pub operation: Option<String>,
    /// The workspace this scope resolves against, when one is in force.
    pub workspace: Option<usize>,
}

/// One `INCLUDE` or `EXCLUDE` selector.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Selector {
    /// An exact reference or path identifying one entity.
    Exact(String),
    /// A `GLOB` pattern, evaluated only against workspace-relative paths.
    Glob(String),
    /// A `REGEX` pattern.
    Regex(String),
    /// A reference to another declaration, retained as identity.
    Reference(String),
    /// The WORKSPACE path form `PATH(REF(workspace.one), "src/main.py")`.
    ///
    /// `types_v0.1.0.json` material identity: "WORKSPACE form uses the resolved
    /// workspace declaration identity and exact decoded relative STRING", so
    /// both parts are kept exactly as written and compared as one identity. The
    /// relative half is also the only subject a `GLOB` selector can consume —
    /// `pattern_profiles/GLOB` is `workspace_relative` — after normalization.
    Relative { workspace: String, relative: String },
    /// Any other constructor form no single literal identifies, kept as the
    /// exact rendering of what was written. It identifies one entity like
    /// [`Selector::Exact`] does, and an `ACTION` naming the same entity writes
    /// the same form, so the two compare by rendering.
    Expression(String),
}

impl Selector {
    pub fn as_str(&self) -> &str {
        match self {
            Selector::Exact(s)
            | Selector::Glob(s)
            | Selector::Regex(s)
            | Selector::Reference(s)
            | Selector::Expression(s) => s,
            Selector::Relative { relative, .. } => relative,
        }
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Selector::Exact(s) => write!(f, "{s}"),
            Selector::Glob(s) => write!(f, "GLOB({s:?})"),
            Selector::Regex(s) => write!(f, "REGEX({s:?})"),
            Selector::Reference(s) => write!(f, "REF({s})"),
            Selector::Relative {
                workspace,
                relative,
            } => write!(f, "PATH(REF({workspace}), {relative:?})"),
            Selector::Expression(s) => write!(f, "{s}"),
        }
    }
}

/// One resolved `WORKSPACE` declaration.
///
/// `05_SEMANTICS/02`: "WORKSPACE supplies an explicit base path and access
/// MODE; it grants no operation by itself."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRecord {
    pub declaration: usize,
    pub id: String,
    pub source: SourceId,
    pub span: Span,
    /// The explicit base path.
    pub path: String,
    /// The declared access mode.
    pub mode: String,
}

// ---------------------------------------------------------------------------
// Step 6a: establishing effective authority and priority
// ---------------------------------------------------------------------------

use crate::engine::Engine;
use crate::syntax;

/// Establish effective authority and priority for every rule clause.
///
/// Two defaults are applied here and both are read from the registry rather
/// than written into this crate:
///
/// * `field_signatures#/blocks/SPECIFICATION/fields/AUTHORITY/default` is
///   `500` — the document's own authority, matching "Effective AUTHORITY is
///   0..1000; local default 500";
/// * every clause's own `AUTHORITY` default is `null`, so a clause that
///   declares none takes its **source document's** authority. That is the same
///   quantity `05_SEMANTICS/04` calls "their source authority" when it caps
///   imported declarations.
///
/// `PRIORITY` is different in kind: its registered default is `0` and the prose
/// adds that it "never inherits from an enclosing or referenced clause". So an
/// absent `PRIORITY` is `0` even inside a document that declares one elsewhere,
/// and no enclosing value is ever consulted.
pub(crate) fn establish(engine: &mut Engine) {
    let document_authority = document_authorities(engine);
    let ceilings = import_ceilings(engine);

    let declarations: Vec<(usize, String, SourceId)> = engine
        .resolved
        .declarations()
        .all()
        .iter()
        .enumerate()
        .map(|(index, d)| (index, d.block.clone(), d.source.clone()))
        .collect();

    for (index, block, source) in declarations {
        let Some(kind) = RuleKind::from_block(&block) else {
            continue;
        };
        let Some(syntax_block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        let declaration = engine
            .resolved
            .declarations()
            .get(index)
            .expect("index came from the same index");
        let id = declaration.id.qualified();
        let span = declaration.id_span;

        let declared_authority =
            syntax::field_integer(syntax_block, "AUTHORITY").and_then(|v| u32::try_from(v).ok());
        let source_authority = document_authority
            .get(&source)
            .copied()
            .unwrap_or_else(|| engine.contracts.authority_bounds().local_default);
        // A clause with no AUTHORITY takes its source document's authority.
        let own = declared_authority.unwrap_or(source_authority);

        // "Imported declarations cannot exceed the lower of their source
        // authority and IMPORT authority."
        let import_ceiling = ceilings.get(&source).copied().flatten();
        let ceiling = match import_ceiling {
            Some(limit) => Some(limit.min(source_authority)),
            None if source != *engine.resolved.root() => Some(source_authority),
            None => None,
        };
        let authority = match ceiling {
            Some(limit) => own.min(limit),
            None => own,
        };

        // `REQUIRE` and `PREFER` declare no OPERATION or TARGET of their own:
        // they name an `ACTION` by reference, and "A reachable required ACTION
        // authorizes exactly its declared OPERATION, TARGET, PARAMETER values,
        // OUTPUT". So the subject a `FORBID` is compared against is the
        // referenced action's, never a field the clause does not have.
        let subject = subject_of(engine, syntax_block);

        let priority_default = engine
            .contracts
            .field_default_integer(&block, "PRIORITY")
            .unwrap_or(engine.contracts.priority_bounds().optional_default as i64);
        let declared_priority = syntax::field_integer(syntax_block, "PRIORITY");
        let priority = declared_priority.unwrap_or(priority_default);
        let priority =
            i32::try_from(priority).unwrap_or(engine.contracts.priority_bounds().minimum);

        engine.authorities.push(AuthorityRecord {
            declaration: index,
            id,
            kind,
            source: source.clone(),
            span,
            authority,
            declared_authority,
            import_ceiling: ceiling,
            priority,
            priority_declared: declared_priority.is_some(),
            // Applicability is decided from `WHEN` once data resolution can
            // supply the values it reads. Until then a clause is applicable:
            // dropping a clause because its condition is not yet evaluated
            // would silently weaken a hard rule.
            applicability: Applicability::Applicable,
            operation: subject.operation,
            target: subject.target,
            scope: scope_of(syntax_block),
        });
    }

    engine.authorities.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.span.start.cmp(&b.span.start))
            .then(a.id.cmp(&b.id))
    });
}

/// Each loaded unit's document authority.
fn document_authorities(engine: &Engine) -> BTreeMap<SourceId, u32> {
    let default = engine
        .contracts
        .field_default_integer("SPECIFICATION", "AUTHORITY")
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(engine.contracts.authority_bounds().local_default);
    let mut out = BTreeMap::new();
    for unit in engine.resolved.units() {
        let declared = unit
            .document()
            .and_then(|document| document.block("SPECIFICATION"))
            .and_then(|block| syntax::field_integer(syntax::DeclBlock::of(block), "AUTHORITY"))
            .and_then(|v| u32::try_from(v).ok());
        out.insert(unit.id().clone(), declared.unwrap_or(default));
    }
    out
}

/// The `IMPORT` authority ceiling each imported unit was loaded under.
///
/// `IMPORT.AUTHORITY` has a registered default of `null`: an import that
/// declares none imposes no ceiling of its own, and only the source document's
/// authority bounds the imported clause.
fn import_ceilings(engine: &Engine) -> BTreeMap<SourceId, Option<u32>> {
    let mut by_import_id: BTreeMap<String, Option<u32>> = BTreeMap::new();
    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        if declaration.block != "IMPORT" {
            continue;
        }
        let declared = syntax::declaration_block(engine.resolved, index)
            .and_then(|block| syntax::field_integer(block, "AUTHORITY"))
            .and_then(|v| u32::try_from(v).ok());
        by_import_id.insert(declaration.id.qualified(), declared);
    }

    let mut out = BTreeMap::new();
    for record in engine.resolved.imports() {
        if let Some(loaded) = record.loaded() {
            let ceiling = by_import_id
                .get(&record.id.qualified())
                .copied()
                .unwrap_or(None);
            // An import chain caps at the tightest ceiling on the path.
            out.entry(loaded.clone())
                .and_modify(|existing: &mut Option<u32>| {
                    *existing = match (*existing, ceiling) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (Some(a), None) => Some(a),
                        (None, b) => b,
                    };
                })
                .or_insert(ceiling);
        }
    }
    out
}

/// The operation and target a clause is about.
struct Subject {
    operation: Option<String>,
    target: Option<String>,
}

/// The subject a clause constrains.
///
/// `ALLOW` and `FORBID` declare `OPERATION` and `TARGET` directly. `REQUIRE`
/// and `PREFER` declare neither; they name an `ACTION`, whose own `OPERATION`
/// and `TARGET` are the subject. A clause naming several actions constrains
/// several subjects, and only the first is recorded here — the rest are
/// recorded by their own action's authorization in phase E, so nothing is
/// dropped.
fn subject_of(engine: &Engine, block: syntax::DeclBlock) -> Subject {
    if let Some(operation) = syntax::field_text(block, "OPERATION") {
        return Subject {
            operation: Some(operation),
            target: target_of(block),
        };
    }
    let Some(field) = block.field("ACTION") else {
        return Subject {
            operation: None,
            target: target_of(block),
        };
    };
    let Some((action_id, _)) = syntax::reference_list(&field.body).into_iter().next() else {
        return Subject {
            operation: None,
            target: None,
        };
    };
    let Some(action) = engine
        .resolved
        .declarations()
        .all()
        .iter()
        .position(|d| d.block == "ACTION" && d.id.qualified() == action_id)
    else {
        return Subject {
            operation: None,
            target: None,
        };
    };
    let Some(action_block) = syntax::declaration_block(engine.resolved, action) else {
        return Subject {
            operation: None,
            target: None,
        };
    };
    Subject {
        operation: syntax::field_text(action_block, "OPERATION"),
        target: target_of(action_block),
    }
}

/// The target a clause names, as a declaration id or a material value.
fn target_of(block: syntax::DeclBlock) -> Option<String> {
    let field = block.field("TARGET")?;
    let expr = syntax::inline_expr(&field.body)?;
    if let Some(id) = syntax::reference_target(expr) {
        return Some(id.to_string());
    }
    // A material target, e.g. `PATH("/a")`. Kept as written so a comparison is
    // against the exact declared value, never a normalized guess.
    Some(crate::eval::render_static(expr))
}

fn scope_of(block: syntax::DeclBlock) -> Option<String> {
    let field = block.field("SCOPE")?;
    let expr = syntax::inline_expr(&field.body)?;
    syntax::reference_target(expr).map(str::to_string)
}
