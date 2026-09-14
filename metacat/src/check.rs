use crate::dual::{self, Dual};
use crate::theory::{Term, Theory};
use crate::union_find::UnionFind;
use hexpr::Operation;
use open_hypergraphs::category::Arrow;
use open_hypergraphs::lax::OpenHypergraph;
use open_hypergraphs::lax::functor::{self, Functor};
use open_hypergraphs::strict::vec::FiniteFunction;
use thiserror::Error;

pub type CheckGraph = OpenHypergraph<(), Dual<Operation>>;

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
    #[error("Check graph has different source and target boundary arities")]
    BoundaryArityMismatch,
    #[error("Boundary metavariables {first} and {second} were identified")]
    BoundaryCollision { first: usize, second: usize },
    #[error(
        "Boundary metavariable {boundary} is not bare (constructors: {constructors:?}, matchers: {matchers:?})"
    )]
    NonBareBoundary {
        boundary: usize,
        constructors: Vec<Head>,
        matchers: Vec<Head>,
    },
    #[error("Node class {class} has conflicting constructors {constructors:?}")]
    ConstructorClash {
        class: usize,
        constructors: Vec<Head>,
    },
    #[error("Node class {class} has unmatched matcher {matcher:?}")]
    UnmatchedMatcher { class: usize, matcher: Head },
}

/// Check a first-order derivation against explicit source and target type maps.
pub fn check(theory: &Theory, source: Term, target: Term, arrow: &mut Term) -> Result<(), Error> {
    // Construct the graph $source⁺ ; Syntax(arrow) ; target⁻$
    let graph = check_graph(theory, source, target, arrow.clone())?;

    // Compute the checking relation (σ in the paper)
    let mut relation = checking_relation(&graph)?;

    // Check σ is valid
    validate(&graph, &mut relation)
}

/// Map a derivation body `f` to `Syntax(f)` in the polarized syntax category.
pub fn syntax(theory: &Theory, mut arrow: Term) -> Result<CheckGraph, Error> {
    arrow.quotient().map_err(Error::InvalidQuotient)?;
    functor::try_define_map_arrow(&AsType(theory), &arrow).ok_or(Error::InvalidTypeMaps)
}

/// Construct `Check(d) = s+ ; Syntax(f) ; t-` for `d = (f, s, t)`.
pub fn check_graph(
    theory: &Theory,
    mut source: Term,
    mut target: Term,
    arrow: Term,
) -> Result<CheckGraph, Error> {
    source.quotient().map_err(Error::InvalidQuotient)?;
    target.quotient().map_err(Error::InvalidQuotient)?;

    let syntax = syntax(theory, arrow)?;
    let mut graph = dual::into_fwd(source)
        .lax_compose(&syntax)
        .and_then(|graph| graph.lax_compose(&dual::into_rev(target)))
        .ok_or(Error::InvalidTypeMaps)?;
    graph.quotient().map_err(Error::InvalidQuotient)?;
    Ok(graph)
}

/// Map generating arrows of a theory to their polarized type maps `s- ; t+`.
#[derive(Clone)]
struct AsType<'a>(&'a Theory);

impl Functor<(), Operation, (), Dual<Operation>> for AsType<'_> {
    fn map_object(&self, _: &()) -> impl ExactSizeIterator<Item = ()> {
        std::iter::once(())
    }

    fn map_operation(&self, operation: &Operation, source: &[()], target: &[()]) -> CheckGraph {
        let arrow = self
            .0
            .get_arrow(operation)
            .expect("missing arrow in theory");
        let (s, t) = &arrow.type_maps;

        assert_eq!(source.len(), s.targets.len());
        assert_eq!(target.len(), t.targets.len());

        dual::into_rev(s.clone())
            .compose(&dual::into_fwd(t.clone()))
            .expect("type-map boundaries should compose")
    }

    fn map_arrow(&self, arrow: &Term) -> CheckGraph {
        functor::try_define_map_arrow(self, arrow).expect("arrow should be quotiented")
    }
}

/// Compute the least checking relation from `notes/checking.tex`.
///
/// Propagation is deliberately directed: a `Fwd(f)` constructor may discharge
/// a `Rev(f)` matcher, while the reverse orientation does not propagate.
fn checking_relation(graph: &CheckGraph) -> Result<UnionFind, Error> {
    if graph.sources.len() != graph.targets.len() {
        return Err(Error::BoundaryArityMismatch);
    }

    let mut relation = UnionFind::new(graph.hypergraph.nodes.len());
    for (source, target) in graph.sources.iter().zip(&graph.targets) {
        relation.union(source.0, target.0);
    }

    loop {
        let mut changed = false;

        for (constructor_id, constructor_label) in graph.hypergraph.edges.iter().enumerate() {
            let Dual::Fwd(operation) = constructor_label else {
                continue;
            };
            let constructor = &graph.hypergraph.adjacency[constructor_id];

            for (matcher_id, matcher_label) in graph.hypergraph.edges.iter().enumerate() {
                let Dual::Rev(matcher_operation) = matcher_label else {
                    continue;
                };
                if operation != matcher_operation {
                    continue;
                }

                let matcher = &graph.hypergraph.adjacency[matcher_id];
                if constructor.sources.len() != matcher.targets.len()
                    || constructor.targets.len() != matcher.sources.len()
                {
                    return Err(Error::InvalidTypeMaps);
                }

                for output_port in 0..constructor.targets.len() {
                    let constructor_output = constructor.targets[output_port].0;
                    let matcher_input = matcher.sources[output_port].0;
                    if !relation.equivalent(constructor_output, matcher_input) {
                        continue;
                    }

                    for input_port in 0..constructor.sources.len() {
                        changed |= relation.union(
                            constructor.sources[input_port].0,
                            matcher.targets[input_port].0,
                        );
                    }
                }
            }
        }

        if !changed {
            return Ok(relation);
        }
    }
}

