use open_hypergraphs::lax::functor::Functor;
use open_hypergraphs::lax::*;
use open_hypergraphs::strict::vec::FiniteFunction;
use std::fmt::Debug;
use thiserror::Error;

use crate::ssa::{SSA, SSAError, ssa};
use crate::theory::{OperationKey, Theory};
use crate::tree::*;
use crate::{dual, dual::Dual};

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
}

#[derive(Debug, Error)]
pub struct PartialResult<O> {
    pub partial_result: Vec<Option<Tree<(), O>>>,
    pub cause: EvalError,
}

impl<O: Debug> std::fmt::Display for PartialResult<O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}; partial result: {:?}",
            self.cause, self.partial_result
        )
    }
}

#[derive(Debug, Clone)]
pub struct EvalStep<O> {
    pub ssa: SSA<(), Dual<O>>,
    pub inputs: Vec<Tree<(), O>>,
    pub state: Vec<Option<Tree<(), O>>>,
}

#[derive(Debug, Clone)]
pub struct CheckTrace<O> {
    pub term: OpenHypergraph<(), OperationKey>,
    pub type_term: OpenHypergraph<(), Dual<O>>,
    pub quotient: FiniteFunction,
    pub node_type_indices: Vec<usize>,
    pub ssa: Vec<SSA<(), Dual<O>>>,
    pub eval_steps: Vec<EvalStep<O>>,
    pub result: Vec<Tree<(), O>>,
    pub node_types: Vec<Tree<(), O>>,
}

// TODO: include location info (NodeId)
#[derive(Debug, Error)]
pub enum EvalError {
    #[error("Could not merge values {0} and {1}")]
    MergeError(String, String),
    #[error("Could not pop symbol {1} of {0:?}")]
    MatchError(EdgeId, String),
}

/// Typecheck a term, returning an assignment of "types" to each of its nodes
pub fn check<O: Eq + Clone + Debug + std::fmt::Display>(
    theory: &Theory<O>, // *arrow* theory
    source: OpenHypergraph<(), O>,
    target: OpenHypergraph<(), O>,
    arrow: &mut OpenHypergraph<(), OperationKey>,
) -> Result<Vec<Tree<(), O>>, Error<O>> {
    check_trace(theory, source, target, arrow).map(|trace| trace.node_types)
}

pub fn check_trace<O: Eq + Clone + Debug + std::fmt::Display>(
    theory: &Theory<O>, // *arrow* theory
    source: OpenHypergraph<(), O>,
    target: OpenHypergraph<(), O>,
    arrow: &mut OpenHypergraph<(), OperationKey>,
) -> Result<CheckTrace<O>, Error<O>> {
    let term = arrow.clone();
    let (type_term, quotient, node_type_indices) = type_term(theory, source, target, arrow)?;
    let ssa = ssa(type_term.clone().to_strict())?;
    let (result, eval_steps) = eval_type_trace(type_term.clone())?;
    let node_types = node_type_indices
        .iter()
        .map(|i| result[*i].clone())
        .collect();

    Ok(CheckTrace {
        term,
        type_term,
        quotient,
        node_type_indices,
        ssa,
        eval_steps,
        result,
        node_types,
    })
}

/// Compute the type map `source+ ; arrow.s- ; arrow.t+ ; target-`.
pub fn type_term<O: Eq + Clone + Debug + std::fmt::Display>(
    theory: &Theory<O>,
    source: OpenHypergraph<(), O>,
    target: OpenHypergraph<(), O>,
    arrow: &mut OpenHypergraph<(), OperationKey>,
) -> Result<(OpenHypergraph<(), Dual<O>>, FiniteFunction, Vec<usize>), Error<O>> {
    let (mut type_term, offset, size) = raw_type_term(theory, source, target, arrow)?;

    let quotient = type_term.quotient().map_err(Error::InvalidQuotient)?;
    let node_type_indices = (offset..offset + size).map(|i| quotient.table[i]).collect();

    Ok((type_term, quotient, node_type_indices))
}

