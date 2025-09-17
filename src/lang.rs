/// User-defined signatures
use open_hypergraphs::lax::OpenHypergraph;
use open_hypergraphs::lax::var;
use std::cell::RefCell;
use std::rc::Rc;

/// Arity/Coarity pair
pub type Biprofile = (usize, usize);
pub type Term<T> = OpenHypergraph<Obj, Arr<T>>;
pub type Builder<T> = Rc<RefCell<Term<T>>>;
pub type Var<T> = var::Var<Obj, Arr<T>>;

/// A lookup table of T to a fixed biprofile
/// NOTE: T should be a "checked" type - this might be checked *externally* against user-defined
/// stuff, but it should be 'pre-checked'!
/// TODO: maybe this should be a trait?
/*
#[derive(Debug, Clone)]
pub struct Signature<T> {
    // TODO: arity/coarity
    pub symbols: HashMap<T, Biprofile>,
}
*/

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Obj;

impl std::fmt::Display for Obj {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
}

/// Operations in the syntax hypergraph
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arr<T> {
    Copy,   // Copy a value. 1 → N
    Equal,  // Unify two expressions. N → 1
    Fwd(T), // T : N → 1
    Rev(T), // T* : 1 → N
}

impl<T> var::HasVar for Arr<T> {
    fn var() -> Self {
        Arr::Copy
    }
}

impl<T: std::fmt::Display> std::fmt::Display for Arr<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Arr::Copy => write!(f, "Δ"),
            Arr::Equal => write!(f, "="),
            Arr::Fwd(t) => write!(f, "{}", t),
            Arr::Rev(t) => write!(f, "{}*", t),
        }
    }
}

impl<T: Clone> Arr<T> {
    pub fn dual(self) -> Self {
        match self {
            Arr::Copy => Arr::Equal,
            Arr::Equal => Arr::Copy,
            Arr::Fwd(t) => Arr::Rev(t),
            Arr::Rev(t) => Arr::Fwd(t),
        }
    }
}
