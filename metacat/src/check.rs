use open_hypergraphs::lax::functor::{self, Functor};
use open_hypergraphs::lax::*;
use open_hypergraphs::strict::vec::FiniteFunction;
use std::fmt::Debug;
use thiserror::Error;

//use crate::ssa::{SSA, ssa};
use crate::spiders::{extract_spiders, extract_spiders_with_node_map, ExtractSpidersError, WithSpiders};
use crate::ssa::{SSAError, ssa};
use crate::theory::Theory;
use crate::tree::*;
use crate::{dual, dual::Dual};
use hexpr::Operation;

#[derive(Debug, Error)]
pub enum Error<O> {
    #[error("SSA decomposition failed")]
    SSAError(#[from] SSAError),
    #[error("Type maps had invalid arity/coarity")]
    InvalidTypeMaps,
    #[error("Error during type map evaluation {0:?}")]
    PartialResult(#[from] PartialResult<O>),
    #[error("Unable to quotient type map {0:?}")]
    InvalidQuotient(FiniteFunction),
    #[error("Unable to extract spiders")]
    ExtractSpiders(#[from] ExtractSpidersError),
}

#[derive(Debug, Error)]
pub struct PartialResult<O> {
    pub partial_result: Vec<Option<Tree<(), O>>>,
    pub cause: EvalError,
}

// TODO: include location info (NodeId)
#[derive(Debug, Error)]
pub enum EvalError {
    #[error("Could not merge values {0} and {1}")]
    MergeError(String, String),
    #[error("Could not pop symbol {1} of {0:?}")]
    MatchError(EdgeId, String),
}

/// Typecheck a term, returning an assignment of "types" to each of its nodes.
pub fn check(
    theory: &Theory,
    source: OpenHypergraph<(), Operation>,
    target: OpenHypergraph<(), Operation>,
    arrow: &mut OpenHypergraph<(), Operation>,
) -> Result<Vec<Tree<(), Operation>>, Error<Operation>> {
    //////////////////////////////////////////
    // Compute the *type map* `source ; arrow.s† ; arrow.t ; target†`
    let mut fwd = dual::into_fwd(source).map_edges(WithSpiders::Operation);
    let mut rev = dual::into_rev(target).map_edges(WithSpiders::Operation);
    fwd.quotient().map_err(Error::InvalidQuotient)?;
    rev.quotient().map_err(Error::InvalidQuotient)?;
    arrow.quotient().map_err(Error::InvalidQuotient)?;

    // Compute the type map and witness, telling us *where the type map is*
    let (type_map, witness) = functor::map_arrow_witness(&AsType(theory), arrow)
        .ok_or(Error::<Operation>::InvalidTypeMaps)?;

    // Compose together laxly
    let type_term = fwd
        .lax_compose(&type_map)
        .and_then(|f| f.lax_compose(&rev))
        .ok_or(Error::<Operation>::InvalidTypeMaps)?;
    let extraction = extract_spiders_with_node_map(&type_term)?;
    let type_term = extraction.graph.map_edges(flatten_spiders);

    //////////////////////////////////////////
    // Compute types, then select only those from nodes corresponding to nodes in the original term

    let offset = fwd.hypergraph.nodes.len();
    let indices = witness_indices(&witness, offset, &extraction.node_map)
        .ok_or(Error::<Operation>::InvalidTypeMaps)?;

    let results = eval_type(type_term).map_err(|err| project_check_error(err, &indices))?;
    Ok(indices.iter().map(|&i| results[i].clone()).collect())
}

fn flatten_spiders<O>(op: WithSpiders<(), WithSpiders<(), Dual<O>>>) -> WithSpiders<(), Dual<O>> {
    match op {
        WithSpiders::Spider(()) => WithSpiders::Spider(()),
        WithSpiders::Operation(inner) => inner,
    }
}

fn witness_indices(
    witness: &open_hypergraphs::strict::vec::IndexedCoproduct<FiniteFunction>,
    offset: usize,
    node_map: &[Option<NodeId>],
) -> Option<Vec<usize>> {
    let mut cursor = 0;
    let mut result = Vec::with_capacity(witness.sources.table.len());
    for node in 0..witness.sources.table.len() {
        let segment_len = witness.sources.table[node];
        let mapped_node = match segment_len {
            0 => return None,
            _ => witness.values.table[cursor],
        };
        let extracted_node = node_map[offset + mapped_node]?;
        result.push(extracted_node.0);
        cursor += segment_len;
    }
    Some(result)
}

/// Evaluate a type map
pub fn eval_type<O: Clone + Eq + Debug + std::fmt::Display>(
    f: OpenHypergraph<(), WithSpiders<(), Dual<O>>>,
) -> Result<Vec<Tree<(), O>>, Error<O>> {
    // evaluation state initialized all to None, so that source `s` becomes `Leaf s`
    let state: Vec<Option<Tree<(), O>>> = vec![None; f.hypergraph.nodes.len()];
    eval_type_with(f, state)
}

pub fn eval_type_with<O: Clone + Eq + Debug + std::fmt::Display>(
    f: OpenHypergraph<(), WithSpiders<(), Dual<O>>>,
    mut state: Vec<Option<Tree<(), O>>>,
) -> Result<Vec<Tree<(), O>>, Error<O>> {
    for ssa_value in ssa(f.to_strict())? {
        // Symbolic inputs to the op
        let source_values: Vec<Tree<(), O>> = ssa_value
            .sources
            .into_iter()
            .map(|i| {
                state[i.0.0]
                    .clone()
                    .unwrap_or_else(|| Tree::Leaf(i.0.0, ()))
            })
            .collect();

        match ssa_value.op {
            // Push a symbol
            WithSpiders::Operation(Dual::Fwd(arr)) => {
                // Write a tree into each target whose root is this 'arr', recording the *output
                // port* i for each value.
                for (i, node_id) in ssa_value.targets.iter().enumerate() {
                    merge(
                        &mut state[node_id.0.0],
                        Tree::Node(arr.clone(), i, source_values.clone()),
                    )
                    .map_err(|cause| PartialResult {
                        cause,
                        partial_result: state.clone(),
                    })?;
                }
            }

            // Pop a symbol
            WithSpiders::Operation(Dual::Rev(op)) => {
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
                                    return Err(PartialResult {
                                        partial_result: state,
                                        cause: EvalError::MatchError(
                                            ssa_value.edge_id,
                                            format!("{op:?} (children didn't match)"),
                                        ),
                                    }
                                    .into());
                                }
                            }
                        }
                        _ => {
                            return Err(PartialResult {
                                partial_result: state,
                                cause: EvalError::MatchError(ssa_value.edge_id, format!("{op:?}")),
                            }
                            .into());
                        }
                    }
                }

                // TODO: is this correct?
                let children =
                    children.unwrap_or_else(|| vec![Tree::Empty; ssa_value.targets.len()]);
                for (node_id, child) in ssa_value.targets.iter().zip(children.into_iter()) {
                    merge(&mut state[node_id.0.0], child).map_err(|cause| PartialResult {
                        cause,
                        partial_result: state.clone(),
                    })?;
                }
            }

