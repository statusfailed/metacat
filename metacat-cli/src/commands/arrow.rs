use crate::util::forget_labels;

use clap::{Args, Subcommand, ValueEnum};
use hexpr::{Operation, try_interpret};
use metacat::check::check;
use metacat::syntax::TheoryBundle;
use metacat::theory::OperationKey;
use open_hypergraphs::strict::vec::FiniteFunction;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Args)]
pub struct ArrowCommand {
    #[command(subcommand)]
    format: ArrowFormat,
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

#[derive(ValueEnum, Clone, Debug)]
enum Orientation {
    LR,
    TB,
}

#[derive(Debug, Error)]
struct QuotientError(FiniteFunction);

impl std::fmt::Display for QuotientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl ArrowCommand {
    /// Load theories from a file and print the hexpr or SVG for a given arrow name.
    pub fn run(self) -> anyhow::Result<()> {
        let (path, name) = match &self.format {
            ArrowFormat::Hexpr { path, name } | ArrowFormat::Svg { path, name, .. } => {
                (path.clone(), name.clone())
            }
        };

        log::info!("Loading theories to find arrow: {}", name);
        let bundle = TheoryBundle::from_file(path)?;
        let operation: Operation = name.parse()?;

        if let Some(declaration) = bundle.definitions.get(&operation) {
            let def_hexpr = declaration.definition.as_ref().unwrap();
            match self.format {
                ArrowFormat::Hexpr { .. } => {
                    println!("{}", def_hexpr);
                }
                ArrowFormat::Svg { orientation, .. } => {
                    use open_hypergraphs_dot::{Options, svg::to_svg_with};
                    use std::io::Write;

                    let object_theory = bundle.object_theory;
                    let mut term = forget_labels(try_interpret(&bundle.arrow_theory, def_hexpr)?);
                    term.quotient().map_err(QuotientError)?;
                    let source =
                        forget_labels(try_interpret(&object_theory, &declaration.source_map)?);
                    let target =
                        forget_labels(try_interpret(&object_theory, &declaration.target_map)?);

                    let result = check(&bundle.arrow_theory, source, target, &mut term);
                    let coarity = |op: &OperationKey| -> usize {
                        object_theory.type_maps(op).1.targets.len()
                    };

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
}
