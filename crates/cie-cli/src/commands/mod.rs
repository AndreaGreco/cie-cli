//! The CLI commands.

use anyhow::Result;
use cie_firma::keyring::{self, CardSecret};
use cie_firma::{CieSigner, CryptoSigner, Step};
use cie_lib::PcscContext;
use std::cell::Cell;
use std::io::{self, Write};

pub mod auth;
pub mod completions;
pub mod gpg;
pub mod info;
pub mod pcsc_test;
pub mod sign;
pub mod sign_p7m;
pub mod sign_pdf;

/// Opens the card, printing progress with a tick at the end of each line.
///
/// The PIN is resolved *before* the reader is touched: if it is missing, better
/// to find out without having opened a secure channel for nothing.
pub fn open_card(ctx: &PcscContext, pin: Option<String>) -> Result<CieSigner> {
    // Which card this is takes no PIN, and it is the keyring key: without this
    // step one user could not keep more than one CIE.
    let card = cie_firma::identify(ctx).ok();
    let (pin, typed) = resolve_pin(pin, card.as_deref())?;

    // The previous line is closed by the next step arriving: the callback knows
    // what is about to start, not what has finished.
    let open = Cell::new(false);
    let print = |s: Step| {
        if open.get() {
            println!("✓");
        }
        let text = match s {
            Step::Connecting => "Connessione alla carta...",
            Step::SecureChannel => "Apertura del canale sicuro...",
            Step::Pin => "Verifica del PIN...",
            Step::Certificate => "Lettura del certificato...",
        };
        print!("  {} ", text);
        let _ = io::stdout().flush();
        open.set(true);
    };

    println!();
    let outcome = cie_firma::open_and_authenticate(ctx, &pin, &print);
    if open.get() {
        println!("{}", if outcome.is_ok() { "✓" } else { "✗" });
    }

    // Only now is the PIN known to be right, and only now does the certificate
    // say who the holder is: the one moment worth asking.
    if typed
        && let (Ok(signer), Some(id)) = (&outcome, &card)
    {
        offer_to_remember(id, &pin, signer.certificate_der());
    }

    outcome
}

/// The PIN: explicit argument, then keyring, then a prompt.
///
/// `card` is the identifier read off the chip without a PIN, and it is the
/// keyring entry to look for: several CIEs in one keyring stay out of each
/// other's way. Without it (no card, failed read) we go straight to the prompt.
///
/// The returned flag says whether the PIN was typed by hand, i.e. whether it is
/// worth offering to remember it.
///
/// All eight digits are needed. The official middleware keeps four of them in a
/// cache on disk (`IAS::GetFirstPIN`) and asks only for the other four: that is
/// why it insists on the Java card-registration app.
pub fn resolve_pin(pin: Option<String>, card: Option<&str>) -> Result<(String, bool)> {
    let stored = card.and_then(keyring::read);

    let (pin, typed) = match (pin, stored) {
        (Some(p), _) => (p, false),
        (None, Some(entry)) => {
            match entry.holder() {
                Some(who) => println!("  PIN dal portachiavi ({who})."),
                None => println!("  PIN dal portachiavi."),
            }
            (entry.pin, false)
        }
        (None, None) => {
            print!("  Inserisci PIN (8 cifre): ");
            io::stdout().flush()?;
            (rpassword::read_password()?, true)
        }
    };

    if pin.len() != 8 || !pin.chars().all(|c| c.is_ascii_digit()) {
        anyhow::bail!("Il PIN deve essere di 8 cifre numeriche");
    }
    Ok((pin, typed))
}

/// Offers to remember the card in the keyring, after a successful verify.
///
/// The holder's details from the certificate are stored alongside the PIN: they
/// are what lets the card be recognised the moment it is put down, while the
/// certificate itself is still locked behind the PIN.
///
/// A keyring that refuses is no reason to fail: say so and move on, the
/// signature already went through.
fn offer_to_remember(card: &str, pin: &str, certificate: &[u8]) {
    print!("\n  Ricordare il PIN di questa carta nel portachiavi? [s/N] ");
    if io::stdout().flush().is_err() {
        return;
    }
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err()
        || !matches!(answer.trim(), "s" | "S" | "si" | "sì" | "y" | "Y")
    {
        return;
    }

    let holder = cie_lib::DatiTitolare::from_certificate_der(certificate).unwrap_or_default();
    let entry = CardSecret {
        pin: pin.to_string(),
        name: holder.nome,
        surname: holder.cognome,
        tax_code: holder.codice_fiscale,
    };

    match keyring::store(card, &entry) {
        Ok(()) => println!("  Salvato. Carta {card}."),
        Err(e) => println!("  Non salvato: {e:#}"),
    }
}
