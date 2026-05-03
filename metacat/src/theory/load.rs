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

use super::ast::{MergeRawError, ParseRawError, RawTheorySet};
use super::model::{SignatureError, Term, Theory, TheoryArrow, TheoryId, TheorySet};
use super::nat::{NAT_THEORY_NAME, NatKey, NatObj};
use hexpr::{Hexpr, Operation, Signature, try_interpret};
use open_hypergraphs::category::Arrow;
use open_hypergraphs::lax::OpenHypergraph;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error(transparent)]
    ParseRaw(#[from] ParseRawError),
    #[error(transparent)]
    MergeRaw(#[from] MergeRawError),
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

impl TheorySet {
    pub fn from_text(text: &str) -> Result<Self, LoadError> {
        let raw = RawTheorySet::from_text(text)?;
        resolve_raw_theory_set(raw)
    }

    pub fn from_file(path: PathBuf) -> Result<Self, LoadError> {
        let raw = RawTheorySet::from_file(path)?;
        resolve_raw_theory_set(raw)
    }

    /// Parse and merge multiple source strings before resolving theory references.
    ///
    /// This allows theories in one source string to refer to theories declared
    /// in another, as long as the merged raw theory set has no conflicts.
    pub fn from_texts<'a, I>(texts: I) -> Result<Self, LoadError>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let raw = merge_raw_sets(texts.into_iter().map(RawTheorySet::from_text))?;
        resolve_raw_theory_set(raw)
    }

    /// Parse and merge multiple files before resolving theory references.
    ///
    /// This allows theories in one file to refer to theories declared in
    /// another, as long as the merged raw theory set has no conflicts.
    pub fn from_files<I>(paths: I) -> Result<Self, LoadError>
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let raw = merge_raw_sets(paths.into_iter().map(RawTheorySet::from_file))?;
        resolve_raw_theory_set(raw)
    }
}

fn merge_raw_sets<I>(sets: I) -> Result<RawTheorySet, LoadError>
where
    I: IntoIterator<Item = Result<RawTheorySet, ParseRawError>>,
{
    let mut merged = RawTheorySet {
        theories: BTreeMap::new(),
    };
    for set in sets {
        merged = merged.merge(set?)?;
    }
    Ok(merged)
}

fn resolve_raw_theory_set(raw: RawTheorySet) -> Result<TheorySet, LoadError> {
    let nat_id = builtin_nat_theory_id();
    // Reserve an id for the builtin `nat` theory alongside user-defined names so
    // later resolution can treat syntax references uniformly.
    let mut theory_ids: HashMap<Operation, TheoryId> = raw
        .theories
        .keys()
        .cloned()
        .map(|name| {
            let id = TheoryId::new(name.clone());
            (name, id)
        })
        .collect();
    theory_ids.insert(nat_id.0.clone(), nat_id.clone());

    let syntax_bases = resolve_syntax_bases(&raw, &theory_ids)?;
    let order = topological_order(&syntax_bases)?;
    let mut theories = BTreeMap::new();

    for theory_id in order {
        // Each theory is loaded only after its syntax theory has already been
        // inserted into `theories`, so user syntax maps can be interpreted
        // against the previously resolved base.
        let syntax = syntax_bases
            .get(&theory_id)
            .expect("resolved syntax base missing")
            .clone();

        let Some(raw_theory) = raw.theories.get(&theory_id.0) else {
            // The builtin `nat` theory is injected into the environment but has
            // no raw source declaration and no finite arrow table.
            theories.insert(theory_id.clone(), Theory::Nat);
            continue;
        };

        let mut arrows = BTreeMap::new();
        for raw_arrow in raw_theory.arrows.values() {
            let type_maps = interpret_type_maps(
                &theory_id,
                &raw_arrow.name,
                &syntax,
                &raw_arrow.type_maps,
                &theories,
            )?;
            arrows.insert(
                raw_arrow.name.clone(),
                TheoryArrow {
                    raw: raw_arrow.clone(),
                    name: raw_arrow.name.clone(),
                    type_maps,
                    definition: None,
                },
            );
        }

        let mut theory = Theory::Theory {
            syntax: syntax.clone(),
            arrows,
        };

        for raw_arrow in raw_theory.arrows.values() {
            if let Some(definition) = &raw_arrow.definition {
                let body = try_interpret(&theory.local_signature(), definition)
                    .map(|term| forget_labels(term))
                    .map_err(|source| LoadError::DefinitionInterpret {
                        theory: theory_id.clone(),
                        arrow: raw_arrow.name.clone(),
                        source,
                    })?;
                theory
                    .arrows_mut()
                    .expect("user theory should have arrows")
                    .get_mut(&raw_arrow.name)
                    .expect("missing local arrow")
                    .definition = Some(body);
            }
        }

        theories.insert(theory_id, theory);
    }

    Ok(TheorySet { theories })
}

