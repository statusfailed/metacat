//! # Lift functors into categories of definitions

use open_hypergraphs::lax::{
    OpenHypergraph,
    functor::{Functor, define_map_arrow},
};

pub use super::types::*;

/// Given an identity-on-objects functor F : C → D,
/// lift it to one Def(C) → Def(D).
#[derive(Clone, PartialEq)]
pub struct Lift<F> {
    functor: F,
}

impl<F: Functor<O, A1, O, A2> + Clone, K: Clone, O: Clone + PartialEq, A1: Clone, A2: Clone>
    Functor<O, Def<K, A1>, O, Def<K, A2>> for Lift<F>
{
    fn map_object(&self, o: &O) -> impl ExactSizeIterator<Item = O> {
        std::iter::once(o.clone())
    }

    fn map_operation(
        &self,
        a: &Def<K, A1>,
        source: &[O],
        target: &[O],
    ) -> OpenHypergraph<O, Def<K, A2>> {
        match a {
            Def::Def(k) => {
                OpenHypergraph::singleton(Def::Def(k.clone()), source.to_vec(), target.to_vec())
            }
            Def::Arr(a) => {
                let g = self.functor.map_operation(a, source, target);
                g.map_edges(|e| Def::Arr(e))
            }
        }
    }

    fn map_arrow(&self, f: &OpenHypergraph<O, Def<K, A1>>) -> OpenHypergraph<O, Def<K, A2>> {
        define_map_arrow(self, f)
    }
}

/// Apply a functor lifted to a category of definitions.
pub fn lift<F: Functor<O, T, O, U> + Clone, K: Clone, O: Clone + PartialEq, T: Clone, U: Clone>(
    functor: F,
    term: OpenHypergraph<O, Def<K, T>>,
) -> OpenHypergraph<O, Def<K, U>> {
    Lift { functor }.map_arrow(&term)
}
