//! Raw AST for the multi-theory surface syntax.
//!
//! These types stay close to the parsed source:
//! - theory and arrow names are still plain [`hexpr::Operation`]s;
//! - source/target maps and definitions are still plain [`hexpr::Hexpr`]s;
//! - no cross-theory references have been resolved yet.
//!
//! This layer is responsible only for recognizing the expected hexpr shapes and
//! collecting them into a mergeable set of raw theories.

use hexpr::{Hexpr, Operation, ParseError, parse_hexprs};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct RawTheorySet {
    pub theories: BTreeMap<Operation, RawTheory>,
    pub extensions: Vec<Extension>,
}

#[derive(Clone, Debug)]
pub struct RawTheory {
    pub name: Operation,
    pub syntax_category: Operation,
    pub arrows: BTreeMap<Operation, RawTheoryArrow>,
}

#[derive(Clone, Debug)]
pub struct RawTheoryArrow {
    pub name: Operation,
    pub type_maps: (Hexpr, Hexpr),
    pub definition: Option<Hexpr>,
}

#[derive(Clone, Debug)]
/// A conservative raw extension of an existing theory by fresh definitions.
pub struct Extension {
    pub theory: Operation,
    pub arrows: BTreeMap<Operation, RawTheoryArrow>,
}

#[derive(Clone, Debug)]
enum RawTopLevel {
    Theory(RawTheory),
    Extension(Extension),
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
    #[error("Invalid top-level definition: {0}")]
    InvalidTopLevelDefinition(Hexpr),
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum MergeRawError {
    #[error("Theory {0} is declared in multiple inputs")]
    DuplicateTheory(Operation),
    #[error("Theory {theory} has incompatible syntax categories: {left} vs {right}")]
    SyntaxMismatch {
        theory: Operation,
        left: Operation,
        right: Operation,
    },
    #[error("Theory {theory} declares arrow {arrow} multiple times")]
    DuplicateArrow { theory: Operation, arrow: Operation },
}

impl RawTheorySet {
    /// Parse text into a [`RawTheorySet`] consisting of zero or more top-level theory declarations
    /// and conservative extensions.
    pub fn from_text(text: &str) -> Result<Self, ParseRawError> {
        let hexprs = parse_hexprs(text)?;
        let mut theories = BTreeMap::new();
        let mut extensions = Vec::new();

        for hexpr in hexprs {
            match RawTopLevel::try_from_hexpr(hexpr.clone())? {
                RawTopLevel::Theory(theory) => {
                    if theories
                        .insert(theory.name.clone(), theory.clone())
                        .is_some()
                    {
                        return Err(ParseRawError::DuplicateTheory(theory.name));
                    }
                }
                RawTopLevel::Extension(extension) => extensions.push(extension),
            }
        }

        Ok(Self { theories, extensions })
    }

    pub fn from_file(path: PathBuf) -> Result<Self, ParseRawError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_text(&text)
    }

    pub fn merge(mut self, other: Self) -> Result<Self, MergeRawError> {
        for (name, theory) in other.theories {
            match self.theories.remove(&name) {
                None => {
                    self.theories.insert(name, theory);
                }
                Some(existing) => {
                    let merged = existing.merge(theory)?;
                    self.theories.insert(name, merged);
                }
            }
        }
        self.extensions.extend(other.extensions);

        Ok(self)
    }
}

impl RawTheory {
    pub fn merge(mut self, other: Self) -> Result<Self, MergeRawError> {
        debug_assert_eq!(self.name, other.name);

        if self.syntax_category != other.syntax_category {
            return Err(MergeRawError::SyntaxMismatch {
                theory: self.name.clone(),
                left: self.syntax_category.clone(),
                right: other.syntax_category,
            });
        }

        for (name, arrow) in other.arrows {
            if self.arrows.insert(name.clone(), arrow).is_some() {
                return Err(MergeRawError::DuplicateArrow {
                    theory: self.name.clone(),
                    arrow: name,
                });
            }
        }

        Ok(self)
    }

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