fn resolve_syntax_bases(
    raw: &RawTheorySet,
    theory_ids: &HashMap<Operation, TheoryId>,
) -> Result<HashMap<TheoryId, TheoryId>, LoadError> {
    let nat_id = builtin_nat_theory_id();
    let mut bases = HashMap::new();
    // `nat` is the unique root builtin. Giving it itself as a base keeps the
    // dependency graph total while still letting the topo walk stop at the root.
    // This is not merely ad hoc: categorically, the builtin `nat` theory is
    // also the syntax category in which its own arrow profiles live.
    bases.insert(nat_id.clone(), nat_id.clone());

    for raw_theory in raw.theories.values() {
        let theory = theory_ids
            .get(&raw_theory.name)
            .expect("theory id missing")
            .clone();
        // At this stage we only resolve names; interpretation of the actual
        // type maps is deferred until the base theory has been loaded.
        let syntax_base = theory_ids
            .get(&raw_theory.syntax_category)
            .cloned()
            .ok_or_else(|| LoadError::UnknownSyntaxCategory {
                theory: theory.clone(),
                base: raw_theory.syntax_category.clone(),
            })?;
        bases.insert(theory, syntax_base);
    }

    Ok(bases)
}

fn topological_order(
    syntax_bases: &HashMap<TheoryId, TheoryId>,
) -> Result<Vec<TheoryId>, LoadError> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Visiting,
        Done,
    }

    fn visit(
        theory: &TheoryId,
        syntax_bases: &HashMap<TheoryId, TheoryId>,
        marks: &mut HashMap<TheoryId, Mark>,
        order: &mut Vec<TheoryId>,
    ) -> Result<(), LoadError> {
        match marks.get(theory) {
            Some(Mark::Done) => return Ok(()),
            Some(Mark::Visiting) => return Err(LoadError::SyntaxCycle(theory.clone())),
            None => {}
        }

        marks.insert(theory.clone(), Mark::Visiting);
        if let Some(base) = syntax_bases.get(theory) {
            // The builtin `nat` root is represented by the self-edge `nat -> nat`.
            // We do not recurse across that edge.
            if base != theory {
                visit(base, syntax_bases, marks, order)?;
            }
        }
        marks.insert(theory.clone(), Mark::Done);
        // Post-order insertion ensures every syntax dependency appears before
        // the theories that depend on it.
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
    syntax: &TheoryId,
    type_maps: &(hexpr::Hexpr, hexpr::Hexpr),
    theories: &BTreeMap<TheoryId, Theory>,
) -> Result<(Term, Term), LoadError> {
    if is_builtin_nat(syntax) {
        // normalize nat syntax so that "2" means "{1 1}".
        // More concretely, each numeral `n` is taken to be a definition standing for `{1 ..n.. 1}`
        // see nat.rs for more details.
        let normalized = (
            normalize_nat_hexpr(&type_maps.0),
            normalize_nat_hexpr(&type_maps.1),
        );
        interpret_type_maps_with(
            &NatObj,
            &normalized,
            nat_key_to_operation,
            |source| LoadError::NatInterpret {
                theory: theory.clone(),
                arrow: arrow.clone(),
                source,
            },
            || LoadError::InvalidTypeMapDomain {
                theory: theory.clone(),
                arrow: arrow.clone(),
            },
        )
    } else {
        let base_theory = theories
            .get(syntax)
            .expect("base theory should be resolved first");
        let signature = base_theory.local_signature();
        interpret_type_maps_with(
            &signature,
            type_maps,
            std::convert::identity,
            |source| LoadError::SyntaxInterpret {
                theory: theory.clone(),
                arrow: arrow.clone(),
                source,
            },
            || LoadError::InvalidTypeMapDomain {
                theory: theory.clone(),
                arrow: arrow.clone(),
            },
        )
    }
}

fn normalize_nat_hexpr(hexpr: &Hexpr) -> Hexpr {
    match hexpr {
        Hexpr::Composition(parts) => {
            Hexpr::Composition(parts.iter().map(normalize_nat_hexpr).collect())
        }
        Hexpr::Tensor(parts) => Hexpr::Tensor(
            parts
                .iter()
                .flat_map(|part| match normalize_nat_hexpr(part) {
                    Hexpr::Tensor(inner) => inner,
                    other => vec![other],
                })
                .collect(),
        ),
        Hexpr::Frobenius { sources, targets } => Hexpr::Frobenius {
            sources: sources.clone(),
            targets: targets.clone(),
        },
        Hexpr::Operation(op) => normalize_nat_operation(op),
    }
}

fn normalize_nat_operation(op: &Operation) -> Hexpr {
    match op.as_str().parse::<usize>() {
        Ok(0) => Hexpr::Tensor(vec![]),
        Ok(n) => Hexpr::Tensor((0..n).map(|_| nat_one_hexpr()).collect()),
        Err(_) => Hexpr::Operation(op.clone()),
    }
}

