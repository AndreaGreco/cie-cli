//! CLI per la Carta d'Identità Elettronica italiana

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "cie")]
#[command(author, version, about = "CLI per la Carta d'Identità Elettronica italiana")]
#[command(propagate_version = true)]
struct Cli {
    /// Livello di verbosità (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Percorso alla libreria PKCS#11 (default: auto-detect)
    #[arg(long, env = "CIE_PKCS11_LIB")]
    pkcs11_lib: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Lista i lettori di smart card disponibili
    Readers,

    /// Mostra informazioni sulla CIE inserita
    Info,

    /// Mostra i dati del titolare (nome, cognome, CF)
    Data,

    /// Gestione dei certificati
    #[command(subcommand)]
    Cert(CertCommands),

    /// Firma un documento
    Sign {
        /// File da firmare
        #[arg(short, long)]
        input: String,

        /// File di output (default: input.sig)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Verifica una firma
    Verify {
        /// File originale
        #[arg(short, long)]
        input: String,

        /// File della firma
        #[arg(short, long)]
        signature: String,
    },
}

#[derive(Subcommand)]
enum CertCommands {
    /// Lista i certificati sulla CIE
    List,

    /// Esporta un certificato
    Export {
        /// ID o label del certificato
        #[arg(short, long)]
        id: String,

        /// File di output
        #[arg(short, long)]
        output: String,
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
        Commands::Readers => commands::readers::execute(cli.pkcs11_lib.as_deref()),
        Commands::Info => commands::info::execute(cli.pkcs11_lib.as_deref()),
        Commands::Data => commands::data::execute(cli.pkcs11_lib.as_deref()),
        Commands::Cert(cmd) => match cmd {
            CertCommands::List => commands::cert::list(cli.pkcs11_lib.as_deref()),
            CertCommands::Export { id, output } => {
                commands::cert::export(cli.pkcs11_lib.as_deref(), &id, &output)
            }
        },
        Commands::Sign { input, output } => {
            commands::sign::execute(cli.pkcs11_lib.as_deref(), &input, output.as_deref())
        }
        Commands::Verify { input, signature } => {
            commands::verify::execute(&input, &signature)
        }
    }
}
