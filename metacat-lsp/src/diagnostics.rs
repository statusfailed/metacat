use metacat::check::check;
use metacat::theory::{RawTheorySet, Theory, TheoryId, TheorySet};
use std::collections::BTreeSet;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, Position, Range};

use crate::syntax::{is_operation_char, matching_close_offset, position_at_offset, token_at};

/// Current diagnostic strategy:
/// - parse and resolve the in-memory document
/// - check every definition in every user theory
/// - report one coarse file-level error when validation fails
///
/// The next natural extension is source spans in `metacat` errors, then map
/// those spans into precise diagnostic ranges here.
pub fn diagnostics_for_document(current_text: &str, project_texts: &[String]) -> Vec<Diagnostic> {
    match validate_document(project_texts) {
        Ok(theories) => unused_arrow_diagnostics(current_text, &theories),
        Err(message) => vec![document_error_diagnostic(current_text, message)],
    }
}

fn validate_document(texts: &[String]) -> std::result::Result<TheorySet, String> {
    let theories = TheorySet::from_texts(texts.iter().map(String::as_str))
        .map_err(|error| error.to_string())?;

    for (theory_id, theory) in &theories.theories {
        let Theory::Theory { arrows, .. } = theory else {
            continue;
        };

        for declaration in arrows.values().filter(|arrow| arrow.definition.is_some()) {
            let mut term = declaration
                .definition
                .clone()
                .expect("filtered to definitional arrows");
            let (source, target) = declaration.type_maps.clone();
            check(theory, source, target, &mut term).map_err(|error| {
                format!(
                    "Checking '{}.{}' failed: {}",
                    theory_id, declaration.name, error
                )
            })?;
        }
    }

    Ok(theories)
}

fn document_error_diagnostic(text: &str, message: String) -> Diagnostic {
    Diagnostic {
        range: diagnostic_range(text, &message).unwrap_or_else(file_start_range),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("metacat".to_string()),
        message,
        ..Diagnostic::default()
    }
}

fn unused_arrow_diagnostics(text: &str, theories: &TheorySet) -> Vec<Diagnostic> {
    let Ok(raw_current) = RawTheorySet::from_text(text) else {
        return Vec::new();
    };
    let used = used_arrows(theories);
    let mut diagnostics = Vec::new();

    for raw_theory in raw_current.theories.values() {
        let theory_id = TheoryId(raw_theory.name.clone());
        for arrow in raw_theory.arrows.values() {
            if used.contains(&(theory_id.clone(), arrow.name.clone())) {
                continue;
            }
            let Some(range) = declaration_name_range(text, "arr", arrow.name.as_str())
                .or_else(|| declaration_name_range(text, "def", arrow.name.as_str()))
            else {
                continue;
            };
            diagnostics.push(unused_arrow_diagnostic(range, arrow.name.as_str()));
        }
    }

    for extension in &raw_current.extensions {
        let theory_id = TheoryId(extension.theory.clone());
        for arrow in extension.arrows.values() {
            if used.contains(&(theory_id.clone(), arrow.name.clone())) {
                continue;
            }
            let Some(range) = top_level_definition_name_range(
                text,
                extension.theory.as_str(),
                arrow.name.as_str(),
            ) else {
                continue;
            };
            diagnostics.push(unused_arrow_diagnostic(range, arrow.name.as_str()));
        }
    }

    diagnostics
}

fn unused_arrow_diagnostic(range: Range, arrow: &str) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::HINT),
        source: Some("metacat".to_string()),
        message: format!("Arrow '{arrow}' is not used in the loaded metacat project"),
        tags: Some(vec![DiagnosticTag::UNNECESSARY]),
        ..Diagnostic::default()
    }
}

