//! Moduli dei comandi CLI

use anyhow::Result;
use cie_firma::{FirmatarioCie, Passo};
use cie_lib::PcscContext;
use std::cell::Cell;
use std::io::{self, Write};
use std::process::Command;

pub mod auth;
pub mod info;
pub mod pcsc_test;
pub mod sign;
pub mod sign_p7m;
pub mod sign_pdf;

/// Apre la carta stampando l'avanzamento, con la spunta a fine riga.
///
/// Il PIN si risolve *prima* di toccare il lettore: se manca, tanto vale
/// scoprirlo senza aver gia' aperto un canale sicuro.
pub fn apri_carta(ctx: &PcscContext, pin: Option<String>) -> Result<FirmatarioCie> {
    let pin = ottieni_pin(pin)?;

    // La riga precedente si chiude quando arriva il passo successivo: il
    // callback sa cosa sta per iniziare, non cosa e' finito.
    let aperta = Cell::new(false);
    let stampa = |p: Passo| {
        if aperta.get() {
            println!("✓");
        }
        let testo = match p {
            Passo::Connessione => "Connessione alla carta...",
            Passo::CanaleSicuro => "Apertura del canale sicuro...",
            Passo::Pin => "Verifica del PIN...",
            Passo::Certificato => "Lettura del certificato...",
        };
        print!("  {} ", testo);
        let _ = io::stdout().flush();
        aperta.set(true);
    };

    println!();
    let esito = cie_firma::apri_e_autentica(ctx, &pin, &stampa);
    if aperta.get() {
        println!("{}", if esito.is_ok() { "✓" } else { "✗" });
    }
    esito
}

/// Recupera il PIN dal portachiavi di sistema (`secret-tool lookup service cie`).
///
/// Delega al binario di libsecret invece di parlare D-Bus: e' gia' installato su
/// qualunque desktop, e qui serve una sola chiamata. Il PIN non passa mai per la
/// riga di comando ne' per la history della shell.
///
/// Per popolare la voce:
/// `secret-tool store --label='CIE PIN' service cie`
///
/// Qualsiasi intoppo (binario assente, voce mancante, portachiavi bloccato)
/// ricade silenziosamente sul prompt interattivo.
fn pin_da_portachiavi() -> Option<String> {
    let out = Command::new("secret-tool")
        .args(["lookup", "service", "cie"])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let pin = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!pin.is_empty()).then_some(pin)
}

/// Ottiene il PIN: argomento esplicito, poi portachiavi, poi prompt a video.
///
/// Servono tutte e 8 le cifre. Il middleware ufficiale ne tiene 4 in una cache
/// su disco (`IAS::GetFirstPIN`) e chiede solo le altre 4: e' il motivo per cui
/// pretende l'app Java di registrazione della carta.
pub fn ottieni_pin(pin: Option<String>) -> Result<String> {
    let pin = match pin {
        Some(p) => p,
        None => match pin_da_portachiavi() {
            Some(p) => {
                println!("  PIN recuperato dal portachiavi di sistema.");
                p
            }
            None => {
                print!("  Inserisci PIN (8 cifre): ");
                io::stdout().flush()?;
                rpassword::read_password()?
            }
        },
    };

    if pin.len() != 8 || !pin.chars().all(|c| c.is_ascii_digit()) {
        anyhow::bail!("Il PIN deve essere di 8 cifre numeriche");
    }
    Ok(pin)
}
