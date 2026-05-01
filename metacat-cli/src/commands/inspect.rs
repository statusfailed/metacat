use crate::render::{
    print_dot_hypergraph, print_dot_hypergraph_with_edge_labels, print_dot_raw_type_map,
    print_open_hypergraph, print_state,
};
use clap::{Args, Subcommand, ValueEnum};
use hexpr::try_interpret;
use metacat::build::{
    DeclarationTermMode, declaration_check_input, find_arrow_declaration, find_definition,
    forget_labels,
};
use metacat::check::{CheckInput, RawTypeTerm, eval_type, prepare_check};
use metacat::dual;
use metacat::dual::Dual;
use metacat::syntax::{Declaration, TheoryBundle};
use metacat::theory::OperationKey;
use open_hypergraphs::lax::OpenHypergraph;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Args)]
pub struct InspectCommand {
    #[command(subcommand)]
    target: InspectTarget,
}

#[derive(Subcommand)]
enum InspectTarget {
    Declarations {
        #[arg()]
        path: PathBuf,
    },
    Arrow {
        #[arg()]
        path: PathBuf,
        #[arg()]
        name: String,
        #[arg(long, value_enum)]
        stage: Option<InspectArrowStage>,
        #[arg(long, value_enum, default_value_t = InspectFormat::Text)]
        format: InspectFormat,
    },
    Check {
        #[arg()]
        path: PathBuf,
        #[arg()]
        name: String,
        #[arg(long)]
        trace: bool,
    },
}