/// Compute the raw type map `source+ ; arrow.s- ; arrow.t+ ; target-` before the final quotient.
pub fn raw_type_term<O: Eq + Clone + Debug + std::fmt::Display>(
    theory: &Theory<O>,
    source: OpenHypergraph<(), O>,
    target: OpenHypergraph<(), O>,
    arrow: &mut OpenHypergraph<(), OperationKey>,
) -> Result<(OpenHypergraph<(), Dual<O>>, usize, usize), Error<O>> {
    let mut fwd = dual::into_fwd(source);
    let mut rev = dual::into_rev(target);
    fwd.quotient().map_err(Error::InvalidQuotient)?;
    rev.quotient().map_err(Error::InvalidQuotient)?;
    arrow.quotient().map_err(Error::InvalidQuotient)?;

    let type_map = AsType(theory).map_arrow(arrow);

    let type_term = fwd
        .lax_compose(&type_map)
        .and_then(|f| f.lax_compose(&rev))
        .ok_or(Error::<O>::InvalidTypeMaps)?;

    let offset = fwd.hypergraph.nodes.len();
    let size = arrow.hypergraph.nodes.len();

    Ok((type_term, offset, size))
}

/// Evaluate a type map
pub fn eval_type<O: Clone + Eq + Debug + std::fmt::Display>(
    f: OpenHypergraph<(), Dual<O>>,
) -> Result<Vec<Tree<(), O>>, Error<O>> {
    // evaluation state initialized all to None, so that source `s` becomes `Leaf s`
    let state: Vec<Option<Tree<(), O>>> = vec![None; f.hypergraph.nodes.len()];
    eval_type_with(f, state)
}

pub fn eval_type_trace<O: Clone + Eq + Debug + std::fmt::Display>(
    f: OpenHypergraph<(), Dual<O>>,
) -> Result<(Vec<Tree<(), O>>, Vec<EvalStep<O>>), Error<O>> {
    let state: Vec<Option<Tree<(), O>>> = vec![None; f.hypergraph.nodes.len()];
    eval_type_with_trace(f, state)
}

pub fn eval_type_with<O: Clone + Eq + Debug + std::fmt::Display>(
    f: OpenHypergraph<(), Dual<O>>,
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
            Dual::Fwd(arr) => {
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
        };
    }

    // Return final eval state
    Ok(state
        .into_iter()
        .enumerate()
        .map(|(i, opt)| opt.unwrap_or_else(|| Tree::Leaf(i, ())))
        .collect())
}

pub fn eval_type_with_trace<O: Clone + Eq + Debug + std::fmt::Display>(
    f: OpenHypergraph<(), Dual<O>>,
    mut state: Vec<Option<Tree<(), O>>>,
) -> Result<(Vec<Tree<(), O>>, Vec<EvalStep<O>>), Error<O>> {
    let mut steps = Vec::new();

    for ssa_value in ssa(f.to_strict())? {
        let source_values: Vec<Tree<(), O>> = ssa_value
            .sources
            .iter()
            .map(|i| {
                state[i.0.0]
                    .clone()
                    .unwrap_or_else(|| Tree::Leaf(i.0.0, ()))
            })
            .collect();

        match &ssa_value.op {
            Dual::Fwd(arr) => {
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
            Dual::Rev(op) => {
                let mut children = None;
                for (i, v) in source_values.iter().cloned().enumerate() {
                    match v {
                        Tree::Node(arr, j, node_children) if i == j && arr == *op => {
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

                let children =
                    children.unwrap_or_else(|| vec![Tree::Empty; ssa_value.targets.len()]);
                for (node_id, child) in ssa_value.targets.iter().zip(children.into_iter()) {
                    merge(&mut state[node_id.0.0], child).map_err(|cause| PartialResult {
                        cause,
                        partial_result: state.clone(),
                    })?;
                }
            }
        };

        steps.push(EvalStep {
            ssa: ssa_value,
            inputs: source_values,
            state: state.clone(),
        });
    }

    let result = state
        .into_iter()
        .enumerate()
        .map(|(i, opt)| opt.unwrap_or_else(|| Tree::Leaf(i, ())))
        .collect();

    Ok((result, steps))
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
struct AsType<'a, O>(pub &'a Theory<O>);

// wi, wn both are OperationKey
impl<O: Clone> Functor<(), OperationKey, (), Dual<O>> for AsType<'_, O> {
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
        functor::try_define_map_arrow(self, f).unwrap()
    }
}
