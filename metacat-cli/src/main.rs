use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use hexpr::Operation;
use metacat::check::check;
use metacat::dv::dv_check;
use metacat::syntax::SyntaxGraph;
use metacat::theory::{Theory, TheoryId, TheorySet};
use open_hypergraphs_dot::{Options, svg::to_svg_with};
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Orientation {
    Lr,
    Tb,
}

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

    /// Inspect a definition as an open hypergraph.
    Arrow {
        #[command(subcommand)]
        format: ArrowFormat,
    },
}

#[derive(Debug, Subcommand)]
enum ArrowFormat {
    /// Render a definition as SVG on standard output.
    Svg {
        /// Theory containing the definition.
        theory_name: String,

        /// Name of the definition to render.
        name: String,

        /// Theory files to load together.
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Direction in which the rendered graph flows.
        #[arg(short, long, value_enum, default_value_t = Orientation::Lr)]
        orientation: Orientation,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.color {
        colored::control::set_override(true);
    }

    match cli.command {
        Command::Check { theory, paths } => check_files(theory, paths),
        Command::Arrow { format } => arrow(format),
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

        match dv_check(
            theory,
            source,
            target,
            declaration.ar.as_ref(),
            &mut definition,
        ) {
            Ok(_result) => println!(
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

fn arrow(format: ArrowFormat) -> anyhow::Result<()> {
    match format {
        ArrowFormat::Svg {
            theory_name,
            name,
            paths,
            orientation,
        } => render_svg(theory_name, name, paths, orientation),
    }
}

fn render_svg(
    theory_name: String,
    name: String,
    paths: Vec<PathBuf>,
    orientation: Orientation,
) -> anyhow::Result<()> {
    let theories = TheorySet::from_files(paths)?;
    let theory_id = TheoryId(theory_name.parse()?);
    let theory = theories
        .theories
        .get(&theory_id)
        .ok_or_else(|| anyhow::anyhow!("theory '{theory_id}' not found"))?;
    let Theory::Theory { arrows, .. } = theory else {
        anyhow::bail!("theory '{theory_id}' is builtin and has no definitions");
    };

    let operation: Operation = name.parse()?;
    let declaration = arrows
        .get(&operation)
        .ok_or_else(|| anyhow::anyhow!("definition '{name}' not found in theory '{theory_id}'"))?;
    let mut term = declaration.definition.clone().ok_or_else(|| {
        anyhow::anyhow!("arrow '{name}' in theory '{theory_id}' has no definition")
    })?;
    term.quotient()
        .map_err(|quotient| anyhow::anyhow!("unable to quotient definition: {quotient:?}"))?;

    let (source, target) = declaration.type_maps.clone();
    let labels = match check(theory, source, target, &mut term) {
        Ok(result) => {
            let syntax = SyntaxGraph::from_saturation(&result.phi, &result.saturation)?;
            let syntax_labels = syntax.labels()?;
            result
                .proof_classes()
                .table
                .0
                .iter()
                .map(|&class| syntax_labels[class].clone())
                .collect()
        }
        Err(error) => {
            eprintln!("warning: check failed; rendering without wire labels: {error}");
            vec![String::new(); term.hypergraph.nodes.len()]
        }
    };
    let labeled = term
        .with_nodes(|_| labels)
        .ok_or_else(|| anyhow::anyhow!("wire-label count did not match the definition"))?;

    let mut options = Options::default().display();
    options.orientation = match orientation {
        Orientation::Lr => open_hypergraphs_dot::Orientation::LR,
        Orientation::Tb => open_hypergraphs_dot::Orientation::TB,
    };
    io::stdout().write_all(&to_svg_with(&labeled, &options)?)?;
    Ok(())
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

        let Command::Check { theory, paths } = cli.command else {
            panic!("expected check command");
        };
        assert!(cli.color);
        assert_eq!(theory.as_deref(), Some("proof"));
        assert_eq!(
            paths,
            [PathBuf::from("syntax.hex"), PathBuf::from("proof.hex")]
        );
    }

    #[test]
    fn parses_arrow_svg_command() {
        let cli = Cli::try_parse_from([
            "metacat",
            "arrow",
            "svg",
            "proof",
            "example",
            "syntax.hex",
            "proof.hex",
            "--orientation",
            "tb",
        ])
        .expect("valid arrow svg command");

        let Command::Arrow {
            format:
                ArrowFormat::Svg {
                    theory_name,
                    name,
                    paths,
                    orientation,
                },
        } = cli.command
        else {
            panic!("expected arrow svg command");
        };
        assert_eq!(theory_name, "proof");
        assert_eq!(name, "example");
        assert_eq!(
            paths,
            [PathBuf::from("syntax.hex"), PathBuf::from("proof.hex")]
        );
        assert_eq!(orientation, Orientation::Tb);
    }
}
