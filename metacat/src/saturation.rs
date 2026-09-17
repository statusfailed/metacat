//! Fixed-point computation of the wire-saturation relation.
//!
//! Equally labelled edges propagate node equivalence in both directions:
//! equal source tuples identify target tuples, and equal target tuples identify
//! source tuples.

use crate::union_find::UnionFind;
use hexpr::Operation;
use open_hypergraphs::lax::OpenHypergraph;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaturationError {
    InconsistentEdgeShape,
}

/// Compute the wire-saturation relation
pub(crate) fn wire_saturation<O>(
    graph: &OpenHypergraph<O, Operation>,
) -> Result<UnionFind, SaturationError> {
    validate_edge_shapes(graph)?;
    let mut relation = UnionFind::new(graph.hypergraph.nodes.len());

    loop {
        let mut changed = propagate_equal_signatures(graph, &mut relation, Direction::Forward);
        changed |= propagate_equal_signatures(graph, &mut relation, Direction::Backward);
        if !changed {
            return Ok(relation);
        }
    }
}

fn validate_edge_shapes<O>(graph: &OpenHypergraph<O, Operation>) -> Result<(), SaturationError> {
    let mut shapes: BTreeMap<&Operation, (usize, usize)> = BTreeMap::new();
    for (operation, edge) in graph
        .hypergraph
        .edges
        .iter()
        .zip(&graph.hypergraph.adjacency)
    {
        let shape = (edge.sources.len(), edge.targets.len());
        if shapes
            .insert(operation, shape)
            .is_some_and(|known| known != shape)
        {
            return Err(SaturationError::InconsistentEdgeShape);
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Direction {
    Forward,
    Backward,
}

fn propagate_equal_signatures<O>(
    graph: &OpenHypergraph<O, Operation>,
    relation: &mut UnionFind,
    direction: Direction,
) -> bool {
    let mut groups: BTreeMap<(Operation, Vec<usize>), usize> = BTreeMap::new();
    let mut changed = false;

    for (edge_id, operation) in graph.hypergraph.edges.iter().enumerate() {
        let edge = &graph.hypergraph.adjacency[edge_id];
        let (premises, conclusions) = match direction {
            Direction::Forward => (&edge.sources, &edge.targets),
            Direction::Backward => (&edge.targets, &edge.sources),
        };
        let signature = premises.iter().map(|node| relation.find(node.0)).collect();
        let key = (operation.clone(), signature);

        if let Some(&representative_edge_id) = groups.get(&key) {
            let representative = &graph.hypergraph.adjacency[representative_edge_id];
            let representative_conclusions = match direction {
                Direction::Forward => &representative.targets,
                Direction::Backward => &representative.sources,
            };
            for (left, right) in representative_conclusions.iter().zip(conclusions) {
                changed |= relation.union(left.0, right.0);
            }
        } else {
            groups.insert(key, edge_id);
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_hypergraphs::lax::NodeId;

    type TestGraph = OpenHypergraph<(), Operation>;

    fn operation(name: &str) -> Operation {
        name.parse().expect("valid operation")
    }

    fn node(graph: &mut TestGraph) -> NodeId {
        graph.new_node(())
    }

    fn relation_matrix(relation: &mut UnionFind, len: usize) -> Vec<Vec<bool>> {
        (0..len)
            .map(|left| {
                (0..len)
                    .map(|right| relation.equivalent(left, right))
                    .collect()
            })
            .collect()
    }

    fn reference_wire_saturation(graph: &TestGraph) -> Result<UnionFind, SaturationError> {
        validate_edge_shapes(graph)?;
        let mut relation = UnionFind::new(graph.hypergraph.nodes.len());
        loop {
            let mut changed = false;
            for left_id in 0..graph.hypergraph.edges.len() {
                for right_id in left_id + 1..graph.hypergraph.edges.len() {
                    if graph.hypergraph.edges[left_id] != graph.hypergraph.edges[right_id] {
                        continue;
                    }
                    let left = &graph.hypergraph.adjacency[left_id];
                    let right = &graph.hypergraph.adjacency[right_id];
                    if left
                        .sources
                        .iter()
                        .zip(&right.sources)
                        .all(|(left, right)| relation.equivalent(left.0, right.0))
                    {
                        for (left, right) in left.targets.iter().zip(&right.targets) {
                            changed |= relation.union(left.0, right.0);
                        }
                    }
                    if left
                        .targets
                        .iter()
                        .zip(&right.targets)
                        .all(|(left, right)| relation.equivalent(left.0, right.0))
                    {
                        for (left, right) in left.sources.iter().zip(&right.sources) {
                            changed |= relation.union(left.0, right.0);
                        }
                    }
                }
            }
            if !changed {
                return Ok(relation);
            }
        }
    }

    #[test]
    fn saturates_from_sources_to_targets() {
        let mut graph = TestGraph::empty();
        let shared = node(&mut graph);
        let left = node(&mut graph);
        let right = node(&mut graph);
        graph.new_edge(operation("f"), ([shared], [left]));
        graph.new_edge(operation("f"), ([shared], [right]));

        let mut relation = wire_saturation(&graph).expect("valid saturation");
        assert!(relation.equivalent(left.0, right.0));
    }

    #[test]
    fn saturates_from_targets_to_sources() {
        let mut graph = TestGraph::empty();
        let left = node(&mut graph);
        let right = node(&mut graph);
        let shared = node(&mut graph);
        graph.new_edge(operation("f"), ([left], [shared]));
        graph.new_edge(operation("f"), ([right], [shared]));

        let mut relation = wire_saturation(&graph).expect("valid saturation");
        assert!(relation.equivalent(left.0, right.0));
    }

    #[test]
    fn saturation_requires_every_premise_port() {
        let mut graph = TestGraph::empty();
        let shared = node(&mut graph);
        let left_input = node(&mut graph);
        let right_input = node(&mut graph);
        let left_output = node(&mut graph);
        let right_output = node(&mut graph);
        graph.new_edge(operation("f"), ([shared, left_input], [left_output]));
        graph.new_edge(operation("f"), ([shared, right_input], [right_output]));

        let mut relation = wire_saturation(&graph).expect("valid saturation");
        assert!(!relation.equivalent(left_output.0, right_output.0));
    }

    #[test]
    fn backward_saturation_requires_every_target_port() {
        let mut graph = TestGraph::empty();
        let left_input = node(&mut graph);
        let right_input = node(&mut graph);
        let shared = node(&mut graph);
        let left_output = node(&mut graph);
        let right_output = node(&mut graph);
        graph.new_edge(operation("f"), ([left_input], [shared, left_output]));
        graph.new_edge(operation("f"), ([right_input], [shared, right_output]));

        let mut relation = wire_saturation(&graph).expect("valid saturation");
        assert!(!relation.equivalent(left_input.0, right_input.0));
    }

    #[test]
    fn saturation_unifies_every_conclusion_port() {
        let mut graph = TestGraph::empty();
        let shared = node(&mut graph);
        let left_first = node(&mut graph);
        let left_second = node(&mut graph);
        let right_first = node(&mut graph);
        let right_second = node(&mut graph);
        graph.new_edge(operation("f"), ([shared], [left_first, left_second]));
        graph.new_edge(operation("f"), ([shared], [right_first, right_second]));

        let mut relation = wire_saturation(&graph).expect("valid saturation");
        assert!(relation.equivalent(left_first.0, right_first.0));
        assert!(relation.equivalent(left_second.0, right_second.0));
    }

    #[test]
    fn saturation_runs_to_a_fixed_point() {
        let mut graph = TestGraph::empty();
        let shared = node(&mut graph);
        let middle_left = node(&mut graph);
        let middle_right = node(&mut graph);
        let output_left = node(&mut graph);
        let output_right = node(&mut graph);
        graph.new_edge(operation("f"), ([shared], [middle_left]));
        graph.new_edge(operation("f"), ([shared], [middle_right]));
        graph.new_edge(operation("g"), ([middle_left], [output_left]));
        graph.new_edge(operation("g"), ([middle_right], [output_right]));

        let mut relation = wire_saturation(&graph).expect("valid saturation");
        assert!(relation.equivalent(output_left.0, output_right.0));
    }

    #[test]
    fn nullary_edges_with_the_same_label_saturate() {
        let mut graph = TestGraph::empty();
        let left = node(&mut graph);
        let right = node(&mut graph);
        graph.new_edge(operation("constant"), ([], [left]));
        graph.new_edge(operation("constant"), ([], [right]));

        let mut relation = wire_saturation(&graph).expect("valid saturation");
        assert!(relation.equivalent(left.0, right.0));
    }

    #[test]
    fn zero_coarity_edges_saturate_their_sources() {
        let mut graph = TestGraph::empty();
        let left = node(&mut graph);
        let right = node(&mut graph);
        graph.new_edge(operation("discard"), ([left], []));
        graph.new_edge(operation("discard"), ([right], []));

        let mut relation = wire_saturation(&graph).expect("valid saturation");
        assert!(relation.equivalent(left.0, right.0));
    }

    #[test]
    fn equal_labels_must_have_equal_shapes() {
        let mut graph = TestGraph::empty();
        let first = node(&mut graph);
        let second = node(&mut graph);
        graph.new_edge(operation("f"), ([], [first]));
        graph.new_edge(operation("f"), ([first], [second]));

        assert!(matches!(
            wire_saturation(&graph),
            Err(SaturationError::InconsistentEdgeShape)
        ));
    }

    #[test]
    fn different_edge_labels_do_not_saturate() {
        let mut graph = TestGraph::empty();
        let shared = node(&mut graph);
        let left = node(&mut graph);
        let right = node(&mut graph);
        graph.new_edge(operation("f"), ([shared], [left]));
        graph.new_edge(operation("g"), ([shared], [right]));

        let mut relation = wire_saturation(&graph).expect("valid saturation");
        assert!(!relation.equivalent(left.0, right.0));
    }

    #[test]
    fn grouped_saturation_matches_reference() {
        for seed in 0..64usize {
            let mut graph = TestGraph::empty();
            let nodes: Vec<NodeId> = (0..7).map(|_| node(&mut graph)).collect();
            let mut state = seed + 1;
            for edge in 0..8 {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let a = state % nodes.len();
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let b = state % nodes.len();
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let c = state % nodes.len();
                let label = if edge % 2 == 0 { "f" } else { "g" };
                graph.new_edge(operation(label), ([nodes[a], nodes[b]], [nodes[c]]));
            }

            let mut expected = reference_wire_saturation(&graph).expect("valid reference");
            let mut actual = wire_saturation(&graph).expect("valid saturation");
            assert_eq!(
                relation_matrix(&mut actual, nodes.len()),
                relation_matrix(&mut expected, nodes.len()),
                "seed {seed}"
            );
        }
    }
}
