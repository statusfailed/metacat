//! Experimental loader and data model for the multi-theory surface syntax.
//!
//! This module is intentionally additive: it does not replace the existing
//! single-theory loader in [`crate::syntax`], but provides a separate parsing
//! and resolution pipeline for files of the form:
//!
//! ```text
//! (theory fol.syntax nat { ... })
//! (theory fol.proof fol.syntax { ... })
//! ```
//!
//! The implementation is split into:
//! - [`ast`], which parses raw hexprs into a file-level AST;
//! - [`nat`], which defines the builtin `nat` syntax category;
//! - [`model`], which defines the resolved in-memory representation;
//! - [`load`], which resolves theory dependencies and interprets type maps and
//!   definitions into open hypergraphs.

pub mod ast;
pub mod load;
pub mod model;
pub mod nat;

pub use ast::{RawFile, RawTheory, RawTheoryArrow};
pub use load::LoadError;
pub use model::{File, Term, Theory, TheoryArrow, TheoryId};