fn used_arrows(theories: &TheorySet) -> BTreeSet<(TheoryId, hexpr::Operation)> {
    let mut used = BTreeSet::new();

    for (theory_id, theory) in &theories.theories {
        let Theory::Theory { syntax, arrows } = theory else {
            continue;
        };

        for arrow in arrows.values() {
            collect_operations(&arrow.raw.type_maps.0, |operation| {
                used.insert((syntax.clone(), operation.clone()));
            });
            collect_operations(&arrow.raw.type_maps.1, |operation| {
                used.insert((syntax.clone(), operation.clone()));
            });
            if let Some(definition) = &arrow.raw.definition {
                collect_operations(definition, |operation| {
                    used.insert((theory_id.clone(), operation.clone()));
                });
            }
        }
    }

    used
}

fn collect_operations<F>(hexpr: &hexpr::Hexpr, mut visit: F)
where
    F: FnMut(&hexpr::Operation),
{
    collect_operations_with(hexpr, &mut visit);
}

fn collect_operations_with<F>(hexpr: &hexpr::Hexpr, visit: &mut F)
where
    F: FnMut(&hexpr::Operation),
{
    match hexpr {
        hexpr::Hexpr::Composition(parts) | hexpr::Hexpr::Tensor(parts) => {
            for part in parts {
                collect_operations_with(part, visit);
            }
        }
        hexpr::Hexpr::Frobenius { .. } | hexpr::Hexpr::Hole | hexpr::Hexpr::Wire(_) => {}
        hexpr::Hexpr::Operation(operation) => visit(operation),
    }
}

fn diagnostic_range(text: &str, message: &str) -> Option<Range> {
    let arrow = extract_between(message, "arrow ", ": Couldn't")?;
    let operation = extract_after(message, "No such operation ")?;
    let declaration = find_arrow_declaration(text, arrow)?;
    find_operation_after_header(text, declaration, operation).or_else(|| {
        Some(Range {
            start: position_at_offset(text, declaration.start),
            end: position_at_offset(text, declaration.start + 1),
        })
    })
}

fn file_start_range() -> Range {
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: 1,
        },
    }
}

#[derive(Clone, Copy, Debug)]
struct Span {
    start: usize,
    end: usize,
}

fn find_arrow_declaration(text: &str, arrow: &str) -> Option<Span> {
    find_declaration(text, "arr", arrow).or_else(|| find_declaration(text, "def", arrow))
}

fn find_declaration(text: &str, keyword: &str, name: &str) -> Option<Span> {
    let needle = format!("({keyword}");
    let mut search_from = 0;
    while let Some(relative_start) = text[search_from..].find(&needle) {
        let start = search_from + relative_start;
        let mut offset = start + needle.len();
        offset = skip_whitespace(text, offset);
        let token = token_at(text, offset, is_operation_char)?;
        if token.text == name {
            let end = matching_close_offset(text, start)?;
            return Some(Span { start, end });
        }
        search_from = start + 1;
    }
    None
}

fn declaration_name_range(text: &str, keyword: &str, name: &str) -> Option<Range> {
    let needle = format!("({keyword}");
    let mut search_from = 0;
    while let Some(relative_start) = text[search_from..].find(&needle) {
        let start = search_from + relative_start;
        let mut offset = start + needle.len();
        offset = skip_whitespace(text, offset);
        let token = token_at(text, offset, is_operation_char)?;
        if token.text == name {
            return Some(Range {
                start: position_at_offset(text, token.start),
                end: position_at_offset(text, token.end),
            });
        }
        search_from = start + 1;
    }
    None
}

fn top_level_definition_name_range(text: &str, theory: &str, name: &str) -> Option<Range> {
    let needle = "(def";
    let mut search_from = 0;
    while let Some(relative_start) = text[search_from..].find(needle) {
        let start = search_from + relative_start;
        let mut offset = start + needle.len();
        offset = skip_whitespace(text, offset);

        let theory_token = token_at(text, offset, is_operation_char)?;
        offset = skip_whitespace(text, theory_token.end);
        let name_token = token_at(text, offset, is_operation_char)?;
        if theory_token.text == theory && name_token.text == name {
            return Some(Range {
                start: position_at_offset(text, name_token.start),
                end: position_at_offset(text, name_token.end),
            });
        }
        search_from = start + 1;
    }
    None
}

