mod load;
use load::TheoryBundle;

#[derive(ValueEnum, Clone, Debug)]
enum Format {
    Hexpr,
    Svg,
}

use hexpr::*;
use metacat::check::eval_type;
use metacat::check::to_type_map;

// CLI utils
use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "metacat-cli")]
#[command(about = "A tool for checking categorical definitions")]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

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
        #[arg()]
        path: PathBuf,
        #[arg()]
        name: String,
        #[arg(short, long, value_enum, default_value_t = Format::Hexpr)]
        format: Format,
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

    env_logger::Builder::new().filter_level(log_level).init();

    match cli.command {
        Command::Check { path } => check(path),
        Command::Arrow { path, name, format } => arrow(path, name, format),
    }
}

/// Read a file of `Declaration`s into object and arrow theories,
/// then check all definitions.
fn check(path: PathBuf) -> anyhow::Result<()> {
    let TheoryBundle {
        object_theory,
        arrow_theory,
        definitions,
        ..
    } = TheoryBundle::from_file(path)?;

    log::info!("checking definitions");

    for (_operation, declaration) in &definitions {
        let def_hexpr = declaration.definition.as_ref().unwrap(); // Safe because we only store definitions with Some(hexpr)
        log::info!(
            "checking definition {} : {} -> {} = {}",
            declaration.name,
            declaration.source_map,
            declaration.target_map,
            def_hexpr
        );

        // NOTE: we use forget_labels instead of unify, since we have a single-sorted theory.
        let term = forget_labels(try_interpret(&arrow_theory, def_hexpr)?);
        let source = forget_labels(try_interpret(&object_theory, &declaration.source_map)?);
        let target = forget_labels(try_interpret(&object_theory, &declaration.target_map)?);

        let type_term = to_type_map(arrow_theory.clone(), source, target, &term);
        let result = eval_type(type_term);
        log::debug!("eval_type: {:?}", result);

        match result {
            Ok(_) => {
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
fn arrow(path: PathBuf, name: String, format: Format) -> anyhow::Result<()> {
    log::info!("Loading theories to find arrow: {}", name);
    let bundle = TheoryBundle::from_file(path)?;

    // Try to find the operation in the arrow theory
    let operation = &name.parse()?;

    // Look for a definition with the given name
    if let Some(declaration) = bundle.definitions.get(operation) {
        let def_hexpr = declaration.definition.as_ref().unwrap(); // Safe because we only store definitions with Some(hexpr)
        match format {
            Format::Hexpr => {
                println!("{}", def_hexpr);
            }
            Format::Svg => {
                use open_hypergraphs_dot::{Options, svg::to_svg_with};
                use std::io::Write;
                let mut term = forget_labels(try_interpret(&bundle.arrow_theory, def_hexpr)?);
                term.quotient();
                std::io::stdout().write_all(&to_svg_with(
                    &term.clone().map_nodes(|_| ""),
                    &Options::default().display().tb(),
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
