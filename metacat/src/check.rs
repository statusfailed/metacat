use crate::saturation::{SaturationError, wire_saturation};
use crate::theory::{Term, Theory};
use crate::union_find::UnionFind;
use hexpr::Operation;
use open_hypergraphs::array::vec::VecArray;
use open_hypergraphs::category::{Arrow, Spider};
use open_hypergraphs::lax::OpenHypergraph;
use open_hypergraphs::lax::functor::{self, Functor};
use open_hypergraphs::strict::vec::{FiniteFunction, IndexedCoproduct};
use thiserror::Error;

pub type CheckGraph = OpenHypergraph<(), Operation>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub operation: Operation,
    pub port: usize,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Type maps had invalid arity/coarity")]
    InvalidTypeMaps,
    #[error("Unable to quotient graph {0:?}")]
    InvalidQuotient(FiniteFunction),
    #[error("Path morphism has different source and target boundary arities")]
    BoundaryArityMismatch,
    #[error("Boundary position {boundary} has unequal source and target images")]
    BoundaryMismatch { boundary: usize },
    #[error("Boundary metavariables {first} and {second} were identified")]
    BoundaryCollision { first: usize, second: usize },
    #[error("Boundary metavariable {boundary} has constructor heads {heads:?}")]
    HeadedBoundary { boundary: usize, heads: Vec<Head> },
    #[error("Node class {class} contains inconsistent node labels")]
    InconsistentNodeLabels { class: usize },
    #[error("Node class {class} has conflicting constructor heads {heads:?}")]
    HeadClash { class: usize, heads: Vec<Head> },
    #[error("Node class {class} fails the occurs check")]
    OccursCheck { class: usize },
}

impl From<SaturationError> for Error {
    /// Expose malformed saturation input through the checker's type-map error.
    fn from(_: SaturationError) -> Self {
        Self::InvalidTypeMaps
    }
}

/// Check a first-order derivation and return wire saturation transported back
/// to the nodes of `arrow`.
///
/// The result is a quotient map whose domain is the (quotiented) node set of
/// `arrow`. Two arrow nodes have the same image exactly when their path
/// witnesses are equivalent under wire saturation. Callers may inspect this
/// map to enforce application-specific restrictions on identification.
pub fn check(
    theory: &Theory,
    source: Term,
    target: Term,
    arrow: &mut Term,
) -> Result<FiniteFunction, Error> {
    arrow.quotient().map_err(Error::InvalidQuotient)?;
    validate_proof_shapes(theory, arrow)?;
    let (mapped_proof, witness) =
        functor::map_arrow_witness(&PathFunctor(theory), arrow).ok_or(Error::InvalidTypeMaps)?;
    let (path, path_quotient, mapped_proof_offset) = compose_path(source, target, mapped_proof)?;
    let mut arrow_nodes = transport_witness(&witness, mapped_proof_offset, &path_quotient)?;

    let (closed, closure_quotient) = frobenius_closure(path)?;
    transport_nodes(&mut arrow_nodes, &closure_quotient)?;

    let mut relation = wire_saturation(&closed)?;
    validate(&closed, &mut relation)?;
    Ok(induced_mapping(&mut relation, &arrow_nodes))
}

/// Fallible, validating wrapper that applies `PathFunctor` to a proof term.
pub fn map_proof(theory: &Theory, mut arrow: Term) -> Result<CheckGraph, Error> {
    arrow.quotient().map_err(Error::InvalidQuotient)?;
    validate_proof_shapes(theory, &arrow)?;
    functor::try_define_map_arrow(&PathFunctor(theory), &arrow).ok_or(Error::InvalidTypeMaps)
}

