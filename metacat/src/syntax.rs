//! Syntax-term graphs reconstructed from valid wire saturations.
//!
//! Consistency makes every saturation class either headless or gives it one
//! constructor definition: an operation, output port, and tuple of argument
//! classes. Well-foundedness makes those class dependencies acyclic. Together
//! these conditions determine a unique finite syntax term for every class,
//! while this module retains their shared representation as a DAG.

use hexpr::Operation;
use open_hypergraphs::lax::OpenHypergraph;
use open_hypergraphs::strict::vec::FiniteFunction;
use thiserror::Error;

/// A node in a [`SyntaxGraph`], identified with a wire-saturation class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SyntaxId(pub usize);

/// The unique syntax definition attached to a saturation class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxDefinition {
    /// A headless class, treated as a syntax metavariable.
    Variable,
    /// One output of a constructor applied to the given argument classes.
    Constructor {
        operation: Operation,
        output: usize,
        arguments: Vec<SyntaxId>,
        coarity: usize,
    },
}

/// A shared syntax-term DAG indexed by wire-saturation classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxGraph {
    pub definitions: Vec<SyntaxDefinition>,
}

/// Failure to construct or render a syntax-term graph.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SyntaxError {
    #[error("saturation map has {actual} inputs, but the syntax graph has {expected} vertices")]
    SaturationDomainMismatch { expected: usize, actual: usize },
    #[error("node class {class} contains inconsistent node labels")]
    InconsistentNodeLabels { class: usize },
    #[error("node class {class} has conflicting constructor definitions {definitions:?}")]
    DefinitionClash {
        class: usize,
        definitions: Vec<SyntaxDefinition>,
    },
    #[error("syntax class {class} does not exist")]
    UnknownClass { class: usize },
    #[error("syntax class {class} has a cyclic definition")]
    CyclicDefinition { class: usize },
}

impl SyntaxGraph {
    /// Construct the class-indexed syntax DAG determined by a saturation map.
    pub fn from_saturation<O: Eq>(
        graph: &OpenHypergraph<O, Operation>,
        saturation: &FiniteFunction,
    ) -> Result<Self, SyntaxError> {
        let index = DefinitionIndex::new(graph, saturation)?;
        let definitions = (0..saturation.target)
            .map(|class| {
                index
                    .definition(class, graph, saturation)
                    .unwrap_or(SyntaxDefinition::Variable)
            })
            .collect();

        Ok(Self { definitions })
    }

    /// Render the finite syntax term rooted at `root`.
    pub fn pretty(&self, root: SyntaxId) -> Result<String, SyntaxError> {
        let mut states = vec![VisitState::Unseen; self.definitions.len()];
        let mut rendered = vec![None; self.definitions.len()];
        self.pretty_inner(root, &mut states, &mut rendered)
    }

    /// Render every saturation class in class order.
    pub fn labels(&self) -> Result<Vec<String>, SyntaxError> {
        let mut states = vec![VisitState::Unseen; self.definitions.len()];
        let mut rendered = vec![None; self.definitions.len()];
        (0..self.definitions.len())
            .map(|class| self.pretty_inner(SyntaxId(class), &mut states, &mut rendered))
            .collect()
    }

    fn pretty_inner(
        &self,
        root: SyntaxId,
        states: &mut [VisitState],
        rendered: &mut [Option<String>],
    ) -> Result<String, SyntaxError> {
        let Some(definition) = self.definitions.get(root.0) else {
            return Err(SyntaxError::UnknownClass { class: root.0 });
        };
        match states[root.0] {
            VisitState::Visiting => {
                return Err(SyntaxError::CyclicDefinition { class: root.0 });
            }
            VisitState::Done => {
                return Ok(rendered[root.0]
                    .clone()
                    .expect("a rendered term accompanies a completed visit"));
            }
            VisitState::Unseen => states[root.0] = VisitState::Visiting,
        }

        let label = match definition {
            SyntaxDefinition::Variable => format!("x{}", root.0),
            SyntaxDefinition::Constructor {
                operation,
                output,
                arguments,
                coarity,
            } => {
                let children = arguments
                    .iter()
                    .map(|argument| self.pretty_inner(*argument, states, rendered))
                    .collect::<Result<Vec<_>, _>>()?;
                let inner = match children.as_slice() {
                    [] => operation.to_string(),
                    [child] => format!("{operation}({child})"),
                    [left, right] if is_infix(operation) => format!(
                        "{} {operation} {}",
                        self.pretty_operand(arguments[0], left),
                        self.pretty_operand(arguments[1], right)
                    ),
                    _ => format!("{operation}({})", children.join(", ")),
                };
                if *coarity > 1 {
                    format!("π{output}({inner})")
                } else {
                    inner
                }
            }
        };

        states[root.0] = VisitState::Done;
        rendered[root.0] = Some(label.clone());
        Ok(label)
    }

    fn pretty_operand(&self, id: SyntaxId, rendered: &str) -> String {
        match self.definitions.get(id.0) {
            Some(SyntaxDefinition::Constructor {
                operation,
                arguments,
                ..
            }) if arguments.len() == 2 && is_infix(operation) => format!("({rendered})"),
            _ => rendered.to_owned(),
        }
    }
}

/// Minimal witnesses used to validate `Def(C)` without constructing terms.
struct DefinitionIndex {
    witnesses: Vec<Option<DefinitionWitness>>,
}

