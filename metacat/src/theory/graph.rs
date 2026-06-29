//! Theory dependency graph utilities.
//!
//! This module exposes the syntax-category dependency graph induced by a
//! [`RawTheorySet`](super::ast::RawTheorySet), together with topological
//! ordering and basic graph validation.

use super::ast::RawTheorySet;
use super::model::TheoryId;
use super::nat::NAT_THEORY_NAME;
use hexpr::Operation;
use std::collections::{BTreeMap, HashMap, HashSet};

/// A total map from each theory to its syntax-category dependency.
///
/// The builtin `nat` theory is represented by the self-edge `nat -> nat`.
pub type SyntaxDependencyGraph = HashMap<TheoryId, TheoryId>;

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Unknown theory {0}")]
    UnknownTheory(Operation),
    #[error("Unknown syntax category {base} for theory {theory}")]
    UnknownSyntaxCategory { theory: TheoryId, base: Operation },
    #[error("Cycle detected in syntax-category dependencies involving {0}")]
    SyntaxCycle(TheoryId),
}

/// Build the syntax-category dependency graph for a raw theory set.
pub fn syntax_dependency_graph(raw: &RawTheorySet) -> Result<SyntaxDependencyGraph, GraphError> {
    let nat_id = builtin_nat_theory_id();
    let mut theory_ids: HashMap<Operation, TheoryId> = raw
        .theories
        .keys()
        .cloned()
        .map(|name| {
            let id = TheoryId::new(name.clone());
            (name, id)
        })
        .collect();
    theory_ids.insert(nat_id.0.clone(), nat_id.clone());

    let mut bases = HashMap::new();
    // `nat` is the unique root builtin. Giving it itself as a base keeps the
    // dependency graph total while still letting the topo walk stop at the root.
    // This is not merely ad hoc: categorically, the builtin `nat` theory is
    // also the syntax category in which its own arrow profiles live.
    bases.insert(nat_id.clone(), nat_id.clone());

    for raw_theory in raw.theories.values() {
        let theory = theory_ids
            .get(&raw_theory.name)
            .expect("theory id missing")
            .clone();
        let syntax_base = theory_ids
            .get(&raw_theory.syntax_category)
            .cloned()
            .ok_or_else(|| GraphError::UnknownSyntaxCategory {
                theory: theory.clone(),
                base: raw_theory.syntax_category.clone(),
            })?;
        bases.insert(theory, syntax_base);
    }

    Ok(bases)
}

/// Return a topological order in which each syntax category appears before any
/// theory that depends on it.
pub fn topological_order(graph: &SyntaxDependencyGraph) -> Result<Vec<TheoryId>, GraphError> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Visiting,
        Done,
    }

    fn visit(
        theory: &TheoryId,
        graph: &SyntaxDependencyGraph,
        marks: &mut HashMap<TheoryId, Mark>,
        order: &mut Vec<TheoryId>,
    ) -> Result<(), GraphError> {
        match marks.get(theory) {
            Some(Mark::Done) => return Ok(()),
            Some(Mark::Visiting) => return Err(GraphError::SyntaxCycle(theory.clone())),
            None => {}
        }

        marks.insert(theory.clone(), Mark::Visiting);
        if let Some(base) = graph.get(theory) {
            if base != theory {
                visit(base, graph, marks, order)?;
            }
        }
        marks.insert(theory.clone(), Mark::Done);
        order.push(theory.clone());
        Ok(())
    }

    let mut marks = HashMap::new();
    let mut order = Vec::new();
    for theory in graph.keys() {
        visit(theory, graph, &mut marks, &mut order)?;
    }
    Ok(order)
}

