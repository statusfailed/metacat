use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};

use crate::analysis::{
    checked_definition_operation_label, checked_definition_wire_label, theory_set_from_texts,
};
use crate::syntax::{
    CompositionElement, FrobeniusSide, PortSide, composition_around, delimiter_stack_at,
    is_operation_char, is_variable_char, matching_close_offset, position_at_offset,
    scan_composition_elements, token_at,
};

/// Keep presentation here; put semantic lookup in `analysis.rs` and
/// syntax/position mechanics in `syntax.rs`.
pub fn hover_at_position(
    text: &str,
    project_texts: &[String],
    position: Position,
) -> Option<Hover> {
    hover_info_at_position(text, project_texts, position).map(|info| Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: info.to_markdown(),
        }),
        range: Some(info.range),
    })
}

#[derive(Debug)]
struct HoverInfo {
    text: String,
    type_info: String,
    range: Range,
}

impl HoverInfo {
    fn to_markdown(&self) -> String {
        format!("`{} : {}`", self.text, self.type_info)
    }
}

fn hover_info_at_position(
    text: &str,
    project_texts: &[String],
    position: Position,
) -> Option<HoverInfo> {
    let offset = crate::syntax::offset_at_position(text, position)?;
    let inside_frobenius = delimiter_stack_at(text, offset)
        .iter()
        .any(|delimiter| delimiter.char == '[');
    let token = if inside_frobenius {
        token_at(text, offset, is_variable_char)?
    } else {
        token_at(text, offset, is_operation_char)?
    };

    let type_info = if inside_frobenius {
        wire_label_at(text, project_texts, &token)?
    } else {
        operation_label_at(text, project_texts, &token)?
    };

    Some(HoverInfo {
        text: token.text,
        type_info,
        range: Range {
            start: position_at_offset(text, token.start),
            end: position_at_offset(text, token.end),
        },
    })
}

fn operation_label_at(
    text: &str,
    project_texts: &[String],
    token: &crate::syntax::Token,
) -> Option<String> {
    let definition = enclosing_declaration(text, token.start, "def")?;
    let body_start = definition_body_start(text, definition)?;
    if token.start < body_start {
        return None;
    }

    let resolved_theories = theory_set_from_texts(project_texts.iter().map(String::as_str))?;
    let definition_context = definition_context(text, token.start)?;
    let operation_occurrence = operation_occurrence_before(text, token.start, &token.text)?;

    checked_definition_operation_label(
        &resolved_theories,
        &definition_context.theory,
        &definition_context.definition,
        &token.text,
        operation_occurrence,
    )
}

fn wire_label_at(
    text: &str,
    project_texts: &[String],
    token: &crate::syntax::Token,
) -> Option<String> {
    let expression = composition_around(text, token.start)?;
    let elements = scan_composition_elements(text, expression.start, expression.end)?;
    let hovered = elements.iter().position(|element| {
        matches!(
            element,
            CompositionElement::Frobenius(frobenius)
                if token.start >= frobenius.start && token.end <= frobenius.end
        )
    })?;
    let CompositionElement::Frobenius(frobenius) = &elements[hovered] else {
        return None;
    };
    let occurrence = frobenius.variable_occurrence(token.start, &token.text)?;
    let resolved_theories = theory_set_from_texts(project_texts.iter().map(String::as_str))?;
    let definition_context = definition_context(text, token.start)?;

    match occurrence.side {
        FrobeniusSide::Target => {
            let operation = elements.iter().skip(hovered + 1).find_map(|element| {
                let CompositionElement::Operation(operation) = element else {
                    return None;
                };
                Some(operation)
            })?;
            let operation_occurrence =
                operation_occurrence_before(text, operation.start, &operation.text)?;
            checked_definition_wire_label(
                &resolved_theories,
                &definition_context.theory,
                &definition_context.definition,
                &operation.text,
                operation_occurrence,
                PortSide::Source,
                occurrence.index,
            )
        }
        FrobeniusSide::Source => {
            let operation = elements[..hovered].iter().rev().find_map(|element| {
                let CompositionElement::Operation(operation) = element else {
                    return None;
                };
                Some(operation)
            })?;
            let operation_occurrence =
                operation_occurrence_before(text, operation.start, &operation.text)?;
            checked_definition_wire_label(
                &resolved_theories,
                &definition_context.theory,
                &definition_context.definition,
                &operation.text,
                operation_occurrence,
                PortSide::Target,
                occurrence.index,
            )
        }
    }
}

#[derive(Clone, Debug)]
struct DefinitionContext {
    theory: String,
    definition: String,
}

