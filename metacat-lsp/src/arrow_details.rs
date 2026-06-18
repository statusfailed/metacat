use std::collections::{BTreeMap, BTreeSet};

use hexpr::{Hexpr, Operation};
use metacat::theory::ast::{RawTheory, RawTheorySet};
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{Position, Url};

use crate::analysis::{ArrowSpanLabel, arrow_span_label, theory_set_from_texts};
use crate::syntax::{
    delimiter_stack_at, is_operation_char, matching_close_offset, offset_at_position, token_at,
};

#[derive(Clone, Debug, Deserialize)]
pub struct ArrowDetailsParams {
    pub uri: Url,
    pub position: Position,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrowDetails {
    pub declaration_kind: String,
    pub name: String,
    pub source: String,
    pub target: String,
    pub metavariables: Vec<String>,
    pub pretty_metavariables: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
struct ArrowDeclaration {
    start: usize,
    end: usize,
    declaration_kind: String,
    theory: String,
    name: crate::syntax::Token,
    metavariables: Vec<String>,
    source_typevars: Vec<String>,
    target_typevars: Vec<String>,
}

pub fn arrow_details_at_position(
    text: &str,
    project_texts: &[String],
    position: Position,
) -> Option<ArrowDetails> {
    let offset = offset_at_position(text, position)?;
    let declarations = scan_arrow_declarations(text);
    let token = token_at(text, offset, is_operation_char);
    let exact_declaration_name = declarations.iter().find(|declaration| {
        token.as_ref().is_some_and(|token| {
            token.text == declaration.name.text
                && token.start >= declaration.name.start
                && token.end <= declaration.name.end
        })
    });
    let referenced_declaration = declarations.iter().find(|declaration| {
        token.as_ref().is_some_and(|token| {
            token.text == declaration.name.text && !matches!(token.text.as_str(), ":" | "->" | "=")
        })
    });
    let enclosing = declarations
        .iter()
        .find(|declaration| offset >= declaration.start && offset <= declaration.end);
    let declaration = exact_declaration_name
        .or(referenced_declaration)
        .or(enclosing)?;

    let semantic = theory_set_from_texts(project_texts.iter().map(String::as_str))
        .and_then(|theories| {
            arrow_span_label(&theories, &declaration.theory, &declaration.name.text)
        })
        .or_else(|| partial_arrow_span_label(project_texts, declaration));
    let raw_error = raw_typevar_mismatch_error(declaration);
    let span = semantic.or_else(|| raw_error.as_ref().map(|_| empty_span_label(declaration)))?;
    let metavariable_count = span
        .source_metavariable_count
        .max(span.target_metavariable_count);
    let metavariables = declaration
        .metavariables
        .iter()
        .take(metavariable_count)
        .cloned()
        .collect::<Vec<_>>();
    let pretty_metavariables = (0..metavariable_count)
        .map(|index| format!("m{}", index))
        .collect::<Vec<_>>();
    let error = raw_error.or(span.error);

    Some(ArrowDetails {
        declaration_kind: declaration.declaration_kind.clone(),
        name: declaration.name.text.clone(),
        source: span.source,
        target: span.target,
        metavariables,
        pretty_metavariables,
        error,
    })
}

fn partial_arrow_span_label(
    project_texts: &[String],
    declaration: &ArrowDeclaration,
) -> Option<ArrowSpanLabel> {
    let raw = RawTheorySet::from_texts(project_texts.iter().map(String::as_str)).ok()?;
    let raw = with_extensions_lossy(raw);
    let theory: Operation = declaration.theory.parse().ok()?;
    let arrow: Operation = declaration.name.text.parse().ok()?;
    let mut needed = BTreeMap::<Operation, BTreeSet<Operation>>::new();
    let mut visiting = BTreeSet::<(Operation, Operation)>::new();
    collect_arrow_dependencies(&raw, &theory, &arrow, &mut needed, &mut visiting)?;

    let mut subset = RawTheorySet {
        theories: BTreeMap::new(),
        extensions: Vec::new(),
    };
    for (theory_name, arrows) in needed {
        let raw_theory = raw.theories.get(&theory_name)?;
        let mut filtered_arrows = BTreeMap::new();
        for arrow_name in arrows {
            let mut arrow = raw_theory.arrows.get(&arrow_name)?.clone();
            arrow.definition = None;
            filtered_arrows.insert(arrow_name, arrow);
        }
        subset.theories.insert(
            theory_name.clone(),
            RawTheory {
                name: raw_theory.name.clone(),
                syntax_category: raw_theory.syntax_category.clone(),
                arrows: filtered_arrows,
            },
        );
    }

    let theories = match metacat::theory::TheorySet::from_raw(subset) {
        Ok(theories) => theories,
        Err(_) => return None,
    };
    arrow_span_label(&theories, &declaration.theory, &declaration.name.text)
}

fn empty_span_label(declaration: &ArrowDeclaration) -> ArrowSpanLabel {
    ArrowSpanLabel {
        source: String::new(),
        target: String::new(),
        source_metavariable_count: declaration.source_typevars.len(),
        target_metavariable_count: declaration.target_typevars.len(),
        error: None,
    }
}

fn raw_typevar_mismatch_error(declaration: &ArrowDeclaration) -> Option<String> {
    (!declaration.source_typevars.is_empty()
        && !declaration.target_typevars.is_empty()
        && declaration.source_typevars != declaration.target_typevars)
        .then(|| {
        format!(
            "Source and target terms have different metavariables: source {{{}}}, target {{{}}}.",
            declaration.source_typevars.join(", "),
            declaration.target_typevars.join(", ")
        )
    })
}

fn with_extensions_lossy(mut raw: RawTheorySet) -> RawTheorySet {
    for extension in std::mem::take(&mut raw.extensions) {
        let Some(theory) = raw.theories.get_mut(&extension.theory) else {
            continue;
        };
        for (name, arrow) in extension.arrows {
            theory.arrows.entry(name).or_insert(arrow);
        }
    }
    raw
}

fn collect_arrow_dependencies(
    raw: &RawTheorySet,
    theory_name: &Operation,
    arrow_name: &Operation,
    needed: &mut BTreeMap<Operation, BTreeSet<Operation>>,
    visiting: &mut BTreeSet<(Operation, Operation)>,
) -> Option<()> {
    if !visiting.insert((theory_name.clone(), arrow_name.clone())) {
        return Some(());
    }

    let theory = raw.theories.get(theory_name)?;
    let arrow = theory.arrows.get(arrow_name)?;
    needed
        .entry(theory_name.clone())
        .or_default()
        .insert(arrow_name.clone());

    for operation in operations_in(&arrow.type_maps.0)
        .into_iter()
        .chain(operations_in(&arrow.type_maps.1))
    {
        if should_skip_builtin_nat_operation(&theory.syntax_category, &operation) {
            continue;
        }
        collect_arrow_dependencies(raw, &theory.syntax_category, &operation, needed, visiting)?;
    }

    Some(())
}

fn operations_in(hexpr: &Hexpr) -> Vec<Operation> {
    let mut operations = Vec::new();
    collect_operations(hexpr, &mut operations);
    operations
}

fn collect_operations(hexpr: &Hexpr, operations: &mut Vec<Operation>) {
    match hexpr {
        Hexpr::Composition(parts) | Hexpr::Tensor(parts) => {
            for part in parts {
                collect_operations(part, operations);
            }
        }
        Hexpr::Frobenius { .. } => {}
        Hexpr::Operation(operation) => operations.push(operation.clone()),
    }
}

fn should_skip_builtin_nat_operation(syntax_category: &Operation, operation: &Operation) -> bool {
    operation.as_str() == "1"
        || (syntax_category.as_str() == "nat" && operation.as_str().parse::<usize>().is_ok())
}

fn scan_arrow_declarations(text: &str) -> Vec<ArrowDeclaration> {
    let mut declarations = Vec::new();
    for (offset, ch) in text.char_indices() {
        if ch != '(' {
            continue;
        }
        let Some(end) = matching_close_offset(text, offset) else {
            continue;
        };
        if let Some(declaration) = parse_arrow_declaration(text, offset, end) {
            declarations.push(declaration);
        }
    }
    declarations
}

fn parse_arrow_declaration(text: &str, start: usize, end: usize) -> Option<ArrowDeclaration> {
    let tokens = top_level_tokens(text, start + 1, end);
    let kind = tokens.first()?.text.as_str();
    if !matches!(kind, "arr" | "def") {
        return None;
    }

    let colon_index = tokens.iter().position(|token| token.text == ":")?;
    if colon_index < 2 {
        return None;
    }

    let name = tokens.get(colon_index - 1)?.clone();
    let theory = declaration_theory(text, start, end, &tokens, colon_index)?;
    let arrow_index = tokens.iter().position(|token| token.text == "->")?;
    let equal_index = tokens.iter().position(|token| token.text == "=");
    let source_text = text.get(tokens.get(colon_index)?.end..tokens.get(arrow_index)?.start)?;
    let target_end = equal_index
        .and_then(|index| tokens.get(index).map(|token| token.start))
        .unwrap_or(end);
    let target_text = text.get(tokens.get(arrow_index)?.end..target_end)?;
    let source_typevars = leading_typevars_in(source_text);
    let target_typevars = leading_typevars_in(target_text);
    let metavariables = if source_typevars.is_empty() {
        target_typevars.clone()
    } else {
        source_typevars.clone()
    };

    Some(ArrowDeclaration {
        start,
        end,
        declaration_kind: kind.to_string(),
        theory,
        name,
        metavariables,
        source_typevars,
        target_typevars,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::position_at_offset;

    #[test]
    fn arrow_details_returns_partial_semantic_profile_when_full_project_does_not_load() {
        let text = r#"(theory fol.syntax nat {
  (arr wff : 1 -> 1)
  (arr |- : 1 -> 1)
  (arr -> : 2 -> 1)
})

(theory fol.proof fol.syntax {
  (arr broken : missing -> wff)
  (arr ax : {wff wff} -> ([ph ps . ph ps] -> |-))
})"#;
        let offset = text.find("ax :").unwrap();
        let details =
            arrow_details_at_position(text, &[text.to_string()], position_at_offset(text, offset))
                .unwrap();

        assert_eq!(details.name, "ax");
        assert_eq!(details.source, "{wff(m0), wff(m1)}");
        assert_eq!(details.target, "|-(m0 -> m1)");
        assert_eq!(details.metavariables, ["ph", "ps"]);
        assert_eq!(details.pretty_metavariables, ["m0", "m1"]);
        assert_eq!(details.error, None);
    }

    #[test]
    fn arrow_details_uses_apex_metavariables_not_wire_names() {
        let text = include_str!("../../fol.hex");
        let offset = text.find("ax-mp").unwrap();
        let details =
            arrow_details_at_position(text, &[text.to_string()], position_at_offset(text, offset))
                .unwrap();

        assert_eq!(details.name, "ax-mp");
        assert_eq!(details.source, "{|-(m0), |-(m0 -> m1)}");
        assert_eq!(details.target, "|-(m1)");
        assert_eq!(details.metavariables, ["ph", "ps"]);
        assert_eq!(details.pretty_metavariables, ["m0", "m1"]);
        assert_eq!(details.error, None);
    }

    #[test]
    fn arrow_details_metavariables_for_ax5d_are_only_apex_typevars() {
        let text = include_str!("../../fol.hex");
        let offset = text.find("ax5d").unwrap();
        let details =
            arrow_details_at_position(text, &[text.to_string()], position_at_offset(text, offset))
                .unwrap();

        assert_eq!(details.name, "ax5d");
        assert_eq!(details.metavariables, ["x", "ph", "ps"]);
        assert_eq!(details.pretty_metavariables, ["m0", "m1", "m2"]);
        assert!(details.source.contains("m1"));
        assert!(details.source.contains("m2"));
        assert!(details.target.contains("m0"));
        assert!(details.target.contains("m1"));
        assert!(details.target.contains("m2"));
        assert!(!details.metavariables.contains(&"aps".to_string()));
        assert!(!details.metavariables.contains(&"inner".to_string()));
    }

    #[test]
    fn arrow_details_reports_source_target_metavariable_mismatch() {
        let text = r#"(theory fol.syntax nat {
  (arr wff : 1 -> 1)
})

(theory fol.proof fol.syntax {
  (arr bad : ([ph . ph] wff) -> ([ps . ps] wff))
})"#;
        let offset = text.find("bad :").unwrap();
        let details =
            arrow_details_at_position(text, &[text.to_string()], position_at_offset(text, offset))
                .unwrap();

        assert_eq!(details.source, "wff(m0)");
        assert_eq!(details.target, "wff(m0)");
        assert_eq!(details.metavariables, ["ph"]);
        assert_eq!(details.pretty_metavariables, ["m0"]);
        assert_eq!(
            details.error.as_deref(),
            Some("Source and target terms have different metavariables: source {ph}, target {ps}.")
        );
    }
}