#[derive(Default)]
struct ClassInfo {
    constructors: Vec<Head>,
    matchers: Vec<Head>,
}

fn insert_head(heads: &mut Vec<Head>, head: Head) {
    if !heads.contains(&head) {
        heads.push(head);
    }
}

fn validate(graph: &CheckGraph, relation: &mut UnionFind) -> Result<(), Error> {
    let mut boundary_classes = Vec::with_capacity(graph.sources.len());
    for (boundary, source) in graph.sources.iter().enumerate() {
        let class = relation.find(source.0);
        if let Some(first) = boundary_classes.iter().position(|other| *other == class) {
            return Err(Error::BoundaryCollision {
                first,
                second: boundary,
            });
        }
        boundary_classes.push(class);
    }

    let mut classes: Vec<ClassInfo> = (0..graph.hypergraph.nodes.len())
        .map(|_| ClassInfo::default())
        .collect();
    for (edge_id, label) in graph.hypergraph.edges.iter().enumerate() {
        let adjacency = &graph.hypergraph.adjacency[edge_id];
        match label {
            Dual::Fwd(operation) => {
                for (port, node) in adjacency.targets.iter().enumerate() {
                    let class = relation.find(node.0);
                    insert_head(
                        &mut classes[class].constructors,
                        Head {
                            operation: operation.clone(),
                            port,
                        },
                    );
                }
            }
            Dual::Rev(operation) => {
                for (port, node) in adjacency.sources.iter().enumerate() {
                    let class = relation.find(node.0);
                    insert_head(
                        &mut classes[class].matchers,
                        Head {
                            operation: operation.clone(),
                            port,
                        },
                    );
                }
            }
        }
    }

    for (boundary, class) in boundary_classes.iter().copied().enumerate() {
        let info = &classes[class];
        if !info.constructors.is_empty() || !info.matchers.is_empty() {
            return Err(Error::NonBareBoundary {
                boundary,
                constructors: info.constructors.clone(),
                matchers: info.matchers.clone(),
            });
        }
    }

    for (class, info) in classes.iter().enumerate() {
        if relation.find(class) != class || boundary_classes.contains(&class) {
            continue;
        }

        if info.constructors.len() > 1 {
            return Err(Error::ConstructorClash {
                class,
                constructors: info.constructors.clone(),
            });
        }
        for matcher in &info.matchers {
            if !info.constructors.contains(matcher) {
                return Err(Error::UnmatchedMatcher {
                    class,
                    matcher: matcher.clone(),
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_hypergraphs::lax::NodeId;

    fn operation() -> Operation {
        "f".parse().expect("valid operation")
    }

    fn polarized_pair() -> (CheckGraph, [usize; 4]) {
        let mut graph = CheckGraph::empty();
        let (_, (constructor_sources, constructor_targets)) =
            graph.new_operation(Dual::Fwd(operation()), vec![()], vec![()]);
        let (_, (matcher_sources, matcher_targets)) =
            graph.new_operation(Dual::Rev(operation()), vec![()], vec![()]);
        (
            graph,
            [
                constructor_sources[0].0,
                constructor_targets[0].0,
                matcher_sources[0].0,
                matcher_targets[0].0,
            ],
        )
    }

    #[test]
    fn propagates_from_constructor_output_to_matcher_input() {
        let (
            mut graph,
            [
                constructor_input,
                constructor_output,
                matcher_input,
                matcher_output,
            ],
        ) = polarized_pair();
        graph.sources = vec![NodeId(constructor_output)];
        graph.targets = vec![NodeId(matcher_input)];

        let mut relation = checking_relation(&graph).expect("valid relation");
        assert!(relation.equivalent(constructor_input, matcher_output));
    }

    #[test]
    fn does_not_propagate_from_matcher_output_to_constructor_input() {
        let (
            mut graph,
            [
                constructor_input,
                constructor_output,
                matcher_input,
                matcher_output,
            ],
        ) = polarized_pair();
        graph.sources = vec![NodeId(constructor_input)];
        graph.targets = vec![NodeId(matcher_output)];

        let mut relation = checking_relation(&graph).expect("valid relation");
        assert!(!relation.equivalent(constructor_output, matcher_input));
        assert!(matches!(
            validate(&graph, &mut relation),
            Err(Error::UnmatchedMatcher { .. })
        ));
    }
}
