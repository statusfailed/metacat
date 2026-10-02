//! Disjoint-variable checking after first-order checking.
//!
//! An annotation `ar: B → C` orders metavariables as `B + C` and expands to
//! `(id_B, ar; 0, 1_C)`. An absent annotation uses `B = ∅, C = M`, hence the
//! default block matrix `(id_B, 0; 0, 1_C)`.
//!
//! We compute boundary-extended reachability on the wire saturation of
//! `Φ(path(p; s, t))`, then check each generator occurrence's forbidden pairs
//! and its internal and boundary bareness requirements. Permission matrices
//! themselves are never transitively closed.

use crate::check::{self, CheckResult};
use crate::finrel::{FinRelError, FinRelTerm, FiniteRelation};
use crate::theory::{Term, Theory};
use hexpr::Operation;
use open_hypergraphs::category::Arrow;
use open_hypergraphs::lax::EdgeId;
use std::collections::BTreeMap;
use std::fmt;

/// Where a DV condition was declared or used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DvLocation {
    Boundary,
    Generator {
        occurrence: EdgeId,
        operation: Operation,
    },
}

impl fmt::Display for DvLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boundary => write!(f, "proposed type"),
            Self::Generator {
                occurrence,
                operation,
            } => write!(f, "generator {operation} at proof edge {}", occurrence.0),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DvError {
    #[error(transparent)]
    Check(#[from] check::Error),
    #[error("{location}: invalid admissible-reachability term: {source}")]
    InvalidAnnotation {
        location: DvLocation,
        #[source]
        source: FinRelError,
    },
    #[error(
        "{location}: admissible reachability has shape {bare} -> {formulae}, but the type has {metavariables} metavariables"
    )]
    InvalidArity {
        location: DvLocation,
        bare: usize,
        formulae: usize,
        metavariables: usize,
    },
    #[error(
        "{location}: metavariable {metavariable} must be bare, but its saturated class {class} has a constructor definition"
    )]
    InternalBareness {
        location: DvLocation,
        metavariable: usize,
        class: usize,
    },
    #[error(
        "{location}: metavariable {metavariable} requires boundary metavariable {boundary} to be declared bare"
    )]
    BoundaryBareness {
        location: DvLocation,
        metavariable: usize,
        boundary: usize,
    },
    #[error("{location}: forbidden reachability from metavariable {from} to metavariable {to}")]
    ForbiddenReachability {
        location: DvLocation,
        from: usize,
        to: usize,
    },
}

/// Check a derivation and its DV conditions, returning the usual checking witness.
///
/// `ar` is the proposed type's admissible reachability `B_M → C_M`. The first
/// `|B_M|` boundary metavariables are bare variables; the remaining `|C_M|` are
/// formulae. `None` is shorthand for the default block matrix described above.
pub fn dv_check(
    theory: &Theory,
    source: Term,
    target: Term,
    ar: Option<&FinRelTerm>,
    arrow: &mut Term,
) -> Result<CheckResult, DvError> {
    let result = check::check(theory, source, target, arrow)?;
    let boundary_ar = interpret_ar(ar, result.phi.sources.len(), DvLocation::Boundary)?;
    let boundary: Vec<usize> = result
        .phi
        .sources
        .iter()
        .map(|node| result.saturation.table.0[node.0])
        .collect();
    let mut boundary_index = vec![None; result.saturation.target];
    for (index, &class) in boundary.iter().enumerate() {
        boundary_index[class] = Some(index);
    }
    let (reach, headed) = boundary_extended_reachability(&result, &boundary, &boundary_ar);

    // Each declaration's FinRel term needs interpreting only once, even when
    // the proof contains several occurrences with different metavariable maps.
    let mut annotations = BTreeMap::new();
    for generator in &result.generator_metavariables {
        let operation = &arrow.hypergraph.edges[generator.occurrence.0];
        let location = DvLocation::Generator {
            occurrence: generator.occurrence,
            operation: operation.clone(),
        };
        let ar = match annotations.entry(operation.clone()) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let declaration = theory
                    .get_arrow(operation)
                    .expect("ordinary checking validated the proof's generators");
                entry.insert(interpret_ar(
                    declaration.ar.as_ref(),
                    generator.mapping.table.0.len(),
                    location.clone(),
                )?)
            }
        };
        let classes = generator
            .mapping
            .compose(&result.saturation)
            .expect("a generator witness maps into Φ(path(p; s, t))");

        for (metavariable, &class) in classes.table.0.iter().take(ar.source()).enumerate() {
            if headed[class] {
                return Err(DvError::InternalBareness {
                    location,
                    metavariable,
                    class,
                });
            }
            if let Some(boundary) = boundary_index[class] {
                if boundary >= boundary_ar.source() {
                    return Err(DvError::BoundaryBareness {
                        location,
                        metavariable,
                        boundary,
                    });
                }
            }
        }

        // This is R ⊆ ar_I, tested by pulling R back along each iota_i and
        // comparing it with the local expanded permission matrix.
        for (from, &source_class) in classes.table.0.iter().enumerate() {
            for (to, &target_class) in classes.table.0.iter().enumerate() {
                if !permitted(ar, from, to) && reach.contains(source_class, target_class) {
                    return Err(DvError::ForbiddenReachability { location, from, to });
                }
            }
        }
    }
    Ok(result)
}

/// Interpret `ar: B → C` and check that its interfaces partition `M = B + C`.
fn interpret_ar(
    ar: Option<&FinRelTerm>,
    metavariables: usize,
    location: DvLocation,
) -> Result<FiniteRelation, DvError> {
    let Some(term) = ar else {
        return Ok(FiniteRelation::empty(0, metavariables));
    };
    if term.sources.len() + term.targets.len() != metavariables {
        return Err(DvError::InvalidArity {
            location,
            bare: term.sources.len(),
            formulae: term.targets.len(),
            metavariables,
        });
    }
    FiniteRelation::from_term(term)
        .map_err(|source| DvError::InvalidAnnotation { location, source })
}

/// Read an entry of the expanded block matrix `(id_B, ar; 0, 1_C)`.
fn permitted(ar: &FiniteRelation, from: usize, to: usize) -> bool {
    match (from < ar.source(), to < ar.source()) {
        (true, true) => from == to,
        (true, false) => ar.contains(from, to - ar.source()),
        (false, true) => false,
        (false, false) => true,
    }
}

/// Compute R on saturated classes, recording every constructor target for bareness.
/// Only `id_M ∪ ar_M`, not the expanded permission matrix, extends the boundary.
fn boundary_extended_reachability(
    result: &CheckResult,
    boundary: &[usize],
    ar: &FiniteRelation,
) -> (FiniteRelation, Vec<bool>) {
    let mut reach = FiniteRelation::empty(result.saturation.target, result.saturation.target);
    let mut headed = vec![false; result.saturation.target];
    for edge in &result.phi.hypergraph.adjacency {
        for target in &edge.targets {
            let target_class = result.saturation.table.0[target.0];
            headed[target_class] = true;
            for source in &edge.sources {
                reach.insert(result.saturation.table.0[source.0], target_class);
            }
        }
    }
    for from in 0..ar.source() {
        for to in 0..ar.target() {
            if ar.contains(from, to) {
                reach.insert(boundary[from], boundary[ar.source() + to]);
            }
        }
    }
    reach.reflexive_transitive_closure();
    (reach, headed)
}
