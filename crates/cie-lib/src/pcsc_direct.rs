//! Comunicazione diretta con la CIE tramite PC/SC
//!
//! Questo modulo implementa la comunicazione diretta con la Carta d'Identità Elettronica
//! usando lo standard PC/SC, bypassando completamente il middleware CIE.

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

    /// Connette a un lettore specifico
    pub fn connect(&self, reader: &str) -> Result<PcscCard> {
        info!("Connessione al lettore: {}", reader);

        let reader_cstr = std::ffi::CString::new(reader)
            .map_err(|_| CieError::NoReaderFound)?;

        let card = self.ctx.connect(&reader_cstr, ShareMode::Shared, Protocols::ANY)
            .map_err(|_| CieError::NoCiePresent)?;        Ok(PcscCard { card })
    }

    /// Trova e connette alla prima CIE disponibile
    pub fn connect_to_cie(&self) -> Result<PcscCard> {
        let readers = self.list_readers()?;

        if readers.is_empty() {
            return Err(CieError::NoReaderFound);
        }

        for reader in readers {
            debug!("Tentativo connessione a: {}", reader);
            match self.connect(&reader) {
                Ok(card) => {
                    info!("Connesso al lettore: {}", reader);
                    return Ok(card);
                }
                Err(e) => {
                    debug!("Connessione fallita: {:?}", e);
                    continue;
                }
            }
        }

        Err(CieError::NoCiePresent)
    }
}

/// Cosa c'e' sul lettore, senza aprire nulla.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CardStatus {
    #[default]
    NoReader,
    NoCard,
    Present,
}

/// Sorveglia lettori e carte, cosi' quello che viene collegato dopo l'avvio
/// non passa inosservato.
///
/// PC/SC blocca finche' non succede qualcosa invece di farci fare polling, e lo
/// pseudo-lettore `\\?PnP?\Notification` estende la stessa cosa ai lettori.
pub struct ReaderWatch {
    ctx: Context,
    states: Vec<ReaderState>,
}

impl ReaderWatch {
    pub fn new() -> Result<Self> {
        let ctx = Context::establish(Scope::User)
            .map_err(|e| CieError::Transport(format!("contesto PC/SC: {:?}", e)))?;
        Ok(Self {
            ctx,
            states: vec![ReaderState::new(PNP_NOTIFICATION(), State::UNAWARE)],
        })
    }

    /// Blocca finche' lettori o carta non cambiano, poi dice cosa c'e' adesso.
    ///
    /// La prima chiamata torna subito: non sappiamo ancora niente, quindi la
    /// situazione attuale e' gia' di per se' un cambiamento.
    pub fn wait(&mut self) -> Result<CardStatus> {
        self.states
            .retain(|s| !s.event_state().intersects(State::UNKNOWN | State::IGNORE));

        // `list_readers` protesta invece di restituire una lista vuota quando
        // di lettori non ce n'e' nessuno, che qui e' solo una delle risposte.
        let mut buf = [0; 2048];
        if let Ok(names) = self.ctx.list_readers(&mut buf) {
            for name in names {
                if !self.states.iter().any(|s| s.name() == name) {
                    self.states.push(ReaderState::new(name, State::UNAWARE));
                }
            }
        }
        for s in &mut self.states {
            s.sync_current_state();
        }

        self.ctx
            .get_status_change(None, &mut self.states)
            .map_err(|e| CieError::Transport(format!("attesa sul lettore: {:?}", e)))?;

        Ok(self.status())
    }

    /// Se il lettore c'e' lo chiediamo di nuovo, invece di dedurlo dagli stati.
    ///
    /// Un lettore staccato resta nella lista fino al giro dopo: PC/SC lo marca
    /// morto, non lo toglie. Dedurlo da li' vorrebbe dire rispondere "nessuna
    /// carta" a chi il lettore l'ha appena sfilato.
    fn status(&self) -> CardStatus {
        let mut buf = [0; 2048];
        let readers = self.ctx.list_readers(&mut buf).map_or(0, Iterator::count);
        if readers == 0 {
            return CardStatus::NoReader;
        }

        if self.states.iter().any(|s| s.event_state().contains(State::PRESENT)) {
            CardStatus::Present
        } else {
            CardStatus::NoCard
        }
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

    /// La prima `wait()` deve tornare subito con la situazione attuale: e' su
    /// questo che si regge la barra di stato appena la finestra si apre.
    #[test]
    #[ignore] // Richiede lettore fisico
    fn test_watch_reports_at_once() {
        let mut watch = ReaderWatch::new().unwrap();
        let status = watch.wait().unwrap();
        println!("stato iniziale: {:?}", status);
        assert_ne!(status, CardStatus::NoReader, "lettore non visto");
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
