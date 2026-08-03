//! Comando per firmare un file con la chiave on-card della CIE.

use anyhow::{Context, Result};
use cie_firma::{firma_detached, CryptoSigner};
use cie_lib::PcscContext;
use std::path::Path;

/// Firma il digest SHA-256 di un file con la chiave della carta.
///
/// Produce una firma RSA grezza (PKCS#1 v1.5, 256 byte), non un contenitore
/// CAdES o PAdES: e' il mattone su cui quelli si costruiscono.
pub fn execute(percorso: String, pin: Option<String>, uscita: Option<String>) -> Result<()> {
    let contenuto = std::fs::read(&percorso).with_context(|| format!("lettura di {}", percorso))?;

    println!("\n╔══════════════════════════════════════╗");
    println!("║           FIRMA CON CIE              ║");
    println!("╚══════════════════════════════════════╝\n");
    println!("File:   {} ({} byte)", percorso, contenuto.len());
    println!("SHA256: {}", hex::encode(cie_lib::crypto::sha256(&contenuto)));

    let ctx = PcscContext::new()?;
    let firmatario = super::apri_carta(&ctx, pin)?;

    println!("\n═══ Firma ═══");
    // `firma_detached` ricontrolla il risultato con la pubblica del
    // certificato: se torna un Ok, la firma e' gia' stata verificata.
    let firma = firma_detached(&contenuto, &firmatario)?;
    println!("  Firma prodotta: {} byte", firma.len());
    println!("  ✓ Verificata con la chiave pubblica del certificato");

    let out_firma = uscita.unwrap_or_else(|| format!("{}.sig", percorso));
    std::fs::write(&out_firma, &firma)
        .with_context(|| format!("scrittura di {}", out_firma))?;
    println!("\n  Firma detached (RSA grezza): {}", out_firma);

    // Il certificato serve a chiunque debba verificare: senza, la firma e'
    // solo un blocco di byte.
    let out_cert = format!("{}.cer", Path::new(&percorso).display());
    std::fs::write(&out_cert, firmatario.certificate_der())
        .with_context(|| format!("scrittura di {}", out_cert))?;
    println!("  Certificato:                {}", out_cert);

    println!("\n  Il file originale non e' stato modificato: questa e' una firma");
    println!("  separata, non un documento firmato. Per un contenitore che tenga");
    println!("  insieme documento e firma: `cie sign-p7m` (CAdES) o `cie sign-pdf`");
    println!("  (PAdES, solo per i PDF).");

    println!("\nVerifica con OpenSSL:");
    println!("  openssl x509 -inform DER -in {} -pubkey -noout > pub.pem", out_cert);
    println!("  openssl dgst -sha256 -verify pub.pem -signature {} {}", out_firma, percorso);

    Ok(())
}
