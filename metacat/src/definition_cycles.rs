//! Reject circular justification through the definitions used by a proof.
//!
//! Proof-graph acyclicity does not rule out one declaration invoking itself,
//! directly or through other definitions. Walk those dependencies separately;
//! primitive arrows are leaves, and unrelated declarations are not inspected.

use crate::check::Error;
use crate::theory::{Term, Theory};
use std::collections::HashMap;

#[derive(Clone, Copy)]
enum Mark {
    Visiting,
    Done,
}

/// Check all transitively referenced definitions, without unfolding their bodies.
/// An explicit DFS stack avoids call-stack growth for long chains of lemmas.
pub(crate) fn validate_definition_cycles(theory: &Theory, proof: &Term) -> Result<(), Error> {
    let mut marks = HashMap::new();
    let mut stack = vec![(None, proof.hypergraph.edges.iter())];

    while let Some((_, dependencies)) = stack.last_mut() {
        let Some(operation) = dependencies.next() else {
            let (operation, _) = stack.pop().expect("the DFS stack is nonempty");
            if let Some(operation) = operation {
                marks.insert(operation, Mark::Done);
            }
            continue;
        };

        match marks.get(operation) {
            Some(Mark::Done) => continue,
            Some(Mark::Visiting) => {
                return Err(Error::CyclicDefinition {
                    operation: operation.clone(),
                });
            }
            None => {}
        }

        let declaration = theory.get_arrow(operation).ok_or(Error::InvalidTypeMaps)?;
        if let Some(body) = &declaration.definition {
            marks.insert(operation, Mark::Visiting);
            stack.push((Some(operation), body.hypergraph.edges.iter()));
        } else {
            marks.insert(operation, Mark::Done);
        }
    }
    Ok(())
}