fn declaration_theory(
    text: &str,
    start: usize,
    end: usize,
    tokens: &[crate::syntax::Token],
    colon_index: usize,
) -> Option<String> {
    if tokens.first()?.text == "def" && colon_index >= 3 {
        return Some(tokens.get(colon_index - 2)?.text.clone());
    }

    delimiter_stack_at(text, start)
        .iter()
        .rev()
        .filter(|delimiter| delimiter.char == '(')
        .find_map(|delimiter| {
            let theory_end = matching_close_offset(text, delimiter.offset)?;
            if theory_end < end {
                return None;
            }
            let theory_tokens = top_level_tokens(text, delimiter.offset + 1, theory_end);
            if theory_tokens.first()?.text == "theory" {
                Some(theory_tokens.get(1)?.text.clone())
            } else {
                None
            }
        })
}

fn top_level_tokens(text: &str, start: usize, end: usize) -> Vec<crate::syntax::Token> {
    let mut tokens = Vec::new();
    let mut offset = start;
    while offset < end {
        offset = skip_whitespace_and_comments(text, offset, end);
        if offset >= end {
            break;
        }

        let Some(ch) = text.get(offset..).and_then(|slice| slice.chars().next()) else {
            break;
        };
        match ch {
            '(' | '{' | '[' => {
                offset = matching_close_offset(text, offset)
                    .map_or(offset + ch.len_utf8(), |end| end + 1);
            }
            _ if is_operation_char(ch) => {
                if let Some(token) = token_at(text, offset, is_operation_char) {
                    offset = token.end;
                    tokens.push(token);
                } else {
                    offset += ch.len_utf8();
                }
            }
            _ => offset += ch.len_utf8(),
        }
    }
    tokens
}

