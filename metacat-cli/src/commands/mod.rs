pub mod arrow;
pub mod check;
pub mod inspect;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum Command {
    Check(check::CheckCommand),
    Arrow(arrow::ArrowCommand),
    Inspect(inspect::InspectCommand),
}

impl Command {
    pub fn run(self) -> anyhow::Result<()> {
        match self {
            Command::Check(command) => command.run(),
            Command::Arrow(command) => command.run(),
            Command::Inspect(command) => command.run(),
        }
    }
}