fn find_operation_after_header(text: &str, declaration: Span, operation: &str) -> Option<Range> {
    let mut offset = declaration.start + 1;
    offset = skip_non_whitespace(text, offset, declaration.end);
    offset = skip_whitespace(text, offset);
    let name = token_at(text, offset, is_operation_char)?;
    offset = name.end;

    let header_separator = find_header_separator(text, offset, declaration.end)?;
    let mut search_from = header_separator + 1;
    while search_from < declaration.end {
        let relative = text[search_from..declaration.end].find(operation)?;
        let start = search_from + relative;
        if operation_token_matches(text, start, declaration.end, operation) {
            return Some(Range {
                start: position_at_offset(text, start),
                end: position_at_offset(text, start + operation.len()),
            });
        }
        search_from = start + operation.len().max(1);
    }
    None
}

fn find_header_separator(text: &str, mut offset: usize, end: usize) -> Option<usize> {
    while offset < end {
        offset = skip_whitespace(text, offset);
        let token = token_at(text, offset, is_operation_char)?;
        if token.text == ":" {
            return Some(token.start);
        }
        offset = token.end;
    }
    None
}

fn operation_token_matches(text: &str, start: usize, end: usize, operation: &str) -> bool {
    if start + operation.len() > end {
        return false;
    }
    if &text[start..start + operation.len()] != operation {
        return false;
    }
    let before = text[..start].chars().next_back();
    let after = text[start + operation.len()..].chars().next();
    !before.is_some_and(is_operation_char) && !after.is_some_and(is_operation_char)
}

fn extract_between<'a>(text: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let start = text.find(prefix)? + prefix.len();
    let end = text[start..].find(suffix)? + start;
    Some(&text[start..end])
}

fn extract_after<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let start = text.rfind(prefix)? + prefix.len();
    Some(text[start..].trim())
}

fn skip_whitespace(text: &str, mut offset: usize) -> usize {
    while offset < text.len() {
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

fn skip_non_whitespace(text: &str, mut offset: usize, end: usize) -> usize {
    while offset < end {
        let Some(ch) = text[offset..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
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
    fn diagnostic_range_points_to_unknown_operation_in_arrow_type_map() {
        let text = r#"(theory program type {
  (arr :.unpack : {[x t.]
    ({[.x] [.t]} :)
  } -> {[x t.]
    [.x]
    [.t]
    ({[.x] [.t]} :)
  })
})"#;
        let message = "Failed to interpret syntax map for theory program, arrow :.unpack: Couldn't parse op :: No such operation :".to_string();
        let diagnostic = document_error_diagnostic(text, message);
        let expected = text.find("} :)").unwrap() + 2;

        assert_eq!(diagnostic.range.start, position_at_offset(text, expected));
        assert_eq!(diagnostic.range.end, position_at_offset(text, expected + 1));
    }

    #[test]
    fn unused_arrows_are_reported_as_hints() {
        let text = r#"(theory syntax nat {
  (arr wff : 1 -> 1)
  (arr unused-syntax : 1 -> 1)
})

(theory proof syntax {
  (arr used : wff -> wff)
  (arr unused : wff -> wff)
  (def theorem : wff -> wff = (used))
})"#;
        let diagnostics = diagnostics_for_document(text, &[text.to_string()]);
        let messages = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("unused-syntax"))
        );
        assert!(messages.iter().any(|message| message.contains("unused")));
        assert!(messages.iter().any(|message| message.contains("theorem")));
        assert!(!messages.iter().any(|message| message.contains("'wff'")));
        assert!(!messages.iter().any(|message| message.contains("'used'")));
        assert!(diagnostics.iter().all(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::HINT)
                && diagnostic.tags == Some(vec![DiagnosticTag::UNNECESSARY])
        }));
    }
}
