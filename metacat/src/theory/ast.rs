//! Raw AST for the multi-theory surface syntax.
//!
//! These types stay close to the parsed source:
//! - theory and arrow names are still plain [`hexpr::Operation`]s;
//! - declaration annotations are retained as uninterpreted hexpr pairs;
//! - source/target maps and definitions are still plain [`hexpr::Hexpr`]s;
//! - no cross-theory references have been resolved yet.
//!
//! This layer is responsible only for recognizing the expected hexpr shapes and
//! collecting them into a mergeable set of raw theories.

use hexpr::{Hexpr, Operation, ParseError, parse_hexprs};
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
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

/// An uninterpreted declaration annotation written as `(kind value)`.
pub type RawAnnotation = (Hexpr, Hexpr);

#[derive(Clone, Debug)]
pub struct RawTheoryArrow {
    pub name: Operation,
    pub annotations: Vec<RawAnnotation>,
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
    #[error(transparent)]
    Merge(#[from] MergeRawError),
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

#[derive(Debug, thiserror::Error)]
pub enum ExtensionsError {
    #[error("Extension targets unknown theory {0}")]
    UnknownTheory(Operation),
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
            match RawTopLevel::try_from_hexpr(hexpr)? {
                RawTopLevel::Theory(theory) => match theories.entry(theory.name.clone()) {
                    Entry::Occupied(_) => {
                        return Err(ParseRawError::DuplicateTheory(theory.name));
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(theory);
                    }
                },
                RawTopLevel::Extension(extension) => extensions.push(extension),
            }
        }

        Ok(Self {
            theories,
            extensions,
        })
    }

    /// Parse and merge multiple source strings into one [`RawTheorySet`].
    pub fn from_texts<'a, I>(texts: I) -> Result<Self, ParseRawError>
    where
        I: IntoIterator<Item = &'a str>,
    {
        merge_raw_sets(texts.into_iter().map(Self::from_text))
    }

    pub fn from_file(path: PathBuf) -> Result<Self, ParseRawError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_text(&text)
    }

    /// Parse and merge multiple files into one [`RawTheorySet`].
    pub fn from_files<I>(paths: I) -> Result<Self, ParseRawError>
    where
        I: IntoIterator<Item = PathBuf>,
    {
        merge_raw_sets(paths.into_iter().map(Self::from_file))
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

    /// Fold top-level extensions into their target theories.
    pub fn with_extensions(mut self) -> Result<Self, ExtensionsError> {
        for extension in self.extensions.drain(..) {
            let theory = self
                .theories
                .get_mut(&extension.theory)
                .ok_or_else(|| ExtensionsError::UnknownTheory(extension.theory.clone()))?;
            for (name, arrow) in extension.arrows {
                if theory.arrows.insert(name.clone(), arrow).is_some() {
                    return Err(ExtensionsError::DuplicateArrow {
                        theory: theory.name.clone(),
                        arrow: name,
                    });
                }
            }
        }
        Ok(self)
    }

    /// Render this raw theory set as valid theory hexpr text.
    ///
    /// The output is intended to roundtrip through [`RawTheorySet::from_text`].
    pub fn to_hexpr_text(&self) -> String {
        let mut out = String::new();
        let mut first = true;
        for theory in self.theories.values() {
            if !first {
                out.push_str("\n\n");
            }
            first = false;
            out.push_str(&theory_to_hexpr_text(theory));
        }
        for extension in &self.extensions {
            if !first {
                out.push_str("\n\n");
            }
            first = false;
            out.push_str(&extension_to_hexpr_text(extension));
        }
        out
    }
}

