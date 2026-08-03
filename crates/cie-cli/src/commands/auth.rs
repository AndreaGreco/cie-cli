//! Comando per autenticarsi con la CIE (verifica PIN)

use anyhow::Result;
use cie_lib::{CieCard, CieType, PcscContext, SecureMessaging};
use std::io::{self, Write};

/// Un passo dell'handshake: nome da mostrare e metodo da invocare.
type Passo = (&'static str, fn(&mut SecureMessaging, &cie_lib::PcscCard) -> cie_lib::Result<()>);


/// Esegue l'autenticazione con PIN e legge i dati protetti
pub fn execute(pin: Option<String>, dump_cert: Option<String>) -> Result<()> {
    println!("\n╔══════════════════════════════════════╗");
    println!("║       AUTENTICAZIONE CIE             ║");
    println!("╚══════════════════════════════════════╝\n");

    let ctx = PcscContext::new()?;

    println!("Connessione alla CIE...");
    let card = ctx.connect_to_cie()?;
    println!("✓ Connesso!\n");

    let atr = card.get_atr()?;
    let cie_type = CieType::from_atr(&atr);
    println!("Tipo CIE: {:?}", cie_type);

    // Seleziona IAS e poi CIE: senza la seconda la VERIFY PIN risponde 6985.
    CieCard::new(&card)?;
    println!("✓ Applicazione CIE selezionata\n");

    println!("═══ Inizializzazione Secure Messaging ═══");

    let mut sm = SecureMessaging::new(cie_type);

    // L'ordine conta: i primi due passi sono in chiaro, il canale si apre con lo
    // scambio DH, e solo dentro il canale si leggono chiave del chip e DAPP.
    let passi: [Passo; 5] = [
        ("Parametri Diffie-Hellman", SecureMessaging::init_dh_params),
        ("Parametri CA external auth", SecureMessaging::init_ext_auth_key_param),
        ("Scambio chiavi DH", SecureMessaging::dh_key_exchange),
        ("Chiave pubblica del chip", SecureMessaging::read_dapp_pub_key),
        ("Autenticazione mutua (DAPP)", SecureMessaging::dapp),
    ];

    for (i, (nome, passo)) in passi.iter().enumerate() {
        print!("  {}. {}... ", i + 1, nome);
        io::stdout().flush()?;
        match passo(&mut sm, &card) {
            Ok(()) => println!("✓"),
            Err(e) => {
                println!("✗");
                println!("\n  Errore: {}", e);
                println!("  Il canale sicuro non e' stato stabilito, mi fermo qui.");
                return Ok(());
            }
        }
    }

    println!("\n✓ Canale sicuro attivo e chip autenticato.");

    println!("\n═══ Verifica PIN ═══");

    // ponytail: niente pre-check del contatore tentativi. La VERIFY con campo
    // dati vuoto risponde 6988 su questa carta, e farebbe avanzare l'SSC del
    // comando appena prima della verifica vera. I tentativi rimasti arrivano
    // comunque dentro il 63Cx quando il PIN e' sbagliato.
    //
    // Il PIN si chiede solo ora: se il canale non si apre, non serve digitarlo.
    // A differenza del middleware ufficiale qui servono tutte e 8 le cifre: il
    // C++ ne tiene 4 in una cache su disco (IAS::GetFirstPIN) e chiede solo le
    // altre 4, ed e' il motivo per cui pretende l'app Java di registrazione.
    let pin = super::ottieni_pin(pin)?;

    match sm.verify_pin(&card, &pin) {
        Ok(()) => println!("  ✓ PIN corretto, autenticazione riuscita.\n"),
        Err(e) => {
            println!("  ✗ {}\n", e);
            return Ok(());
        }
    }

    println!("═══ Lettura Dati Protetti ═══");

    print!("  Seriale CIE (0x1002): ");
    match sm.read_file(&card, 0x1002) {
        Ok(data) if data.is_empty() => println!("(vuoto)"),
        Ok(data) => {
            let fino = data.len().min(9);
            match std::str::from_utf8(&data[..fino]) {
                Ok(s) => println!("{}", s.trim_end_matches('\0')),
                Err(_) => println!("HEX {}", hex::encode(&data)),
            }
        }
        Err(e) => println!("errore: {}", e),
    }

    print!("  Certificato CIE (0x1003): ");
    match sm.read_file(&card, 0x1003) {
        Ok(data) if data.is_empty() => println!("(vuoto)"),
        Ok(data) => {
            println!("{} byte", data.len());
            if let Some(percorso) = &dump_cert {
                std::fs::write(percorso, &data)?;
                println!("  Certificato salvato in {}", percorso);
            }
            if let Some(dati) = cie_lib::DatiTitolare::from_certificate_der(&data) {
                println!("\n═══ DATI TITOLARE ═══");
                for (etichetta, valore) in [
                    ("Nome", &dati.nome),
                    ("Cognome", &dati.cognome),
                    ("Codice Fiscale", &dati.codice_fiscale),
                ] {
                    if let Some(v) = valore {
                        println!("  {}: {}", etichetta, v);
                    }
                }
            }
        }
        Err(e) => println!("errore: {}", e),
    }

    println!("\n╔══════════════════════════════════════╗");
    println!("║        OPERAZIONE COMPLETATA         ║");
    println!("╚══════════════════════════════════════╝");

    Ok(())
}
