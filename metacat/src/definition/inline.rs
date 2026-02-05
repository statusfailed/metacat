//! # Inline operations of categories with definitions

use open_hypergraphs::lax::{
    OpenHypergraph,
    functor::{Functor, try_define_map_arrow},
};

pub use super::types::*;

use std::collections::HashMap;
use std::hash::Hash;

#[derive(Clone)]
pub struct Inline<K, O, A> {
    env: HashMap<K, OpenHypergraph<O, A>>,
}

// Def<String, Copy> → lang::Term
// Copy → Copy

// TODO: generalise definition?
impl<K: Clone + Hash + Eq, O: Clone + PartialEq, A: Clone> Functor<O, Def<K, A>, O, Option<A>>
    for Inline<K, O, A>
{
    fn map_object(&self, o: &O) -> impl ExactSizeIterator<Item = O> {
        std::iter::once(o.clone())
    }

    fn map_operation(
        &self,
        a: &Def<K, A>,
        source: &[O],
        target: &[O],
    ) -> OpenHypergraph<O, Option<A>> {
        match a {
            Def::Def(k) => match self.env.get(k) {
                Some(term) => term.clone().map_edges(Some),
                None => OpenHypergraph::singleton(None, source.to_vec(), target.to_vec()),
            },
            Def::Arr(a) => {
                OpenHypergraph::singleton(Some(a.clone()), source.to_vec(), target.to_vec())
            }
        }
    }

    fn map_arrow(&self, f: &OpenHypergraph<O, Def<K, A>>) -> OpenHypergraph<O, Option<A>> {
        try_define_map_arrow(self, f).unwrap()
    }
}

// TODO: error with missing definitions!
pub fn inline<K: Clone + Hash + Eq, O: Clone + PartialEq, A: Clone + PartialEq>(
    env: HashMap<K, OpenHypergraph<O, A>>,
    term: OpenHypergraph<O, Def<K, A>>,
) -> Option<OpenHypergraph<O, A>> {
    let result = Inline { env }.map_arrow(&term);
    result.with_edges(|edges| match edges.into_iter().collect() {
        Some(e) => e,
        None => vec![], // TODO: this is a naughty hack which nevertheless works
    })
}