#[derive(ValueEnum, Clone, Debug)]
enum InspectArrowStage {
    Term,
    Source,
    Target,
    RawTypeMap,
    TypeTerm,
    ProofTypeMap,
    Ssa,
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
enum InspectFormat {
    Text,
    Dot,
    Formula,
}

impl InspectCommand {
    pub fn run(self) -> anyhow::Result<()> {
        match self.target {
            InspectTarget::Declarations { path } => inspect_declarations(path),
            InspectTarget::Arrow {
                path,
                name,
                stage,
                format,
            } => inspect_arrow(path, name, stage, format),
            InspectTarget::Check { path, name, trace } => inspect_check(path, name, trace),
        }
    }
}

fn inspect_declarations(path: PathBuf) -> anyhow::Result<()> {
    let bundle = TheoryBundle::from_file(path)?;

    print_declaration_group("objects", &bundle.declarations, "object");
    print_declaration_group("arrows", &bundle.declarations, "arrow");
    print_declaration_group("definitions", &bundle.declarations, "def-arrow");

    Ok(())
}

fn print_declaration_group(title: &str, declarations: &[Declaration], theory: &str) {
    println!("{title}:");
    let mut decls: Vec<&Declaration> = declarations
        .iter()
        .filter(|decl| decl.theory.as_str() == theory)
        .collect();
    decls.sort_by_key(|decl| decl.name.as_str().to_string());

    if decls.is_empty() {
        println!("  <none>");
    }

    for decl in decls {
        println!(
            "  {} : {} -> {}",
            decl.name, decl.source_map, decl.target_map
        );
        if let Some(definition) = &decl.definition {
            println!("    body: {definition}");
        }
    }
}

fn inspect_arrow(
    path: PathBuf,
    name: String,
    stage: Option<InspectArrowStage>,
    format: InspectFormat,
) -> anyhow::Result<()> {
    let bundle = TheoryBundle::from_file(path)?;
    let Some(stage) = stage else {
        let declaration = find_arrow_declaration(&bundle, &name)?;
        return inspect_arrow_build_match(&bundle, declaration, format);
    };

    let declaration = find_definition(&bundle, &name)?;
    let def_hexpr = declaration.definition.as_ref().unwrap();
    let mut term = forget_labels(try_interpret(&bundle.arrow_theory, def_hexpr)?);

    if format == InspectFormat::Text {
        println!(
            "{} : {} -> {}",
            declaration.name, declaration.source_map, declaration.target_map
        );
        println!("body: {def_hexpr}");
    }

    match stage {
        InspectArrowStage::Source | InspectArrowStage::Target => {
            let (stage_name, map) = match stage {
                InspectArrowStage::Source => ("source", &declaration.source_map),
                InspectArrowStage::Target => ("target", &declaration.target_map),
                _ => unreachable!(),
            };
            let graph = forget_labels(try_interpret(&bundle.object_theory, map)?);

            match format {
                InspectFormat::Text => {
                    println!();
                    println!("{stage_name}:");
                    print_open_hypergraph(&graph);
                }
                InspectFormat::Dot => {
                    let labels = object_map_node_labels(&graph, &bundle);
                    print_dot_hypergraph(&graph, labels.as_deref());
                }
                InspectFormat::Formula => {
                    return Err(anyhow::anyhow!(
                        "--format formula is not available for --stage source or --stage target"
                    ));
                }
            }
        }
        InspectArrowStage::Term => match format {
            InspectFormat::Text => {
                term.quotient()
                    .map_err(|quotient| anyhow::anyhow!("invalid term quotient: {:?}", quotient))?;
                println!();
                println!("term:");
                print_open_hypergraph(&term);
            }
            InspectFormat::Dot => {
                term.quotient()
                    .map_err(|quotient| anyhow::anyhow!("invalid term quotient: {:?}", quotient))?;
                let labels = term_node_labels(&bundle, declaration, &term);
                print_dot_hypergraph(&term, labels.as_deref());
            }
            InspectFormat::Formula => {
                return Err(anyhow::anyhow!(
                    "--format formula is only available for --stage type-term and --stage proof-type-map"
                ));
            }
        },
        InspectArrowStage::RawTypeMap => {
            let coarity =
                |op: &OperationKey| -> usize { bundle.object_theory.type_maps(op).1.targets.len() };
            let input = declaration_check_input(&bundle, declaration, DeclarationTermMode::Body)?;
            let prepared = prepare_check(&bundle.arrow_theory, input)?;

            match format {
                InspectFormat::Text => {
                    println!();
                    println!("raw-type-map:");
                    println!(
                        "  proof node range before quotient: {}..{}",
                        prepared
                            .raw_type_term
                            .proof_node_range_before_quotient
                            .start,
                        prepared.raw_type_term.proof_node_range_before_quotient.end
                    );
                    print_open_hypergraph(&prepared.raw_type_term.graph);
                }
                InspectFormat::Dot => {
                    let labels = raw_type_map_node_labels(&prepared.raw_type_term.graph, &coarity);
                    print_dot_raw_type_map(&prepared.raw_type_term, labels.as_deref());
                }
                InspectFormat::Formula => {
                    return Err(anyhow::anyhow!(
                        "--format formula is only available for --stage type-term and --stage proof-type-map"
                    ));
                }
            }
        }
        InspectArrowStage::ProofTypeMap => {
            let input =
                declaration_check_input(&bundle, declaration, DeclarationTermMode::InlinedBody)?;
            term = input.term.clone();
            let prepared = prepare_check(&bundle.arrow_theory, input)?;

            let coarity =
                |op: &OperationKey| -> usize { bundle.object_theory.type_maps(op).1.targets.len() };

            match format {
                InspectFormat::Text => {
                    println!();
                    println!("proof-type-map:");
                    print_open_hypergraph(&prepared.proof_type_map);
                }
                InspectFormat::Dot => {
                    let edge_labels = proof_type_map_edge_labels(&bundle, &term);
                    let labels = type_map_node_labels(&prepared.proof_type_map, &coarity)
                        .or_else(|| term_node_labels(&bundle, declaration, &term));
                    print_dot_hypergraph_with_edge_labels(
                        &prepared.proof_type_map,
                        labels.as_deref(),
                        &edge_labels,
                    );
                }
                InspectFormat::Formula => {
                    println!("{}", proof_type_map_formula(&bundle, &term));
                }
            }
        }
        InspectArrowStage::TypeTerm => {
            let input =
                declaration_check_input(&bundle, declaration, DeclarationTermMode::InlinedBody)?;
            term = input.term.clone();
            let prepared = prepare_check(&bundle.arrow_theory, input)?;

            let coarity =
                |op: &OperationKey| -> usize { bundle.object_theory.type_maps(op).1.targets.len() };

            match format {
                InspectFormat::Text => {
                    println!();
                    println!("type-term:");
                    println!("  quotient: {:?}", prepared.quotient);
                    println!(
                        "  proof node type indices: {:?}",
                        prepared.node_type_indices
                    );
                    print_open_hypergraph(&prepared.type_term);
                }
                InspectFormat::Dot => {
                    let edge_labels =
                        type_term_edge_labels(&bundle, &term, &prepared.raw_type_term);
                    let labels = type_map_node_labels(&prepared.type_term, &coarity);
                    print_dot_hypergraph_with_edge_labels(
                        &prepared.type_term,
                        labels.as_deref(),
                        &edge_labels,
                    );
                }
                InspectFormat::Formula => {
                    println!("{}", type_term_formula(&bundle, declaration, &term));
                }
            }
        }
        InspectArrowStage::Ssa => {
            if format != InspectFormat::Text {
                return Err(anyhow::anyhow!("--stage ssa only supports --format text"));
            }

            let input = declaration_check_input(&bundle, declaration, DeclarationTermMode::Body)?;
            let prepared = prepare_check(&bundle.arrow_theory, input)?;

            println!();
            println!("ssa:");
            for value in metacat::ssa::ssa(prepared.type_term.to_strict())? {
                println!("  {value}");
            }
        }
    }

    Ok(())
}

fn inspect_arrow_build_match(
    bundle: &TheoryBundle,
    declaration: &Declaration,
    format: InspectFormat,
) -> anyhow::Result<()> {
    if format == InspectFormat::Dot {
        return Err(anyhow::anyhow!(
            "--format dot is not available for the declaration-level arrow inspector"
        ));
    }
    if format == InspectFormat::Formula {
        return Err(anyhow::anyhow!(
            "--format formula is not available for the declaration-level arrow inspector"
        ));
    }

    let source = forget_labels(try_interpret(
        &bundle.object_theory,
        &declaration.source_map,
    )?);
    let target = forget_labels(try_interpret(
        &bundle.object_theory,
        &declaration.target_map,
    )?);
    let source_match = dual::into_rev(source);
    let target_build = dual::into_fwd(target);

    println!(
        "{} {} : {} -> {}",
        declaration.theory, declaration.name, declaration.source_map, declaration.target_map
    );
    println!();
    println!("match:");
    println!("  input pattern: {}", declaration.source_map);
    print_build_match_operations("  operations", &source_match);
    println!();
    println!("build:");
    println!("  output pattern: {}", declaration.target_map);
    print_build_match_operations("  operations", &target_build);
    println!();
    println!("implementation:");
    match &declaration.definition {
        Some(definition) => println!("  body: {definition}"),
        None => println!("  primitive"),
    }

    Ok(())
}

fn print_build_match_operations(label: &str, graph: &OpenHypergraph<(), Dual<OperationKey>>) {
    if graph.hypergraph.edges.is_empty() {
        println!("{label}: none; variables are passed through");
        return;
    }

    println!("{label}:");
    for (i, edge) in graph.hypergraph.edges.iter().enumerate() {
        let adjacency = &graph.hypergraph.adjacency[i];
        println!(
            "    {edge} : {} -> {}",
            adjacency.sources.len(),
            adjacency.targets.len()
        );
    }
}

fn term_node_labels(
    bundle: &TheoryBundle,
    declaration: &Declaration,
    term: &OpenHypergraph<(), OperationKey>,
) -> Option<Vec<String>> {
    let coarity =
        |op: &OperationKey| -> usize { bundle.object_theory.type_maps(op).1.targets.len() };
    let mut quotient_term = term.clone();
    let quotient = quotient_term.quotient().ok()?;
    let source = forget_labels(try_interpret(&bundle.object_theory, &declaration.source_map).ok()?);
    let target = forget_labels(try_interpret(&bundle.object_theory, &declaration.target_map).ok()?);
    let prepared = prepare_check(
        &bundle.arrow_theory,
        CheckInput {
            source,
            target,
            term: quotient_term,
        },
    )
    .ok()?;
    let (result, _) = eval_type(prepared.type_term.clone()).ok()?;
    let types: Vec<_> = prepared
        .node_type_indices
        .iter()
        .map(|i| result[*i].clone())
        .collect();

    Some(
        quotient
            .table
            .iter()
            .map(|node| {
                types
                    .get(*node)
                    .map(|tree| tree.pretty(Some(&coarity)))
                    .unwrap_or_default()
            })
            .collect(),
    )
}

fn type_map_node_labels(
    type_map: &OpenHypergraph<(), Dual<OperationKey>>,
    coarity: &dyn Fn(&OperationKey) -> usize,
) -> Option<Vec<String>> {
    eval_type(type_map.clone()).ok().map(|(trees, _)| {
        trees
            .iter()
            .map(|tree| tree.pretty(Some(&coarity)))
            .collect()
    })
}

fn raw_type_map_node_labels(
    raw_type_map: &OpenHypergraph<(), Dual<OperationKey>>,
    coarity: &dyn Fn(&OperationKey) -> usize,
) -> Option<Vec<String>> {
    let mut quotient_graph = raw_type_map.clone();
    let quotient = quotient_graph.quotient().ok()?;
    let quotient_labels = type_map_node_labels(&quotient_graph, coarity)?;
    Some(
        quotient
            .table
            .iter()
            .map(|node| quotient_labels.get(*node).cloned().unwrap_or_default())
            .collect(),
    )
}

fn object_map_node_labels(
    graph: &OpenHypergraph<(), OperationKey>,
    bundle: &TheoryBundle,
) -> Option<Vec<String>> {
    let coarity =
        |op: &OperationKey| -> usize { bundle.object_theory.type_maps(op).1.targets.len() };
    let mut quotient_graph = graph.clone();
    let quotient = quotient_graph.quotient().ok()?;
    let (quotient_trees, _) = eval_type(dual::into_fwd(quotient_graph)).ok()?;
    let quotient_labels: Vec<String> = quotient_trees
        .iter()
        .map(|tree| tree.pretty(Some(&coarity)))
        .collect();

    Some(
        quotient
            .table
            .iter()
            .map(|node| quotient_labels.get(*node).cloned().unwrap_or_default())
            .collect(),
    )
}

fn proof_type_map_edge_labels(
    bundle: &TheoryBundle,
    term: &OpenHypergraph<(), OperationKey>,
) -> Vec<String> {
    let mut labels = Vec::new();
    for op in &term.hypergraph.edges {
        let (source_type, target_type) = bundle.arrow_theory.type_maps(op);
        let match_edges = source_type.hypergraph.edges.iter();
        let build_edges = target_type.hypergraph.edges.iter();
        let match_count = match_edges.len();
        let build_count = build_edges.len();

        for (i, edge) in match_edges.enumerate() {
            labels.push(format!(
                "{op} match {}/{}: {}",
                i + 1,
                match_count,
                Dual::Rev(edge.clone())
            ));
        }

        for (i, edge) in build_edges.enumerate() {
            labels.push(format!(
                "{op} build {}/{}: {}",
                i + 1,
                build_count,
                Dual::Fwd(edge.clone())
            ));
        }
    }

    labels
}

fn type_term_edge_labels(
    bundle: &TheoryBundle,
    term: &OpenHypergraph<(), OperationKey>,
    raw: &RawTypeTerm<OperationKey>,
) -> Vec<String> {
    let mut labels: Vec<String> = raw
        .graph
        .hypergraph
        .edges
        .iter()
        .map(|edge| edge.to_string())
        .collect();

    let source_count = raw.source.edge_range.len();
    for (offset, edge_index) in raw.source.edge_range.clone().enumerate() {
        labels[edge_index] = format!(
            "source+ {}/{}: {}",
            offset + 1,
            source_count,
            raw.graph.hypergraph.edges[edge_index]
        );
    }

    let target_count = raw.target.edge_range.len();
    for (offset, edge_index) in raw.target.edge_range.clone().enumerate() {
        labels[edge_index] = format!(
            "target- {}/{}: {}",
            offset + 1,
            target_count,
            raw.graph.hypergraph.edges[edge_index]
        );
    }

    let mut edge_index = raw.proof.edge_range.start;
    for op in &term.hypergraph.edges {
        let (source_type, target_type) = bundle.arrow_theory.type_maps(op);
        let match_count = source_type.hypergraph.edges.len();
        let build_count = target_type.hypergraph.edges.len();

        for i in 0..match_count {
            labels[edge_index] = format!(
                "{op} match {}/{}: {}",
                i + 1,
                match_count,
                raw.graph.hypergraph.edges[edge_index]
            );
            edge_index += 1;
        }

        for i in 0..build_count {
            labels[edge_index] = format!(
                "{op} build {}/{}: {}",
                i + 1,
                build_count,
                raw.graph.hypergraph.edges[edge_index]
            );
            edge_index += 1;
        }
    }

    labels
}

fn type_term_formula(
    bundle: &TheoryBundle,
    declaration: &Declaration,
    term: &OpenHypergraph<(), OperationKey>,
) -> String {
    let mut parts = vec![format!("source+({})", declaration.source_map)];
    parts.extend(proof_type_map_formula_parts(bundle, term));
    parts.push(format!("target-({})", declaration.target_map));
    parts.join(" ; ")
}

fn proof_type_map_formula(
    bundle: &TheoryBundle,
    term: &OpenHypergraph<(), OperationKey>,
) -> String {
    proof_type_map_formula_parts(bundle, term).join(" ; ")
}

fn proof_type_map_formula_parts(
    bundle: &TheoryBundle,
    term: &OpenHypergraph<(), OperationKey>,
) -> Vec<String> {
    let mut parts = Vec::new();
    for op in &term.hypergraph.edges {
        let (source_type, target_type) = bundle.arrow_theory.type_maps(op);
        if !source_type.hypergraph.edges.is_empty() {
            parts.push(format!("{op}-"));
        }
        if !target_type.hypergraph.edges.is_empty() {
            parts.push(format!("{op}+"));
        }
    }
    parts
}

fn inspect_check(path: PathBuf, name: String, trace: bool) -> anyhow::Result<()> {
    if !trace {
        return Err(anyhow::anyhow!("inspect check currently requires --trace"));
    }

    let bundle = TheoryBundle::from_file(path)?;
    let declaration = find_arrow_declaration(&bundle, &name)?;
    let input =
        declaration_check_input(&bundle, declaration, DeclarationTermMode::PrimitiveOrBody)?;

    let coarity =
        |op: &OperationKey| -> usize { bundle.object_theory.type_maps(op).1.targets.len() };

    println!(
        "{} : {} -> {}",
        declaration.name, declaration.source_map, declaration.target_map
    );
    match &declaration.definition {
        Some(definition) => println!("body: {definition}"),
        None => println!("body: <primitive>"),
    }
    println!();
    println!("term:");
    print_open_hypergraph(&input.term);

    println!();
    println!("type-term:");
    let prepared = match prepare_check(&bundle.arrow_theory, input) {
        Ok(prepared) => prepared,
        Err(error) => {
            println!("  failed while building type-term: {error}");
            return Err(error.into());
        }
    };
    println!("  quotient: {:?}", prepared.quotient);
    println!(
        "  proof node type indices: {:?}",
        prepared.node_type_indices
    );
    print_open_hypergraph(&prepared.type_term);

    println!();
    println!("ssa:");
    match metacat::ssa::ssa(prepared.type_term.clone().to_strict()) {
        Ok(ssa) => {
            for value in &ssa {
                println!("  {value}");
            }
        }
        Err(error) => {
            println!("  failed: {error}");
            print_ssa_debug(&prepared.type_term);
            return Err(error.into());
        }
    };

    println!();
    println!("eval:");
    let (result, eval_steps) = match eval_type(prepared.type_term.clone()) {
        Ok(result) => result,
        Err(error) => {
            println!("  failed while evaluating type-term: {error}");
            return Err(error.into());
        }
    };
    for (i, step) in eval_steps.iter().enumerate() {
        let inputs: Vec<String> = step
            .inputs
            .iter()
            .map(|t| t.pretty(Some(&coarity)))
            .collect();
        println!("  step {i}: {}", step.ssa);
        println!("    inputs: {}", inputs.join(", "));
        print_state("    state", &step.state, &coarity);
    }
    println!();
    println!("node types:");
    for (i, node_type_index) in prepared.node_type_indices.iter().enumerate() {
        match result.get(*node_type_index) {
            Some(ty) => println!("  node {i}: {}", ty.pretty(Some(&coarity))),
            None => println!("  node {i}: <missing result at index {node_type_index}>"),
        }
    }

    Ok(())
}

fn print_ssa_debug(type_term: &OpenHypergraph<(), Dual<OperationKey>>) {
    let (layers, unvisited) = metacat::ssa::parallel_ssa_cyclic(type_term.clone().to_strict());
    let scheduled: HashSet<usize> = layers
        .iter()
        .flatten()
        .map(|value| value.edge_id.0)
        .collect();
    let blocked: Vec<usize> = (0..type_term.hypergraph.edges.len())
        .filter(|edge_id| !scheduled.contains(edge_id))
        .collect();

    println!("  partial layers:");
    if layers.is_empty() {
        println!("    <none>");
    }
    for (i, layer) in layers.iter().enumerate() {
        println!("    layer {i}:");
        for value in layer {
            println!("      {value}");
        }
    }

    println!("  blocked edges:");
    if blocked.is_empty() {
        println!("    <none>");
    }
    for edge_id in blocked {
        let edge = &type_term.hypergraph.edges[edge_id];
        let adjacency = &type_term.hypergraph.adjacency[edge_id];
        let sources: Vec<String> = adjacency
            .sources
            .iter()
            .map(|node| format!("v{}", node.0))
            .collect();
        let targets: Vec<String> = adjacency
            .targets
            .iter()
            .map(|node| format!("v{}", node.0))
            .collect();
        println!(
            "    e{edge_id}: [{}] --{}--> [{}]",
            sources.join(", "),
            edge,
            targets.join(", ")
        );
    }

    let dependencies = edge_dependencies(type_term);
    println!("  edge dependencies:");
    if dependencies.is_empty() {
        println!("    <none>");
    }
    for (edge_id, dependency_id, via_nodes) in &dependencies {
        println!(
            "    e{edge_id} depends on e{dependency_id} via {}",
            via_nodes.join(", ")
        );
    }

    let mut mutual_dependencies = Vec::new();
    for (edge_id, dependency_id, via_nodes) in &dependencies {
        if dependencies
            .iter()
            .any(|(other_edge, other_dependency, _)| {
                other_edge == dependency_id && other_dependency == edge_id
            })
        {
            mutual_dependencies.push((*edge_id, *dependency_id, via_nodes.clone()));
        }
    }
    println!("  cycle candidates:");
    if mutual_dependencies.is_empty() {
        println!("    <none detected from direct edge dependencies>");
    }
    for (edge_id, dependency_id, via_nodes) in mutual_dependencies {
        println!(
            "    e{edge_id} <-> e{dependency_id} via {}",
            via_nodes.join(", ")
        );
    }

    println!("  unvisited node flags: {:?}", unvisited);
}

fn edge_dependencies(
    graph: &OpenHypergraph<(), Dual<OperationKey>>,
) -> Vec<(usize, usize, Vec<String>)> {
    let mut dependencies = Vec::new();

    for (edge_id, adjacency) in graph.hypergraph.adjacency.iter().enumerate() {
        for (dependency_id, dependency_adjacency) in graph.hypergraph.adjacency.iter().enumerate() {
            if edge_id == dependency_id {
                continue;
            }

            let via_nodes: Vec<String> = adjacency
                .sources
                .iter()
                .filter(|source| dependency_adjacency.targets.contains(source))
                .map(|node| format!("v{}", node.0))
                .collect();

            if !via_nodes.is_empty() {
                dependencies.push((edge_id, dependency_id, via_nodes));
            }
        }
    }

    dependencies
}
