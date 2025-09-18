use open_hypergraphs::lax::{OpenHypergraph, var::*};
use std::cell::RefCell;
use std::rc::Rc;

pub fn build_typed<const ARITY: usize, F, O: Clone, A: HasVar + Clone>(
    source_types: [O; ARITY],
    f: F,
) -> BuildResult<O, A>
where
    F: Fn(&Rc<RefCell<OpenHypergraph<O, A>>>, [Var<O, A>; ARITY]) -> Vec<Var<O, A>>,
{
    use std::array;
    build(move |state| {
        // use from_fn to avoid having to clone source_types
        let sources: [Var<_, _>; ARITY] =
            array::from_fn(|i| Var::new(state.clone(), source_types[i].clone()));

        let sources_vec = sources.iter().cloned().collect();
        let targets = f(state, sources);
        (sources_vec, targets)
    })
}

pub fn iter_to_array<T, const N: usize>(iter: impl Iterator<Item = T>) -> Option<[T; N]> {
    iter.collect::<Vec<T>>().try_into().ok()
}
