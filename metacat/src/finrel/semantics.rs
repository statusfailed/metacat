//! Interpret FinRel syntax as Boolean matrices using directed reachability.
//! Shared wires, including Frobenius syntax, contribute paths of length zero.

use super::syntax::{FinRelOp, FinRelSignature, FinRelTerm};
use hexpr::Signature;
use open_hypergraphs::strict::vec::FiniteFunction;

/// A Boolean matrix with source-indexed rows and target-indexed columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FiniteRelation {
    rows: Vec<Vec<bool>>,
    // Retain the target size even for a relation with no rows.
    target: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum FinRelError {
    #[error("Unable to quotient finite-relation term: {0:?}")]
    InvalidQuotient(FiniteFunction),
    #[error("Finite-relation edge {edge} ({operation:?}) has invalid arity/coarity")]
    InvalidGeneratorShape { edge: usize, operation: FinRelOp },
}

impl FiniteRelation {
    /// Interpret a FinRel term, including its wire identifications.
    pub fn from_term(term: &FinRelTerm) -> Result<Self, FinRelError> {
        let mut term = term.clone();
        term.quotient().map_err(FinRelError::InvalidQuotient)?;
        let mut reach = Self::empty(term.hypergraph.nodes.len(), term.hypergraph.nodes.len());
        for (edge, (operation, adjacency)) in term
            .hypergraph
            .edges
            .iter()
            .zip(&term.hypergraph.adjacency)
            .enumerate()
        {
            let (source, target) = FinRelSignature.profile(operation);
            if adjacency.sources.len() != source.len() || adjacency.targets.len() != target.len() {
                return Err(FinRelError::InvalidGeneratorShape {
                    edge,
                    operation: *operation,
                });
            }
            for source in &adjacency.sources {
                for target in &adjacency.targets {
                    reach.insert(source.0, target.0);
                }
            }
        }
        reach.reflexive_transitive_closure();

        let mut relation = Self::empty(term.sources.len(), term.targets.len());
        for (i, source) in term.sources.iter().enumerate() {
            for (j, target) in term.targets.iter().enumerate() {
                if reach.contains(source.0, target.0) {
                    relation.insert(i, j);
                }
            }
        }
        Ok(relation)
    }

    pub fn source(&self) -> usize {
        self.rows.len()
    }

    pub fn target(&self) -> usize {
        self.target
    }

    pub fn rows(&self) -> &[Vec<bool>] {
        &self.rows
    }

    /// Whether the given source and target positions are related.
    pub fn contains(&self, source: usize, target: usize) -> bool {
        self.rows[source][target]
    }

    pub(crate) fn empty(source: usize, target: usize) -> Self {
        Self {
            rows: vec![vec![false; target]; source],
            target,
        }
    }

    pub(crate) fn insert(&mut self, source: usize, target: usize) {
        self.rows[source][target] = true;
    }

    /// Close a square relation under identity and composition (Warshall's algorithm).
    pub(crate) fn reflexive_transitive_closure(&mut self) {
        assert_eq!(self.source(), self.target());
        for (i, row) in self.rows.iter_mut().enumerate() {
            row[i] = true;
        }
        for intermediate in 0..self.source() {
            let successors = self.rows[intermediate].clone();
            for row in &mut self.rows {
                if row[intermediate] {
                    for (reachable, via_intermediate) in row.iter_mut().zip(&successors) {
                        *reachable |= via_intermediate;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relation(text: &str) -> FiniteRelation {
        let expression = text.parse().unwrap();
        let term = hexpr::try_interpret(&FinRelSignature, &expression)
            .unwrap()
            .map_nodes(|_| ());
        FiniteRelation::from_term(&term).unwrap()
    }

    #[test]
    fn interprets_generators_and_preserves_empty_dimensions() {
        for (text, source, target, rows) in [
            ("dup", 1, 2, vec![vec![true, true]]),
            ("sup", 2, 1, vec![vec![true], vec![true]]),
            ("del", 1, 0, vec![vec![]]),
            ("sel", 0, 1, vec![]),
            ("(del sel)", 1, 1, vec![vec![false]]),
            ("{}", 0, 0, vec![]),
        ] {
            let actual = relation(text);
            assert_eq!(
                (actual.source(), actual.target()),
                (source, target),
                "{text}"
            );
            assert_eq!(actual.rows(), rows, "{text}");
        }
    }

    #[test]
    fn composite_relations_agree_with_frobenius_wiring() {
        let composite = relation("({dup _} {_ sup})");
        assert_eq!(composite.rows(), &[vec![true, true], vec![false, true]]);
        assert_eq!(composite, relation("([x y . x x y] {_ sup})"));
        assert_eq!(relation("(dup sup)"), relation("_"));
        assert_eq!(relation("(dup {del _})"), relation("_"));
        assert_eq!(relation("[x y . y x]"), {
            let mut swap = FiniteRelation::empty(2, 2);
            swap.insert(0, 1);
            swap.insert(1, 0);
            swap
        });
    }

    #[test]
    fn closure_handles_cycles_and_paths_against_node_order() {
        let mut reach = FiniteRelation::empty(4, 4);
        reach.insert(3, 2);
        reach.insert(2, 1);
        reach.insert(1, 3);
        reach.reflexive_transitive_closure();
        assert_eq!(
            reach.rows(),
            &[
                vec![true, false, false, false],
                vec![false, true, true, true],
                vec![false, true, true, true],
                vec![false, true, true, true],
            ]
        );
    }
}
