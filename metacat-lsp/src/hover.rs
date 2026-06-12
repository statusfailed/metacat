use hexpr::Operation;
use metacat::theory::RawTheorySet;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};

use crate::analysis::{operation_port_type, operation_profile};
use crate::syntax::{
    CompositionElement, FrobeniusSide, PortSide, composition_around, delimiter_stack_at,
    is_operation_char, is_variable_char, position_at_offset, scan_composition_elements, token_at,
};

/// Hover is intentionally narrow right now: it only reports wire/type
/// information. Keep presentation here; put semantic lookup in `analysis.rs`
/// and syntax/position mechanics in `syntax.rs`.
pub fn hover_at_position(text: &str, position: Position) -> Option<Hover> {
    hover_info_at_position(text, position).map(|info| Hover {
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

fn hover_info_at_position(text: &str, position: Position) -> Option<HoverInfo> {
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
        wire_type_at(text, &token)?
    } else {
        operation_type_at(text, &token)?
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

fn operation_type_at(text: &str, token: &crate::syntax::Token) -> Option<String> {
    if matches!(token.text.as_str(), "theory" | "arr" | "def") {
        return None;
    }
    let theories = RawTheorySet::from_text(text).ok()?;
    let operation: Operation = token.text.parse().ok()?;
    operation_profile(&theories, &operation)
}

fn wire_type_at(text: &str, token: &crate::syntax::Token) -> Option<String> {
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
    let theories = RawTheorySet::from_text(text).ok()?;

    match occurrence.side {
        FrobeniusSide::Target => {
            let operation = elements.iter().skip(hovered + 1).find_map(|element| {
                let CompositionElement::Operation(operation) = element else {
                    return None;
                };
                Some(operation)
            })?;
            operation_port_type(&theories, &operation.text, PortSide::Source, occurrence.index)
        }
        FrobeniusSide::Source => {
            let operation = elements[..hovered].iter().rev().find_map(|element| {
                let CompositionElement::Operation(operation) = element else {
                    return None;
                };
                Some(operation)
            })?;
            operation_port_type(&theories, &operation.text, PortSide::Target, occurrence.index)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_reports_frobenius_variable_type() {
        let text = r#"(theory fol.syntax nat {
  (arr wff : 1 -> 1)
  (arr |- : 1 -> 1)
  (arr -> : 2 -> 1)
})

(theory fol.proof fol.syntax {
  (arr wi : {wff wff} -> (-> wff))
  (def p : wff -> ([x . x x] -> |-) =
    ([x . x x]
      {
        {[ph.]
          ([.ph ph] wi [id.])
        }
      }
    )
  )
})"#;
        let offset = text.find("[.ph ph]").unwrap() + 2;
        let info = hover_info_at_position(text, position_at_offset(text, offset)).unwrap();

        assert_eq!(info.text, "ph");
        assert_eq!(info.type_info, "wff");
        assert_eq!(info.to_markdown(), "`ph : wff`");
    }

    #[test]
    fn hover_reports_operation_profile() {
        let text = r#"(theory fol.syntax nat {
  (arr wff : 1 -> 1)
  (arr -> : 2 -> 1)
})

(theory fol.proof fol.syntax {
  (arr wi : {wff wff} -> (-> wff))
})"#;
        let offset = text.find("wi").unwrap();
        let info = hover_info_at_position(text, position_at_offset(text, offset)).unwrap();

        assert_eq!(info.text, "wi");
        assert_eq!(info.type_info, "{wff wff} -> (-> wff)");
        assert_eq!(info.to_markdown(), "`wi : {wff wff} -> (-> wff)`");
    }
}
