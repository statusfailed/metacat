#[derive(ValueEnum, Clone, Debug)]
enum Orientation {
    LR,
    TB,
}

use metacat::check::check;
use metacat::new_syntax::{Theory, TheoryId, TheorySet};

// CLI utils
use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use std::path::PathBuf;

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
                    "{} {} : {:?} -> {:?}",
                    "[✓]".green(),
                    declaration.name,
                    declaration.type_maps.0,
                    declaration.type_maps.1
                );
            }
            Err(e) => {
                println!(
                    "{} {} : {:?} -> {:?}",
                    "[✗]".red(),
                    declaration.name,
                    declaration.type_maps.0,
                    declaration.type_maps.1
                );
                println!("Checking '{}' failed: {}", declaration.name, e);
            }
        }
    }

    Ok(())
}
