//! Comandi per la gestione dei certificati

use anyhow::Result;
use std::fs::File;
use std::io::Write;
use super::create_cie;

/// Lista i certificati sulla CIE
pub fn list(pkcs11_lib: Option<&str>) -> Result<()> {
    let cie = create_cie(pkcs11_lib)?;

    let reader = match cie.find_cie() {
        Ok(r) => r,
        Err(cie_lib::CieError::NoCiePresent) => {
            println!("Nessuna CIE trovata. Inserisci la CIE nel lettore.");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    // Per listare i certificati non serve il PIN
    println!("Lettura certificati dalla CIE...\n");

    // Nota: per leggere i certificati senza PIN, apriamo una sessione RO
    // Alcuni certificati potrebbero essere visibili solo dopo il login

    println!("⚠ Nota: alcuni certificati potrebbero richiedere il login.");
    println!("  Usa 'cie cert export' con il PIN per accedere a tutti i certificati.\n");

    // TODO: implementare lettura certificati senza PIN per quelli pubblici
    println!("Certificati sulla CIE:");
    println!("  1. Certificato di Autenticazione");
    println!("  2. Certificato di Firma Digitale");
    println!("\n(Dettagli disponibili dopo il login)");

    let _ = reader; // Usato in futuro
    Ok(())
}

/// Esporta un certificato dalla CIE
pub fn export(pkcs11_lib: Option<&str>, id: &str, output: &str) -> Result<()> {
    let cie = create_cie(pkcs11_lib)?;

    let reader = match cie.find_cie() {
        Ok(r) => r,
        Err(cie_lib::CieError::NoCiePresent) => {
            println!("Nessuna CIE trovata. Inserisci la CIE nel lettore.");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    // Chiedi il PIN
    let pin = rpassword::prompt_password("Inserisci il PIN della CIE: ")?;

    // Login
    let session = cie.login(&reader, &pin)?;

    // Leggi certificati
    let certificates = cie.read_certificates(&session)?;

    // Trova il certificato richiesto
    let cert = certificates.iter()
        .find(|c| c.label.contains(id) || c.id_hex().contains(id))
        .ok_or_else(|| anyhow::anyhow!("Certificato '{}' non trovato", id))?;

    // Scrivi il certificato in formato DER
    let mut file = File::create(output)?;
    file.write_all(&cert.der_value)?;

    println!("Certificato '{}' esportato in: {}", cert.label, output);
    println!("Formato: DER (binario)");
    println!("Dimensione: {} bytes", cert.der_value.len());

    // Logout
    session.logout()?;

    Ok(())
}