/// Defensive preflight before applying `Path`: check each proof edge's arity
/// and coarity against its declaration, and ensure the type legs share an apex.
fn validate_proof_shapes(theory: &Theory, arrow: &Term) -> Result<(), Error> {
    for (operation, edge) in arrow
        .hypergraph
        .edges
        .iter()
        .zip(&arrow.hypergraph.adjacency)
    {
        let Some(declaration) = theory.get_arrow(operation) else {
            return Err(Error::InvalidTypeMaps);
        };
        let (source_map, target_map) = &declaration.type_maps;
        if edge.sources.len() != source_map.targets.len()
            || edge.targets.len() != target_map.targets.len()
            || source_map.sources.len() != target_map.sources.len()
        {
            return Err(Error::InvalidTypeMaps);
        }
    }
    Ok(())
}

/// Construct `path(p; s, t) = s ; Path(p) ; t†`.
pub fn path(theory: &Theory, source: Term, target: Term, arrow: Term) -> Result<CheckGraph, Error> {
    let mapped_proof = map_proof(theory, arrow)?;
    compose_path(source, target, mapped_proof).map(|(path, _, _)| path)
}

/// Construct the graph representing `Phi(path(p; s, t))`.
pub fn check_graph(
    theory: &Theory,
    source: Term,
    target: Term,
    arrow: Term,
) -> Result<CheckGraph, Error> {
    frobenius_closure(path(theory, source, target, arrow)?).map(|(graph, _)| graph)
}

/// Form `s ; Path(p) ; t†`, retaining the quotient and proof offset needed to
/// transport proof-node witnesses into the resulting path.
fn compose_path(
    mut source: Term,
    mut target: Term,
    mapped_proof: CheckGraph,
) -> Result<(CheckGraph, FiniteFunction, usize), Error> {
    source.quotient().map_err(Error::InvalidQuotient)?;
    target.quotient().map_err(Error::InvalidQuotient)?;

    let mapped_proof_offset = source.hypergraph.nodes.len();
    let mut path = source
        .lax_compose(&mapped_proof)
        .and_then(|path| path.lax_compose(&target.dagger()))
        .ok_or(Error::InvalidTypeMaps)?;
    let quotient = path.quotient().map_err(Error::InvalidQuotient)?;
    Ok((path, quotient, mapped_proof_offset))
}

/// In a hypergraph category, `delta ; (f tensor id) ; mu` is represented by
/// identifying corresponding source and target interface nodes of `f`.
fn frobenius_closure(mut path: CheckGraph) -> Result<(CheckGraph, FiniteFunction), Error> {
    if path.sources.len() != path.targets.len() {
        return Err(Error::BoundaryArityMismatch);
    }

    for (source, target) in path.sources.clone().into_iter().zip(path.targets.clone()) {
        path.unify(source, target);
    }
    let quotient = path.quotient().map_err(Error::InvalidQuotient)?;
    Ok((path, quotient))
}

/// Locate each proof-node witness in the composed path, then transport it
/// through the path's structural quotient.
fn transport_witness(
    witness: &IndexedCoproduct<FiniteFunction>,
    mapped_proof_offset: usize,
    quotient: &FiniteFunction,
) -> Result<Vec<usize>, Error> {
    let mut cursor = 0;
    let mut nodes = Vec::with_capacity(witness.sources.table.0.len());

    for &segment_len in &witness.sources.table.0 {
        if segment_len != 1 {
            return Err(Error::InvalidTypeMaps);
        }
        let Some(&mapped_node) = witness.values.table.0.get(cursor) else {
            return Err(Error::InvalidTypeMaps);
        };
        let composed_node = mapped_proof_offset + mapped_node;
        let Some(&quotiented_node) = quotient.table.0.get(composed_node) else {
            return Err(Error::InvalidTypeMaps);
        };
        nodes.push(quotiented_node);
        cursor += segment_len;
    }

    if cursor != witness.values.table.0.len() {
        return Err(Error::InvalidTypeMaps);
    }
    Ok(nodes)
}

/// Transport already-located nodes through a subsequent graph quotient.
fn transport_nodes(nodes: &mut [usize], quotient: &FiniteFunction) -> Result<(), Error> {
    for node in nodes {
        let Some(&image) = quotient.table.0.get(*node) else {
            return Err(Error::InvalidTypeMaps);
        };
        *node = image;
    }
    Ok(())
}

