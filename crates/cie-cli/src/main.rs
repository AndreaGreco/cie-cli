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

    /// Autentica con PIN e legge dati protetti
    Auth {
        /// PIN (8 cifre). SCONSIGLIATO: resta nella history della shell e in `ps`.
        /// Senza questo flag il PIN si legge dal portachiavi
        /// (`secret-tool store --label='CIE PIN' service cie`) o si chiede a video.
        #[arg(short, long)]
        pin: Option<String>,

        /// Salva su file il certificato letto dalla carta (DER).
        #[arg(long, value_name = "FILE")]
        dump_cert: Option<String>,
    },

    /// Firma il digest SHA-256 di un file con la chiave della carta
    Sign {
        /// File da firmare
        file: String,

        /// PIN (8 cifre). Senza questo flag si usa il portachiavi o il prompt.
        #[arg(short, long)]
        pin: Option<String>,

        /// Dove scrivere la firma (default: <file>.sig)
        #[arg(short, long, value_name = "FILE")]
        output: Option<String>,
    },
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
        Commands::Auth { pin, dump_cert } => commands::auth::execute(pin, dump_cert),
        Commands::Sign { file, pin, output } => commands::sign::execute(file, pin, output),
    }
}