fn leading_typevars_in(text: &str) -> Vec<String> {
    let mut offset = 0usize;
    while offset < text.len() {
        let Some(ch) = text.get(offset..).and_then(|slice| slice.chars().next()) else {
            break;
        };
        match ch {
            '[' => return source_variables_in_frobenius(text, offset).unwrap_or_default(),
            '(' | '{' => {
                let Some(end) = matching_close_offset(text, offset) else {
                    break;
                };
                let nested = leading_typevars_in(&text[offset + ch.len_utf8()..end]);
                if !nested.is_empty() {
                    return nested;
                }
                offset = end + 1;
            }
            _ => offset += ch.len_utf8(),
        }
    }
    Vec::new()
}

fn source_variables_in_frobenius(text: &str, start: usize) -> Option<Vec<String>> {
    let end = matching_close_offset(text, start)?;
    let mut variables = Vec::new();
    let mut offset = start + 1;
    while offset < end {
        let Some(ch) = text.get(offset..).and_then(|slice| slice.chars().next()) else {
            break;
        };
        match ch {
            '.' => break,
            _ if is_variable_char(ch) => {
                let start = offset;
                offset += ch.len_utf8();
                while offset < text.len() {
                    let Some(next) = text.get(offset..).and_then(|slice| slice.chars().next())
                    else {
                        break;
                    };
                    if !is_variable_char(next) {
                        break;
                    }
                    offset += next.len_utf8();
                }
                let variable = text[start..offset].to_string();
                if !variables.contains(&variable) {
                    variables.push(variable);
                }
            }
            _ => offset += ch.len_utf8(),
        }
    }
    Some(variables)
}

fn is_variable_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')
}

fn skip_whitespace_and_comments(text: &str, mut offset: usize, end: usize) -> usize {
    while offset < end {
        let Some(ch) = text.get(offset..).and_then(|slice| slice.chars().next()) else {
            break;
        };
        if ch.is_whitespace() {
            offset += ch.len_utf8();
            continue;
        }
        if ch == '#' {
            while offset < end {
                let Some(ch) = text.get(offset..).and_then(|slice| slice.chars().next()) else {
                    break;
                };
                offset += ch.len_utf8();
                if ch == '\n' {
                    break;
                }
            }
            continue;
        }
        break;
    }
    offset
}
