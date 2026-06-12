use hexpr::{Hexpr, Operation};
use metacat::theory::RawTheorySet;

use crate::syntax::PortSide;

pub fn operation_profile(theories: &RawTheorySet, operation: &Operation) -> Option<String> {
    for theory in theories.theories.values() {
        let Some(arrow) = theory.arrows.get(operation) else {
            continue;
        };
        return Some(format!("{} -> {}", arrow.type_maps.0, arrow.type_maps.1));
    }
    None
}

pub fn operation_port_type(
    theories: &RawTheorySet,
    operation: &str,
    side: PortSide,
    index: usize,
) -> Option<String> {
    let operation: Operation = operation.parse().ok()?;
    for theory in theories.theories.values() {
        let Some(arrow) = theory.arrows.get(&operation) else {
            continue;
        };
        let type_map = match side {
            PortSide::Source => &arrow.type_maps.0,
            PortSide::Target => &arrow.type_maps.1,
        };
        return type_port(type_map, index);
    }
    None
}

fn type_port(type_map: &Hexpr, index: usize) -> Option<String> {
    match type_map {
        Hexpr::Tensor(parts) => parts.get(index).map(ToString::to_string),
        value if index == 0 => Some(value.to_string()),
        _ => None,
    }
}
