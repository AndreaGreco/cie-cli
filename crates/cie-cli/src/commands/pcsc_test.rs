//! Comando per testare la comunicazione PC/SC diretta

use anyhow::Result;
use cie_lib::{PcscContext, CieCard, CieFileId};

pub fn execute() -> Result<()> {
    println!("\n╔══════════════════════════════════════╗");
    println!("║    TEST COMUNICAZIONE PC/SC DIRETTA  ║");
    println!("╚══════════════════════════════════════╝\n");

    // Crea contesto PC/SC
    println!("Inizializzazione PC/SC...");
    let ctx = PcscContext::new()?;

    // Lista lettori
    println!("\nLettori disponibili:");
    let readers = ctx.list_readers()?;
    for (i, reader) in readers.iter().enumerate() {
        println!("  {}. {}", i + 1, reader);
    }

    if readers.is_empty() {
        println!("\n✗ Nessun lettore trovato!");
        return Ok(());
    }

    // Connetti alla CIE
    println!("\nConnessione alla CIE...");
    let card = ctx.connect_to_cie()?;
    println!("✓ Connesso!");

    // Leggi ATR
    println!("\nATR (Answer To Reset):");
    let atr = card.get_atr()?;
    println!("  {}", hex::encode(&atr));
    println!("  Lunghezza: {} bytes", atr.len());

    // Inizializza comunicazione CIE (seleziona applicazione)
    println!("\nInizializzazione comunicazione CIE...");
    let cie_card = CieCard::new(&card)?;
    println!("✓ Applicazione CIE selezionata!");

    // Test lettura file
    println!("\n═══ Test Lettura File ═══");

    // Test tutti i file ID noti
    let files = [
        (CieFileId::IdServizi, "IdServizi"),
        (CieFileId::SerialeCIE, "SerialeCIE"),
        (CieFileId::CertCIE, "CertCIE"),
        (CieFileId::CertificateAuthority, "CertificateAuthority"),
        (CieFileId::ServiziPubKey, "ServiziPubKey"),
        (CieFileId::SOD, "SOD"),
        (CieFileId::DH, "DH"),
    ];

    for (file_id, name) in files {
        print!("  {} (0x{:04X}): ", name, file_id as u16);

        match cie_card.read_file(file_id) {
            Ok(data) => {
                println!("{} bytes", data.len());
                if !data.is_empty() && data.len() <= 64 {
                    println!("    HEX: {}", hex::encode(&data));
                } else if !data.is_empty() {
                    println!("    HEX: {}...", hex::encode(&data[..32]));
                }
            }
            Err(e) => {
                println!("Errore: {}", e);
            }
        }
    }

    println!("\n╔══════════════════════════════════════╗");
    println!("║         TEST COMPLETATO!             ║");
    println!("╚══════════════════════════════════════╝");

    Ok(())
}
