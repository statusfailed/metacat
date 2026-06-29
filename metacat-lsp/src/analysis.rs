use hexpr::Operation;
use metacat::check::{check, eval_type};
use metacat::theory::{Term, Theory, TheoryId, TheorySet};
use metacat::{dual, tree::Tree};
use std::collections::BTreeMap;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrowSpanLabel {
    pub source: String,
    pub target: String,
    pub source_metavariable_count: usize,
    pub target_metavariable_count: usize,
    pub error: Option<String>,
}

pub fn arrow_span_label(
    theories: &TheorySet,
    theory_name: &str,
    arrow_name: &str,
) -> Option<ArrowSpanLabel> {
    let theory_id = TheoryId(theory_name.parse().ok()?);
    let theory = theories.theories.get(&theory_id)?;
    let Theory::Theory { syntax, arrows } = theory else {
        return None;
    };
    let syntax_theory = theories.theories.get(syntax)?;
    let arrow_op: Operation = arrow_name.parse().ok()?;
    let declaration = arrows.get(&arrow_op)?;
    let (source, target) = &declaration.type_maps;

    let source_metavariable_count = source.sources.len();
    let target_metavariable_count = target.sources.len();
    let error = (source_metavariable_count != target_metavariable_count).then(|| {
        format!(
            "Source and target terms have different metavariable counts: source has {}, target has {}.",
            source_metavariable_count, target_metavariable_count
        )
    });

    Some(ArrowSpanLabel {
        source: format_type_map_term(source, syntax_theory)?,
        target: format_type_map_term(target, syntax_theory)?,
        source_metavariable_count,
        target_metavariable_count,
        error,
    })
}

fn format_type_map_term(term: &Term, syntax_theory: &Theory) -> Option<String> {
    let mut term = dual::into_fwd(term.clone());
    term.quotient().ok()?;
    let targets = term.targets.iter().map(|node| node.0).collect::<Vec<_>>();
    let sources = term.sources.iter().map(|node| node.0).collect::<Vec<_>>();
    let labels = eval_type(term).ok()?;
    let boundary = targets
        .iter()
        .map(|node| pretty_tree(labels.get(*node)?, syntax_theory))
        .collect::<Option<Vec<_>>>()?;
    Some(replace_source_leaves(
        &format_wire_list(&boundary),
        &sources,
    ))
}

fn pretty_tree(tree: &Tree<(), Operation>, syntax_theory: &Theory) -> Option<String> {
    tree.try_pretty(Some(&|op: &Operation| {
        syntax_theory.coarity_of(op).ok_or(())
    }))
    .ok()
}

fn replace_source_leaves(text: &str, sources: &[usize]) -> String {
    let replacements = sources
        .iter()
        .enumerate()
        .map(|(index, node)| (format!("x{}", node), format!("m{}", index)))
        .collect::<BTreeMap<_, _>>();
    replace_tokens(text, &replacements)
}

fn replace_tokens(text: &str, replacements: &BTreeMap<String, String>) -> String {
    let mut result = String::new();
    let mut offset = 0usize;
    while offset < text.len() {
        let Some(ch) = text.get(offset..).and_then(|slice| slice.chars().next()) else {
            break;
        };
        if !(ch == 'x' || ch == 'm') {
            result.push(ch);
            offset += ch.len_utf8();
            continue;
        }

        let start = offset;
        offset += ch.len_utf8();
        let digit_start = offset;
        while offset < text.len() {
            let Some(next) = text.get(offset..).and_then(|slice| slice.chars().next()) else {
                break;
            };
            if !next.is_ascii_digit() {
                break;
            }
            offset += next.len_utf8();
        }
        let candidate = &text[start..offset];
        if offset > digit_start
            && is_pretty_token_start(text, start)
            && is_pretty_token_end(text, offset)
            && let Some(replacement) = replacements.get(candidate)
        {
            result.push_str(replacement);
        } else {
            result.push_str(candidate);
        }
    }
    result
}

fn is_pretty_token_start(text: &str, offset: usize) -> bool {
    !text
        .get(..offset)
        .and_then(|prefix| prefix.chars().next_back())
        .is_some_and(is_pretty_token_char)
}

fn is_pretty_token_end(text: &str, offset: usize) -> bool {
    !text
        .get(offset..)
        .and_then(|suffix| suffix.chars().next())
        .is_some_and(is_pretty_token_char)
}

fn is_pretty_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')
}

fn format_wire_list(labels: &[String]) -> String {
    match labels {
        [] => "1".to_string(),
        [label] => label.clone(),
        _ => format!("{{{}}}", labels.join(", ")),
    }
}
