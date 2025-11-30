//! Comando per autenticarsi con la CIE (verifica PIN)

use anyhow::Result;
use cie_lib::{PcscContext, CieCard, CieType};
use std::io::{self, Write};

/// Esegue l'autenticazione con PIN e legge i dati protetti
pub fn execute(pin: Option<String>) -> Result<()> {
    println!("\n╔══════════════════════════════════════╗");
    println!("║       AUTENTICAZIONE CIE             ║");
    println!("╚══════════════════════════════════════╝\n");

    // Crea contesto PC/SC
    let ctx = PcscContext::new()?;

    // Connetti alla CIE
    println!("Connessione alla CIE...");
    let card = ctx.connect_to_cie()?;
    println!("✓ Connesso!\n");

    // Leggi ATR e tipo
    let atr = card.get_atr()?;
    let cie_type = CieType::from_atr(&atr);
    println!("Tipo CIE: {:?}", cie_type);

    // Inizializza CieCard
    let mut cie_card = CieCard::new(&card)?;
    println!("✓ Applicazione CIE inizializzata!\n");

    // Controlla tentativi rimanenti
    if let Some(retries) = cie_card.get_pin_retries() {
        println!("⚠ Tentativi PIN rimanenti: {}", retries);
        if retries <= 1 {
            println!("⚠ ATTENZIONE: Pochi tentativi! Se sbagli il PIN verrà bloccato!");
        }
    }

    // Ottieni il PIN (da argomento o da input nascosto)
    let pin = match pin {
        Some(p) => p,
        None => {
            print!("\nInserisci PIN (8 cifre): ");
            io::stdout().flush()?;

            // Input nascosto con rpassword
            rpassword::read_password()?
        }
    };

    // Valida formato PIN
    if pin.len() != 8 {
        println!("✗ Errore: Il PIN deve essere di 8 cifre");
        return Ok(());
    }
    if !pin.chars().all(|c| c.is_ascii_digit()) {
        println!("✗ Errore: Il PIN deve contenere solo cifre");
        return Ok(());
    }

    println!("\n═══ Verifica PIN ═══");
    match cie_card.verify_pin(&pin) {
        Ok(()) => {
            println!("✓ PIN corretto! Autenticazione riuscita.\n");

            // Ora proviamo a leggere i dati protetti
            println!("═══ Lettura Dati Protetti ═══");

            // Seriale CIE (0x1002)
            print!("  Seriale CIE (0x1002): ");
            match cie_card.read_seriale() {
                Ok(data) => {
                    if data.is_empty() {
                        println!("(vuoto)");
                    } else {
                        // Il seriale è nei primi 9 bytes come stringa
                        let serial_len = data.len().min(9);
                        if let Ok(s) = std::str::from_utf8(&data[..serial_len]) {
                            println!("{}", s.trim_end_matches('\0'));
                        } else {
                            println!("HEX: {}", hex::encode(&data));
                        }
                    }
                }
                Err(e) => println!("Errore: {}", e),
            }

            // Certificato CIE (0x1003) - per estrarre nome/cognome
            print!("  Certificato CIE (0x1003): ");
            match cie_card.read_cert_cie() {
                Ok(data) => {
                    if data.is_empty() {
                        println!("(vuoto)");
                    } else {
                        println!("{} bytes", data.len());

                        // Prova a estrarre dati dal certificato X.509
                        if let Some(dati) = cie_lib::DatiTitolare::from_certificate_der(&data) {
                            println!("\n═══ DATI TITOLARE ═══");
                            if let Some(ref nome) = dati.nome {
                                println!("  Nome: {}", nome);
                            }
                            if let Some(ref cognome) = dati.cognome {
                                println!("  Cognome: {}", cognome);
                            }
                            if let Some(ref cf) = dati.codice_fiscale {
                                println!("  Codice Fiscale: {}", cf);
                            }
                        }
                    }
                }
                Err(e) => println!("Errore: {}", e),
            }
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            if err_msg.contains("Secure Messaging") {
                println!("✗ La CIE richiede Secure Messaging per la verifica PIN.");
                println!();
                println!("  ⚠ Il protocollo Secure Messaging non è ancora implementato.");
                println!("  Questo protocollo richiede:");
                println!("    1. Scambio chiavi Diffie-Hellman con la carta");
                println!("    2. Autenticazione mutua (DAPP)");
                println!("    3. Crittografia 3DES/AES degli APDU");
                println!();
                println!("  Per ora puoi usare l'app CIE ID ufficiale per operazioni");
                println!("  che richiedono il PIN.");
            } else {
                println!("✗ {}", e);
            }
        }
    }

    println!("\n╔══════════════════════════════════════╗");
    println!("║           OPERAZIONE COMPLETATA      ║");
    println!("╚══════════════════════════════════════╝");

    Ok(())
}