/// Compute the relation on proof nodes induced by the wire saturation relation
fn induced_mapping(relation: &mut UnionFind, nodes: &[usize]) -> FiniteFunction {
    let mut representatives = Vec::new();
    let table = nodes
        .iter()
        .map(|&node| {
            let representative = relation.find(node);
            representatives
                .iter()
                .position(|&known| known == representative)
                .unwrap_or_else(|| {
                    representatives.push(representative);
                    representatives.len() - 1
                })
        })
        .collect();

    FiniteFunction::new(VecArray(table), representatives.len())
        .expect("induced wire saturation should be a finite function")
}

/// The identity-on-objects symmetric monoidal functor `Path`.
#[derive(Clone)]
struct PathFunctor<'a>(&'a Theory);

impl Functor<(), Operation, (), Operation> for PathFunctor<'_> {
    /// `Path` is identity-on-objects in the single-sorted checker.
    fn map_object(&self, _: &()) -> impl ExactSizeIterator<Item = ()> {
        std::iter::once(())
    }

    /// Map a proof generator `g` with type span `(s, t)` to `s† ; t`.
    fn map_operation(&self, operation: &Operation, source: &[()], target: &[()]) -> CheckGraph {
        let arrow = self
            .0
            .get_arrow(operation)
            .expect("missing arrow in theory");
        let (s, t) = &arrow.type_maps;

        assert_eq!(source.len(), s.targets.len());
        assert_eq!(target.len(), t.targets.len());

        s.dagger()
            .compose(t)
            .expect("type-map boundaries should compose")
    }

    /// Extend the generator mapping over a complete proof term.
    fn map_arrow(&self, arrow: &Term) -> CheckGraph {
        functor::try_define_map_arrow(self, arrow).expect("arrow should be quotiented")
    }
}

#[derive(Default)]
struct ClassInfo {
    heads: Vec<Head>,
}

/// Record a distinct constructor head, ignoring repeated occurrences of the
/// same operation and output port.
fn insert_head(heads: &mut Vec<Head>, head: Head) {
    if !heads.contains(&head) {
        heads.push(head);
    }
}

/// Check label and constructor-head consistency in every saturated class,
/// returning the head information needed for boundary validation.
fn class_info<'a, O: Eq>(
    graph: &'a OpenHypergraph<O, Operation>,
    relation: &mut UnionFind,
) -> Result<Vec<ClassInfo>, Error> {
    let mut labels: Vec<Option<&'a O>> = (0..graph.hypergraph.nodes.len()).map(|_| None).collect();
    for (node, label) in graph.hypergraph.nodes.iter().enumerate() {
        let class = relation.find(node);
        match labels[class] {
            Some(known) if known != label => {
                return Err(Error::InconsistentNodeLabels { class });
            }
            Some(_) => {}
            None => labels[class] = Some(label),
        }
    }

    let mut classes: Vec<ClassInfo> = (0..graph.hypergraph.nodes.len())
        .map(|_| ClassInfo::default())
        .collect();
    for (edge_id, operation) in graph.hypergraph.edges.iter().enumerate() {
        for (port, node) in graph.hypergraph.adjacency[edge_id]
            .targets
            .iter()
            .enumerate()
        {
            let class = relation.find(node.0);
            insert_head(
                &mut classes[class].heads,
                Head {
                    operation: operation.clone(),
                    port,
                },
            );
        }
    }

    for (class, info) in classes.iter().enumerate() {
        if relation.find(class) == class && info.heads.len() > 1 {
            return Err(Error::HeadClash {
                class,
                heads: info.heads.clone(),
            });
        }
    }
    Ok(classes)
}

