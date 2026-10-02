//! The path functor and occurrence witnesses for its action on proof terms.

use crate::theory::{Term, Theory};
use hexpr::Operation;
use open_hypergraphs::array::vec::VecArray;
use open_hypergraphs::category::{Arrow, Spider};
use open_hypergraphs::lax::functor::Functor;
use open_hypergraphs::lax::{EdgeId, NodeId, OpenHypergraph};
use open_hypergraphs::strict::vec::FiniteFunction;

pub type CheckGraph = OpenHypergraph<(), Operation>;

/// The image of a proof under [`PathFunctor`], with node provenance retained.
#[derive(Debug, Clone, PartialEq)]
pub struct PathImage {
    pub graph: CheckGraph,
    /// The map from proof nodes to vertices of `Path(p)`.
    pub proof_nodes: FiniteFunction,
    /// One metavariable map for each generator occurrence of the proof.
    pub generator_metavariables: Vec<GeneratorMetavariables>,
}

/// A proof generator occurrence's metavariable map into the accompanying graph.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratorMetavariables {
    /// The occurrence of `g` in the original proof hypergraph.
    pub occurrence: EdgeId,
    /// The map into `PathImage::graph` or, after transport, `CheckResult::phi`.
    pub mapping: FiniteFunction,
}

struct GeneratorPath {
    graph: CheckGraph,
    metavariables: FiniteFunction,
}

/// The identity-on-objects symmetric monoidal functor `Path`.
#[derive(Clone)]
pub struct PathFunctor<'a> {
    theory: &'a Theory,
}

impl<'a> PathFunctor<'a> {
    pub fn new(theory: &'a Theory) -> Self {
        Self { theory }
    }

    /// Map a proof while retaining every generator occurrence's metavariables.
    pub fn map_arrow_with_metavariables(&self, proof: &Term) -> Option<PathImage> {
        if !proof.hypergraph.is_strict() {
            return None;
        }

        let node_count = proof.hypergraph.nodes.len();
        let mut operations = CheckGraph::empty();
        let mut generator_metavariables = Vec::with_capacity(proof.hypergraph.edges.len());

        for (edge, operation) in proof.hypergraph.edges.iter().enumerate() {
            let adjacency = &proof.hypergraph.adjacency[edge];
            let mapped =
                self.map_generator(operation, adjacency.sources.len(), adjacency.targets.len())?;
            let operation_offset = operations.hypergraph.nodes.len();
            let metavariables: Vec<usize> = mapped
                .metavariables
                .table
                .0
                .iter()
                .map(|node| operation_offset + node)
                .collect();
            generator_metavariables.push((EdgeId(edge), metavariables));
            operations.tensor_assign(mapped.graph);
        }

        let sources = finite_node_map(&proof.sources, node_count)?;
        let targets = finite_node_map(&proof.targets, node_count)?;
        let edge_sources = finite_node_map(
            &proof
                .hypergraph
                .adjacency
                .iter()
                .flat_map(|edge| edge.sources.iter().copied())
                .collect::<Vec<_>>(),
            node_count,
        )?;
        let edge_targets = finite_node_map(
            &proof
                .hypergraph
                .adjacency
                .iter()
                .flat_map(|edge| edge.targets.iter().copied())
                .collect::<Vec<_>>(),
            node_count,
        )?;

        let identity_map = FiniteFunction::identity(node_count);
        let identity = CheckGraph::identity(vec![(); node_count]);
        let left = CheckGraph::spider(
            sources,
            (&identity_map + &edge_sources)?,
            vec![(); node_count],
        )?;
        let right = CheckGraph::spider(
            (&identity_map + &edge_targets)?,
            targets,
            vec![(); node_count],
        )?;

        // Path(p) = left ; (id ⊗ ⊗_e Path(g_e)) ; right. Lax composition
        // concatenates nodes, so these two offsets locate the retained copies.
        let proof_node_offset = left.hypergraph.nodes.len();
        let operation_offset = proof_node_offset + identity.hypergraph.nodes.len();
        let graph = left
            .lax_compose(&identity.tensor(&operations))?
            .lax_compose(&right)?;
        let graph_node_count = graph.hypergraph.nodes.len();

        let proof_nodes = FiniteFunction::new(
            VecArray(
                (0..node_count)
                    .map(|node| proof_node_offset + node)
                    .collect(),
            ),
            graph_node_count,
        )?;
        let generator_metavariables = generator_metavariables
            .into_iter()
            .map(|(occurrence, nodes)| {
                let mapping = FiniteFunction::new(
                    VecArray(
                        nodes
                            .into_iter()
                            .map(|node| operation_offset + node)
                            .collect(),
                    ),
                    graph_node_count,
                )?;
                Some(GeneratorMetavariables {
                    occurrence,
                    mapping,
                })
            })
            .collect::<Option<Vec<_>>>()?;

        Some(PathImage {
            graph,
            proof_nodes,
            generator_metavariables,
        })
    }

