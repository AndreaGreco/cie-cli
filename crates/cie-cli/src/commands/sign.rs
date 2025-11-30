//! Comando per la firma di documenti

use anyhow::Result;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use super::create_cie;
use cie_lib::SigningMechanism;

pub fn execute(pkcs11_lib: Option<&str>, input: &str, output: Option<&str>) -> Result<()> {
    let cie = create_cie(pkcs11_lib)?;

    // Verifica che il file esista
    let input_path = Path::new(input);
    if !input_path.exists() {
        anyhow::bail!("File non trovato: {}", input);
    }

    // Determina il file di output
    let output_path = output
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}.sig", input));

    println!("╔══════════════════════════════════════╗");
    println!("║         FIRMA DIGITALE CIE           ║");
    println!("╚══════════════════════════════════════╝\n");

    // Trova la CIE
    let reader = match cie.find_cie() {
        Ok(r) => r,
        Err(cie_lib::CieError::NoCiePresent) => {
            println!("Nessuna CIE trovata. Inserisci la CIE nel lettore.");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    println!("CIE trovata: {}", reader.slot_info.description);
    if let Some(ref token) = reader.token_info {
        println!("Seriale: {}\n", token.serial_number);
    }

    // Leggi il file
    println!("File da firmare: {}", input);
    let data = fs::read(input_path)?;
    println!("Dimensione: {} bytes", data.len());

    // Chiedi il PIN
    let pin = rpassword::prompt_password("Inserisci il PIN della CIE: ")?;

    // Login
    println!("\nAutenticazione in corso...");
    let session = cie.login(&reader, &pin)?;
    println!("✓ Autenticazione riuscita");

    // Firma
    println!("Firma in corso...");
    let signature = cie.sign(&session, &data, SigningMechanism::Sha256RsaPkcs)?;
    println!("✓ Firma completata");

    // Scrivi la firma
    let mut file = File::create(&output_path)?;
    file.write_all(&signature)?;

    println!("\n╔══════════════════════════════════════╗");
    println!("║           FIRMA COMPLETATA           ║");
    println!("╚══════════════════════════════════════╝");
    println!("\nFile originale: {}", input);
    println!("Firma salvata:  {}", output_path);
    println!("Algoritmo:      SHA-256 with RSA PKCS#1 v1.5");
    println!("Dimensione firma: {} bytes", signature.len());

    // Logout
    session.logout()?;

    Ok(())
}
