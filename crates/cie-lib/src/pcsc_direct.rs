//! Comunicazione diretta con la CIE tramite PC/SC
//!
//! Questo modulo implementa la comunicazione diretta con la Carta d'Identità Elettronica
//! usando lo standard PC/SC, bypassando completamente il middleware CIE.

use std::time::Duration;

use pcsc::*;
use tracing::{debug, info, warn};

use crate::error::{CieError, Result};

/// Context PC/SC per la comunicazione con la CIE
pub struct PcscContext {
    ctx: Context,
}

impl PcscContext {
    /// Crea un nuovo contesto PC/SC
    pub fn new() -> Result<Self> {
        info!("Inizializzazione contesto PC/SC");
        let ctx = Context::establish(Scope::User)
            .map_err(|e| CieError::Transport(format!("contesto PC/SC: {:?}", e)))?;
        Ok(Self { ctx })
    }

    /// Lista tutti i lettori disponibili
    pub fn list_readers(&self) -> Result<Vec<String>> {
        let mut readers_buf = [0; 2048];
        let readers = self.ctx.list_readers(&mut readers_buf)
            .map_err(|_| CieError::NoReaderFound)?;

        let reader_names: Vec<String> = readers
            .map(|r| r.to_str().unwrap_or("").to_string())
            .filter(|s| !s.is_empty())
            .collect();

        debug!("Trovati {} lettori", reader_names.len());
        Ok(reader_names)
    }

    /// Connette a un lettore specifico e prende la carta in esclusiva.
    ///
    /// La transazione PC/SC resta aperta per tutta la vita della connessione:
    /// il canale sicuro dura decine di APDU con un contatore che avanza a ogni
    /// comando, e un SELECT di qualcun altro in mezzo (un'altra nostra
    /// connessione, OpenSC, il browser) lo romperebbe senza appello. Chi arriva
    /// dopo aspetta, o si prende un `SharingViolation` invece di sporcarci la
    /// sessione.
    ///
    /// Si chiude da sola: `SCardDisconnect`, che il `Drop` di `pcsc::Card`
    /// chiama con `ResetCard`, rilascia anche la transazione. Il reset e' voluto:
    /// cancella il PIN verificato e le chiavi di sessione dalla carta.
    pub fn connect(&self, reader: &str) -> Result<PcscCard> {
        info!("Connessione al lettore: {}", reader);

        let reader_cstr = std::ffi::CString::new(reader)
            .map_err(|_| CieError::NoReaderFound)?;

        let mut card = self.ctx.connect(&reader_cstr, ShareMode::Shared, Protocols::ANY)
            .map_err(from_pcsc)?;
        take_exclusive(&mut card)?;
        ready(&card)?;
        Ok(PcscCard { card })
    }

    /// Trova e connette alla prima CIE disponibile.
    ///
    /// Riprova per un po' se la carta non e' ancora pronta: chiudere la
    /// connessione precedente l'ha resettata, e un lettore NFC per resettarla
    /// spegne e riaccende il campo. Chi firma due volte di seguito arriva
    /// proprio in quella finestra.
    pub fn connect_to_cie(&self) -> Result<PcscCard> {
        let mut attempt = 1;
        loop {
            match self.connect_once() {
                Err(e) if e.is_transient() && attempt < ATTEMPTS => {
                    debug!("carta non pronta ({}), tentativo {} di {}", e, attempt, ATTEMPTS);
                    std::thread::sleep(PAUSE * attempt);
                    attempt += 1;
                }
                outcome => return outcome,
            }
        }
    }

    fn connect_once(&self) -> Result<PcscCard> {
        let readers = self.list_readers()?;

        if readers.is_empty() {
            return Err(CieError::NoReaderFound);
        }

        // Fra i lettori senza carta, uno con la carta che si sta svegliando:
        // e' il suo errore quello che conta, perche' e' l'unico che passa.
        let mut last = CieError::NoCiePresent;
        for reader in readers {
            debug!("Tentativo connessione a: {}", reader);
            match self.connect(&reader) {
                Ok(card) => {
                    info!("Connesso al lettore: {}", reader);
                    return Ok(card);
                }
                Err(e) => {
                    debug!("Connessione fallita: {:?}", e);
                    if !matches!(e, CieError::NoCiePresent) {
                        last = e;
                    }
                }
            }
        }

        Err(last)
    }
}

/// Quante volte provare a connettersi prima di dire che la carta non c'e'.
const ATTEMPTS: u32 = 5;

/// La pausa fra un tentativo e l'altro, che cresce a ogni giro: in tutto
/// fanno circa due secondi, abbastanza per un lettore NFC che riattiva la carta.
const PAUSE: Duration = Duration::from_millis(200);

/// Apre la transazione e ne butta la guardia.
///
/// `pcsc::Transaction` prende in prestito la `Card`, e quindi non puo' vivere
/// nella stessa struct: dimenticarla tiene aperta la transazione senza tenere
/// il prestito, e la chiude la disconnessione.
///
/// `ResetCard` qui vuol dire che qualcuno ha resettato la carta dopo la nostra
/// connessione: basta riconnettersi, la carta e' la stessa.
fn take_exclusive(card: &mut Card) -> Result<()> {
    for _ in 0..ATTEMPTS {
        match card.transaction2() {
            Ok(tx) => {
                std::mem::forget(tx);
                return Ok(());
            }
            Err((card, Error::ResetCard)) => {
                debug!("carta resettata prima della transazione, riconnessione");
                card.reconnect(ShareMode::Shared, Protocols::ANY, Disposition::LeaveCard)
                    .map_err(from_pcsc)?;
            }
            Err((_, e)) => return Err(from_pcsc(e)),
        }
    }
    Err(CieError::NotReady("la carta continua a essere resettata".into()))
}

