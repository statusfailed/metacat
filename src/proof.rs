/// User-defined signatures
use open_hypergraphs::{
    category::*,
    lax::{OpenHypergraph, var::HasVar},
};
use std::collections::HashMap;

use crate::definition::Def;
use crate::definition::inline::inline;
use crate::fol::FOL;
use crate::interpreter::{Interpreter, InterpreterError};
use crate::lang;
use crate::tree::Tree;

pub type Path = String;

// source/target type maps for both definitions and declarations
#[derive(Debug, Clone, PartialEq)]
pub struct Type<T> {
    pub source: lang::Term<T>,
    pub target: lang::Term<T>,
}

impl<T: Clone> Type<T> {
    pub fn composed(&self) -> lang::Term<T> {
        crate::dual::dual(self.source.clone())
            .compose(&self.target)
            .unwrap()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Copy,
}

impl Prim {
    pub fn to_arr<T>(self) -> lang::Arr<T> {
        lang::Arr::Copy
    }
}

impl HasVar for Prim {
    fn var() -> Self {
        Prim::Copy
    }
}

// every arrow is simply a name.
// Whether that name is declared (an axiom) or defined (a proof) is user-defined.
pub type Arr = Def<Path, Prim>;

// A lookup from each axiom or proof to its type map
pub type Env<T> = HashMap<Path, lang::Term<T>>;

// Objects are the same as the underlying signature.
// TODO: Obj should be in FOL, not in lang!
pub use crate::lang::Obj;
pub type Proof = OpenHypergraph<Obj, Arr>;

// check a proof
// TODO: missing the "claimed" type of the term!
pub fn check(proof: Proof, env: Env<FOL>) -> Result<Vec<Tree<Obj, FOL>>, InterpreterError> {
    let mapped = proof.map_edges(|e| match e {
        Def::Arr(a) => Def::Arr(a.to_arr()),
        Def::Def(d) => Def::Def(d),
    });

    // map every operation to its type maps.
    let inlined = inline(env, mapped).unwrap();

    // What are the inputs to the interpreter???
    //  → FREE VARIABLES
    // What are the outputs?
    // If a proof has free variables, we can't really check it??
    // WRONG: we need to use its *type maps* to generate them!
    // Let's assume it's empty for now.

    // TODO: convert the output (HashMap<NodeId, Value>) to a Vec<Value>. NodeId are newtype for
    // usize.
    Ok(Interpreter.run_state(inlined, vec![])?.0)
}
