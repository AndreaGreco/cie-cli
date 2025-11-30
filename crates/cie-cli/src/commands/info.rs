//! Comando per mostrare informazioni sulla CIE tramite PC/SC diretto

use anyhow::Result;
use cie_lib::{PcscContext, CieCard, CieType};

pub fn execute() -> Result<()> {
    println!("\n╔══════════════════════════════════════╗");
    println!("║      INFORMAZIONI CIE (PC/SC)        ║");
    println!("╚══════════════════════════════════════╝\n");

    // Crea contesto PC/SC
    let ctx = PcscContext::new()?;

    // Lista lettori
    println!("Lettori disponibili:");
    let readers = ctx.list_readers()?;
    if readers.is_empty() {
        println!("✗ Nessun lettore trovato!");
        return Ok(());
    }
    for (i, reader) in readers.iter().enumerate() {
        println!("  {}. {}", i + 1, reader);
    }

    // Connetti alla CIE
    println!("\nConnessione alla CIE...");
    let card = ctx.connect_to_cie()?;
    println!("✓ Connesso!\n");

    // Leggi ATR
    println!("═══ ATR (Answer To Reset) ═══");
    let atr = card.get_atr()?;
    println!("  {}", hex::encode(&atr));
    println!("  Lunghezza: {} bytes", atr.len());

    // Determina tipo CIE dall'ATR
    let cie_type = CieType::from_atr(&atr);
    println!("  Tipo CIE: {:?}\n", cie_type);

    // Inizializza comunicazione CIE (seleziona IAS/CIE AID)
    println!("═══ Selezione Applicazione CIE ═══");
    let cie_card = CieCard::new(&card)?;
    println!("✓ Applicazione CIE inizializzata!\n");

    // Prova a leggere i file della CIE
    println!("═══ Lettura File CIE ═══");

    // 1. ID Servizi (0x1001) - Identificativo servizi (pubblico)
    print!("  IdServizi (0x1001): ");
    match cie_card.read_id_servizi() {
        Ok(data) => {
            println!("{} bytes", data.len());
            if !data.is_empty() {
                // Prova a interpretare come stringa ASCII
                if let Ok(s) = std::str::from_utf8(&data) {
                    println!("    ID: {}", s.trim_end_matches('\0'));
                } else {
                    println!("    HEX: {}", hex::encode(&data));
                }
            }
        }
        Err(e) => println!("Errore: {}", e),
    }

    // 2. Seriale CIE (0x1002) - Richiede PIN
    print!("  Seriale (0x1002): ");
    match cie_card.read_seriale() {
        Ok(data) => {
            if data.is_empty() {
                println!("(richiede PIN)");
            } else {
                println!("{} bytes", data.len());
                if let Ok(s) = std::str::from_utf8(&data) {
                    println!("    Testo: {}", s.trim_end_matches('\0'));
                } else {
                    println!("    HEX: {}", hex::encode(&data));
                }
            }
        }
        Err(e) => println!("Errore: {}", e),
    }

    // 3. Certificato CIE (0x1003) - Richiede PIN
    print!("  CertCIE (0x1003): ");
    match cie_card.read_cert_cie() {
        Ok(data) => {
            if data.is_empty() {
                println!("(richiede PIN)");
            } else {
                println!("{} bytes", data.len());
                println!("    Primi 32 bytes: {}", hex::encode(&data[..data.len().min(32)]));

                // Prova a estrarre dati dal certificato X.509
                if let Some(dati) = cie_lib::DatiTitolare::from_certificate_der(&data) {
                    if dati.nome.is_some() || dati.cognome.is_some() || dati.codice_fiscale.is_some() {
                        println!("    ═══ Dati Titolare ═══");
                        if let Some(ref nome) = dati.nome {
                            println!("      Nome: {}", nome);
                        }
                        if let Some(ref cognome) = dati.cognome {
                            println!("      Cognome: {}", cognome);
                        }
                        if let Some(ref cf) = dati.codice_fiscale {
                            println!("      CF: {}", cf);
                        }
                    }
                }
            }
        }
        Err(e) => println!("Errore: {}", e),
    }

    // 4. Certificato CA (0x1004) - Pubblico
    print!("  CertCA (0x1004): ");
    match cie_card.read_cert_ca() {
        Ok(data) => {
            println!("{} bytes", data.len());
            if !data.is_empty() {
                // Il CertCA potrebbe essere una chiave pubblica RSA
                // Inizia con 30 82 01 0a = SEQUENCE, 266 bytes
                if data[0] == 0x30 {
                    println!("    Tipo: Struttura ASN.1 (SEQUENCE)");
                }
                println!("    Primi 32 bytes: {}", hex::encode(&data[..data.len().min(32)]));
            }
        }
        Err(e) => println!("Errore: {}", e),
    }

    // 5. PAN - Primary Account Number (0xD003) - Pubblico
    print!("  PAN (0xD003): ");
    match cie_card.read_pan() {
        Ok(data) => {
            println!("{} bytes", data.len());
            if !data.is_empty() {
                println!("    HEX: {}", hex::encode(&data));

                // Estrai il numero seriale dal PAN (bytes 5-10)
                if let Some(serial) = cie_lib::apdu::CieCard::get_serial_from_pan(&data) {
                    println!("    Seriale (da PAN): {}", serial);
                }
            }
        }
        Err(e) => println!("Errore: {}", e),
    }    println!("\n╔══════════════════════════════════════╗");
    println!("║           LETTURA COMPLETATA         ║");
    println!("╚══════════════════════════════════════╝");

    Ok(())
}
