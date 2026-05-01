use hexpr::Operation;
use hexpr::try_interpret;
use metacat::check::CheckInput;
use metacat::definition::Def;
use metacat::definition::inline::inline;
use metacat::syntax::{Declaration, TheoryBundle};
use metacat::theory::OperationKey;
use open_hypergraphs::lax::OpenHypergraph;
use std::collections::{HashMap, HashSet};

pub fn find_definition<'a>(
    bundle: &'a TheoryBundle,
    name: &str,
) -> anyhow::Result<&'a Declaration> {
    let operation: Operation = name.parse()?;
    bundle
        .definitions
        .get(&operation)
        .ok_or_else(|| anyhow::anyhow!("definition '{}' not found", name))
}

pub fn find_arrow_declaration<'a>(
    bundle: &'a TheoryBundle,
    name: &str,
) -> anyhow::Result<&'a Declaration> {
    bundle
        .declarations
        .iter()
        .find(|declaration| {
            declaration.name.as_str() == name
                && matches!(declaration.theory.as_str(), "arrow" | "def-arrow")
        })
        .ok_or_else(|| anyhow::anyhow!("arrow '{}' not found", name))
}

pub fn forget_labels<T, A>(
    f: open_hypergraphs::lax::OpenHypergraph<T, A>,
) -> open_hypergraphs::lax::OpenHypergraph<(), A> {
    f.map_nodes(|_| ())
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
) -> anyhow::Result<CheckInput<OperationKey>> {
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

pub fn declaration_term(
    bundle: &TheoryBundle,
    declaration: &Declaration,
    source: &OpenHypergraph<(), OperationKey>,
    target: &OpenHypergraph<(), OperationKey>,
    mode: DeclarationTermMode,
) -> anyhow::Result<OpenHypergraph<(), OperationKey>> {
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
                .ok_or_else(|| {
                    anyhow::anyhow!("arrow '{}' not found in arrow theory", declaration.name)
                })?;
            Ok(OpenHypergraph::singleton(
                key,
                vec![(); source.targets.len()],
                vec![(); target.targets.len()],
            ))
        }
        (None, DeclarationTermMode::Body) | (None, DeclarationTermMode::InlinedBody) => Err(
            anyhow::anyhow!("declaration '{}' has no definition body", declaration.name),
        ),
    }
}

pub fn inline_definitions(
    bundle: &TheoryBundle,
    mut term: OpenHypergraph<(), OperationKey>,
) -> anyhow::Result<OpenHypergraph<(), OperationKey>> {
    let mut definition_keys = HashSet::new();
    let mut env = HashMap::new();

    for declaration in bundle.definitions.values() {
        let key = bundle
            .arrow_theory
            .get_operation_key(declaration.name.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "definition '{}' not found in arrow theory",
                    declaration.name
                )
            })?;
        let definition = declaration.definition.as_ref().unwrap();
        let mut body = forget_labels(try_interpret(&bundle.arrow_theory, definition)?);
        body.quotient().map_err(|quotient| {
            anyhow::anyhow!(
                "invalid quotient in definition '{}': {:?}",
                declaration.name,
                quotient
            )
        })?;

        definition_keys.insert(key.clone());
        env.insert(key, body);
    }

    term.quotient()
        .map_err(|quotient| anyhow::anyhow!("invalid quotient before inlining: {:?}", quotient))?;

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

        term = inline(env.clone(), lifted).ok_or_else(|| {
            anyhow::anyhow!("failed to inline definitions; a definition may be missing")
        })?;
        term.quotient().map_err(|quotient| {
            anyhow::anyhow!("invalid quotient after inlining: {:?}", quotient)
        })?;
    }

    let remaining: Vec<String> = term
        .hypergraph
        .edges
        .iter()
        .filter(|edge| definition_keys.contains(*edge))
        .map(|edge| edge.to_string())
        .collect();
    Err(anyhow::anyhow!(
        "recursive or too-deep definitions remain after inlining: {}",
        remaining.join(", ")
    ))
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
