use crate::check::CheckInput;
use crate::definition::Def;
use crate::definition::inline::inline;
use crate::syntax::{Declaration, TheoryBundle};
use crate::theory::OperationKey;
use hexpr::try_interpret;
use open_hypergraphs::lax::OpenHypergraph;
use open_hypergraphs::strict::vec::FiniteFunction;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("declaration '{0}' has no definition body")]
    MissingDefinitionBody(String),
    #[error("arrow '{0}' not found in arrow theory")]
    MissingArrow(String),
    #[error("definition '{0}' not found")]
    MissingDefinition(String),
    #[error("arrow declaration '{0}' not found")]
    MissingArrowDeclaration(String),
    #[error("invalid quotient before inlining: {0:?}")]
    InvalidQuotientBeforeInlining(FiniteFunction),
    #[error("invalid quotient after inlining: {0:?}")]
    InvalidQuotientAfterInlining(FiniteFunction),
    #[error("invalid quotient in definition '{definition}': {quotient:?}")]
    InvalidDefinitionQuotient {
        definition: String,
        quotient: FiniteFunction,
    },
    #[error("failed to inline definitions; a definition may be missing")]
    InlineFailed,
    #[error("recursive or too-deep definitions remain after inlining: {0}")]
    RecursiveDefinitions(String),
    #[error("hexpr interpret error: {0}")]
    Interpret(#[from] hexpr::interpret::Error<crate::theory::Error>),
}

pub enum DeclarationTermMode {
    Body,
    InlinedBody,
    PrimitiveOrBody,
}

pub fn declaration_check_input(
    bundle: &TheoryBundle,
    declaration: &Declaration,
    mode: DeclarationTermMode,
) -> Result<CheckInput<OperationKey>, Error> {
    let source = forget_labels(try_interpret(
        &bundle.object_theory,
        &declaration.source_map,
    )?);
    let target = forget_labels(try_interpret(
        &bundle.object_theory,
        &declaration.target_map,
    )?);
    let term = declaration_term(bundle, declaration, &source, &target, mode)?;

    Ok(CheckInput {
        source,
        target,
        term,
    })
}

pub fn find_arrow_definition<'a>(
    bundle: &'a TheoryBundle,
    name: &str,
) -> Result<&'a Declaration, Error> {
    bundle
        .definitions
        .values()
        .find(|declaration| declaration.name.as_str() == name)
        .ok_or_else(|| Error::MissingDefinition(name.to_string()))
}

pub fn find_arrow_declaration<'a>(
    bundle: &'a TheoryBundle,
    name: &str,
) -> Result<&'a Declaration, Error> {
    bundle
        .declarations
        .iter()
        .find(|declaration| {
            declaration.name.as_str() == name
                && matches!(declaration.theory.as_str(), "arrow" | "def-arrow")
        })
        .ok_or_else(|| Error::MissingArrowDeclaration(name.to_string()))
}

pub fn declaration_term(
    bundle: &TheoryBundle,
    declaration: &Declaration,
    source: &OpenHypergraph<(), OperationKey>,
    target: &OpenHypergraph<(), OperationKey>,
    mode: DeclarationTermMode,
) -> Result<OpenHypergraph<(), OperationKey>, Error> {
    match (&declaration.definition, mode) {
        (Some(definition), DeclarationTermMode::Body)
        | (Some(definition), DeclarationTermMode::PrimitiveOrBody) => Ok(forget_labels(
            try_interpret(&bundle.arrow_theory, definition)?,
        )),
        (Some(definition), DeclarationTermMode::InlinedBody) => {
            let term = forget_labels(try_interpret(&bundle.arrow_theory, definition)?);
            inline_definitions(bundle, term)
        }
        (None, DeclarationTermMode::PrimitiveOrBody) => {
            let key = bundle
                .arrow_theory
                .get_operation_key(declaration.name.as_str())
                .ok_or_else(|| Error::MissingArrow(declaration.name.to_string()))?;
            Ok(OpenHypergraph::singleton(
                key,
                vec![(); source.targets.len()],
                vec![(); target.targets.len()],
            ))
        }
        (None, DeclarationTermMode::Body) | (None, DeclarationTermMode::InlinedBody) => {
            Err(Error::MissingDefinitionBody(declaration.name.to_string()))
        }
    }
}

pub fn inline_definitions(
    bundle: &TheoryBundle,
    mut term: OpenHypergraph<(), OperationKey>,
) -> Result<OpenHypergraph<(), OperationKey>, Error> {
    let mut definition_keys = HashSet::new();
    let mut env = HashMap::new();

    for declaration in bundle.definitions.values() {
        let key = bundle
            .arrow_theory
            .get_operation_key(declaration.name.as_str())
            .ok_or_else(|| Error::MissingArrow(declaration.name.to_string()))?;
        let definition = declaration.definition.as_ref().unwrap();
        let mut body = forget_labels(try_interpret(&bundle.arrow_theory, definition)?);
        body.quotient()
            .map_err(|quotient| Error::InvalidDefinitionQuotient {
                definition: declaration.name.to_string(),
                quotient,
            })?;

        definition_keys.insert(key.clone());
        env.insert(key, body);
    }

    term.quotient()
        .map_err(Error::InvalidQuotientBeforeInlining)?;

    for _ in 0..=definition_keys.len() {
        if !contains_definition_edge(&term, &definition_keys) {
            return Ok(term);
        }

        let lifted = term.clone().map_edges(|edge| {
            if definition_keys.contains(&edge) {
                Def::Def(edge)
            } else {
                Def::Arr(edge)
            }
        });

        term = inline(env.clone(), lifted).ok_or(Error::InlineFailed)?;
        term.quotient()
            .map_err(Error::InvalidQuotientAfterInlining)?;
    }

    let remaining: Vec<String> = term
        .hypergraph
        .edges
        .iter()
        .filter(|edge| definition_keys.contains(*edge))
        .map(|edge| edge.to_string())
        .collect();
    Err(Error::RecursiveDefinitions(remaining.join(", ")))
}

pub fn forget_labels<T, A>(f: OpenHypergraph<T, A>) -> OpenHypergraph<(), A> {
    f.map_nodes(|_| ())
}

fn contains_definition_edge(
    term: &OpenHypergraph<(), OperationKey>,
    definition_keys: &HashSet<OperationKey>,
) -> bool {
    term.hypergraph
        .edges
        .iter()
        .any(|edge| definition_keys.contains(edge))
}

impl TheoryBundle {
    pub fn declaration_check_input(
        &self,
        declaration: &Declaration,
        mode: DeclarationTermMode,
    ) -> Result<CheckInput<OperationKey>, Error> {
        declaration_check_input(self, declaration, mode)
    }
}
