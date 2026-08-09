//! CLI per la Carta d'Identità Elettronica italiana

mod commands;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueHint};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "cie")]
#[command(author, version, about = "CLI per la Carta d'Identità Elettronica italiana (PC/SC diretto)")]
#[command(propagate_version = true)]
pub struct Cli {
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
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        dump_cert: Option<String>,
    },

    /// Firma il digest SHA-256 di un file con la chiave della carta
    Sign {
        /// File da firmare
        #[arg(value_hint = ValueHint::FilePath)]
        file: String,

        /// PIN (8 cifre). Senza questo flag si usa il portachiavi o il prompt.
        #[arg(short, long)]
        pin: Option<String>,

        /// Dove scrivere la firma (default: <file>.sig)
        #[arg(short, long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        output: Option<String>,
    },

    /// Firma un file qualsiasi in CAdES: contenuto e firma in un unico .p7m
    SignP7m {
        /// File da firmare
        #[arg(value_hint = ValueHint::FilePath)]
        file: String,

        /// PIN (8 cifre). Senza questo flag si usa il portachiavi o il prompt.
        #[arg(short, long)]
        pin: Option<String>,

        /// Dove scrivere la busta (default: <file>.p7m)
        #[arg(short, long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        output: Option<String>,
    },

    /// Firma un PDF in PAdES: la firma finisce dentro il file, non a fianco
    SignPdf {
        /// PDF da firmare
        #[arg(value_hint = ValueHint::FilePath)]
        file: String,

        /// PIN (8 cifre). Senza questo flag si usa il portachiavi o il prompt.
        #[arg(short, long)]
        pin: Option<String>,

        /// Dove scrivere il PDF firmato (default: <file>-firmato.pdf)
        #[arg(short, long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        output: Option<String>,

        /// Pagina su cui disegnare il riquadro (da 1)
        #[arg(long, default_value_t = 1, value_name = "N")]
        pagina: u32,

        /// Motivo della firma, mostrato nel riquadro
        #[arg(long)]
        motivo: Option<String>,

        /// Firma di certificazione: il documento risulta tuo e dichiari cosa
        /// resta lecito modificare. 1 = niente, 2 = moduli e altre firme
        /// (default), 3 = anche annotazioni. Ammessa solo sulla prima firma.
        #[arg(
            long,
            value_name = "LIVELLO",
            num_args = 0..=1,
            default_missing_value = "2",
            value_parser = clap::value_parser!(u8).range(1..=3),
        )]
        certifica: Option<u8>,
    },

    /// Certificazione OpenPGP con la chiave della carta
    Gpg {
        #[command(subcommand)]
        azione: GpgCommands,
    },

    /// Installa il completamento automatico dei comandi nella tua shell
    Completions {
        /// bash, zsh, fish... (default: quella indicata da $SHELL)
        shell: Option<clap_complete::Shell>,

        /// Scrive lo script dove la shell lo trova da sola, invece di stamparlo
        #[arg(long)]
        install: bool,
    },
}

#[derive(Subcommand)]
enum GpgCommands {
    /// Certifica con la CIE una User ID di una chiave OpenPGP
    Certify {
        /// Chiave OpenPGP da certificare (armor ASCII o binaria)
        #[arg(value_hint = ValueHint::FilePath)]
        chiave: String,

        /// Quale User ID certificare (da 1). Senza, si sceglie a video.
        #[arg(long, value_name = "N")]
        uid: Option<usize>,

        /// PIN (8 cifre). Senza questo flag si usa il portachiavi o il prompt.
        #[arg(short, long)]
        pin: Option<String>,

        /// Dove scrivere la chiave certificata (default: <chiave>-certificata.asc)
        #[arg(short, long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        output: Option<String>,

        /// Non chiedere conferma se il nome della User ID non corrisponde alla CIE
        #[arg(long)]
        yes: bool,
    },

    /// Verifica le certification CIE presenti su una chiave OpenPGP
    Verify {
        /// Chiave OpenPGP da esaminare
        #[arg(value_hint = ValueHint::FilePath)]
        chiave: String,

        /// Certificatore con cui verificare (cie-certifier.asc)
        #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
        certifier: Option<String>,
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
        Commands::SignP7m { file, pin, output } => commands::sign_p7m::execute(file, pin, output),
        Commands::SignPdf { file, pin, output, pagina, motivo, certifica } => {
            // Le pagine si contano da 1 per chi legge, da 0 per il PDF.
            let indice = pagina.checked_sub(1).context("le pagine si contano da 1")?;
            commands::sign_pdf::execute(file, pin, output, indice, motivo, certifica)
        }
        Commands::Gpg { azione } => match azione {
            GpgCommands::Certify { chiave, uid, pin, output, yes } => {
                commands::gpg::certify_key(chiave, uid, pin, output, yes)
            }
            GpgCommands::Verify { chiave, certifier } => {
                commands::gpg::verify_key(chiave, certifier)
            }
        },
        Commands::Completions { shell, install } => {
            commands::completions::execute(shell, install)
        }
    }
}
