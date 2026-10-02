//! Syntax and Boolean-matrix semantics for finite relations.

pub mod semantics;
pub mod syntax;

pub use semantics::{FinRelError, FiniteRelation};
pub use syntax::{FinRelOp, FinRelSignature, FinRelSignatureError, FinRelTerm};
