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
//! A complete file can be loaded directly from a string:
//!
//! ```rust
//! use metacat::new_syntax::{File, Theory, TheoryId};
//!
//! let file = File::from_text(
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
