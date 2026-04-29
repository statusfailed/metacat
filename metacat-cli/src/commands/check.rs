use crate::util::forget_labels;

use clap::Args;
use colored::*;
use hexpr::try_interpret;
use metacat::check::check;
use metacat::syntax::TheoryBundle;
use std::path::PathBuf;

#[derive(Args)]
pub struct CheckCommand {
    #[arg()]
    path: PathBuf,
}

impl CheckCommand {
    /// Read a file of declarations into object and arrow theories, then check all definitions.
    pub fn run(self) -> anyhow::Result<()> {
        let TheoryBundle {
            object_theory,
            arrow_theory,
            definitions,
            ..
        } = TheoryBundle::from_file(self.path)?;

        log::info!("checking definitions");

        for (operation, declaration) in &definitions {
            let def_hexpr = declaration.definition.as_ref().unwrap();
            log::info!(
                "checking definition {} : {} -> {} = {}",
                operation,
                declaration.source_map,
                declaration.target_map,
                def_hexpr
            );

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
}