fn definition_context(text: &str, offset: usize) -> Option<DefinitionContext> {
    let definition = enclosing_declaration(text, offset, "def")?;
    let names = declaration_header_names(text, definition)?;
    match names.as_slice() {
        [definition] => Some(DefinitionContext {
            theory: enclosing_declaration(text, offset, "theory")
                .and_then(|span| declaration_first_name(text, span))?,
            definition: definition.clone(),
        }),
        [theory, definition] => Some(DefinitionContext {
            theory: theory.clone(),
            definition: definition.clone(),
        }),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

fn enclosing_declaration(text: &str, offset: usize, keyword: &str) -> Option<Span> {
    delimiter_stack_at(text, offset)
        .iter()
        .rev()
        .filter(|delimiter| delimiter.char == '(')
        .find_map(|delimiter| {
            let end = matching_close_offset(text, delimiter.offset)?;
            if declaration_keyword(text, delimiter.offset, end).as_deref() == Some(keyword) {
                Some(Span {
                    start: delimiter.offset,
                    end,
                })
            } else {
                None
            }
        })
}

fn declaration_keyword(text: &str, start: usize, end: usize) -> Option<String> {
    let offset = skip_whitespace(text, start + 1, end);
    let token = token_at(text, offset, is_operation_char)?;
    Some(token.text)
}

fn declaration_header_names(text: &str, span: Span) -> Option<Vec<String>> {
    let mut offset = skip_whitespace(text, span.start + 1, span.end);
    offset = token_at(text, offset, is_operation_char)?.end;

    let mut names = Vec::new();
    loop {
        offset = skip_whitespace(text, offset, span.end);
        let token = token_at(text, offset, is_operation_char)?;
        if token.text == ":" {
            break;
        }
        names.push(token.text);
        offset = token.end;
    }
    Some(names)
}

fn declaration_first_name(text: &str, span: Span) -> Option<String> {
    let mut offset = skip_whitespace(text, span.start + 1, span.end);
    offset = token_at(text, offset, is_operation_char)?.end;
    offset = skip_whitespace(text, offset, span.end);
    Some(token_at(text, offset, is_operation_char)?.text)
}

fn operation_occurrence_before(
    text: &str,
    operation_start: usize,
    operation: &str,
) -> Option<usize> {
    let definition = enclosing_declaration(text, operation_start, "def")?;
    let mut offset = definition_body_start(text, definition)?;
    let mut count = 0usize;
    while offset < operation_start {
        offset = skip_until_operation_char(text, offset, operation_start);
        if offset >= operation_start {
            break;
        }
        let token = token_at(text, offset, is_operation_char)?;
        if token.text == operation {
            count += 1;
        }
        offset = token.end;
    }
    Some(count)
}

fn definition_body_start(text: &str, definition: Span) -> Option<usize> {
    let mut offset = definition.start + 1;
    while offset < definition.end {
        offset = skip_until_operation_char(text, offset, definition.end);
        let token = token_at(text, offset, is_operation_char)?;
        if token.text == "=" {
            return Some(token.end);
        }
        offset = token.end;
    }
    None
}

fn skip_whitespace(text: &str, mut offset: usize, end: usize) -> usize {
    while offset < end {
        let Some(ch) = text[offset..].chars().next() else {
            break;
        };
        if !ch.is_whitespace() {
            break;
        }
        offset += ch.len_utf8();
    }
    offset
}

fn skip_until_operation_char(text: &str, mut offset: usize, end: usize) -> usize {
    while offset < end {
        let Some(ch) = text[offset..].chars().next() else {
            break;
        };
        if is_operation_char(ch) {
            break;
        }
        offset += ch.len_utf8();
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_reports_frobenius_variable_type() {
        let text = include_str!("../../fol.hex");
        let p2 = text.find("(def p2").unwrap();
        let offset = text[p2..].find("[.ph ph]").unwrap() + p2 + 2;
        let project_texts = vec![text.to_string()];
        let info =
            hover_info_at_position(text, &project_texts, position_at_offset(text, offset)).unwrap();

        assert_eq!(info.text, "ph");
        assert_eq!(info.type_info, "wff(x0)");
        assert_eq!(info.to_markdown(), "`ph : wff(x0)`");
    }

    #[test]
    fn hover_uses_checked_graph_label_not_raw_hexpr_fallback() {
        let text = include_str!("../../fol.hex");
        let p2 = text.find("(def p2").unwrap();
        let offset = text[p2..].find("[.ph ph]").unwrap() + p2 + 2;
        let project_texts = vec![text.to_string()];
        let info =
            hover_info_at_position(text, &project_texts, position_at_offset(text, offset)).unwrap();

        assert_eq!(info.text, "ph");
        assert_eq!(info.type_info, "wff(x0)");
        assert!(!info.type_info.contains("[."));
    }

    #[test]
    fn hover_supports_top_level_extension_definitions() {
        let text = r#"(theory fol.syntax nat {
  (arr wff : 1 -> 1)
  (arr |- : 1 -> 1)
  (arr -> : 2 -> 1)
})

(theory fol.proof fol.syntax {
  (arr ax-1 : {wff wff} -> ([x y . x y x] {[x] ->} -> |-))
})

(def fol.proof p2 : wff -> ([ph . ph ph] -> [i . ph i] -> |-) =
  {[ph.]
    ([.ph ph] ax-1)
  })"#;
        let offset = text.find("[.ph ph]").unwrap() + 2;
        let project_texts = vec![text.to_string()];
        let info =
            hover_info_at_position(text, &project_texts, position_at_offset(text, offset)).unwrap();

        assert_eq!(info.text, "ph");
        assert_eq!(info.type_info, "wff(x0)");
    }

    #[test]
    fn hover_reports_operation_wire_labels() {
        let text = include_str!("../../fol.hex");
        let p2 = text.find("(def p2").unwrap();
        let offset = text[p2..].find("ax-1").unwrap() + p2;
        let project_texts = vec![text.to_string()];
        let info =
            hover_info_at_position(text, &project_texts, position_at_offset(text, offset)).unwrap();

        assert_eq!(info.text, "ax-1");
        assert!(info.type_info.contains("wff(x0)"));
        assert!(info.type_info.contains("|-"));
        assert!(info.type_info.contains("->"));
        assert!(!info.type_info.contains("[."));
    }
}
