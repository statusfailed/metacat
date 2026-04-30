use crate::render::{
    print_dot_hypergraph, print_dot_raw_type_map, print_open_hypergraph, print_state,
};
use crate::util::{find_arrow_declaration, find_definition, forget_labels};

use clap::{Args, Subcommand, ValueEnum};
use hexpr::try_interpret;
use metacat::check::{check, check_trace, eval_type, raw_type_term, type_term};
use metacat::dual;
use metacat::dual::Dual;
use metacat::syntax::{Declaration, TheoryBundle};
use metacat::theory::OperationKey;
use open_hypergraphs::lax::OpenHypergraph;
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
    RawTypeMap,
    TypeMap,
    Ssa,
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
enum InspectFormat {
    Text,
    Dot,
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
        InspectArrowStage::Term => match format {
            InspectFormat::Text => {
                println!();
                println!("term:");
                print_open_hypergraph(&term);
            }
            InspectFormat::Dot => {
                let labels = term_node_labels(&bundle, declaration, &term);
                print_dot_hypergraph(&term, labels.as_deref());
            }
        },
        InspectArrowStage::RawTypeMap => {
            let coarity =
                |op: &OperationKey| -> usize { bundle.object_theory.type_maps(op).1.targets.len() };
            let source = forget_labels(try_interpret(
                &bundle.object_theory,
                &declaration.source_map,
            )?);
            let target = forget_labels(try_interpret(
                &bundle.object_theory,
                &declaration.target_map,
            )?);
            let raw_type_map = raw_type_term(&bundle.arrow_theory, source, target, &mut term)?;

            match format {
                InspectFormat::Text => {
                    println!();
                    println!("raw-type-map:");
                    println!(
                        "  proof node range before quotient: {}..{}",
                        raw_type_map.proof_node_range_before_quotient.start,
                        raw_type_map.proof_node_range_before_quotient.end
                    );
                    print_open_hypergraph(&raw_type_map.graph);
                }
                InspectFormat::Dot => {
                    let labels = raw_type_map_node_labels(&raw_type_map.graph, &coarity);
                    print_dot_raw_type_map(&raw_type_map, labels.as_deref());
                }
            }
        }
        InspectArrowStage::TypeMap => {
            let coarity =
                |op: &OperationKey| -> usize { bundle.object_theory.type_maps(op).1.targets.len() };
            let source = forget_labels(try_interpret(
                &bundle.object_theory,
                &declaration.source_map,
            )?);
            let target = forget_labels(try_interpret(
                &bundle.object_theory,
                &declaration.target_map,
            )?);
            let (type_map, quotient, node_type_indices) =
                type_term(&bundle.arrow_theory, source, target, &mut term)?;

            match format {
                InspectFormat::Text => {
                    println!();
                    println!("type-map:");
                    println!("  quotient: {:?}", quotient);
                    println!("  proof node type indices: {:?}", node_type_indices);
                    print_open_hypergraph(&type_map);
                }
                InspectFormat::Dot => {
                    let labels = type_map_node_labels(&type_map, &coarity);
                    print_dot_hypergraph(&type_map, labels.as_deref());
                }
            }
        }
        InspectArrowStage::Ssa => {
            if format == InspectFormat::Dot {
                return Err(anyhow::anyhow!(
                    "--format dot is only available for --stage term, --stage raw-type-map, and --stage type-map"
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
            let (type_map, _, _) = type_term(&bundle.arrow_theory, source, target, &mut term)?;

            println!();
            println!("ssa:");
            for value in metacat::ssa::ssa(type_map.to_strict())? {
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
    let types = check(&bundle.arrow_theory, source, target, &mut quotient_term).ok()?;

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
    eval_type(type_map.clone()).ok().map(|trees| {
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

fn inspect_check(path: PathBuf, name: String, trace: bool) -> anyhow::Result<()> {
    if !trace {
        return Err(anyhow::anyhow!("inspect check currently requires --trace"));
    }

    let bundle = TheoryBundle::from_file(path)?;
    let declaration = find_definition(&bundle, &name)?;
    let def_hexpr = declaration.definition.as_ref().unwrap();
    let mut term = forget_labels(try_interpret(&bundle.arrow_theory, def_hexpr)?);
    let source = forget_labels(try_interpret(
        &bundle.object_theory,
        &declaration.source_map,
    )?);
    let target = forget_labels(try_interpret(
        &bundle.object_theory,
        &declaration.target_map,
    )?);

    let trace = check_trace(&bundle.arrow_theory, source, target, &mut term)?;
    let coarity =
        |op: &OperationKey| -> usize { bundle.object_theory.type_maps(op).1.targets.len() };

    println!(
        "{} : {} -> {}",
        declaration.name, declaration.source_map, declaration.target_map
    );
    println!("body: {def_hexpr}");
    println!();
    println!("term:");
    print_open_hypergraph(&trace.term);
    println!();
    println!("type-map:");
    println!("  quotient: {:?}", trace.quotient);
    println!("  proof node type indices: {:?}", trace.node_type_indices);
    print_open_hypergraph(&trace.type_term);
    println!();
    println!("ssa:");
    for value in &trace.ssa {
        println!("  {value}");
    }
    println!();
    println!("eval:");
    for (i, step) in trace.eval_steps.iter().enumerate() {
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
    for (i, ty) in trace.node_types.iter().enumerate() {
        println!("  node {i}: {}", ty.pretty(Some(&coarity)));
    }

    Ok(())
}
