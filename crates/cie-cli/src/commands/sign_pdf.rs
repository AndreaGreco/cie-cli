//! Comando per firmare un PDF in PAdES con la chiave on-card della CIE.
//!
//! A differenza di `cie sign`, che produce una firma separata, qui la firma
//! finisce *dentro* il PDF: il file resta un PDF, si apre normalmente, e i
//! lettori che sanno leggere le firme la mostrano.
//!
//! Il riquadro visibile sta in basso a destra. Per sceglierne uno a mano c'e'
//! `cie-gui`, che mostra la pagina e lo fa trascinare.

use anyhow::{Context, Result};
use cie_firma::{firma_pdf, OpzioniPades};
use cie_lib::PcscContext;

/// Firma un PDF con la chiave della carta e lo riscrive con la firma dentro.
pub fn execute(
    percorso: String,
    pin: Option<String>,
    uscita: Option<String>,
    pagina: u32,
    motivo: Option<String>,
    certifica: Option<u8>,
) -> Result<()> {
    let pdf = std::fs::read(&percorso).with_context(|| format!("lettura di {}", percorso))?;

    if !pdf.starts_with(b"%PDF-") {
        anyhow::bail!("{} non e' un PDF", percorso);
    }

    println!("\n╔══════════════════════════════════════╗");
    println!("║        FIRMA PAdES CON CIE           ║");
    println!("╚══════════════════════════════════════╝\n");
    println!("File: {} ({} byte)", percorso, pdf.len());

    let ctx = PcscContext::new()?;
    let firmatario = super::apri_carta(&ctx, pin)?;

    println!("\n═══ Firma ═══");
    let opzioni = OpzioniPades {
        pagina,
        motivo,
        certifica,
        ..Default::default()
    };
    let firmato = firma_pdf(&pdf, &firmatario, &opzioni)?;
    println!("  PDF firmato: {} byte", firmato.len());
    if let Some(livello) = certifica {
        let cosa = match livello {
            1 => "nessuna modifica successiva",
            3 => "moduli, firme e annotazioni",
            _ => "moduli e altre firme",
        };
        println!("  Firma di certificazione: resta permesso {}", cosa);
    }

    let out = uscita.unwrap_or_else(|| {
        percorso
            .strip_suffix(".pdf")
            .map(|base| format!("{}-firmato.pdf", base))
            .unwrap_or_else(|| format!("{}-firmato.pdf", percorso))
    });
    std::fs::write(&out, &firmato).with_context(|| format!("scrittura di {}", out))?;

    println!("\n  Scritto: {}", out);
    println!("\n  Il certificato della CIE e' di autenticazione, non di firma");
    println!("  qualificata: un validatore che controlla l'EKU puo' segnalarlo.");
    println!("\nVerifica:");
    println!("  pdfsig {}", out);

    Ok(())
}