        let mut arrows = BTreeMap::new();
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

        let mut arrows = BTreeMap::new();
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

impl Extension {
    fn try_from_top_level_def(hexpr: Hexpr) -> Option<Self> {
        let Hexpr::Composition(parts) = hexpr else {
            return None;
        };

        match &parts[..] {
            [
                kind,
                Hexpr::Operation(theory),
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
                let raw_arrow = RawTheoryArrow {
                    name: name.clone(),
                    type_maps: (source.clone(), target.clone()),
                    definition: Some(def.clone()),
                };
                let mut arrows = BTreeMap::new();
                arrows.insert(name.clone(), raw_arrow);
                Some(Self {
                    theory: theory.clone(),
                    arrows,
                })
            }
            _ => None,
        }
    }
}

impl RawTopLevel {
    fn try_from_hexpr(hexpr: Hexpr) -> Result<Self, ParseRawError> {
        if let Some(theory) = RawTheory::try_from_hexpr(hexpr.clone()) {
            return Ok(Self::Theory(theory));
        }
        if let Some(extension) = Extension::try_from_top_level_def(hexpr.clone()) {
            return Ok(Self::Extension(extension));
        }
        if matches!(&hexpr, Hexpr::Composition(parts) if matches!(parts.first(), Some(Hexpr::Operation(op)) if op.as_str() == "def")) {
            return Err(ParseRawError::InvalidTopLevelDefinition(hexpr));
        }
        Err(ParseRawError::InvalidTheoryDeclaration(hexpr))
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
        let raw = RawTheorySet::from_text(
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
        assert!(raw.extensions.is_empty());
        assert!(raw.theories.contains_key(&"fol.syntax".parse()?));
        assert!(raw.theories.contains_key(&"fol.proof".parse()?));
        Ok(())
    }

    #[test]
    fn invalid_theory_reports_error() {
        let err = RawTheorySet::from_text(
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

    #[test]
    fn raw_theory_sets_merge() -> Result<(), Box<dyn std::error::Error>> {
        let lhs = RawTheorySet::from_text(
            r#"
            (theory fol.syntax nat {
              (arr wff : 1 -> 1)
            })
            "#,
        )?;
        let rhs = RawTheorySet::from_text(
            r#"
            (theory fol.syntax nat {
              (arr -> : 2 -> 1)
            })
            "#,
        )?;

        let merged = lhs.merge(rhs)?;
        let theory = merged.theories.get(&"fol.syntax".parse()?).unwrap();
        assert_eq!(theory.arrows.len(), 2);
        Ok(())
    }

    #[test]
    fn parse_top_level_definition_as_extension() -> Result<(), Box<dyn std::error::Error>> {
        let raw = RawTheorySet::from_text(
            r#"
            (theory fol.syntax nat {
              (arr wff : 1 -> 1)
            })

            (def fol.syntax boxed : 1 -> 1 = wff)
            "#,
        )?;

        assert_eq!(raw.extensions.len(), 1);
        let ext = &raw.extensions[0];
        assert_eq!(ext.theory, "fol.syntax".parse()?);
        assert!(ext.arrows.contains_key(&"boxed".parse()?));
        Ok(())
    }

    #[test]
    fn merge_rejects_duplicate_arrows() -> Result<(), Box<dyn std::error::Error>> {
        let lhs = RawTheorySet::from_text(
            r#"
            (theory fol.syntax nat {
              (arr wff : 1 -> 1)
            })
            "#,
        )?;
        let rhs = RawTheorySet::from_text(
            r#"
            (theory fol.syntax nat {
              (arr wff : 1 -> 1)
            })
            "#,
        )?;

        let err = lhs.merge(rhs).unwrap_err();
        assert!(matches!(err, MergeRawError::DuplicateArrow { .. }));
        Ok(())
    }
}