/// La carta ha finito di accendersi.
///
/// Guardato prima di mandare qualunque APDU: una carta ancora a meta' del
/// reset risponde al primo comando con un errore di trasporto, e a quel punto
/// non si sa piu' se il comando sia arrivato o no.
///
/// `POWERED` e non `SPECIFIC`: pcsc-lite riporta una carta pronta come
/// `PRESENT | POWERED | NEGOTIABLE` anche dopo la connessione, e chiedere il
/// protocollo negoziato la dava per non pronta per sempre.
fn ready(card: &Card) -> Result<()> {
    let status = card.status2_owned().map_err(from_pcsc)?.status();
    if status.contains(Status::PRESENT | Status::POWERED) {
        Ok(())
    } else {
        Err(CieError::NotReady(format!("stato del lettore {status:?}")))
    }
}

/// Traduce un errore PC/SC distinguendo quello che passa da solo.
fn from_pcsc(e: Error) -> CieError {
    match e {
        Error::NoSmartcard => CieError::NoCiePresent,
        Error::ResetCard
        | Error::RemovedCard
        | Error::UnpoweredCard
        | Error::UnresponsiveCard
        | Error::SharingViolation => CieError::NotReady(format!("{e:?}")),
        e => CieError::Transport(format!("{e:?}")),
    }
}

/// Rappresenta una connessione attiva con una CIE
pub struct PcscCard {
    card: Card,
}

impl PcscCard {
    /// Invia un comando APDU alla carta e riceve la risposta
    pub fn transmit(&self, apdu: &[u8]) -> Result<Vec<u8>> {
        let (data, sw) = self.transmit_raw(apdu)?;

        if sw == 0x9000 {
            Ok(data)
        } else {
            warn!("Status word non OK: {:04X}", sw);
            Err(CieError::Card { sw })
        }
    }

    /// Invia un comando APDU e ritorna data + status word
    /// Questo metodo non genera errore per status word diversi da 9000
    pub fn transmit_raw(&self, apdu: &[u8]) -> Result<(Vec<u8>, u16)> {
        debug!("TX: {}", hex::encode(apdu));

        let mut response_buf = [0; MAX_BUFFER_SIZE];
        let response = self.card.transmit(apdu, &mut response_buf)
            .map_err(|e| CieError::Transport(format!("{:?}", e)))?;

        debug!("RX: {}", hex::encode(response));

        // Verifica lo status word (ultimi 2 bytes)
        if response.len() < 2 {
            return Err(CieError::Protocol("Risposta APDU troppo corta: manca lo status word".into()));
        }

        let data = response[..response.len() - 2].to_vec();
        let sw1 = response[response.len() - 2];
        let sw2 = response[response.len() - 1];
        let sw = ((sw1 as u16) << 8) | (sw2 as u16);

        Ok((data, sw))
    }

    /// Ottiene l'ATR (Answer To Reset) della carta
    pub fn get_atr(&self) -> Result<Vec<u8>> {
        let mut atr_buf = [0; MAX_ATR_SIZE];
        let atr = self.card.get_attribute(Attribute::AtrString, &mut atr_buf)
            .map_err(|e| CieError::Transport(format!("lettura ATR: {:?}", e)))?;

        debug!("ATR: {}", hex::encode(atr));
        Ok(atr.to_vec())
    }

    /// Invia APDU gestendo automaticamente GET RESPONSE (61xx) e retry (6Cxx)
    pub fn transmit_auto(&self, apdu: &[u8]) -> Result<(Vec<u8>, u16)> {
        let (mut data, mut sw) = self.transmit_raw(apdu)?;

        // Se SW1=0x61, ci sono più dati da leggere con GET RESPONSE
        while (sw >> 8) == 0x61 {
            let le = (sw & 0xFF) as u8;
            let get_response = vec![0x00, 0xC0, 0x00, 0x00, le];
            let (more_data, new_sw) = self.transmit_raw(&get_response)?;
            data.extend_from_slice(&more_data);
            sw = new_sw;
        }

        // Se SW1=0x6C, riprova con Le corretto
        if (sw >> 8) == 0x6C && apdu.len() >= 5 {
            let le = (sw & 0xFF) as u8;
            let mut new_apdu = apdu.to_vec();
            // Imposta Le corretto (ultimo byte)
            if let Some(last) = new_apdu.last_mut() {
                *last = le;
            }
            return self.transmit_raw(&new_apdu);
        }

        Ok((data, sw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Richiede carta fisica
    fn test_list_readers() {
        let ctx = PcscContext::new().unwrap();
        let readers = ctx.list_readers().unwrap();
        println!("Lettori trovati: {:?}", readers);
        assert!(!readers.is_empty());
    }

    #[test]
    #[ignore] // Richiede carta fisica
    fn test_connect() {
        let ctx = PcscContext::new().unwrap();
        let card = ctx.connect_to_cie().unwrap();
        let atr = card.get_atr().unwrap();
        println!("ATR: {}", hex::encode(&atr));
        assert!(!atr.is_empty());
    }
}
