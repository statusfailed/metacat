use crate::util::{DeclarationTermMode, declaration_check_input};

use clap::Args;
use colored::*;
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
        let bundle = TheoryBundle::from_file(self.path)?;

        log::info!("checking definitions");

        for (operation, declaration) in &bundle.definitions {
            let def_hexpr = declaration.definition.as_ref().unwrap();
            log::info!(
                "checking definition {} : {} -> {} = {}",
                operation,
                declaration.source_map,
                declaration.target_map,
                def_hexpr
            );

            let input = declaration_check_input(&bundle, declaration, DeclarationTermMode::Body)?;
            let result = check(&bundle.arrow_theory, input);
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
