use clap::{Parser, Subcommand};
use colored::Colorize;
use metacat::check::check;
use metacat::theory::{Theory, TheoryId, TheorySet};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "metacat", version, about = "A categorical theorem prover")]
struct Cli {
    /// Force colorized output, even when stdout is not a terminal.
    #[arg(long)]
    color: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check every definition in one or more theory files.
    Check {
        /// Check only the named theory.
        #[arg(long)]
        theory: Option<String>,

        /// Theory files to load together.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.color {
        colored::control::set_override(true);
    }

    match cli.command {
        Command::Check { theory, paths } => check_files(theory, paths),
    }
}

fn check_files(theory: Option<String>, paths: Vec<PathBuf>) -> anyhow::Result<()> {
    let theories = TheorySet::from_files(paths)?;
    let failures = match theory {
        Some(theory_name) => {
            let theory_id = TheoryId(theory_name.parse()?);
            let theory = theories
                .theories
                .get(&theory_id)
                .ok_or_else(|| anyhow::anyhow!("theory '{theory_id}' not found"))?;
            if matches!(theory, Theory::Nat) {
                anyhow::bail!("theory '{theory_id}' is builtin and cannot be checked");
            }
            check_theory(&theory_id, theory)
        }
        None => theories
            .theories
            .iter()
            .filter(|(_, theory)| !matches!(theory, Theory::Nat))
            .map(|(theory_id, theory)| check_theory(theory_id, theory))
            .sum(),
    };

    if failures == 0 {
        Ok(())
    } else {
        anyhow::bail!("{failures} definition(s) failed to check")
    }
}

fn check_theory(theory_id: &TheoryId, theory: &Theory) -> usize {
    let Theory::Theory { arrows, .. } = theory else {
        return 0;
    };

    let mut failures = 0;
    for declaration in arrows.values().filter(|arrow| arrow.definition.is_some()) {
        let mut definition = declaration
            .definition
            .clone()
            .expect("definition was filtered above");
        let (source, target) = declaration.type_maps.clone();

        match check(theory, source, target, &mut definition) {
            Ok(()) => println!(
                "{} {} {} : {} -> {}",
                "[✓]".green(),
                theory_id,
                declaration.name,
                declaration.raw.type_maps.0,
                declaration.raw.type_maps.1
            ),
            Err(error) => {
                failures += 1;
                eprintln!(
                    "{} {} {} : {} -> {}\n    {}",
                    "[✗]".red(),
                    theory_id,
                    declaration.name,
                    declaration.raw.type_maps.0,
                    declaration.raw.type_maps.1,
                    error
                );
            }
        }
    }

    failures
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_check_command() {
        let cli = Cli::try_parse_from([
            "metacat",
            "--color",
            "check",
            "--theory",
            "proof",
            "syntax.hex",
            "proof.hex",
        ])
        .expect("valid check command");

        let Command::Check { theory, paths } = cli.command;
        assert!(cli.color);
        assert_eq!(theory.as_deref(), Some("proof"));
        assert_eq!(
            paths,
            [PathBuf::from("syntax.hex"), PathBuf::from("proof.hex")]
        );
    }
}