fn nat_one_hexpr() -> Hexpr {
    Hexpr::Operation(
        "1".parse()
            .expect("builtin nat numeral should parse as operation"),
    )
}

fn interpret_type_maps_with<S, F, E, I>(
    signature: &S,
    type_maps: &(hexpr::Hexpr, hexpr::Hexpr),
    map_edge: F,
    map_error: E,
    invalid_domain: I,
) -> Result<(Term, Term), LoadError>
where
    S: Signature<Obj = ()>,
    F: Fn(S::Arr) -> Operation + Copy,
    E: Fn(hexpr::interpret::Error<S::Error>) -> LoadError + Copy,
    I: Fn() -> LoadError + Copy,
{
    let source = try_interpret(signature, &type_maps.0)
        .map(forget_labels)
        .map(|term| term.map_edges(map_edge))
        .map_err(map_error)?;
    let target = try_interpret(signature, &type_maps.1)
        .map(forget_labels)
        .map(|term| term.map_edges(map_edge))
        .map_err(map_error)?;
    if source.source() != target.source() {
        return Err(invalid_domain());
    }
    Ok((source, target))
}

fn forget_labels<T, A>(f: OpenHypergraph<T, A>) -> OpenHypergraph<(), A> {
    f.map_nodes(|_| ())
}

fn builtin_nat_theory_id() -> TheoryId {
    TheoryId(
        NAT_THEORY_NAME
            .parse()
            .expect("builtin nat theory name should parse"),
    )
}

fn is_builtin_nat(theory: &TheoryId) -> bool {
    theory.0.as_str() == NAT_THEORY_NAME
}

fn nat_key_to_operation(key: NatKey) -> Operation {
    key.0
        .to_string()
        .parse()
        .expect("decimal numeral should parse as hexpr operation")
}

impl Theory {
    fn arrows_mut(&mut self) -> Option<&mut BTreeMap<Operation, TheoryArrow>> {
        match self {
            Theory::Nat => None,
            Theory::Theory { arrows, .. } => Some(arrows),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_multiple_theories() -> Result<(), Box<dyn std::error::Error>> {
        let file = TheorySet::from_text(
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

        let nat_id = TheoryId("nat".parse()?);
        let syntax_id = TheoryId("fol.syntax".parse()?);
        let proof_id = TheoryId("fol.proof".parse()?);

        let nat = file.theories.get(&nat_id).unwrap();
        let syntax = file.theories.get(&syntax_id).unwrap();
        let proof = file.theories.get(&proof_id).unwrap();

        assert!(matches!(nat, Theory::Nat));
        assert!(
            matches!(syntax, Theory::Theory { syntax: id, arrows } if *id == nat_id && arrows.len() == 3)
        );
        assert!(
            matches!(proof, Theory::Theory { syntax: id, arrows } if *id == syntax_id && arrows.len() == 3)
        );
        assert!(
            matches!(proof, Theory::Theory { arrows, .. } if arrows.values().any(|arrow| arrow.definition.is_some()))
        );
        Ok(())
    }

    #[test]
    fn loads_from_multiple_texts() -> Result<(), Box<dyn std::error::Error>> {
        let file = TheorySet::from_texts([
            r#"
            (theory fol.syntax nat {
              (arr wff : 1 -> 1)
              (arr -> : 2 -> 1)
            })
            "#,
            r#"
            (theory fol.proof fol.syntax {
              (arr wi : {wff wff} -> (-> wff))
            })
            "#,
        ])?;

        let syntax_id = TheoryId("fol.syntax".parse()?);
        let proof_id = TheoryId("fol.proof".parse()?);
        assert!(matches!(
            file.theories.get(&proof_id),
            Some(Theory::Theory { syntax, arrows }) if *syntax == syntax_id && arrows.len() == 1
        ));
        Ok(())
    }

    #[test]
    fn expands_nat_numerals_to_tensors_of_one() -> Result<(), Box<dyn std::error::Error>> {
        let file = TheorySet::from_text(
            r#"
            (theory smol.syntax nat {
              (arr -> : 2 -> 1)
            })
            "#,
        )?;

        let theory_id = TheoryId("smol.syntax".parse()?);
        let Theory::Theory { arrows, .. } = file.theories.get(&theory_id).unwrap() else {
            panic!("expected user theory");
        };
        let arrow = arrows.get(&"->".parse()?).unwrap();

        assert_eq!(
            arrow.type_maps.0.hypergraph.edges,
            vec!["1".parse()?, "1".parse()?]
        );
        assert_eq!(arrow.type_maps.1.hypergraph.edges, vec!["1".parse()?]);

        Ok(())
    }

    #[test]
    fn rejects_dependency_cycles() -> Result<(), Box<dyn std::error::Error>> {
        let err = TheorySet::from_text(
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
        let err = TheorySet::from_text(
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