impl DefinitionIndex {
    fn new<O: Eq>(
        graph: &OpenHypergraph<O, Operation>,
        saturation: &FiniteFunction,
    ) -> Result<Self, SyntaxError> {
        let node_count = graph.hypergraph.nodes.len();
        if saturation.table.0.len() != node_count {
            return Err(SyntaxError::SaturationDomainMismatch {
                expected: node_count,
                actual: saturation.table.0.len(),
            });
        }

        let mut labels: Vec<Option<&O>> = (0..saturation.target).map(|_| None).collect();
        for (node, label) in graph.hypergraph.nodes.iter().enumerate() {
            let class = saturation.table.0[node];
            match labels[class] {
                Some(known) if known != label => {
                    return Err(SyntaxError::InconsistentNodeLabels { class });
                }
                Some(_) => {}
                None => labels[class] = Some(label),
            }
        }

        let mut witnesses = vec![None; saturation.target];
        for (edge, adjacency) in graph.hypergraph.adjacency.iter().enumerate() {
            for (output, node) in adjacency.targets.iter().enumerate() {
                let class = saturation.table.0[node.0];
                let candidate = DefinitionWitness { edge, output };
                match witnesses[class] {
                    None => witnesses[class] = Some(candidate),
                    Some(known) if same_definition(known, candidate, graph, saturation) => {}
                    Some(known) => {
                        return Err(SyntaxError::DefinitionClash {
                            class,
                            definitions: vec![
                                make_definition(known, graph, saturation),
                                make_definition(candidate, graph, saturation),
                            ],
                        });
                    }
                }
            }
        }

        Ok(Self { witnesses })
    }

    fn definition<O>(
        &self,
        class: usize,
        graph: &OpenHypergraph<O, Operation>,
        saturation: &FiniteFunction,
    ) -> Option<SyntaxDefinition> {
        self.witnesses[class].map(|witness| make_definition(witness, graph, saturation))
    }
}

#[derive(Clone, Copy)]
struct DefinitionWitness {
    edge: usize,
    output: usize,
}

fn same_definition<O>(
    left: DefinitionWitness,
    right: DefinitionWitness,
    graph: &OpenHypergraph<O, Operation>,
    saturation: &FiniteFunction,
) -> bool {
    let left_edge = &graph.hypergraph.adjacency[left.edge];
    let right_edge = &graph.hypergraph.adjacency[right.edge];
    graph.hypergraph.edges[left.edge] == graph.hypergraph.edges[right.edge]
        && left.output == right.output
        && left_edge.targets.len() == right_edge.targets.len()
        && left_edge.sources.len() == right_edge.sources.len()
        && left_edge
            .sources
            .iter()
            .zip(&right_edge.sources)
            .all(|(left_source, right_source)| {
                saturation.table.0[left_source.0] == saturation.table.0[right_source.0]
            })
}

fn make_definition<O>(
    witness: DefinitionWitness,
    graph: &OpenHypergraph<O, Operation>,
    saturation: &FiniteFunction,
) -> SyntaxDefinition {
    let edge = &graph.hypergraph.adjacency[witness.edge];
    SyntaxDefinition::Constructor {
        operation: graph.hypergraph.edges[witness.edge].clone(),
        output: witness.output,
        arguments: edge
            .sources
            .iter()
            .map(|node| SyntaxId(saturation.table.0[node.0]))
            .collect(),
        coarity: edge.targets.len(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Unseen,
    Visiting,
    Done,
}

fn is_infix(operation: &Operation) -> bool {
    !operation
        .to_string()
        .starts_with(|character: char| character.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_hypergraphs::array::vec::VecArray;
    use open_hypergraphs::lax::NodeId;

    fn operation(name: &str) -> Operation {
        name.parse().expect("valid operation")
    }

    fn node(graph: &mut OpenHypergraph<(), Operation>) -> NodeId {
        graph.new_node(())
    }

    #[test]
    fn reconstructs_shared_syntax_terms() {
        let mut graph = OpenHypergraph::empty();
        let left_argument = node(&mut graph);
        let right_argument = node(&mut graph);
        let output = node(&mut graph);
        graph.new_edge(operation("f"), ([left_argument], [output]));
        graph.new_edge(operation("f"), ([right_argument], [output]));
        let saturation =
            FiniteFunction::new(VecArray(vec![0, 0, 1]), 2).expect("valid saturation map");

        let syntax = SyntaxGraph::from_saturation(&graph, &saturation).unwrap();

        assert_eq!(syntax.pretty(SyntaxId(1)).unwrap(), "f(x0)");
    }

    #[test]
    fn rejects_one_head_with_distinct_argument_classes() {
        let mut graph = OpenHypergraph::empty();
        let left_argument = node(&mut graph);
        let right_argument = node(&mut graph);
        let output = node(&mut graph);
        graph.new_edge(operation("f"), ([left_argument], [output]));
        graph.new_edge(operation("f"), ([right_argument], [output]));
        let saturation =
            FiniteFunction::new(VecArray(vec![0, 1, 2]), 3).expect("valid saturation map");

        assert!(matches!(
            SyntaxGraph::from_saturation(&graph, &saturation),
            Err(SyntaxError::DefinitionClash { class: 2, .. })
        ));
    }

    #[test]
    fn renders_multi_output_constructor_projections() {
        let mut graph = OpenHypergraph::empty();
        let argument = node(&mut graph);
        let first = node(&mut graph);
        let second = node(&mut graph);
        graph.new_edge(operation("split"), ([argument], [first, second]));
        let saturation =
            FiniteFunction::new(VecArray(vec![0, 1, 2]), 3).expect("valid saturation map");

        let syntax = SyntaxGraph::from_saturation(&graph, &saturation).unwrap();

        assert_eq!(syntax.pretty(SyntaxId(1)).unwrap(), "π0(split(x0))");
        assert_eq!(syntax.pretty(SyntaxId(2)).unwrap(), "π1(split(x0))");
    }
}
