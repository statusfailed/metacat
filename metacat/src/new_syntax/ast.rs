//! Raw AST for the multi-theory surface syntax.
//!
//! These types stay close to the parsed source:
//! - theory and arrow names are still plain [`hexpr::Operation`]s;
//! - source/target maps and definitions are still plain [`hexpr::Hexpr`]s;
//! - no cross-theory references have been resolved yet.
//!
//! This layer is responsible only for recognizing the expected hexpr shapes and
//! collecting them into a file-level structure.

use hexpr::{Hexpr, Operation, ParseError, parse_hexprs};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct RawFile {
    pub theories: HashMap<Operation, RawTheory>,
}

#[derive(Clone, Debug)]
pub struct RawTheory {
    pub name: Operation,
    pub syntax_category: Operation,
    pub arrows: HashMap<Operation, RawTheoryArrow>,
}

#[derive(Clone, Debug)]
pub struct RawTheoryArrow {
    pub name: Operation,
    pub type_maps: (Hexpr, Hexpr),
    pub definition: Option<Hexpr>,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseRawError {
    #[error("Invalid theory declaration: {0}")]
    InvalidTheoryDeclaration(Hexpr),
    #[error("Invalid arrow declaration in theory {theory}: {declaration}")]
    InvalidArrowDeclaration {
        theory: Operation,
        declaration: Hexpr,
    },
    #[error("Duplicate theory declaration: {0}")]
    DuplicateTheory(Operation),
    #[error("Duplicate arrow declaration {arrow} in theory {theory}")]
    DuplicateArrow { theory: Operation, arrow: Operation },
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),
}

impl RawFile {
    /// Parse text into a [`RawFile`] consisting of zero or more top-level theory declarations
    pub fn from_text(text: &str) -> Result<Self, ParseRawError> {
        let hexprs = parse_hexprs(text)?;
        let mut theories = HashMap::new();

        for hexpr in hexprs {
            let theory = RawTheory::try_from_hexpr(hexpr.clone())
                .ok_or(ParseRawError::InvalidTheoryDeclaration(hexpr))?;
            if theories
                .insert(theory.name.clone(), theory.clone())
                .is_some()
            {
                return Err(ParseRawError::DuplicateTheory(theory.name));
            }
        }

        Ok(Self { theories })
    }
}

impl RawTheory {
    fn try_from_hexpr(hexpr: Hexpr) -> Option<Self> {
        let Hexpr::Composition(parts) = hexpr else {
            return None;
        };

        let [keyword, name, syntax_category, body] = &parts[..] else {
            return None;
        };

        if !is_operation(keyword, "theory") {
            return None;
        }

        let Hexpr::Operation(name) = name else {
            return None;
        };
        let Hexpr::Operation(syntax_category) = syntax_category else {
            return None;
        };
        let Hexpr::Tensor(body) = body else {
            return None;
        };

        let mut arrows = HashMap::new();
        for declaration in body {
            let arrow = RawTheoryArrow::try_from_hexpr(declaration.clone())?;
            if arrows.insert(arrow.name.clone(), arrow.clone()).is_some() {
                return None;
            }
        }

        Some(Self {
            name: name.clone(),
            syntax_category: syntax_category.clone(),
            arrows,
        })
    }

    pub fn from_hexpr(hexpr: Hexpr) -> Result<Self, ParseRawError> {
        let Hexpr::Composition(parts) = &hexpr else {
            return Err(ParseRawError::InvalidTheoryDeclaration(hexpr));
        };

        let theory_name = match &parts[..] {
            [keyword, Hexpr::Operation(name), _, _] if is_operation(keyword, "theory") => {
                name.clone()
            }
            _ => return Err(ParseRawError::InvalidTheoryDeclaration(hexpr)),
        };

        let Hexpr::Composition(parts) = hexpr.clone() else {
            unreachable!();
        };
        let [_, name, syntax_category, body] = &parts[..] else {
            return Err(ParseRawError::InvalidTheoryDeclaration(hexpr));
        };
        let Hexpr::Operation(name) = name else {
            return Err(ParseRawError::InvalidTheoryDeclaration(hexpr));
        };
        let Hexpr::Operation(syntax_category) = syntax_category else {
            return Err(ParseRawError::InvalidTheoryDeclaration(hexpr));
        };
        let Hexpr::Tensor(body) = body else {
            return Err(ParseRawError::InvalidTheoryDeclaration(hexpr));
        };

        let mut arrows = HashMap::new();
        for declaration in body {
            let arrow = RawTheoryArrow::try_from_hexpr(declaration.clone()).ok_or_else(|| {
                ParseRawError::InvalidArrowDeclaration {
                    theory: theory_name.clone(),
                    declaration: declaration.clone(),
                }
            })?;
            if arrows.insert(arrow.name.clone(), arrow.clone()).is_some() {
                return Err(ParseRawError::DuplicateArrow {
                    theory: theory_name.clone(),
                    arrow: arrow.name,
                });
            }
        }

        Ok(Self {
            name: name.clone(),
            syntax_category: syntax_category.clone(),
            arrows,
        })
    }
}

impl RawTheoryArrow {
    fn try_from_hexpr(hexpr: Hexpr) -> Option<Self> {
        let Hexpr::Composition(parts) = hexpr else {
            return None;
        };

        match &parts[..] {
            [kind, Hexpr::Operation(name), colon, source, arrow, target]
                if is_operation(kind, "arr")
                    && is_operation(colon, ":")
                    && is_operation(arrow, "->") =>
            {
                Some(Self {
                    name: name.clone(),
                    type_maps: (source.clone(), target.clone()),
                    definition: None,
                })
            }
            [
                kind,
                Hexpr::Operation(name),
                colon,
                source,
                arrow,
                target,
                eq,
                def,
            ] if is_operation(kind, "def")
                && is_operation(colon, ":")
                && is_operation(arrow, "->")
                && is_operation(eq, "=") =>
            {
                Some(Self {
                    name: name.clone(),
                    type_maps: (source.clone(), target.clone()),
                    definition: Some(def.clone()),
                })
            }
            _ => None,
        }
    }
}

fn is_operation(hexpr: &Hexpr, literal: &str) -> bool {
    matches!(hexpr, Hexpr::Operation(op) if op.as_str() == literal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_raw_file() -> Result<(), Box<dyn std::error::Error>> {
        let raw = RawFile::from_text(
            r#"
            (theory fol.syntax nat {
              (arr wff : 1 -> 1)
              (arr -> : 2 -> 1)
            })

            (theory fol.proof fol.syntax {
              (arr wi : {wff wff} -> (-> wff))
              (def win : {wff wff} -> (-> -. wff) = (wi))
            })
            "#,
        )?;

        assert_eq!(raw.theories.len(), 2);
        assert!(raw.theories.contains_key(&"fol.syntax".parse()?));
        assert!(raw.theories.contains_key(&"fol.proof".parse()?));
        Ok(())
    }

    #[test]
    fn invalid_theory_reports_error() {
        let err = RawFile::from_text(
            r#"
            (theory fol.syntax nat
              (arr wff : 1 -> 1)
            )
            "#,
        )
        .unwrap_err();

        eprintln!("invalid theory parse error: {err}");
        assert!(matches!(err, ParseRawError::InvalidTheoryDeclaration(_)));
    }
}
