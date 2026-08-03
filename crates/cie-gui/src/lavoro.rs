//! La carta lavora fuori dal thread della finestra.
//!
//! L'handshake costa circa un secondo, la firma qualcosa in piu': farlo dentro
//! `update()` congelerebbe la finestra abbastanza da farla dichiarare morta al
//! compositore.

use anyhow::{Context, Result};
use cie_firma::{firma_detached, firma_p7m, firma_pdf, OpzioniPades, Passo};
use cie_lib::PcscContext;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    /// PAdES: la firma finisce dentro il PDF.
    Pades,
    /// CAdES enveloping: documento e firma in un `.p7m`.
    P7m,
    /// Firma RSA grezza a fianco del file, piu' il certificato.
    Detached,
}

impl Tipo {
    pub fn etichetta(self) -> &'static str {
        match self {
            Tipo::Pades => "PAdES — la firma dentro il PDF",
            Tipo::P7m => "CAdES (.p7m) — documento e firma in un unico file",
            Tipo::Detached => "Firma separata (.sig) — piu' il certificato",
        }
    }
}

/// Tutto quello che serve alla firma, gia' di proprieta' del thread.
pub struct Richiesta {
    pub byte: Arc<Vec<u8>>,
    pub tipo: Tipo,
    pub pin: String,
    pub uscita: PathBuf,
    pub opzioni: OpzioniPades,
}

pub enum Messaggio {
    /// A che punto siamo, da mostrare cosi' com'e'.
    Passo(String),
    /// Fatto: il file scritto (per la firma separata, quello della firma).
    Fatto(PathBuf),
    /// La catena di errori, gia' formattata.
    Errore(String),
}

/// Avvia la firma su un thread e restituisce il canale da svuotare in `update`.
pub fn avvia(richiesta: Richiesta) -> Receiver<Messaggio> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let esito = firma(&richiesta, &tx);
        let _ = match esito {
            Ok(percorso) => tx.send(Messaggio::Fatto(percorso)),
            Err(e) => tx.send(Messaggio::Errore(format!("{e:#}"))),
        };
    });
    rx
}

fn firma(r: &Richiesta, tx: &Sender<Messaggio>) -> Result<PathBuf> {
    let avanzamento = |p: Passo| {
        let testo = match p {
            Passo::Connessione => "Connessione alla carta…",
            Passo::CanaleSicuro => "Apertura del canale sicuro…",
            Passo::Pin => "Verifica del PIN…",
            Passo::Certificato => "Lettura del certificato…",
        };
        let _ = tx.send(Messaggio::Passo(testo.to_string()));
    };

    let ctx = PcscContext::new()?;
    let firmatario = cie_firma::apri_e_autentica(&ctx, &r.pin, &avanzamento)?;

    let _ = tx.send(Messaggio::Passo("Firma del documento…".to_string()));

    let prodotto = match r.tipo {
        Tipo::Pades => firma_pdf(&r.byte, &firmatario, &r.opzioni)?,
        Tipo::P7m => firma_p7m(&r.byte, &firmatario)?,
        Tipo::Detached => {
            let firma = firma_detached(&r.byte, &firmatario)?;
            // Senza certificato la firma separata e' solo un blocco di byte:
            // chi deve verificarla non ha con cosa.
            let cert = r.uscita.with_extension("cer");
            std::fs::write(&cert, cie_firma::CryptoSigner::certificate_der(&firmatario))
                .with_context(|| format!("scrittura di {}", cert.display()))?;
            firma
        }
    };

    std::fs::write(&r.uscita, &prodotto)
        .with_context(|| format!("scrittura di {}", r.uscita.display()))?;

    Ok(r.uscita.clone())
}

/// Il percorso proposto per il file firmato, secondo il tipo di firma.
///
/// Sono gli stessi suffissi della CLI: i due strumenti devono produrre gli
/// stessi nomi, altrimenti si finisce con due copie dello stesso documento.
pub fn uscita_predefinita(origine: &std::path::Path, tipo: Tipo) -> PathBuf {
    match tipo {
        Tipo::Pades => {
            let gambo = origine
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "documento".to_string());
            origine.with_file_name(format!("{gambo}-firmato.pdf"))
        }
        // `.p7m` e `.sig` si appendono al nome intero, estensione compresa:
        // `contratto.pdf.p7m` dice ancora che dentro c'e' un PDF.
        Tipo::P7m => aggiungi_estensione(origine, "p7m"),
        Tipo::Detached => aggiungi_estensione(origine, "sig"),
    }
}

fn aggiungi_estensione(percorso: &std::path::Path, ext: &str) -> PathBuf {
    let mut nome = percorso.file_name().unwrap_or_default().to_os_string();
    nome.push(".");
    nome.push(ext);
    percorso.with_file_name(nome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn i_nomi_proposti_sono_quelli_della_cli() {
        let f = Path::new("/tmp/contratto.pdf");
        assert_eq!(
            uscita_predefinita(f, Tipo::Pades),
            Path::new("/tmp/contratto-firmato.pdf")
        );
        assert_eq!(
            uscita_predefinita(f, Tipo::P7m),
            Path::new("/tmp/contratto.pdf.p7m")
        );
        assert_eq!(
            uscita_predefinita(f, Tipo::Detached),
            Path::new("/tmp/contratto.pdf.sig")
        );
    }

    /// Un file senza estensione non deve perdere il nome per strada.
    #[test]
    fn regge_anche_un_file_senza_estensione() {
        let f = Path::new("/tmp/lettera");
        assert_eq!(
            uscita_predefinita(f, Tipo::P7m),
            Path::new("/tmp/lettera.p7m")
        );
        assert_eq!(
            uscita_predefinita(f, Tipo::Pades),
            Path::new("/tmp/lettera-firmato.pdf")
        );
    }
}
