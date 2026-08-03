//! Moduli dei comandi CLI

use anyhow::Result;
use std::io::{self, Write};
use std::process::Command;

pub mod auth;
pub mod info;
pub mod pcsc_test;
pub mod sign;

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
