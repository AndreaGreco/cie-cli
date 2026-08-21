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
