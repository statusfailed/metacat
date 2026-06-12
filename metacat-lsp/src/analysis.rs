use hexpr::Operation;
use metacat::check::check;
use metacat::theory::{Theory, TheoryId, TheorySet};

use crate::syntax::PortSide;

pub fn theory_set_from_texts<'a>(texts: impl IntoIterator<Item = &'a str>) -> Option<TheorySet> {
    TheorySet::from_texts(texts).ok()
}

pub fn checked_definition_wire_label(
    theories: &TheorySet,
    theory_name: &str,
    definition_name: &str,
    operation_name: &str,
    operation_occurrence: usize,
    side: PortSide,
    port_index: usize,
) -> Option<String> {
    let theory_id = TheoryId(theory_name.parse().ok()?);
    let theory = theories.theories.get(&theory_id)?;
    let Theory::Theory { syntax, arrows } = theory else {
        return None;
    };

    let definition_op: Operation = definition_name.parse().ok()?;
    let declaration = arrows.get(&definition_op)?;
    let mut term = declaration.definition.clone()?;
    let (source, target) = declaration.type_maps.clone();
    let labels = check(theory, source, target, &mut term).ok()?;

    let operation: Operation = operation_name.parse().ok()?;
    let edge_id = term
        .hypergraph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| **edge == operation)
        .nth(operation_occurrence)
        .map(|(edge_id, _)| edge_id)?;
    let adjacency = term.hypergraph.adjacency.get(edge_id)?;
    let node = match side {
        PortSide::Source => adjacency.sources.get(port_index)?,
        PortSide::Target => adjacency.targets.get(port_index)?,
    };

    let syntax_theory = theories.theories.get(syntax)?;
    labels
        .get(node.0)?
        .try_pretty(Some(&|op: &Operation| {
            syntax_theory.coarity_of(op).ok_or(())
        }))
        .ok()
}

pub fn checked_definition_operation_label(
    theories: &TheorySet,
    theory_name: &str,
    definition_name: &str,
    operation_name: &str,
    operation_occurrence: usize,
) -> Option<String> {
    let theory_id = TheoryId(theory_name.parse().ok()?);
    let theory = theories.theories.get(&theory_id)?;
    let Theory::Theory { syntax, arrows } = theory else {
        return None;
    };

    let definition_op: Operation = definition_name.parse().ok()?;
    let declaration = arrows.get(&definition_op)?;
    let mut term = declaration.definition.clone()?;
    let (source, target) = declaration.type_maps.clone();
    let labels = check(theory, source, target, &mut term).ok()?;

    let operation: Operation = operation_name.parse().ok()?;
    let edge_id = term
        .hypergraph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| **edge == operation)
        .nth(operation_occurrence)
        .map(|(edge_id, _)| edge_id)?;
    let adjacency = term.hypergraph.adjacency.get(edge_id)?;
    let syntax_theory = theories.theories.get(syntax)?;

    let pretty_label = |node_id: usize| {
        labels
            .get(node_id)?
            .try_pretty(Some(&|op: &Operation| {
                syntax_theory.coarity_of(op).ok_or(())
            }))
            .ok()
    };
    let sources = adjacency
        .sources
        .iter()
        .map(|node| pretty_label(node.0))
        .collect::<Option<Vec<_>>>()?;
    let targets = adjacency
        .targets
        .iter()
        .map(|node| pretty_label(node.0))
        .collect::<Option<Vec<_>>>()?;

    Some(format!(
        "{} -> {}",
        format_wire_list(&sources),
        format_wire_list(&targets)
    ))
}

fn format_wire_list(labels: &[String]) -> String {
    match labels {
        [] => "1".to_string(),
        [label] => label.clone(),
        _ => format!("{{{}}}", labels.join(", ")),
    }
}