/// Extract the raw subset consisting of the given theories together with all of
/// their transitive syntax-category dependencies.
///
/// The returned subset contains only user theories and extensions targeting
/// those theories. The builtin `nat` dependency remains implicit.
pub fn transitive_dependency_subset<I>(
    roots: I,
    raw: &RawTheorySet,
) -> Result<RawTheorySet, GraphError>
where
    I: IntoIterator<Item = Operation>,
{
    let graph = syntax_dependency_graph(raw)?;
    let nat_id = builtin_nat_theory_id();
    let mut included = HashSet::new();

    for root in roots {
        let theory_id = TheoryId(root.clone());
        if theory_id == nat_id {
            continue;
        }
        if !raw.theories.contains_key(&root) {
            return Err(GraphError::UnknownTheory(root));
        }
        include_transitive_dependencies(&theory_id, &graph, &mut included);
    }

    let theories = raw
        .theories
        .iter()
        .filter(|(name, _)| included.contains(&TheoryId((*name).clone())))
        .map(|(name, theory)| (name.clone(), theory.clone()))
        .collect::<BTreeMap<_, _>>();

    let extensions = raw
        .extensions
        .iter()
        .filter(|extension| included.contains(&TheoryId(extension.theory.clone())))
        .cloned()
        .collect();

    Ok(RawTheorySet {
        theories,
        extensions,
    })
}

fn include_transitive_dependencies(
    theory: &TheoryId,
    graph: &SyntaxDependencyGraph,
    included: &mut HashSet<TheoryId>,
) {
    // Skip nodes we've already visited while computing the dependency closure.
    if !included.insert(theory.clone()) {
        return;
    }
    if let Some(base) = graph.get(theory) {
        // The builtin `nat` root is represented by the self-edge `nat -> nat`,
        // so we stop the recursive walk there instead of looping.
        if base != theory {
            include_transitive_dependencies(base, graph, included);
        }
    }
}

pub fn builtin_nat_theory_id() -> TheoryId {
    TheoryId(
        NAT_THEORY_NAME
            .parse()
            .expect("builtin nat theory name should parse"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_orders_dependencies() -> Result<(), Box<dyn std::error::Error>> {
        let raw = RawTheorySet::from_text(
            r#"
            (theory fol.syntax nat {
              (arr wff : 1 -> 1)
            })

            (theory fol.proof fol.syntax {
              (arr wi : {wff wff} -> wff)
            })
            "#,
        )?;

        let graph = syntax_dependency_graph(&raw)?;
        let order = topological_order(&graph)?;

        let nat = builtin_nat_theory_id();
        let syntax = TheoryId("fol.syntax".parse()?);
        let proof = TheoryId("fol.proof".parse()?);

        let nat_idx = order.iter().position(|id| id == &nat).unwrap();
        let syntax_idx = order.iter().position(|id| id == &syntax).unwrap();
        let proof_idx = order.iter().position(|id| id == &proof).unwrap();

        assert!(nat_idx < syntax_idx);
        assert!(syntax_idx < proof_idx);
        Ok(())
    }

    #[test]
    fn subset_includes_transitive_dependencies_and_extensions()
    -> Result<(), Box<dyn std::error::Error>> {
        let raw = RawTheorySet::from_text(
            r#"
            (theory fol.syntax nat {
              (arr wff : 1 -> 1)
            })

            (theory fol.proof fol.syntax {
              (arr wi : {wff wff} -> wff)
            })

            (theory unrelated nat {
              (arr q : 1 -> 1)
            })

            (def fol.proof win : {wff wff} -> wff = wi)
            (def unrelated box : 1 -> 1 = q)
            "#,
        )?;

        let subset = transitive_dependency_subset(["fol.proof".parse()?], &raw)?;

        assert!(subset.theories.contains_key(&"fol.syntax".parse()?));
        assert!(subset.theories.contains_key(&"fol.proof".parse()?));
        assert!(!subset.theories.contains_key(&"unrelated".parse()?));
        assert_eq!(subset.extensions.len(), 1);
        assert_eq!(subset.extensions[0].theory, "fol.proof".parse()?);
        Ok(())
    }
}
