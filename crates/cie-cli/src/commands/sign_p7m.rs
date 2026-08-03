//! Comando per firmare un file qualsiasi in CAdES con la chiave on-card.
//!
//! A differenza di `cie sign`, che produce una firma RSA grezza, e di
//! `cie sign-pdf`, che vale solo per i PDF, qui il file *e* la firma finiscono
//! insieme in un unico DER: e' il `.p7m` della prassi italiana.

use anyhow::{Context, Result};
use cie_firma::firma_p7m;
use cie_lib::PcscContext;

/// Firma un file con la chiave della carta e lo riscrive dentro un `.p7m`.
pub fn execute(percorso: String, pin: Option<String>, uscita: Option<String>) -> Result<()> {
    let contenuto = std::fs::read(&percorso).with_context(|| format!("lettura di {}", percorso))?;

    println!("\n╔══════════════════════════════════════╗");
    println!("║        FIRMA CAdES (.p7m)            ║");
    println!("╚══════════════════════════════════════╝\n");
    println!("File: {} ({} byte)", percorso, contenuto.len());

    let ctx = PcscContext::new()?;
    let firmatario = super::apri_carta(&ctx, pin)?;

    println!("\n═══ Firma ═══");
    let busta = firma_p7m(&contenuto, &firmatario)?;
    println!("  Busta CAdES: {} byte", busta.len());
    println!("  ✓ Il contenuto originale si rilegge dalla busta");

    let out = uscita.unwrap_or_else(|| format!("{}.p7m", percorso));
    std::fs::write(&out, &busta).with_context(|| format!("scrittura di {}", out))?;

    println!("\n  Scritto: {}", out);
    println!("\n  Il certificato della CIE e' di autenticazione, non di firma");
    println!("  qualificata: un validatore che controlla l'EKU puo' segnalarlo.");
    println!("\nVerifica:");
    println!("  openssl cms -verify -in {} -inform DER -noverify", out);

    Ok(())
}
