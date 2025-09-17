//! Convert an OpenHypergraph to SSA form
use open_hypergraphs::array::vec::VecKind;
use open_hypergraphs::{lax, strict};
use std::fmt::{self, Debug, Display};

/// A single static assignment of the form
/// `s₀, s₁, s₂, ... = op(t₀, t₁, ..., tn)`
/// where each `s_i`, `t_i` is a
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SSA<O, A> {
    pub op: A,
    pub edge_id: lax::EdgeId,
    pub sources: Vec<(lax::NodeId, O)>, // source nodes and type labels
    pub targets: Vec<(lax::NodeId, O)>, // target nodes and type labels
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SSAError {
    Cycle,
}

/// Parallel SSA decomposition of an *acyclic* open hypergraph.
/// [`SSAError::Cycle`] returned on cycle detection.
pub fn parallel_ssa<O: Clone, A: Clone>(
    f: strict::OpenHypergraph<VecKind, O, A>,
) -> Result<Vec<Vec<SSA<O, A>>>, SSAError> {
    let (result, unvisited) = parallel_ssa_cyclic(f);
    // check we got an acyclic input
    if unvisited.contains(&1) {
        Err(SSAError::Cycle)
    } else {
        Ok(result)
    }
}

/// Best-effort SSA decomposition, allowing cycles (returning the "unvisited" nodes from the
/// layered operations topological sort)
pub fn parallel_ssa_cyclic<O: Clone, A: Clone>(
    f: strict::OpenHypergraph<VecKind, O, A>,
) -> (Vec<Vec<SSA<O, A>>>, Vec<usize>) {
    // partial topological ordering on edges
    let (op_order, unvisited) = strict::layer::layered_operations(&f);

    // Convert to nonstrict
    let f = lax::OpenHypergraph::from_strict(f);

    // Keep as partial ordering - each layer is a Vec<SSA>
    let result = op_order
        .iter()
        .map(|layer| {
            layer
                .0
                .iter()
                .map(|edge_id| {
                    let lax::Hyperedge { sources, targets } =
                        f.hypergraph.adjacency[*edge_id].clone();
                    let op = f.hypergraph.edges[*edge_id].clone();
                    SSA {
                        op,
                        edge_id: lax::EdgeId(*edge_id),
                        sources: sources
                            .iter()
                            .map(|id| (*id, f.hypergraph.nodes[id.0].clone()))
                            .collect(),
                        targets: targets
                            .iter()
                            .map(|id| (*id, f.hypergraph.nodes[id.0].clone()))
                            .collect(),
                    }
                })
                .collect()
        })
        .collect();

    (result, unvisited.0)
}

/// Totally-ordered SSA decomposition of an *acyclic* open hypergraph.
/// [`SSAError::Cycle`] returned on cycle detection.
pub fn ssa<O: Clone, A: Clone>(
    f: strict::OpenHypergraph<VecKind, O, A>,
) -> Result<Vec<SSA<O, A>>, SSAError> {
    // Flatten the partial order into a total order
    parallel_ssa(f).map(|v| v.into_iter().flatten().collect())
}

impl<O: Debug, A: Debug> Display for SSA<O, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Print targets
        let target_strs: Vec<String> = self
            .targets
            .iter()
            .map(|(node_id, _type)| format!("v{}", node_id.0))
            .collect();

        // Print sources
        let source_strs: Vec<String> = self
            .sources
            .iter()
            .map(|(node_id, _type)| format!("v{}", node_id.0))
            .collect();

        write!(
            f,
            "{}:\t{} = {:?}({})",
            self.edge_id.0,
            target_strs.join(", "),
            self.op,
            source_strs.join(", ")
        )
    }
}

#[cfg(test)]
mod tests;
