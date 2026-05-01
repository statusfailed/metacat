use open_hypergraphs::lax::functor::Functor;
use open_hypergraphs::lax::*;
use open_hypergraphs::strict::vec::FiniteFunction;
use std::fmt::Debug;
use std::ops::Range;
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
pub struct CheckInput<O> {
    pub source: OpenHypergraph<(), O>,
    pub target: OpenHypergraph<(), O>,
    pub term: OpenHypergraph<(), OperationKey>,
}

#[derive(Debug, Clone)]
pub struct PreparedCheck<O> {
    pub input: CheckInput<O>,
    pub proof_type_map: OpenHypergraph<(), Dual<O>>,
    pub raw_type_term: RawTypeTerm<O>,
    pub type_term: OpenHypergraph<(), Dual<O>>,
    pub quotient: FiniteFunction,
    pub node_type_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct CheckResult<O> {
    pub node_types: Vec<Tree<(), O>>,
}

#[derive(Debug, Clone)]
pub struct CheckTrace<O> {
    pub prepared: PreparedCheck<O>,
    pub ssa: Vec<SSA<(), Dual<O>>>,
    pub eval_steps: Vec<EvalStep<O>>,
    pub result: Vec<Tree<(), O>>,
    pub node_types: Vec<Tree<(), O>>,
}

#[derive(Debug, Clone)]
pub struct TypeMapComponent {
    pub node_range: Range<usize>,
    pub edge_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct RawTypeTerm<O> {
    pub graph: OpenHypergraph<(), Dual<O>>,
    pub source: TypeMapComponent,
    pub proof: TypeMapComponent,
    pub target: TypeMapComponent,
    pub proof_node_range_before_quotient: Range<usize>,
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
    input: CheckInput<O>,
) -> Result<CheckResult<O>, Error<O>> {
    run_check(prepare_check(theory, input)?)
}

pub fn check_trace<O: Eq + Clone + Debug + std::fmt::Display>(
    theory: &Theory<O>, // *arrow* theory
    input: CheckInput<O>,
) -> Result<CheckTrace<O>, Error<O>> {
    run_check_trace(prepare_check(theory, input)?)
}

pub fn prepare_check<O: Eq + Clone + Debug + std::fmt::Display>(
    theory: &Theory<O>,
    input: CheckInput<O>,
) -> Result<PreparedCheck<O>, Error<O>> {
    let proof_type_map = proof_type_map(theory, input.term.clone())?;
    let raw_type_term = raw_type_term(
        theory,
        input.source.clone(),
        input.target.clone(),
        input.term.clone(),
    )?;
    let (type_term, quotient, node_type_indices) = quotient_type_term(&raw_type_term)?;

    Ok(PreparedCheck {
        input,
        proof_type_map,
        raw_type_term,
        type_term,
        quotient,
        node_type_indices,
    })
}

fn run_check<O: Eq + Clone + Debug + std::fmt::Display>(
    prepared: PreparedCheck<O>,
) -> Result<CheckResult<O>, Error<O>> {
    run_check_trace(prepared).map(|trace| CheckResult {
        node_types: trace.node_types,
    })
}

fn run_check_trace<O: Eq + Clone + Debug + std::fmt::Display>(
    prepared: PreparedCheck<O>,
) -> Result<CheckTrace<O>, Error<O>> {
    let (result, eval_steps) = eval_type(prepared.type_term.clone())?;
    let ssa = eval_steps.iter().map(|step| step.ssa.clone()).collect();
    let node_types = prepared
        .node_type_indices
        .iter()
        .map(|i| result[*i].clone())
        .collect();

    Ok(CheckTrace {
        prepared,
        ssa,
        eval_steps,
        result,
        node_types,
    })
}

/// Compute the proof term's type map, without composing the declaration source/target checks.
fn proof_type_map<O: Eq + Clone + Debug + std::fmt::Display>(
    theory: &Theory<O>,
    arrow: OpenHypergraph<(), OperationKey>,
) -> Result<OpenHypergraph<(), Dual<O>>, Error<O>> {
    let mut arrow = arrow;
    arrow.quotient().map_err(Error::InvalidQuotient)?;
    let mut type_map = AsType(theory).map_arrow(&arrow);
    type_map.quotient().map_err(Error::InvalidQuotient)?;
    Ok(type_map)
}

fn quotient_type_term<O: Eq + Clone + Debug + std::fmt::Display>(
    raw: &RawTypeTerm<O>,
) -> Result<(OpenHypergraph<(), Dual<O>>, FiniteFunction, Vec<usize>), Error<O>> {
    let mut type_term = raw.graph.clone();
    let quotient = type_term.quotient().map_err(Error::InvalidQuotient)?;
    let node_type_indices = raw
        .proof_node_range_before_quotient
        .clone()
        .map(|i| quotient.table[i])
        .collect();

    Ok((type_term, quotient, node_type_indices))
}

/// Compute the raw checker type term `source+ ; proof-type-map ; target-` before the final quotient.
fn raw_type_term<O: Eq + Clone + Debug + std::fmt::Display>(
    theory: &Theory<O>,
    source: OpenHypergraph<(), O>,
    target: OpenHypergraph<(), O>,
    arrow: OpenHypergraph<(), OperationKey>,
) -> Result<RawTypeTerm<O>, Error<O>> {
    let mut fwd = dual::into_fwd(source);
    let mut rev = dual::into_rev(target);
    let mut arrow = arrow;
    fwd.quotient().map_err(Error::InvalidQuotient)?;
    rev.quotient().map_err(Error::InvalidQuotient)?;
    arrow.quotient().map_err(Error::InvalidQuotient)?;

    let type_map = AsType(theory).map_arrow(&arrow);
    let source_nodes = 0..fwd.hypergraph.nodes.len();
    let source_edges = 0..fwd.hypergraph.edges.len();
    let proof_nodes = source_nodes.end..source_nodes.end + type_map.hypergraph.nodes.len();
    let proof_edges = source_edges.end..source_edges.end + type_map.hypergraph.edges.len();
    let target_nodes = proof_nodes.end..proof_nodes.end + rev.hypergraph.nodes.len();
    let target_edges = proof_edges.end..proof_edges.end + rev.hypergraph.edges.len();
    let proof_node_range_before_quotient =
        source_nodes.end..source_nodes.end + arrow.hypergraph.nodes.len();

    let type_term = fwd
        .lax_compose(&type_map)
        .and_then(|f| f.lax_compose(&rev))
        .ok_or(Error::<O>::InvalidTypeMaps)?;

    Ok(RawTypeTerm {
        graph: type_term,
        source: TypeMapComponent {
            node_range: source_nodes,
            edge_range: source_edges,
        },
        proof: TypeMapComponent {
            node_range: proof_nodes,
            edge_range: proof_edges,
        },
        target: TypeMapComponent {
            node_range: target_nodes,
            edge_range: target_edges,
        },
        proof_node_range_before_quotient,
    })
}

/// Evaluate a type map
pub fn eval_type<O: Clone + Eq + Debug + std::fmt::Display>(
    f: OpenHypergraph<(), Dual<O>>,
) -> Result<(Vec<Tree<(), O>>, Vec<EvalStep<O>>), Error<O>> {
    // evaluation state initialized all to None, so that source `s` becomes `Leaf s`
    let mut state: Vec<Option<Tree<(), O>>> = vec![None; f.hypergraph.nodes.len()];
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
                // Write a tree into each target whose root is this 'arr', recording the output port.
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
                // Ensure each input has the expected op/port and all input trees share children.
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

fn merge<O: Debug + Eq>(
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