            WithSpiders::Spider(()) => {
                let value = if let Some(first) = source_values.first().cloned() {
                    for other in source_values.iter().skip(1) {
                        if *other != first {
                            return Err(PartialResult {
                                partial_result: state,
                                cause: EvalError::MergeError(
                                    format!("{:?}", first),
                                    format!("{:?}", other),
                                ),
                            }
                            .into());
                        }
                    }
                    first
                } else {
                    Tree::Leaf(state.len() + ssa_value.edge_id.0, ())
                };

                for node_id in ssa_value.targets.iter() {
                    merge(&mut state[node_id.0.0], value.clone()).map_err(|cause| PartialResult {
                        cause,
                        partial_result: state.clone(),
                    })?;
                }
            }
        };
    }

    // Return final eval state
    Ok(state
        .into_iter()
        .enumerate()
        .map(|(i, opt)| opt.unwrap_or_else(|| Tree::Leaf(i, ())))
        .collect())
}

pub fn merge<O: Debug + Eq>(
    value: &mut Option<Tree<(), O>>,
    new: Tree<(), O>,
) -> Result<(), EvalError> {
    // Overwrite None, but ensure other values are equal
    match value {
        None => *value = Some(new),
        Some(t) => {
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

/// Map generating arrows of a Theory into the composites `(src† ; tgt)`
#[derive(Clone)]
struct AsType<'a>(pub &'a Theory);

impl Functor<(), Operation, (), WithSpiders<(), Dual<Operation>>> for AsType<'_> {
    fn map_object(&self, _: &()) -> impl ExactSizeIterator<Item = ()> {
        vec![()].into_iter()
    }

    fn map_operation(
        &self,
        a: &Operation,
        source: &[()],
        target: &[()],
    ) -> OpenHypergraph<(), WithSpiders<(), Dual<Operation>>> {
        let arrow = self.0.get_arrow(a).expect("missing arrow in theory");
        let (s, t) = &arrow.type_maps;

        // assert source/target consistent with syntax
        assert_eq!(source.len(), s.targets.len());
        assert_eq!(target.len(), t.targets.len());

        let type_map = dual::into_rev(s.clone())
            .compose(&dual::into_fwd(t.clone()))
            .unwrap();
        extract_spiders(&type_map).expect("type maps have unit node labels")
    }

    fn map_arrow(&self, f: &OpenHypergraph<(), Operation>) -> OpenHypergraph<(), WithSpiders<(), Dual<Operation>>> {
        functor::try_define_map_arrow(self, f).unwrap()
    }
}

fn project_check_error<O: Clone>(err: Error<O>, indices: &[usize]) -> Error<O> {
    match err {
        Error::PartialResult(partial) => Error::PartialResult(project_partial_result(partial, indices)),
        other => other,
    }
}

fn project_partial_result<O: Clone>(
    partial: PartialResult<O>,
    indices: &[usize],
) -> PartialResult<O> {
    PartialResult {
        partial_result: indices
            .iter()
            .map(|&i| partial.partial_result[i].clone())
            .collect(),
        cause: partial.cause,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_partial_results_to_input_term_nodes() {
        let partial = PartialResult {
            partial_result: vec![
                Some(Tree::<(), &str>::Leaf(0, ())),
                None,
                Some(Tree::<(), &str>::Leaf(2, ())),
                Some(Tree::<(), &str>::Leaf(3, ())),
            ],
            cause: EvalError::MergeError("lhs".into(), "rhs".into()),
        };

        let projected = project_partial_result(partial, &[3, 1]);
        assert_eq!(
            projected.partial_result,
            vec![Some(Tree::<(), &str>::Leaf(3, ())), None]
        );
    }
}
