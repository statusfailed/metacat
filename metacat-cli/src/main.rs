#[derive(ValueEnum, Clone, Debug)]
enum Orientation {
    LR,
    TB,
}

use hexpr::*;
use metacat::check::check;
use metacat::syntax::TheoryBundle;
use metacat::theory::OperationKey;
use open_hypergraphs::strict::vec::FiniteFunction;

use thiserror::Error;

// CLI utils
use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use std::path::PathBuf;

#[derive(Debug, Error)]
pub struct QuotientError(FiniteFunction);

impl std::fmt::Display for QuotientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

#[derive(Parser)]
#[command(name = "metacat-cli", version=env!("CARGO_PKG_VERSION"),)]
#[command(about = "A tool for checking categorical definitions")]
#[command(version = env!("CARGO_PKG_VERSION"),)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(long, help = "Force enable colors")]
    color: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Check {
        #[arg()]
        path: PathBuf,
    },
    Arrow {
        #[command(subcommand)]
        format: ArrowFormat,
    },
}

#[derive(Subcommand)]
enum ArrowFormat {
    Hexpr {
        #[arg()]
        path: PathBuf,
        #[arg()]
        name: String,
    },
    Svg {
        #[arg()]
        path: PathBuf,
        #[arg()]
        name: String,
        #[arg(short, long, value_enum, default_value_t = Orientation::LR)]
        orientation: Orientation,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logger based on verbosity level
    let log_level = match cli.verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

    // Force enable colors if requested
    if cli.color {
        colored::control::set_override(true);
    }

    env_logger::Builder::new()
        .filter_level(log_level)
        .parse_default_env()
        .write_style(if cli.color {
            env_logger::WriteStyle::Always
        } else {
            env_logger::WriteStyle::Auto
        })
        .init();

    match cli.command {
        Command::Check { path } => check_file(path),
        Command::Arrow { format } => arrow(format),
    }
}

/// Read a file of `Declaration`s into object and arrow theories,
/// then check all definitions.
fn check_file(path: PathBuf) -> anyhow::Result<()> {
    let TheoryBundle {
        object_theory,
        arrow_theory,
        definitions,
        ..
    } = TheoryBundle::from_file(path)?;

    log::info!("checking definitions");

    for (operation, declaration) in &definitions {
        let def_hexpr = declaration.definition.as_ref().unwrap(); // Safe because we only store definitions with Some(hexpr)
        log::info!(
            "checking definition {} : {} -> {} = {}",
            operation,
            declaration.source_map,
            declaration.target_map,
            def_hexpr
        );

        // NOTE: we use forget_labels instead of unify, since we have a single-sorted theory.
        let mut term = forget_labels(try_interpret(&arrow_theory, def_hexpr)?);
        let source = forget_labels(try_interpret(&object_theory, &declaration.source_map)?);
        let target = forget_labels(try_interpret(&object_theory, &declaration.target_map)?);

        let result = check(&arrow_theory, source, target, &mut term);
        log::debug!("check: {:?}", result);

        match result {
            Ok(_types) => {
                println!(
                    "{} {} : {} -> {}",
                    "[✓]".green(),
                    declaration.name,
                    declaration.source_map,
                    declaration.target_map
                );
            }
            Err(e) => {
                println!(
                    "{} {} : {} -> {}",
                    "[✗]".red(),
                    declaration.name,
                    declaration.source_map,
                    declaration.target_map
                );
                println!("Checking '{}' failed: {}", declaration.name, e);
            }
        }
    }

    Ok(())
}

/// Load theories from a file and print the hexpr for a given arrow name
fn arrow(format: ArrowFormat) -> anyhow::Result<()> {
    let (path, name) = match &format {
        ArrowFormat::Hexpr { path, name } | ArrowFormat::Svg { path, name, .. } => {
            (path.clone(), name.clone())
        }
    };

    log::info!("Loading theories to find arrow: {}", name);
    let bundle = TheoryBundle::from_file(path)?;

    // Try to find the operation in the arrow theory
    let operation = &name.parse()?;

    // Look for a definition with the given name
    if let Some(declaration) = bundle.definitions.get(operation) {
        let def_hexpr = declaration.definition.as_ref().unwrap(); // Safe because we only store definitions with Some(hexpr)
        match format {
            ArrowFormat::Hexpr { .. } => {
                println!("{}", def_hexpr);
            }
            ArrowFormat::Svg { orientation, .. } => {
                // Render an SVG of the term. We try to compute types if possible, and fall back to
                // unlabeled nodes if checking fails.
                use open_hypergraphs_dot::{Options, svg::to_svg_with};
                use std::io::Write;
                let object_theory = bundle.object_theory;
                let mut term = forget_labels(try_interpret(&bundle.arrow_theory, def_hexpr)?);
                term.quotient().map_err(QuotientError)?;
                let source = forget_labels(try_interpret(&object_theory, &declaration.source_map)?);
                let target = forget_labels(try_interpret(&object_theory, &declaration.target_map)?);

                // Compute types for each node of the open hypergraph
                let result = check(&bundle.arrow_theory, source, target, &mut term);

                // Tell pretty-printer the coarity of each operation
                let coarity =
                    |op: &OperationKey| -> usize { object_theory.type_maps(op).1.targets.len() };

                // Pretty-print computed type trees
                let labels: Vec<String> = match result {
                    Ok(types) => types.iter().map(|t| t.pretty(Some(&coarity))).collect(),
                    Err(e) => {
                        log::warn!("check failed: {e}");
                        vec![String::new(); term.hypergraph.nodes.len()]
                    }
                };

                let mut opts = Options::default().display();
                opts.orientation = match orientation {
                    Orientation::LR => open_hypergraphs_dot::Orientation::LR,
                    Orientation::TB => open_hypergraphs_dot::Orientation::TB,
                };

                std::io::stdout().write_all(&to_svg_with(
                    &term.with_nodes(|_| labels).expect("labels length mismatch"),
                    &opts,
                )?)?;
            }
        }
    } else {
        return Err(anyhow::anyhow!("definition '{}' not found", name));
    }

    Ok(())
}

fn forget_labels<T, A>(
    f: open_hypergraphs::lax::OpenHypergraph<T, A>,
) -> open_hypergraphs::lax::OpenHypergraph<(), A> {
    f.map_nodes(|_| ())
}