fn merge_raw_sets<I>(sets: I) -> Result<RawTheorySet, ParseRawError>
where
    I: IntoIterator<Item = Result<RawTheorySet, ParseRawError>>,
{
    let mut merged = RawTheorySet {
        theories: BTreeMap::new(),
        extensions: Vec::new(),
    };
    for set in sets {
        merged = merged.merge(set?).map_err(ParseRawError::from)?;
    }
    Ok(merged)
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

    fn has_valid_header(hexpr: &Hexpr) -> bool {
        let Hexpr::Composition(parts) = hexpr else {
            return false;
        };
        let [
            keyword,
            Hexpr::Operation(_),
            Hexpr::Operation(_),
            Hexpr::Tensor(_),
        ] = &parts[..]
        else {
            return false;
        };
        is_operation(keyword, "theory")
    }

    /// Preflight by borrowing so a top-level parse error can retain the whole
    /// original expression without cloning it on the successful path.
    fn is_valid_hexpr(hexpr: &Hexpr) -> bool {
        if !Self::has_valid_header(hexpr) {
            return false;
        }
        let Hexpr::Composition(parts) = hexpr else {
            unreachable!("validated theory header");
        };
        let Hexpr::Tensor(body) = &parts[3] else {
            unreachable!("validated theory body");
        };
        let mut names = BTreeSet::new();
        for declaration in body {
            let Hexpr::Composition(parts) = declaration else {
                return false;
            };
            if RawTheoryArrow::annotations_end(parts, 1).is_none() {
                return false;
            }
            let Hexpr::Operation(name) = &parts[1] else {
                unreachable!("validated arrow name");
            };
            if !names.insert(name) {
                return false;
            }
        }
        true
    }

    pub fn from_hexpr(hexpr: Hexpr) -> Result<Self, ParseRawError> {
        if !Self::has_valid_header(&hexpr) {
            return Err(ParseRawError::InvalidTheoryDeclaration(hexpr));
        }
        let Hexpr::Composition(parts) = hexpr else {
            unreachable!("validated theory header");
        };
        let parts: [Hexpr; 4] = parts.try_into().expect("validated theory header");
        let [
            _,
            Hexpr::Operation(name),
            Hexpr::Operation(syntax_category),
            Hexpr::Tensor(body),
        ] = parts
        else {
            unreachable!("validated theory header");
        };

        let mut arrows = BTreeMap::new();
        for declaration in body {
            let arrow = RawTheoryArrow::try_from_hexpr(declaration).map_err(|declaration| {
                ParseRawError::InvalidArrowDeclaration {
                    theory: name.clone(),
                    declaration,
                }
            })?;
            match arrows.entry(arrow.name.clone()) {
                Entry::Occupied(_) => {
                    return Err(ParseRawError::DuplicateArrow {
                        theory: name,
                        arrow: arrow.name,
                    });
                }
                Entry::Vacant(entry) => {
                    entry.insert(arrow);
                }
            }
        }

        Ok(Self {
            name,
            syntax_category,
            arrows,
        })
    }
}

impl Extension {
    fn try_from_top_level_def(hexpr: Hexpr) -> Result<Self, Hexpr> {
        let Hexpr::Composition(parts) = &hexpr else {
            return Err(hexpr);
        };

        if !parts.first().is_some_and(|kind| is_operation(kind, "def")) {
            return Err(hexpr);
        }
        let Some(Hexpr::Operation(theory)) = parts.get(1) else {
            return Err(hexpr);
        };
        if RawTheoryArrow::annotations_end(parts, 2).is_none() {
            return Err(hexpr);
        }
        let theory = theory.clone();
        let Hexpr::Composition(parts) = hexpr else {
            unreachable!("validated top-level definition");
        };
        let raw_arrow = RawTheoryArrow::from_validated_parts(parts, 2);
        let mut arrows = BTreeMap::new();
        arrows.insert(raw_arrow.name.clone(), raw_arrow);
        Ok(Self { theory, arrows })
    }
}

impl RawTopLevel {
    fn try_from_hexpr(hexpr: Hexpr) -> Result<Self, ParseRawError> {
        if RawTheory::is_valid_hexpr(&hexpr) {
            return RawTheory::from_hexpr(hexpr).map(Self::Theory);
        }
        let hexpr = match Extension::try_from_top_level_def(hexpr) {
            Ok(extension) => return Ok(Self::Extension(extension)),
            Err(hexpr) => hexpr,
        };
        if matches!(&hexpr, Hexpr::Composition(parts) if matches!(parts.first(), Some(Hexpr::Operation(op)) if op.as_str() == "def"))
        {
            return Err(ParseRawError::InvalidTopLevelDefinition(hexpr));
        }
        Err(ParseRawError::InvalidTheoryDeclaration(hexpr))
    }
}

impl RawTheoryArrow {
    fn try_from_hexpr(hexpr: Hexpr) -> Result<Self, Hexpr> {
        let Hexpr::Composition(parts) = &hexpr else {
            return Err(hexpr);
        };
        if Self::annotations_end(parts, 1).is_none() {
            return Err(hexpr);
        }
        let Hexpr::Composition(parts) = hexpr else {
            unreachable!("validated arrow declaration");
        };
        Ok(Self::from_validated_parts(parts, 1))
    }

    /// Validate the declaration and locate `:`, using `name_index` for the name.
    /// Top-level definitions have an extra theory name before the arrow name.
    fn annotations_end(parts: &[Hexpr], name_index: usize) -> Option<usize> {
        let kind = parts.first()?;
        let Hexpr::Operation(_) = parts.get(name_index)? else {
            return None;
        };

        let annotations_end = if is_operation(kind, "arr") {
            let annotations_end = parts.len().checked_sub(4)?;
            let [colon, _, arrow, _] = &parts[annotations_end..] else {
                return None;
            };
            if !is_operation(colon, ":") || !is_operation(arrow, "->") {
                return None;
            }
            annotations_end
        } else if is_operation(kind, "def") {
            let annotations_end = parts.len().checked_sub(6)?;
            let [colon, _, arrow, _, eq, _] = &parts[annotations_end..] else {
                return None;
            };
            if !is_operation(colon, ":") || !is_operation(arrow, "->") || !is_operation(eq, "=") {
                return None;
            }
            annotations_end
        } else {
            return None;
        };

        let annotations_start = name_index + 1;
        if annotations_end < annotations_start {
            return None;
        }
        if !parts[annotations_start..annotations_end]
            .iter()
            .all(|annotation| matches!(annotation, Hexpr::Composition(pair) if pair.len() == 2))
        {
            return None;
        }
        Some(annotations_end)
    }