    /// Construct `Path(g) = s_g† ; t_g` and locate its metavariable interface.
    fn map_generator(
        &self,
        operation: &Operation,
        source_arity: usize,
        target_arity: usize,
    ) -> Option<GeneratorPath> {
        let declaration = self.theory.get_arrow(operation)?;
        let (source, target) = &declaration.type_maps;
        if source_arity != source.targets.len()
            || target_arity != target.targets.len()
            || source.sources.len() != target.sources.len()
        {
            return None;
        }

        let graph = source.dagger().lax_compose(target)?;
        let metavariables = FiniteFunction::new(
            VecArray(source.sources.iter().map(|node| node.0).collect()),
            graph.hypergraph.nodes.len(),
        )?;
        Some(GeneratorPath {
            graph,
            metavariables,
        })
    }
}

impl Functor<(), Operation, (), Operation> for PathFunctor<'_> {
    /// `Path` is identity-on-objects in the single-sorted checker.
    fn map_object(&self, _: &()) -> impl ExactSizeIterator<Item = ()> {
        std::iter::once(())
    }

    /// Map a proof generator `g` with type span `(s, t)` to `s† ; t`.
    fn map_operation(&self, operation: &Operation, source: &[()], target: &[()]) -> CheckGraph {
        self.map_generator(operation, source.len(), target.len())
            .expect("proof generator should have valid type maps")
            .graph
    }

    /// Extend the generator mapping over a complete proof term.
    fn map_arrow(&self, proof: &Term) -> CheckGraph {
        self.map_arrow_with_metavariables(proof)
            .expect("proof should be quotiented and have valid type maps")
            .graph
    }
}

fn finite_node_map(nodes: &[NodeId], node_count: usize) -> Option<FiniteFunction> {
    FiniteFunction::new(
        VecArray(nodes.iter().map(|node| node.0).collect()),
        node_count,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theory::{TheoryId, TheorySet};
    use open_hypergraphs::lax::functor;

    fn operation(name: &str) -> Operation {
        name.parse().expect("valid operation")
    }

    fn proof_theory(source: &str) -> (TheorySet, TheoryId) {
        let theories = TheorySet::from_text(source).expect("valid theories");
        (theories, TheoryId(operation("proof")))
    }

    #[test]
    fn custom_mapping_matches_the_generic_functor_mapping() {
        let (theories, proof_id) = proof_theory(
            r#"
            (theory syntax nat {
              (arr wff : 1 -> 1)
              (arr -. : 1 -> 1)
            })

            (theory proof syntax {
              (arr wn : wff -> (-. wff))
              (def twice : wff -> (-. -. wff) = (wn wn))
            })
            "#,
        );
        let theory = theories.theories.get(&proof_id).unwrap();
        let mut definition = theory
            .get_arrow(&operation("twice"))
            .unwrap()
            .definition
            .as_ref()
            .unwrap()
            .clone();
        definition.quotient().expect("quotientable definition");
        let functor = PathFunctor::new(theory);

        let custom = functor
            .map_arrow_with_metavariables(&definition)
            .expect("valid custom mapping");
        let (generic, generic_proof_nodes) =
            functor::map_arrow_witness(&functor, &definition).expect("valid generic mapping");

        assert_eq!(custom.graph, generic);
        assert!(
            generic_proof_nodes
                .sources
                .table
                .0
                .iter()
                .all(|&segment| segment == 1)
        );
        assert_eq!(custom.proof_nodes, generic_proof_nodes.values);
    }

    #[test]
    fn records_metavariables_for_each_generator_occurrence() {
        let (theories, proof_id) = proof_theory(
            r#"
            (theory syntax nat {
              (arr wff : 1 -> 1)
            })

            (theory proof syntax {
              (arr id : wff -> wff)
            })
            "#,
        );
        let theory = theories.theories.get(&proof_id).unwrap();
        let mut proof = Term::empty();
        let first_source = proof.new_node(());
        let first_target = proof.new_node(());
        let second_source = proof.new_node(());
        let second_target = proof.new_node(());
        proof.new_edge(operation("id"), ([first_source], [first_target]));
        proof.new_edge(operation("id"), ([second_source], [second_target]));
        proof.sources = vec![first_source, second_source];
        proof.targets = vec![first_target, second_target];

        let image = PathFunctor::new(theory)
            .map_arrow_with_metavariables(&proof)
            .expect("valid path image");

        assert_eq!(image.generator_metavariables.len(), 2);
        assert_eq!(image.generator_metavariables[0].occurrence, EdgeId(0));
        assert_eq!(image.generator_metavariables[1].occurrence, EdgeId(1));
        assert_eq!(image.generator_metavariables[0].mapping.table.0.len(), 1);
        assert_eq!(image.generator_metavariables[1].mapping.table.0.len(), 1);
        assert_ne!(
            image.generator_metavariables[0].mapping.table.0,
            image.generator_metavariables[1].mapping.table.0
        );
    }
}
