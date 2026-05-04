//! Loader and data model for the multi-theory surface syntax.
//!
//! This module provides the primary parsing and resolution pipeline for files
//! of the form:
//!
//! ```text
//! (theory fol.syntax nat { ... })
//! (theory fol.proof fol.syntax { ... })
//! ```
//!
//! A complete file can be loaded directly from a string:
//!
//! ```rust
//! use metacat::theory::{Theory, TheoryId, TheorySet};
//!
//! let file = TheorySet::from_text(
//!     r#"
//!     (theory fol.syntax nat {
//!       (arr wff : 1 -> 1)
//!       (arr -> : 2 -> 1)
//!       (arr -. : 1 -> 1)
//!     })
//!
//!     (theory fol.proof fol.syntax {
//!       (arr wn : wff -> (-. wff))
//!       (arr wi : {wff wff} -> (-> wff))
//!       (def win : {wff wff} -> (-> -. wff) = (wi wn))
//!     })
//!     "#,
//! )?;
//!
//! let syntax_id = TheoryId("fol.syntax".parse()?);
//! let proof_id = TheoryId("fol.proof".parse()?);
//!
//! assert!(matches!(file.theories.get(&syntax_id), Some(Theory::Theory { .. })));
//! assert!(matches!(file.theories.get(&proof_id), Some(Theory::Theory { .. })));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Multiple source strings can be loaded together before resolution:
//!
//! ```rust
//! use metacat::theory::{Theory, TheoryId, TheorySet};
//!
//! let file = TheorySet::from_texts([
//!     r#"
//!     (theory fol.syntax nat {
//!       (arr wff : 1 -> 1)
//!       (arr -> : 2 -> 1)
//!     })
//!     "#,
//!     r#"
//!     (theory fol.proof fol.syntax {
//!       (arr wi : {wff wff} -> (-> wff))
//!     })
//!     "#,
//! ])?;
//!
//! let proof_id = TheoryId("fol.proof".parse()?);
//! assert!(matches!(file.theories.get(&proof_id), Some(Theory::Theory { .. })));
//! # Ok::<(), Box<dyn std::error::Error>>(())
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

pub use ast::{MergeRawError, RawTheorySet};
pub use load::LoadError;
pub use model::{Term, Theory, TheoryArrow, TheoryId, TheorySet};
