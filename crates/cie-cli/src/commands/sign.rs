//! Comando per firmare un file con la chiave on-card della CIE

use anyhow::{Context, Result};
use cie_lib::{secure_messaging, CieCard, CieType, PcscContext};
use cie_lib::titolare::ChiavePubblicaRsa;
use std::path::Path;

/// Firma il digest SHA-256 di un file con la chiave della carta.
///
/// Produce una firma RSA grezza (PKCS#1 v1.5, 256 byte), non un contenitore
/// CAdES o PAdES: e' il mattone su cui quelli si costruiscono.
pub fn execute(percorso: String, pin: Option<String>, uscita: Option<String>) -> Result<()> {
    let contenuto = std::fs::read(&percorso)
        .with_context(|| format!("lettura di {}", percorso))?;

    let hash = cie_lib::crypto::sha256(&contenuto);

    println!("\n╔══════════════════════════════════════╗");
    println!("║           FIRMA CON CIE              ║");
    println!("╚══════════════════════════════════════╝\n");
    println!("File:   {} ({} byte)", percorso, contenuto.len());
    println!("SHA256: {}", hex::encode(hash));

    let ctx = PcscContext::new()?;
    let card = ctx.connect_to_cie()?;
    let cie_type = CieType::from_atr(&card.get_atr()?);
    CieCard::new(&card)?;

    print!("\nApertura del canale sicuro... ");
    let mut sm = secure_messaging::establish_secure_messaging(&card, cie_type)?;
    println!("✓");

    println!("\n═══ Verifica PIN ═══");
    let pin = super::ottieni_pin(pin)?;
    sm.verify_pin(&card, &pin)?;
    println!("  ✓ PIN corretto");

    println!("\n═══ Firma ═══");
    let firma = sm.sign_sha256(&card, &hash)?;
    println!("  Firma prodotta: {} byte", firma.len());

    // Verifica in locale con la pubblica del certificato: se non torna, il
    // problema e' nostro e non ha senso mandarla a un validatore esterno.
    let certificato = sm.read_file(&card, 0x1003)?;
    let pubblica = ChiavePubblicaRsa::from_certificate_der(&certificato)?;

    if pubblica.verifica_sha256(&hash, &firma) {
        println!("  ✓ Verificata con la chiave pubblica del certificato");
    } else {
        println!("  ✗ La firma NON verifica: qualcosa non torna nella catena");
        anyhow::bail!("verifica locale fallita");
    }

    let out_firma = uscita.unwrap_or_else(|| format!("{}.sig", percorso));
    std::fs::write(&out_firma, &firma)
        .with_context(|| format!("scrittura di {}", out_firma))?;
    println!("\n  Firma detached (RSA grezza): {}", out_firma);

    // Il certificato serve a chiunque debba verificare: senza, la firma e'
    // solo un blocco di byte.
    let out_cert = format!("{}.cer", Path::new(&percorso).display());
    std::fs::write(&out_cert, &certificato)
        .with_context(|| format!("scrittura di {}", out_cert))?;
    println!("  Certificato:                {}", out_cert);

    println!("\n  Il file originale non e' stato modificato: questa e' una firma");
    println!("  separata, non un documento firmato. Per un PDF che si apra con la");
    println!("  firma incorporata serve un contenitore PAdES, non ancora previsto.");

    println!("\nVerifica con OpenSSL:");
    println!("  openssl x509 -inform DER -in {} -pubkey -noout > pub.pem", out_cert);
    println!("  openssl dgst -sha256 -verify pub.pem -signature {} {}", out_firma, percorso);

    Ok(())
}
