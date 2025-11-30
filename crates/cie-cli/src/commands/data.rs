//! Comando per mostrare i dati del titolare della CIE

use anyhow::Result;
use super::create_cie;

pub fn execute(pkcs11_lib: Option<&str>) -> Result<()> {
    let cie = create_cie(pkcs11_lib)?;

    let reader = match cie.find_cie() {
        Ok(r) => r,
        Err(cie_lib::CieError::NoCiePresent) => {
            println!("Nessuna CIE trovata. Inserisci la CIE nel lettore.");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    println!("Lettura dati dalla CIE...\n");

    // Leggi i certificati pubblici (senza PIN)
    let certificates = cie.read_public_certificates(&reader)?;

    if certificates.is_empty() {
        println!("Nessun certificato trovato sulla CIE.");
        println!("Potrebbe essere necessario inserire il PIN per accedere ai dati.");
        return Ok(());
    }

    println!("╔══════════════════════════════════════╗");
    println!("║        DATI DEL TITOLARE CIE         ║");
    println!("╚══════════════════════════════════════╝\n");

    let mut found_data = false;

    for cert in &certificates {
        if let Some(dati) = cert.dati_titolare() {
            found_data = true;

            println!("Certificato: {}", cert.label);
            println!("─────────────────────────────────────");

            if let Some(ref cognome) = dati.cognome {
                println!("  Cognome:        {}", cognome);
            }
            if let Some(ref nome) = dati.nome {
                println!("  Nome:           {}", nome);
            }
            if let Some(ref cf) = dati.codice_fiscale {
                println!("  Codice Fiscale: {}", cf);
            }
            if let Some(ref num) = dati.numero_cie {
                println!("  Numero CIE:     {}", num);
            }
            if let Some(ref cn) = dati.nome_completo {
                println!("  Subject CN:     {}", cn);
            }
            println!();
        }
    }

    if !found_data {
        println!("Nessun dato del titolare trovato nei certificati.");
        println!("\nCertificati disponibili:");
        for cert in &certificates {
            println!("  - {} (ID: {})", cert.label, cert.id_hex());
            println!("    Dimensione: {} bytes", cert.der_value.len());
        }
    }

    Ok(())
}
