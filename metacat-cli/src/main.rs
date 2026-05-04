#[derive(ValueEnum, Clone, Debug)]
enum Orientation {
    LR,
    TB,
}

use hexpr::Operation;
use metacat::check::check;
use metacat::theory::{Theory, TheoryId, TheorySet};
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
        theory_name: String,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
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
        theory_name: String,
        #[arg()]
        name: String,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    Svg {
        #[arg()]
        theory_name: String,
        #[arg()]
        name: String,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
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
        Command::Check { theory_name, paths } => check_files(theory_name, paths),
        Command::Arrow { format } => arrow(format),
    }
}

/// Read one or more files of `Declaration`s into object and arrow theories,
/// then check all definitions.
fn check_files(theory_name: String, paths: Vec<PathBuf>) -> anyhow::Result<()> {
    let theories = TheorySet::from_files(paths)?;
    let theory_id = TheoryId(theory_name.parse()?);
    let theory = theories
        .theories
        .get(&theory_id)
        .ok_or_else(|| anyhow::anyhow!("theory '{}' not found", theory_id))?;
    let Theory::Theory { arrows, .. } = theory else {
        anyhow::bail!("theory '{}' is builtin and cannot be checked", theory_id);
    };

    log::info!("checking definitions");

    for (operation, declaration) in arrows
        .iter()
        .filter(|(_, arrow)| arrow.definition.is_some())
    {
        let mut term = declaration.definition.clone().unwrap();
        let (source, target) = declaration.type_maps.clone();
        log::info!("checking definition {}", operation);

        let result = check(theory, source, target, &mut term);
        log::debug!("check: {:?}", result);

        match result {
            Ok(_types) => {
                println!(
                    "{} {} : {} -> {}",
                    "[✓]".green(),
                    declaration.name,
                    declaration.raw.type_maps.0,
                    declaration.raw.type_maps.1
                );
            }
            Err(e) => {
                println!(
                    "{} {} : {} -> {}",
                    "[✗]".red(),
                    declaration.name,
                    declaration.raw.type_maps.0,
                    declaration.raw.type_maps.1
                );
                println!("Checking '{}' failed: {}", declaration.name, e);
            }
        }
    }

    Ok(())
}

fn arrow(format: ArrowFormat) -> anyhow::Result<()> {
    let (theory_name, name, paths) = match &format {
        ArrowFormat::Hexpr {
            theory_name,
            name,
            paths,
        }
        | ArrowFormat::Svg {
            theory_name,
            name,
            paths,
            ..
        } => (theory_name.clone(), name.clone(), paths.clone()),
    };

    let theories = TheorySet::from_files(paths)?;
    let theory_id = TheoryId(theory_name.parse()?);
    let theory = theories
        .theories
        .get(&theory_id)
        .ok_or_else(|| anyhow::anyhow!("theory '{}' not found", theory_id))?;
    let Theory::Theory { syntax, arrows } = theory else {
        anyhow::bail!(
            "theory '{}' is builtin and has no definitional arrows",
            theory_id
        );
    };

    let operation: Operation = name.parse()?;
    let declaration = arrows.get(&operation).ok_or_else(|| {
        anyhow::anyhow!("definition '{}' not found in theory '{}'", name, theory_id)
    })?;
    let def_term = declaration.definition.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "arrow '{}' in theory '{}' has no definition",
            name,
            theory_id
        )
    })?;

    match format {
        ArrowFormat::Hexpr { .. } => {
            let raw_def = declaration
                .raw
                .definition
                .as_ref()
                .expect("resolved definitional arrow should retain raw definition");
            println!("{raw_def}");
        }
        ArrowFormat::Svg { orientation, .. } => {
            use open_hypergraphs_dot::{Options, svg::to_svg_with};
            use std::io::Write;

            let mut term = def_term;
            term.quotient().map_err(QuotientError)?;
            let (source, target) = declaration.type_maps.clone();
            let result = check(theory, source, target, &mut term);

            let syntax_theory = theories
                .theories
                .get(syntax)
                .expect("syntax theory should be present");
            let labels: Vec<String> = match result {
                Ok(types) => types
                    .iter()
                    .map(|t| {
                        t.try_pretty(Some(&|op: &Operation| {
                            syntax_theory.coarity_of(op).ok_or_else(|| {
                                anyhow::anyhow!("coarity lookup failed for operation '{op}'")
                            })
                        }))
                    })
                    .collect::<anyhow::Result<_>>()?,
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

    Ok(())
}
