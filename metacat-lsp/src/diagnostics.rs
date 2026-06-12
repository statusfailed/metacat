use metacat::check::check;
use metacat::theory::{Theory, TheorySet};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

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
        .map(|message| vec![document_diagnostic(message)])
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

fn document_diagnostic(message: String) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 1,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("metacat".to_string()),
        message,
        ..Diagnostic::default()
    }
}
