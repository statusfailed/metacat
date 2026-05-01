//! Resolved data model for the multi-theory syntax.
//!
//! In contrast to [`super::ast`], this layer stores interpreted terms:
//! - a theory has a resolved syntax base, either builtin `nat` or another
//!   theory;
//! - each arrow stores interpreted type maps in its syntax base;
//! - definitions, when present, are interpreted as terms in the theory itself.
//!
//! The local [`hexpr::Signature`] adapter defined here is what allows
//! definition bodies to be interpreted against a theory once its arrow
//! declarations have been registered.

use super::nat::Nat;
use hexpr::{Operation, Signature};
use open_hypergraphs::lax::OpenHypergraph;
use std::collections::HashMap;

/// A term in the builtin `nat` syntax category.
pub type NatTerm = OpenHypergraph<(), Nat>;

/// A term in a resolved user theory, whose edges are local arrow keys.
pub type TheoryTerm = OpenHypergraph<(), ArrowKey>;

/// Stable identifier for a resolved theory within a loaded file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TheoryId(pub Operation);

/// Key for an arrow declared in a specific resolved theory.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ArrowKey {
    pub theory: TheoryId,
    pub name: Operation,
}

/// The syntax category in which a theory's type maps are interpreted.
#[derive(Clone, Debug)]
pub enum SyntaxBase {
    /// The builtin `nat` syntax category.
    Nat,
    /// Another resolved theory, used as a syntax category.
    Theory(TheoryId),
}

/// An interpreted type-map term, tagged by the syntax category it belongs to.
#[derive(Clone, Debug)]
pub enum SyntaxTerm {
    /// A term in builtin `nat`.
    Nat(NatTerm),
    /// A term in another resolved theory used as syntax.
    Theory(TheoryTerm),
}

/// Fully resolved contents of a multi-theory source file.
#[derive(Clone, Debug)]
pub struct File {
    pub theories: HashMap<TheoryId, Theory>,
}

/// A resolved theory together with its syntax base and declared arrows.
#[derive(Clone, Debug)]
pub struct Theory {
    pub id: TheoryId,
    pub syntax_base: SyntaxBase,
    pub arrows: HashMap<ArrowKey, TheoryArrow>,
}

/// A resolved arrow declaration, optionally with a definitional body.
#[derive(Clone, Debug)]
pub struct TheoryArrow {
    pub key: ArrowKey,
    pub type_maps: (SyntaxTerm, SyntaxTerm),
    pub definition: Option<TheoryTerm>,
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

impl std::fmt::Display for ArrowKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.theory, self.name)
    }
}

impl SyntaxTerm {
    pub fn source_arity(&self) -> usize {
        match self {
            SyntaxTerm::Nat(term) => term.sources.len(),
            SyntaxTerm::Theory(term) => term.sources.len(),
        }
    }

    pub fn target_arity(&self) -> usize {
        match self {
            SyntaxTerm::Nat(term) => term.targets.len(),
            SyntaxTerm::Theory(term) => term.targets.len(),
        }
    }
}

impl Theory {
    pub fn get_arrow_key(&self, op: &Operation) -> Option<ArrowKey> {
        self.arrows
            .keys()
            .find(|key| key.name == *op)
            .cloned()
    }

    pub fn get_arrow(&self, key: &ArrowKey) -> Option<&TheoryArrow> {
        self.arrows.get(key)
    }

    pub fn local_signature(&self) -> TheorySignature<'_> {
        TheorySignature { theory: self }
    }
}

pub struct TheorySignature<'a> {
    pub theory: &'a Theory,
}

impl Signature for TheorySignature<'_> {
    type Arr = ArrowKey;
    type Obj = ();
    type Error = SignatureError;

    fn try_parse_op(&self, op: &Operation) -> Result<Self::Arr, Self::Error> {
        self.theory
            .get_arrow_key(op)
            .ok_or_else(|| SignatureError::NoSuchOperation(op.clone()))
    }

    fn profile(&self, op: &Self::Arr) -> (Vec<Option<Self::Obj>>, Vec<Option<Self::Obj>>) {
        let arrow = &self.theory.arrows[op];
        let (source_map, target_map) = &arrow.type_maps;
        (
            vec![Some(()); source_map.target_arity()],
            vec![Some(()); target_map.target_arity()],
        )
    }
}
