//! CLI per la Carta d'Identità Elettronica italiana

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "cie")]
#[command(author, version, about = "CLI per la Carta d'Identità Elettronica italiana (PC/SC diretto)")]
#[command(propagate_version = true)]
struct Cli {
    /// Livello di verbosità (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Mostra informazioni sulla CIE
    Info,

    /// Test comunicazione PC/SC diretta con debug completo
    PcscTest,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    let log_level = match cli.verbose {
        0 => Level::WARN,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    // Esegui il comando
    match cli.command {
        Commands::Info => commands::info::execute(),
        Commands::PcscTest => commands::pcsc_test::execute(),
    }
}