/// Check the saturated graph's consistency, well-foundedness, and the three
/// boundary conditions characterizing an identity morphism.
fn validate(graph: &CheckGraph, relation: &mut UnionFind) -> Result<(), Error> {
    if graph.sources.len() != graph.targets.len() {
        return Err(Error::BoundaryArityMismatch);
    }

    let classes = class_info(graph, relation)?;
    check_well_founded(graph, relation)?;

    let mut boundary_classes = Vec::with_capacity(graph.sources.len());
    for (boundary, (source, target)) in graph.sources.iter().zip(&graph.targets).enumerate() {
        if !relation.equivalent(source.0, target.0) {
            return Err(Error::BoundaryMismatch { boundary });
        }
        let source_class = relation.find(source.0);
        if let Some(first) = boundary_classes
            .iter()
            .position(|other| *other == source_class)
        {
            return Err(Error::BoundaryCollision {
                first,
                second: boundary,
            });
        }
        if !classes[source_class].heads.is_empty() {
            return Err(Error::HeadedBoundary {
                boundary,
                heads: classes[source_class].heads.clone(),
            });
        }
        boundary_classes.push(source_class);
    }

    Ok(())
}

/// Perform the occurs check by requiring the saturated classes' directed
/// constructor-dependency graph to be acyclic.
fn check_well_founded(graph: &CheckGraph, relation: &mut UnionFind) -> Result<(), Error> {
    let node_count = graph.hypergraph.nodes.len();
    let representatives: Vec<usize> = (0..node_count).map(|node| relation.find(node)).collect();
    let mut adjacency = vec![Vec::new(); node_count];
    let mut indegree = vec![0usize; node_count];

    for edge in &graph.hypergraph.adjacency {
        for source in &edge.sources {
            let source_class = representatives[source.0];
            for target in &edge.targets {
                let target_class = representatives[target.0];
                adjacency[source_class].push(target_class);
                indegree[target_class] += 1;
            }
        }
    }

    let class_count = representatives
        .iter()
        .enumerate()
        .filter(|(node, representative)| *node == **representative)
        .count();
    let mut stack: Vec<usize> = representatives
        .iter()
        .enumerate()
        .filter_map(|(node, representative)| {
            (node == *representative && indegree[node] == 0).then_some(node)
        })
        .collect();
    let mut visited = 0;

    while let Some(class) = stack.pop() {
        visited += 1;
        for &target in &adjacency[class] {
            indegree[target] -= 1;
            if indegree[target] == 0 {
                stack.push(target);
            }
        }
    }

    if visited != class_count {
        // Every class left by Kahn's algorithm has an incoming edge from
        // another remaining class. Following those predecessors must enter a
        // cycle, so the reported class really does satisfy reach(C, C), rather
        // than merely lying downstream of a cycle.
        let is_remaining = |node: usize| representatives[node] == node && indegree[node] > 0;
        let mut predecessor = vec![None; node_count];
        for source in (0..node_count).filter(|&node| is_remaining(node)) {
            for &target in &adjacency[source] {
                if is_remaining(target) {
                    predecessor[target] = Some(source);
                }
            }
        }

        let mut class = (0..node_count)
            .find(|&node| is_remaining(node))
            .expect("an unvisited class should remain after detecting a cycle");
        for _ in 0..class_count {
            class = predecessor[class]
                .expect("every remaining class should have a remaining predecessor");
        }
        return Err(Error::OccursCheck { class });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theory::{TheoryId, TheorySet};
    use open_hypergraphs::lax::NodeId;

    fn operation(name: &str) -> Operation {
        name.parse().expect("valid operation")
    }

    fn node(graph: &mut CheckGraph) -> NodeId {
        graph.new_node(())
    }

    #[test]
    fn consistency_allows_repeated_heads() {
        let mut graph = CheckGraph::empty();
        let left_input = node(&mut graph);
        let right_input = node(&mut graph);
        let output = node(&mut graph);
        graph.new_edge(operation("f"), ([left_input], [output]));
        graph.new_edge(operation("f"), ([right_input], [output]));
        let mut relation = UnionFind::new(graph.hypergraph.nodes.len());

        class_info(&graph, &mut relation).expect("repeated heads are consistent");
    }

    #[test]
    fn consistency_rejects_conflicting_heads() {
        let mut graph = CheckGraph::empty();
        let left_input = node(&mut graph);
        let right_input = node(&mut graph);
        let output = node(&mut graph);
        graph.new_edge(operation("f"), ([left_input], [output]));
        graph.new_edge(operation("g"), ([right_input], [output]));
        let mut relation = UnionFind::new(graph.hypergraph.nodes.len());

        assert!(matches!(
            class_info(&graph, &mut relation),
            Err(Error::HeadClash { .. })
        ));
    }

    #[test]
    fn consistency_rejects_distinct_node_labels() {
        let mut graph = OpenHypergraph::<&str, Operation>::empty();
        graph.new_node("left");
        graph.new_node("right");
        let mut relation = UnionFind::new(2);
        relation.union(0, 1);

        assert!(matches!(
            class_info(&graph, &mut relation),
            Err(Error::InconsistentNodeLabels { .. })
        ));
    }

    #[test]
    fn compact_dagger_only_swaps_interfaces() {
        let mut graph = CheckGraph::empty();
        let source = node(&mut graph);
        let target = node(&mut graph);
        graph.new_edge(operation("f"), ([source], [target]));
        graph.sources = vec![source];
        graph.targets = vec![target];

        let dagger = graph.dagger();

        assert_eq!(dagger.sources, vec![target]);
        assert_eq!(dagger.targets, vec![source]);
        assert_eq!(dagger.hypergraph, graph.hypergraph);
    }

    #[test]
    fn path_functor_maps_a_generator_to_its_span_path() {
        let theories = TheorySet::from_text(
            r#"
            (theory syntax nat {
              (arr wff : 1 -> 1)
              (arr -. : 1 -> 1)
            })

            (theory proof syntax {
              (arr wn : wff -> (-. wff))
            })
            "#,
        )
        .unwrap();
        let proof = theories
            .theories
            .get(&TheoryId(operation("proof")))
            .unwrap();
        let generator = proof.get_arrow(&operation("wn")).unwrap();

        let mapped = PathFunctor(proof).map_operation(&operation("wn"), &[()], &[()]);
        let expected = generator
            .type_maps
            .0
            .dagger()
            .compose(&generator.type_maps.1)
            .unwrap();

        assert_eq!(mapped, expected);
    }

    #[test]
    fn malformed_proof_edge_shape_returns_an_error() {
        let theories = TheorySet::from_text(
            r#"
            (theory syntax nat {
              (arr wff : 1 -> 1)
            })

            (theory proof syntax {
              (arr g : wff -> wff)
            })
            "#,
        )
        .unwrap();
        let proof = theories
            .theories
            .get(&TheoryId(operation("proof")))
            .unwrap();
        let malformed = Term::singleton(operation("g"), vec![], vec![()]);

        assert!(matches!(
            map_proof(proof, malformed),
            Err(Error::InvalidTypeMaps)
        ));
    }

    #[test]
    fn frobenius_closure_identifies_corresponding_boundaries() {
        let mut graph = CheckGraph::empty();
        let source = node(&mut graph);
        let target = node(&mut graph);
        graph.sources = vec![source];
        graph.targets = vec![target];

        let (closed, quotient) = frobenius_closure(graph).expect("valid closure");

        assert_eq!(closed.sources, closed.targets);
        assert_eq!(quotient.table.0[source.0], quotient.table.0[target.0]);
    }

    #[test]
    fn frobenius_closure_matches_the_explicit_spider_composite() {
        let path = CheckGraph::singleton(operation("f"), vec![()], vec![()]);
        let split = CheckGraph::spider(
            FiniteFunction::new(VecArray(vec![0]), 1).unwrap(),
            FiniteFunction::new(VecArray(vec![0, 0]), 1).unwrap(),
            vec![()],
        )
        .unwrap();
        let merge = CheckGraph::spider(
            FiniteFunction::new(VecArray(vec![0, 0]), 1).unwrap(),
            FiniteFunction::new(VecArray(vec![0]), 1).unwrap(),
            vec![()],
        )
        .unwrap();
        let identity = CheckGraph::identity(vec![()]);
        let mut explicit = split
            .compose(&path.tensor(&identity))
            .and_then(|graph| graph.compose(&merge))
            .unwrap();
        explicit.quotient().unwrap();

        let (implicit, _) = frobenius_closure(path).expect("valid closure");

        assert_eq!(implicit, explicit);
    }

    #[test]
    fn occurs_check_rejects_a_self_loop() {
        let mut graph = CheckGraph::empty();
        let cyclic = node(&mut graph);
        graph.new_edge(operation("f"), ([cyclic], [cyclic]));
        let mut relation = UnionFind::new(graph.hypergraph.nodes.len());

        assert!(matches!(
            check_well_founded(&graph, &mut relation),
            Err(Error::OccursCheck { .. })
        ));
    }

    #[test]
    fn occurs_check_rejects_a_longer_cycle() {
        let mut graph = CheckGraph::empty();
        let first = node(&mut graph);
        let second = node(&mut graph);
        graph.new_edge(operation("f"), ([first], [second]));
        graph.new_edge(operation("g"), ([second], [first]));
        let mut relation = UnionFind::new(graph.hypergraph.nodes.len());

        assert!(matches!(
            check_well_founded(&graph, &mut relation),
            Err(Error::OccursCheck { .. })
        ));
    }

    #[test]
    fn occurs_check_reports_a_class_on_the_cycle() {
        let mut graph = CheckGraph::empty();
        let downstream = node(&mut graph);
        let first = node(&mut graph);
        let second = node(&mut graph);
        graph.new_edge(operation("f"), ([first], [second]));
        graph.new_edge(operation("g"), ([second], [first]));
        graph.new_edge(operation("h"), ([second], [downstream]));
        let mut relation = UnionFind::new(graph.hypergraph.nodes.len());

        let Err(Error::OccursCheck { class }) = check_well_founded(&graph, &mut relation) else {
            panic!("expected an occurs-check failure");
        };
        assert!(class == first.0 || class == second.0);
    }

    #[test]
    fn boundary_must_be_head_free() {
        let mut graph = CheckGraph::empty();
        let input = node(&mut graph);
        let boundary = node(&mut graph);
        graph.new_edge(operation("f"), ([input], [boundary]));
        graph.sources = vec![boundary];
        graph.targets = vec![boundary];
        let mut relation = wire_saturation(&graph).expect("valid saturation");

        assert!(matches!(
            validate(&graph, &mut relation),
            Err(Error::HeadedBoundary { .. })
        ));
    }

    #[test]
    fn boundary_images_must_be_equal() {
        let mut graph = CheckGraph::empty();
        let source = node(&mut graph);
        let target = node(&mut graph);
        graph.sources = vec![source];
        graph.targets = vec![target];
        let mut relation = wire_saturation(&graph).expect("valid saturation");

        assert!(matches!(
            validate(&graph, &mut relation),
            Err(Error::BoundaryMismatch { .. })
        ));
    }

    #[test]
    fn boundary_map_must_be_injective() {
        let mut graph = CheckGraph::empty();
        let boundary = node(&mut graph);
        graph.sources = vec![boundary, boundary];
        graph.targets = vec![boundary, boundary];
        let mut relation = wire_saturation(&graph).expect("valid saturation");

        assert!(matches!(
            validate(&graph, &mut relation),
            Err(Error::BoundaryCollision {
                first: 0,
                second: 1
            })
        ));
    }

    #[test]
    fn induced_mapping_identifies_original_nodes_in_the_same_class() {
        let mut relation = UnionFind::new(4);
        relation.union(0, 2);

        let mapping = induced_mapping(&mut relation, &[0, 1, 2, 3]);

        assert_eq!(mapping.table.0, vec![0, 1, 0, 2]);
        assert_eq!(mapping.target, 3);
    }
}
