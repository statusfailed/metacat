mod commands;
mod render;

use clap::Parser;
use commands::Command;

#[derive(Parser)]
#[command(name = "metacat-cli", version=env!("CARGO_PKG_VERSION"))]
#[command(about = "A tool for checking categorical definitions")]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(long, help = "Force enable colors")]
    color: bool,

    #[command(subcommand)]
    command: Command,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let log_level = match cli.verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

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

    cli.command.run()
}
