//! Resolved data model for the multi-theory syntax.
//!
//! In contrast to [`super::ast`], this layer stores interpreted terms:
//! - a user theory refers to its syntax category by [`TheoryId`];
//! - each arrow stores interpreted type maps in its syntax base;
//! - definitions, when present, are interpreted as terms in the theory itself.
//!
//! The local [`hexpr::Signature`] adapter defined here is what allows
//! definition bodies to be interpreted against a theory once its arrow
//! declarations have been registered.

use super::ast::RawTheoryArrow;
use hexpr::{Operation, Signature};
use open_hypergraphs::lax::OpenHypergraph;
use std::collections::BTreeMap;

/// An interpreted term in either a syntax base or a resolved theory.
pub type Term = OpenHypergraph<(), Operation>;

/// Stable identifier for a resolved theory within a loaded file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TheoryId(pub Operation);

/// Fully resolved contents of a multi-theory source file.
#[derive(Clone, Debug)]
pub struct TheorySet {
    pub theories: BTreeMap<TheoryId, Theory>,
}

/// A resolved theory in a loaded file.
#[derive(Clone, Debug)]
pub enum Theory {
    /// The builtin `nat` syntax category.
    Nat,
    /// A theory over some syntax category.
    Theory {
        syntax: TheoryId,
        arrows: BTreeMap<Operation, TheoryArrow>,
    },
}

/// A resolved arrow declaration, optionally with a definitional body.
#[derive(Clone, Debug)]
pub struct TheoryArrow {
    pub raw: RawTheoryArrow,
    pub name: Operation,
    pub type_maps: (Term, Term),
    pub definition: Option<Term>,
}

/// Error returned when interpreting hexprs against a resolved theory signature.
#[derive(Clone, Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("No such operation {0}")]
    NoSuchOperation(Operation),
}

impl TheoryId {
    pub fn new(name: Operation) -> Self {
        Self(name)
    }
}

impl std::fmt::Display for TheoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Theory {
    pub fn get_arrow(&self, op: &Operation) -> Option<&TheoryArrow> {
        match self {
            Theory::Nat => None,
            Theory::Theory { arrows, .. } => arrows.get(op),
        }
    }

    pub fn local_signature(&self) -> TheorySignature<'_> {
        TheorySignature { theory: self }
    }
}

pub struct TheorySignature<'a> {
    pub theory: &'a Theory,
}

impl Signature for TheorySignature<'_> {
    type Arr = Operation;
    type Obj = ();
    type Error = SignatureError;

    fn try_parse_op(&self, op: &Operation) -> Result<Self::Arr, Self::Error> {
        self.theory
            .get_arrow(op)
            .map(|_| op.clone())
            .ok_or_else(|| SignatureError::NoSuchOperation(op.clone()))
    }

    fn profile(&self, op: &Self::Arr) -> (Vec<Option<Self::Obj>>, Vec<Option<Self::Obj>>) {
        let arrow = self
            .theory
            .get_arrow(op)
            .expect("TheorySignature::profile called on unresolved operation");
        let (source_map, target_map) = &arrow.type_maps;
        (
            vec![Some(()); source_map.targets.len()],
            vec![Some(()); target_map.targets.len()],
        )
    }
}
