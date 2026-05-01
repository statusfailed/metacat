//! Loader for the multi-theory syntax.
//!
//! This module performs the second phase after parsing:
//! - resolve each theory's syntax-category dependency;
//! - reject unknown bases and dependency cycles;
//! - interpret arrow type maps in the appropriate syntax category;
//! - interpret definitions against the completed local theory signature.
//!
//! The result is a resolved [`super::model::File`] whose theories are ready for
//! downstream checking and tooling.

use super::ast::{ParseRawError, RawFile};
use super::model::{
    ArrowKey, File, SignatureError, SyntaxBase, SyntaxTerm, Theory, TheoryArrow, TheoryId,
};
use super::nat::NatObj;
use hexpr::{Operation, try_interpret};
use open_hypergraphs::category::Arrow;
use open_hypergraphs::lax::OpenHypergraph;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error(transparent)]
    ParseRaw(#[from] ParseRawError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unknown syntax category {base} for theory {theory}")]
    UnknownSyntaxCategory { theory: TheoryId, base: Operation },
    #[error("Cycle detected in syntax-category dependencies involving {0}")]
    SyntaxCycle(TheoryId),
    #[error("Failed to interpret nat syntax map for theory {theory}, arrow {arrow}: {source}")]
    NatInterpret {
        theory: TheoryId,
        arrow: Operation,
        #[source]
        source: hexpr::interpret::Error<std::num::ParseIntError>,
    },
    #[error("Failed to interpret syntax map for theory {theory}, arrow {arrow}: {source}")]
    SyntaxInterpret {
        theory: TheoryId,
        arrow: Operation,
        #[source]
        source: hexpr::interpret::Error<SignatureError>,
    },
    #[error("Failed to interpret definition for theory {theory}, arrow {arrow}: {source}")]
    DefinitionInterpret {
        theory: TheoryId,
        arrow: Operation,
        #[source]
        source: hexpr::interpret::Error<SignatureError>,
    },
    #[error("Arrow {arrow} in theory {theory}: source and target maps must have same domain")]
    InvalidTypeMapDomain { theory: TheoryId, arrow: Operation },
}

impl File {
    pub fn from_text(text: &str) -> Result<Self, LoadError> {
        let raw = RawFile::from_text(text)?;
        resolve_raw_file(raw)
    }

    pub fn from_file(path: PathBuf) -> Result<Self, LoadError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_text(&text)
    }
}

fn resolve_raw_file(raw: RawFile) -> Result<File, LoadError> {
    let theory_ids: HashMap<Operation, TheoryId> = raw
        .theories
        .keys()
        .cloned()
        .map(|name| {
            let id = TheoryId::new(name.clone());
            (name, id)
        })
        .collect();

    let syntax_bases = resolve_syntax_bases(&raw, &theory_ids)?;
    let order = topological_order(&syntax_bases)?;
    let mut theories = HashMap::new();

    for theory_id in order {
        let raw_theory = raw.theories.get(&theory_id.0).expect("resolved theory missing");
        let syntax_base = syntax_bases.get(&theory_id).expect("resolved syntax base missing");

        let mut arrows = HashMap::new();
        for raw_arrow in raw_theory.arrows.values() {
            let key = ArrowKey {
                theory: theory_id.clone(),
                name: raw_arrow.name.clone(),
            };
            let type_maps =
                interpret_type_maps(&theory_id, &raw_arrow.name, syntax_base, &raw_arrow.type_maps, &theories)?;
            arrows.insert(
                key.clone(),
                TheoryArrow {
                    key,
                    type_maps,
                    definition: None,
                },
            );
        }

        let mut theory = Theory {
            id: theory_id.clone(),
            syntax_base: syntax_base.clone(),
            arrows,
        };

        for raw_arrow in raw_theory.arrows.values() {
            if let Some(definition) = &raw_arrow.definition {
                let key = theory
                    .get_arrow_key(&raw_arrow.name)
                    .expect("missing local arrow key");
                let body = try_interpret(&theory.local_signature(), definition)
                    .map(|term| forget_labels(term))
                    .map_err(|source| LoadError::DefinitionInterpret {
                        theory: theory_id.clone(),
                        arrow: raw_arrow.name.clone(),
                        source,
                    })?;
                theory.arrows.get_mut(&key).expect("missing local arrow").definition = Some(body);
            }
        }

        theories.insert(theory_id, theory);
    }

    Ok(File { theories })
}

fn resolve_syntax_bases(
    raw: &RawFile,
    theory_ids: &HashMap<Operation, TheoryId>,
) -> Result<HashMap<TheoryId, SyntaxBase>, LoadError> {
    let mut bases = HashMap::new();

    for raw_theory in raw.theories.values() {
        let theory = theory_ids
            .get(&raw_theory.name)
            .expect("theory id missing")
            .clone();
        let syntax_base = if raw_theory.syntax_category.as_str() == "nat" {
            SyntaxBase::Nat
        } else {
            let base = theory_ids.get(&raw_theory.syntax_category).cloned().ok_or_else(|| {
                LoadError::UnknownSyntaxCategory {
                    theory: theory.clone(),
                    base: raw_theory.syntax_category.clone(),
                }
            })?;
            SyntaxBase::Theory(base)
        };
        bases.insert(theory, syntax_base);
    }

    Ok(bases)
}

