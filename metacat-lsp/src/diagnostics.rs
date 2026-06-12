use metacat::check::check;
use metacat::theory::{Theory, TheorySet};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::syntax::{is_operation_char, matching_close_offset, position_at_offset, token_at};

/// Current diagnostic strategy:
/// - parse and resolve the in-memory document
/// - check every definition in every user theory
/// - report one coarse file-level error when validation fails
///
/// The next natural extension is source spans in `metacat` errors, then map
/// those spans into precise diagnostic ranges here.
pub fn diagnostics_for_document(text: &str) -> Vec<Diagnostic> {
    validate_document(text)
        .err()
        .map(|message| vec![document_diagnostic(text, message)])
        .unwrap_or_default()
}

fn validate_document(text: &str) -> std::result::Result<(), String> {
    let theories = TheorySet::from_text(text).map_err(|error| error.to_string())?;

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

    Ok(())
}

fn document_diagnostic(text: &str, message: String) -> Diagnostic {
    Diagnostic {
        range: diagnostic_range(text, &message).unwrap_or_else(file_start_range),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("metacat".to_string()),
        message,
        ..Diagnostic::default()
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
        let diagnostic = document_diagnostic(text, message);
        let expected = text.find("} :)").unwrap() + 2;

        assert_eq!(diagnostic.range.start, position_at_offset(text, expected));
        assert_eq!(diagnostic.range.end, position_at_offset(text, expected + 1));
    }
}
