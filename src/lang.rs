/// User-defined signatures
use open_hypergraphs::lax::OpenHypergraph;
use open_hypergraphs::lax::var;
use std::cell::RefCell;
use std::rc::Rc;

/// Arity/Coarity pair
pub type Biprofile = (usize, usize);
pub type Term<T> = OpenHypergraph<(), Arr<T>>;
pub type Builder<T> = Rc<RefCell<Term<T>>>;
pub type Var<T> = var::Var<(), Arr<T>>;

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

/// Operations in the syntax hypergraph
#[derive(Debug, Clone)]
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