    /// Move the already-validated fields out, retaining the original AST buffers.
    fn from_validated_parts(mut parts: Vec<Hexpr>, name_index: usize) -> Self {
        let definition = if is_operation(&parts[0], "def") {
            let body = parts.pop().expect("validated definition");
            parts.pop(); // =
            Some(body)
        } else {
            None
        };
        let target = parts.pop().expect("validated target");
        parts.pop(); // ->
        let source = parts.pop().expect("validated source");
        parts.pop(); // :
        let annotations = parts
            .drain(name_index + 1..)
            .map(|annotation| parse_annotation(annotation).expect("validated annotation"))
            .collect();
        let Hexpr::Operation(name) = parts.pop().expect("validated name") else {
            unreachable!("validated arrow name");
        };

        Self {
            name,
            annotations,
            type_maps: (source, target),
            definition,
        }
    }
}

fn parse_annotation(hexpr: Hexpr) -> Option<RawAnnotation> {
    let Hexpr::Composition(parts) = hexpr else {
        return None;
    };
    let [kind, value]: [Hexpr; 2] = parts.try_into().ok()?;
    Some((kind, value))
}

fn theory_to_hexpr_text(theory: &RawTheory) -> String {
    let mut out = format!("(theory {} {} {{\n", theory.name, theory.syntax_category);
    for arrow in theory.arrows.values() {
        out.push_str("  ");
        out.push_str(&arrow_to_hexpr_text(arrow));
        out.push('\n');
    }
    out.push_str("})");
    out
}

fn extension_to_hexpr_text(extension: &Extension) -> String {
    extension
        .arrows
        .values()
        .map(|arrow| top_level_def_to_hexpr_text(&extension.theory, arrow))
        .collect::<Vec<_>>()
        .join("\n")
}

fn arrow_to_hexpr_text(arrow: &RawTheoryArrow) -> String {
    let (source, target) = &arrow.type_maps;
    let annotations = annotations_to_hexpr_text(&arrow.annotations);
    match &arrow.definition {
        None => format!(
            "(arr {}{} : {} -> {})",
            arrow.name, annotations, source, target
        ),
        Some(definition) => {
            format!(
                "(def {}{} : {} -> {} = {})",
                arrow.name, annotations, source, target, definition
            )
        }
    }
}

fn top_level_def_to_hexpr_text(theory: &Operation, arrow: &RawTheoryArrow) -> String {
    let (source, target) = &arrow.type_maps;
    let annotations = annotations_to_hexpr_text(&arrow.annotations);
    let definition = arrow
        .definition
        .as_ref()
        .expect("top-level extension arrows must be bona-fide definitions");
    format!(
        "(def {} {}{} : {} -> {} = {})",
        theory, arrow.name, annotations, source, target, definition
    )
}

