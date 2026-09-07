//! The value model, which is M5's.
//!
//! ## Why there is no second `Value`
//!
//! `lcl_semantics::Value` already represents every material family plus the
//! three non-material sentinels, and `lcl_checker::numeric` already implements
//! exact base-10 arithmetic beneath it. A runtime-local value type would be a
//! second value model of the same language, and the first divergence between
//! them would be a silent semantic fork. So this crate adopts M5's type
//! verbatim and adds only what a *runtime* has and a preflight does not.
//!
//! ## What a runtime adds
//!
//! Two things, and neither is a new variant:
//!
//! 1. **Loop-instance reference identity.** `05_SEMANTICS/12` says two
//!    `REFERENCE` values "compare resolved declaration or loop-instance
//!    identity", and `05_SEMANTICS/01` says "Distinct loop instances have
//!    distinct local binding identities". [`reference_identity`] therefore
//!    qualifies a reference to a loop-local with the exact instance path, so
//!    two instances' references are unequal while two references to the same
//!    instance are equal.
//!
//! 2. **The immediate quantifier sequence**, which is deliberately *not* a
//!    `Value`. `05_SEMANTICS/12`: "such a sequence is non-material and exists
//!    only while that call reduces it; it cannot be stored, nested as data,
//!    indexed, returned, or passed through another function." Making it a
//!    `Value` variant would make all four of those spellable. It lives in
//!    `crate::eval` as an evaluator-internal type instead, which is what
//!    "exists only while that call reduces it" means in a type system.

pub use lcl_semantics::Value;

use crate::state::IterationPath;

/// The retained identity of a reference to `id` seen from `iteration`.
///
/// A reference to an ordinary declaration is its qualified id. A reference to a
/// loop-local binding is that id qualified by the exact instance path, because
/// the two are different identities under `05_SEMANTICS/01`.
pub fn reference_identity(id: &str, iteration: &IterationPath, loop_local: bool) -> String {
    if loop_local && !iteration.is_root() {
        format!("{id}#{iteration}")
    } else {
        id.to_string()
    }
}

/// True for a value that may bind an `OUTPUT`.
///
/// `05_SEMANTICS/05`: "MISSING and UNKNOWN are non-material and never bind or
/// partially bind OUTPUT. NULL binds only when the OUTPUT type or schema
/// explicitly permits NULL." The `NULL` half is the caller's, which knows the
/// declared type; this answers only the non-material half.
pub fn can_bind(value: &Value) -> bool {
    value.is_material()
}