fn topological_order(
    syntax_bases: &HashMap<TheoryId, SyntaxBase>,
) -> Result<Vec<TheoryId>, LoadError> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Visiting,
        Done,
    }

    fn visit(
        theory: &TheoryId,
        syntax_bases: &HashMap<TheoryId, SyntaxBase>,
        marks: &mut HashMap<TheoryId, Mark>,
        order: &mut Vec<TheoryId>,
    ) -> Result<(), LoadError> {
        match marks.get(theory) {
            Some(Mark::Done) => return Ok(()),
            Some(Mark::Visiting) => return Err(LoadError::SyntaxCycle(theory.clone())),
            None => {}
        }

        marks.insert(theory.clone(), Mark::Visiting);
        if let Some(SyntaxBase::Theory(base)) = syntax_bases.get(theory) {
            visit(base, syntax_bases, marks, order)?;
        }
        marks.insert(theory.clone(), Mark::Done);
        order.push(theory.clone());
        Ok(())
    }

    let mut marks = HashMap::new();
    let mut order = Vec::new();
    for theory in syntax_bases.keys() {
        visit(theory, syntax_bases, &mut marks, &mut order)?;
    }
    Ok(order)
}

fn interpret_type_maps(
    theory: &TheoryId,
    arrow: &Operation,
    syntax_base: &SyntaxBase,
    type_maps: &(hexpr::Hexpr, hexpr::Hexpr),
    theories: &HashMap<TheoryId, Theory>,
) -> Result<(SyntaxTerm, SyntaxTerm), LoadError> {
    match syntax_base {
        SyntaxBase::Nat => {
            let source = try_interpret(&NatObj, &type_maps.0)
                .map(forget_labels)
                .map_err(|source| LoadError::NatInterpret {
                    theory: theory.clone(),
                    arrow: arrow.clone(),
                    source,
                })?;
            let target = try_interpret(&NatObj, &type_maps.1)
                .map(forget_labels)
                .map_err(|source| LoadError::NatInterpret {
                    theory: theory.clone(),
                    arrow: arrow.clone(),
                    source,
                })?;
            if source.source() != target.source() {
                return Err(LoadError::InvalidTypeMapDomain {
                    theory: theory.clone(),
                    arrow: arrow.clone(),
                });
            }
            Ok((SyntaxTerm::Nat(source), SyntaxTerm::Nat(target)))
        }
        SyntaxBase::Theory(base) => {
            let base_theory = theories.get(base).expect("base theory should be resolved first");
            let signature = base_theory.local_signature();
            let source = try_interpret(&signature, &type_maps.0)
                .map(forget_labels)
                .map_err(|source| LoadError::SyntaxInterpret {
                    theory: theory.clone(),
                    arrow: arrow.clone(),
                    source,
                })?;
            let target = try_interpret(&signature, &type_maps.1)
                .map(forget_labels)
                .map_err(|source| LoadError::SyntaxInterpret {
                    theory: theory.clone(),
                    arrow: arrow.clone(),
                    source,
                })?;
            if source.source() != target.source() {
                return Err(LoadError::InvalidTypeMapDomain {
                    theory: theory.clone(),
                    arrow: arrow.clone(),
                });
            }
            Ok((SyntaxTerm::Theory(source), SyntaxTerm::Theory(target)))
        }
    }
}

fn forget_labels<T, A>(f: OpenHypergraph<T, A>) -> OpenHypergraph<(), A> {
    f.map_nodes(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_multiple_theories() -> Result<(), Box<dyn std::error::Error>> {
        let file = File::from_text(
            r#"
            (theory fol.syntax nat {
              (arr wff : 1 -> 1)
              (arr -> : 2 -> 1)
              (arr -. : 1 -> 1)
            })

            (theory fol.proof fol.syntax {
              (arr wn : wff -> (-. wff))
              (arr wi : {wff wff} -> (-> wff))
              (def win : {wff wff} -> (-> -. wff) = (wi wn))
            })
            "#,
        )?;

        let syntax_id = TheoryId("fol.syntax".parse()?);
        let proof_id = TheoryId("fol.proof".parse()?);

        let syntax = file.theories.get(&syntax_id).unwrap();
        let proof = file.theories.get(&proof_id).unwrap();

        assert!(matches!(syntax.syntax_base, SyntaxBase::Nat));
        assert!(matches!(proof.syntax_base, SyntaxBase::Theory(ref id) if *id == syntax_id));
        assert_eq!(syntax.arrows.len(), 3);
        assert_eq!(proof.arrows.len(), 3);
        assert!(proof.arrows.values().any(|arrow| arrow.definition.is_some()));
        Ok(())
    }

    #[test]
    fn rejects_dependency_cycles() -> Result<(), Box<dyn std::error::Error>> {
        let err = File::from_text(
            r#"
            (theory a b {
              (arr f : 1 -> 1)
            })
            (theory b a {
              (arr g : f -> f)
            })
            "#,
        )
        .unwrap_err();

        assert!(matches!(err, LoadError::SyntaxCycle(_)));
        Ok(())
    }

    #[test]
    fn rejects_unknown_syntax_base() -> Result<(), Box<dyn std::error::Error>> {
        let err = File::from_text(
            r#"
            (theory a missing {
              (arr f : 1 -> 1)
            })
            "#,
        )
        .unwrap_err();

        assert!(matches!(err, LoadError::UnknownSyntaxCategory { .. }));
        Ok(())
    }
}
