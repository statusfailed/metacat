use crate::render::{
    print_dot_hypergraph, print_dot_raw_type_map, print_open_hypergraph, print_state,
};
use crate::util::{find_definition, forget_labels};

use clap::{Args, Subcommand, ValueEnum};
use hexpr::try_interpret;
use metacat::check::{check_trace, raw_type_term, type_term};
use metacat::syntax::{Declaration, TheoryBundle};
use metacat::theory::OperationKey;
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
        stage: InspectArrowStage,
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
    stage: InspectArrowStage,
    format: InspectFormat,
) -> anyhow::Result<()> {
    let bundle = TheoryBundle::from_file(path)?;
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
            InspectFormat::Dot => print_dot_hypergraph(&term),
        },
        InspectArrowStage::RawTypeMap => {
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
                InspectFormat::Dot => print_dot_raw_type_map(&raw_type_map),
            }
        }
        InspectArrowStage::TypeMap => {
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
                InspectFormat::Dot => print_dot_hypergraph(&type_map),
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
