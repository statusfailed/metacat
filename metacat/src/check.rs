use open_hypergraphs::lax::functor::Functor;
use open_hypergraphs::lax::*;
use std::fmt::Debug;
use thiserror::Error;

//use crate::ssa::{SSA, ssa};
use crate::ssa::{SSAError, ssa};
use crate::theory::{OperationKey, Theory};
use crate::tree::*;
use crate::{dual, dual::Dual};

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("SSA decomposition failed")]
    SSAError(#[from] SSAError),
    #[error("Could not merge values {0} and {1}")]
    MergeError(String, String),
    #[error("Could not pop symbol {1} of {0:?}")]
    MatchError(EdgeId, String),
}

/// Evaluate a type map
pub fn eval_type<O: Clone + Eq + Debug + std::fmt::Display>(
    f: OpenHypergraph<(), Dual<O>>,
) -> Result<Vec<Tree<(), O>>, EvalError> {
    // evaluation state
    let mut state: Vec<Tree<(), O>> = (0..f.hypergraph.nodes.len())
        .map(|i| Tree::Leaf(i, ()))
        .collect();

    for ssa_value in ssa(f.to_strict())? {
        // Symbolic inputs to the op
        let source_values: Vec<Tree<(), O>> = ssa_value
            .sources
            .into_iter()
            .map(|i| state[i.0.0].clone())
            .collect();

        match ssa_value.op {
            // Push a symbol
            Dual::Fwd(arr) => {
                // Write a tree into each target whose root is this 'arr', recording the *output
                // port* i for each value.
                for (i, node_id) in ssa_value.targets.iter().enumerate() {
                    merge(
                        &mut state[node_id.0.0],
                        Tree::Node(arr.clone(), i, source_values.clone()),
                    )?
                }
            }

            // Pop a symbol
            Dual::Rev(op) => {
                // Ensure each input to a Rev op has the expected op label and port,
                // and ensure *all* input trees have the same children.
                let mut children = None;
                for (i, v) in source_values.into_iter().enumerate() {
                    match v {
                        Tree::Node(arr, j, node_children) if i == j && arr == op => {
                            children = match children {
                                None => Some(node_children),
                                Some(children) if children == node_children => Some(children),
                                _ => {
                                    return Err(EvalError::MatchError(
                                        ssa_value.edge_id,
                                        format!("{op:?} (children didn't match)"),
                                    ));
                                }
                            }
                        }
                        _ => {
                            return Err(EvalError::MatchError(
                                ssa_value.edge_id,
                                format!("{op:?}"),
                            ));
                        }
                    }
                }

                // TODO: is this correct?
                let children =
                    children.unwrap_or_else(|| vec![Tree::Empty; ssa_value.targets.len()]);
                for (node_id, child) in ssa_value.targets.iter().zip(children.into_iter()) {
                    merge(&mut state[node_id.0.0], child)?;
                }
            }
        };
    }

    // Return final eval state
    Ok(state)
}

pub fn merge<O: Debug + Eq>(value: &mut Tree<(), O>, new: Tree<(), O>) -> Result<(), EvalError> {
    // Overwrite a Leaf, but ensure other values are equal
    match value {
        Tree::Leaf(_, _) => *value = new,
        t => {
            if *t != new {
                return Err(EvalError::MergeError(
                    format!("{:?}", t),
                    format!("{:?}", new),
                ));
            }
        }
    }

    Ok(())
}

/// Compute the type map of a given term
pub fn to_type_map<O: Clone>(
    theory: Theory<O>,
    source: OpenHypergraph<(), O>,
    target: OpenHypergraph<(), O>,
    arrow: &OpenHypergraph<(), OperationKey>,
) -> OpenHypergraph<(), Dual<O>> {
    // The dualizer functor maps each generator in `arrow` into src†;tgt
    let type_map = AsType(theory).map_arrow(&arrow);

    // TODO: remove unwrap()
    let mut result = dual::into_fwd(source)
        .compose(&type_map)
        .unwrap()
        .compose(&dual::into_rev(target))
        .unwrap();

    result.quotient();
    result
}

/// Map generating arrows of a Theory into the composites `(src† ; tgt)`
// TODO: remove Clone; currently required unnecessarily by open hypergraphs functor impl
#[derive(Clone)]
pub struct AsType<O>(pub Theory<O>);

// wi, wn both are OperationKey
impl<O: Clone> Functor<(), OperationKey, (), Dual<O>> for AsType<O> {
    fn map_object(&self, _: &()) -> impl ExactSizeIterator<Item = ()> {
        vec![()].into_iter()
    }

    fn map_operation(
        &self,
        a: &OperationKey,
        source: &[()],
        target: &[()],
    ) -> OpenHypergraph<(), Dual<O>> {
        let (s, t) = self.0.type_maps(a);

        // assert source/target consistent with syntax
        assert_eq!(source.len(), s.targets.len());
        assert_eq!(target.len(), t.targets.len());

        dual::into_rev(s.clone())
            .compose(&dual::into_fwd(t.clone()))
            .unwrap()
    }

    fn map_arrow(&self, f: &OpenHypergraph<(), OperationKey>) -> OpenHypergraph<(), Dual<O>> {
        functor::define_map_arrow(self, f)
    }
}
