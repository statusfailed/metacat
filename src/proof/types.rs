/// User-defined signatures
use open_hypergraphs::{
    category::*,
    lax::{OpenHypergraph, var::HasVar},
};
use std::collections::HashMap;

use crate::core;
use crate::definition::Def;
use crate::tree::Tree;

// Objects are the same as the underlying signature
// TODO: this should be user-defined!
pub use crate::core::Obj;

// every arrow is either a name (proof/axiom) or primitive (Copy)
// Whether that name is an axiom (i.e., declared) or a proof (i.e., defined) is up to the user.
pub type Arr = Def<Path, Prim>;

/// Proofs
pub type Proof = OpenHypergraph<Obj, Arr>;

/// A lookup from each axiom or proof to its type map
pub type Env<T> = HashMap<Path, core::Term<T>>;

/// The name of an axiom or theorem
pub type Path = String;

// source/target type maps for both definitions and declarations
#[derive(Debug, Clone, PartialEq)]
pub struct Type<T> {
    pub source: core::Term<T>,
    pub target: core::Term<T>,
}

impl<T: Clone> Type<T> {
    pub fn to_core(&self) -> core::Term<T> {
        crate::dual::dual(self.source.clone())
            .compose(&self.target)
            .unwrap()
    }
}

/// Primitive operations available in all proofs.
#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Copy,
}

impl Prim {
    pub fn to_arr<T>(self) -> core::Arr<T> {
        core::Arr::Copy
    }
}

impl HasVar for Prim {
    fn var() -> Self {
        Prim::Copy
    }
}

use super::interpreter::{Interpreter, InterpreterError};

// check a proof, returning a syntax tree for each node of the proof
// TODO: missing the "claimed" type of the term!
pub fn check<T: PartialEq + Clone>(
    env: Env<T>,
    proof: Proof,
) -> Result<Vec<Tree<Obj, T>>, InterpreterError> {
    let values = vec![];
    let result = Interpreter::new(env).run(proof, values);
    result
}