fn annotations_to_hexpr_text(annotations: &[RawAnnotation]) -> String {
    annotations
        .iter()
        .map(|(kind, value)| format!(" ({kind} {value})"))
        .collect::<Vec<_>>()
        .join("")
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
    fn declaration_annotations_parse_and_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let raw = RawTheorySet::from_text(
            r#"
            (theory annotated nat {
              (arr plain : 1 -> 1)
              (arr constrained (dv (del sel)) (metadata :) : 1 -> 1)
              (def derived (dv :) : 1 -> 1 = constrained)
            })

            (def annotated extended (dv :) : 1 -> 1 = plain)
            "#,
        )?;

        let theory = raw.theories.get(&"annotated".parse()?).unwrap();
        assert!(theory.arrows[&"plain".parse()?].annotations.is_empty());

        let constrained = &theory.arrows[&"constrained".parse()?];
        assert_eq!(constrained.annotations.len(), 2);
        assert_eq!(constrained.annotations[0].0.to_string(), "dv");
        assert_eq!(constrained.annotations[0].1.to_string(), "(del sel)");
        assert_eq!(constrained.annotations[1].0.to_string(), "metadata");
        assert_eq!(constrained.annotations[1].1.to_string(), ":");

        let derived = &theory.arrows[&"derived".parse()?];
        assert_eq!(derived.annotations[0].1.to_string(), ":");

        let extended = &raw.extensions[0].arrows[&"extended".parse()?];
        assert_eq!(extended.annotations[0].0.to_string(), "dv");
        assert_eq!(extended.annotations[0].1.to_string(), ":");

        let dumped = raw.to_hexpr_text();
        let reparsed = RawTheorySet::from_text(&dumped)?;
        assert_eq!(dumped, reparsed.to_hexpr_text());
        Ok(())
    }

    #[test]
    fn declaration_annotations_must_be_pairs() {
        let error = RawTheorySet::from_text(
            r#"
            (theory invalid nat {
              (arr bad (dv) : 1 -> 1)
            })
            "#,
        )
        .unwrap_err();

        assert!(matches!(error, ParseRawError::InvalidTheoryDeclaration(_)));
    }

    #[test]
    fn invalid_top_levels_retain_original_expression() -> Result<(), Box<dyn std::error::Error>> {
        for text in [
            "(theory bad nat {(arr f (dv) : 1 -> 1)})",
            "(theory bad nat {(arr f : 1 -> 1) (arr f : 1 -> 1)})",
            "(theory bad nat {(arr f : 1 ->)})",
            "(theory bad nat (arr f : 1 -> 1))",
        ] {
            let original: Hexpr = text.parse()?;
            let ParseRawError::InvalidTheoryDeclaration(declaration) =
                RawTheorySet::from_text(text).unwrap_err()
            else {
                panic!("expected invalid theory: {text}");
            };
            assert_eq!(declaration, original);
        }
        let text = "(def theory f (dv too many) : 1 -> 1 = f)";
        let ParseRawError::InvalidTopLevelDefinition(declaration) =
            RawTheorySet::from_text(text).unwrap_err()
        else {
            panic!("expected invalid top-level definition");
        };
        assert_eq!(declaration, text.parse()?);
        Ok(())
    }

    #[test]
    fn from_hexpr_retains_detailed_arrow_errors() -> Result<(), Box<dyn std::error::Error>> {
        let declaration: Hexpr = "(arr f (dv) : 1 -> 1)".parse()?;
        let text = format!("(theory bad nat {{{declaration}}})");
        let ParseRawError::InvalidArrowDeclaration {
            theory,
            declaration: actual,
        } = RawTheory::from_hexpr(text.parse()?).unwrap_err()
        else {
            panic!("expected invalid arrow");
        };
        assert_eq!(theory.as_str(), "bad");
        assert_eq!(actual, declaration);

        let text = "(theory bad nat {(arr f : 1 -> 1) (arr f : 1 -> 1)})";
        let ParseRawError::DuplicateArrow { theory, arrow } =
            RawTheory::from_hexpr(text.parse()?).unwrap_err()
        else {
            panic!("expected duplicate arrow");
        };
        assert_eq!(theory.as_str(), "bad");
        assert_eq!(arrow.as_str(), "f");
        Ok(())
    }

    #[test]
    fn moves_arrow_fields_without_cloning() -> Result<(), Box<dyn std::error::Error>> {
        let mut expr: Hexpr =
            "(def f ((tag _) ^(label _)) : {^label [x^(a b) . x]} -> {} = (g _))".parse()?;
        let Hexpr::Composition(parts) = &mut expr else {
            unreachable!()
        };
        let Hexpr::Composition(body) = parts.last_mut().unwrap() else {
            unreachable!()
        };
        body.reserve(32);
        let body_ptr = body.as_ptr();
        let Hexpr::Operation(op) = &body[0] else {
            unreachable!()
        };
        let name_ptr = op.as_str().as_ptr();
        let original = expr.clone();
        let arrow = RawTheoryArrow::try_from_hexpr(expr).unwrap();
        assert_eq!(arrow_to_hexpr_text(&arrow).parse::<Hexpr>()?, original);
        let Hexpr::Composition(body) = arrow.definition.as_ref().unwrap() else {
            unreachable!()
        };
        assert_eq!(body.as_ptr(), body_ptr);
        let Hexpr::Operation(op) = &body[0] else {
            unreachable!()
        };
        assert_eq!(op.as_str().as_ptr(), name_ptr);
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

    #[test]
    fn raw_theory_set_display_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
        let raw = RawTheorySet::from_text(
            r#"
            (theory fol.syntax nat {
              (arr wff : 1 -> 1)
              (arr -> : 2 -> 1)
            })

            (theory fol.proof fol.syntax {
              (arr wi : {wff wff} -> (-> wff))
              (def win : {wff wff} -> (-> -. wff) = (wi wn))
            })

            (def fol.syntax boxed : 1 -> 1 = wff)
            "#,
        )?;

        let dumped = raw.to_hexpr_text();
        let reparsed = RawTheorySet::from_text(&dumped)?;

        assert_eq!(dumped, reparsed.to_hexpr_text());
        Ok(())
    }
}
