//! Comandi APDU per la Carta d'Identità Elettronica
//!
//! Riferimento: specifiche CIE 3.0 e codice sorgente cie-middleware-linux

use crate::error::Result;
use crate::pcsc_direct::PcscCard;

/// AID (Application ID) della CIE - IAS (Gemalto style)
/// Usato per accedere ai file come PAN, certificati, ecc.
pub const IAS_AID: &[u8] = &[0xA0, 0x00, 0x00, 0x00, 0x30, 0x80, 0x00, 0x00, 0x00, 0x09, 0x81, 0x60, 0x01];

/// AID della CIE application
pub const CIE_AID: &[u8] = &[0xA0, 0x00, 0x00, 0x00, 0x00, 0x39];

/// Tipo di CIE basato sull'ATR
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CieType {
    Unknown,
    Gemalto,
    STM,
    NXP,
    Actalis,
    Bit4Id,
}

impl CieType {
    /// Determina il tipo di CIE dall'ATR
    pub fn from_atr(atr: &[u8]) -> Self {
        // Pattern da cercare nell'ATR (dal codice C++)
        let patterns = [
            // NXP
            (CieType::NXP, &[0x80, 0x31, 0x80, 0x65, 0x49, 0x54, 0x4E, 0x58, 0x50][..]),
            // Gemalto
            (CieType::Gemalto, &[0x80, 0x31, 0x80, 0x65, 0xB0, 0x85, 0x04, 0x00, 0x11][..]),
            (CieType::Gemalto, &[0x80, 0x31, 0x80, 0x65, 0xB0, 0x85, 0x03, 0x00, 0xEF][..]),
            // STM
            (CieType::STM, &[0x80, 0x66, 0x47, 0x50, 0x00, 0xB8, 0x00, 0x7F][..]),
            (CieType::STM, &[0x80, 0x80, 0x01, 0x01][..]),
            (CieType::STM, &[0x80, 0x01, 0x80, 0x66, 0x47, 0x50, 0x00, 0xB8, 0x00, 0x94][..]),
            // Actalis (vari pattern)
            (CieType::Actalis, &[0x80, 0x01, 0x80, 0x31, 0x80, 0x65, 0x49, 0x54, 0x4A, 0x34, 0x41][..]),
            (CieType::Actalis, &[0x80, 0x01, 0x80, 0x31, 0x80, 0x65, 0x49, 0x54, 0x4A, 0x34, 0x43][..]),
            (CieType::Actalis, &[0x80, 0x01, 0x80, 0x31, 0x80, 0x65, 0x49, 0x54, 0x4A, 0x34, 0x4C][..]),
            // Bit4Id (vari pattern)
            (CieType::Bit4Id, &[0x80, 0x01, 0x80, 0x31, 0x80, 0x65, 0x49, 0x54, 0x4A, 0x34, 0x42][..]),
            (CieType::Bit4Id, &[0x80, 0x01, 0x80, 0x31, 0x80, 0x65, 0x49, 0x54, 0x4A, 0x34, 0x44][..]),
            (CieType::Bit4Id, &[0x80, 0x01, 0x80, 0x31, 0x80, 0x65, 0x49, 0x54, 0x4A, 0x34, 0x49][..]),
        ];

        for (cie_type, pattern) in patterns {
            if Self::contains_pattern(atr, pattern) {
                return cie_type;
            }
        }

        CieType::Unknown
    }

    /// Verifica se l'ATR contiene il pattern
    fn contains_pattern(atr: &[u8], pattern: &[u8]) -> bool {
        if pattern.len() > atr.len() {
            return false;
        }
        atr.windows(pattern.len()).any(|w| w == pattern)
    }

    /// Ritorna true se la carta si prepara con **solo** SELECT MF, senza AID.
    ///
    /// `IAS::SelectAID_IAS` (IAS.cpp:346) divide le carte in due famiglie usando
    /// l'ordinamento dell'enum `CIE_Type` (ATR.h:11):
    /// `Unknown=0, Gemalto=1, STM=2, NXP=3, ACTALIS=4, BIT4ID=5`.
    ///
    /// - `type >= CIE_NXP` (NXP, Actalis, Bit4Id): SELECT MF e basta
    /// - `type < CIE_NXP` (Gemalto, STM): SELECT dell'AID IAS
    ///
    /// Mandare un SELECT AID a una carta della prima famiglia risponde 6A82 e
    /// lascia attivo il DF sbagliato per i GET DATA successivi.
    pub fn selects_mf_only(&self) -> bool {
        matches!(self, CieType::NXP | CieType::Actalis | CieType::Bit4Id)
    }
}

/// File IDs della CIE (dal codice C++)
#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum CieFileId {
    /// File IdServizi - contiene info sui servizi
    IdServizi = 0x1001,
    /// Numero seriale CIE
    SerialeCIE = 0x1002,
    /// Certificato utente CIE
    CertCIE = 0x1003,
    /// Certificato Authority / Chiave pubblica DAPP
    CertificateAuthority = 0x1004,
    /// Chiave pubblica servizi
    ServiziPubKey = 0x1005,
    /// SOD (Security Object Document)
    SOD = 0x1006,
    /// PAN (Primary Account Number) - Numero carta!
    PAN = 0xD003,
    /// DH (Diffie-Hellman)
    DH = 0xD004,
}

/// Comandi APDU per la CIE
pub struct CieApdu;

impl CieApdu {
    /// SELECT MF (Master File) - necessario per carte NXP
    /// Comando: 00 A4 00 00
    pub fn select_mf() -> Vec<u8> {
        vec![0x00, 0xA4, 0x00, 0x00]
    }

    /// SELECT IAS AID
    /// Comando: 00 A4 04 0C + IAS_AID
    pub fn select_ias() -> Vec<u8> {
        let mut apdu = vec![
            0x00, // CLA
            0xA4, // INS - SELECT
            0x04, // P1 - Select by name
            0x0C, // P2 - Nessuna risposta (importante!)
            IAS_AID.len() as u8, // Lc
        ];
        apdu.extend_from_slice(IAS_AID);
        apdu
    }

    /// SELECT CIE AID
    /// Comando: 00 A4 04 0C + CIE_AID
    pub fn select_cie() -> Vec<u8> {
        let mut apdu = vec![
            0x00, // CLA
            0xA4, // INS - SELECT
            0x04, // P1 - Select by name
            0x0C, // P2 - Nessuna risposta
            CIE_AID.len() as u8, // Lc
        ];
        apdu.extend_from_slice(CIE_AID);
        apdu
    }

    /// SELECT FILE: Seleziona un file Elementary File (EF)
    /// Comando: `00 A4 02 04 02 [file_id]`
    pub fn select_file(file_id: u16) -> Vec<u8> {
        vec![
            0x00, // CLA
            0xA4, // INS - SELECT
            0x02, // P1 - Select child EF
            0x04, // P2 - FCP in response
            0x02, // Lc
            (file_id >> 8) as u8,   // File ID MSB
            (file_id & 0xFF) as u8, // File ID LSB
        ]
    }

    /// READ BINARY: Legge dati dal file selezionato
    pub fn read_binary(offset: u16, length: u8) -> Vec<u8> {
        vec![
            0x00, // CLA
            0xB0, // INS - READ BINARY
            (offset >> 8) as u8,   // P1 - Offset MSB
            (offset & 0xFF) as u8, // P2 - Offset LSB
            length, // Le - Expected response length
        ]
    }

}

/// Helper per operazioni comuni sulla CIE
pub struct CieCard<'a> {
    card: &'a PcscCard,
    authenticated: bool,
    cie_type: CieType,
}

impl<'a> CieCard<'a> {
    /// Prepara la carta selezionando il DF corretto.
    ///
    /// Segue `IAS::SelectAID_IAS`: la famiglia della carta decide se si manda un
    /// SELECT MF o un SELECT dell'AID IAS. Sbagliare qui lascia attivo il DF
    /// sbagliato e i GET DATA successivi rispondono con dati incoerenti.
    pub fn new(card: &'a PcscCard) -> Result<Self> {
        use tracing::{debug, info};

        let atr = card.get_atr()?;
        let cie_type = CieType::from_atr(&atr);
        info!("Tipo CIE rilevato: {:?} (ATR {})", cie_type, hex::encode(&atr));

        if cie_type == CieType::Unknown {
            return Err(crate::error::CieError::Protocol(format!(
                "ATR non riconosciuto: {}", hex::encode(&atr)
            )));
        }

        // Due selezioni in sequenza, come CIEtemplateLogin (CIEP11Template.cpp:379):
        // prima l'applicazione IAS, poi quella CIE. Il PIN e i file protetti
        // vivono nel DF della CIE: fermarsi alla prima fa rispondere 6985.
        // Con dati vuoti l'APDU non porta il byte Lc: solo i 4 byte di header.
        let primo = if cie_type.selects_mf_only() {
            (CieApdu::select_mf(), "SELECT MF")
        } else {
            (CieApdu::select_ias(), "SELECT IAS AID")
        };

        for (apdu, cosa) in [primo, (CieApdu::select_cie(), "SELECT CIE AID")] {
            debug!("{} per {:?}...", cosa, cie_type);
            let (_, sw) = card.transmit_raw(&apdu)?;
            if sw != 0x9000 {
                return Err(crate::error::CieError::Card { sw });
            }
            info!("{} riuscito", cosa);
        }

        Ok(Self { card, authenticated: false, cie_type })
    }

    /// Ritorna il tipo di CIE rilevato
    pub fn cie_type(&self) -> CieType {
        self.cie_type
    }

    /// Seleziona un file
    pub fn select_file(&self, file_id: CieFileId) -> Result<Vec<u8>> {
        let apdu = CieApdu::select_file(file_id as u16);
        self.card.transmit(&apdu)
    }

    /// Seleziona un file by ID generico
    pub fn select_file_by_id(&self, file_id: u16) -> Result<Vec<u8>> {
        let apdu = CieApdu::select_file(file_id);
        // Usa transmit_raw per gestire status word diversi
        let (data, sw) = self.card.transmit_raw(&apdu)?;
        if sw == 0x9000 || (sw & 0xFF00) == 0x6200 {
            // 9000 = OK, 62xx = warning (es. 6282 = EOF)
            Ok(data)
        } else {
            Err(crate::error::CieError::Card { sw })
        }
    }

    /// Legge dati binari dal file corrente (usa transmit_raw)
    fn read_binary_raw(&self, offset: u16, length: u8) -> (Vec<u8>, u16) {
        let apdu = CieApdu::read_binary(offset, length);
        match self.card.transmit_raw(&apdu) {
            Ok((data, sw)) => (data, sw),
            Err(_) => (vec![], 0x6F00), // Errore generico
        }
    }

    /// Legge un intero file binario
    pub fn read_file(&self, file_id: CieFileId) -> Result<Vec<u8>> {
        // Prima seleziona il file
        let select_apdu = CieApdu::select_file(file_id as u16);
        let (_, sw) = self.card.transmit_raw(&select_apdu)?;

        // 9000 = OK, 62xx = warning
        if sw != 0x9000 && (sw & 0xFF00) != 0x6200 {
            return Err(crate::error::CieError::Card { sw });
        }

        self.read_current_file()
    }

    /// Legge il file attualmente selezionato (logica dal C++: gestisce 6C, 6282, 6B00)
    pub fn read_current_file(&self) -> Result<Vec<u8>> {
        use tracing::debug;

        let mut data = Vec::new();
        let mut offset = 0u16;
        let mut chunk_size: u8 = 128; // Il C++ usa 128

        loop {
            let (chunk, sw) = self.read_binary_raw(offset, chunk_size);

            debug!("READ_BINARY offset={} len={} -> sw={:04X} data_len={}", offset, chunk_size, sw, chunk.len());

            // Gestione status word (come nel C++)
            if (sw >> 8) == 0x6C {
                // 6Cxx = Wrong Le, xx è la lunghezza corretta
                let correct_len = (sw & 0xFF) as u8;
                debug!("Wrong Le, retrying with len={}", correct_len);
                let (chunk2, sw2) = self.read_binary_raw(offset, correct_len);
                if sw2 == 0x9000 || sw2 == 0x6282 {
                    data.extend_from_slice(&chunk2);
                    offset += chunk2.len() as u16;
                    if sw2 == 0x6282 {
                        // EOF
                        break;
                    }
                } else {
                    break;
                }
            } else if sw == 0x9000 {
                // OK, continua
                data.extend_from_slice(&chunk);
                offset += chunk.len() as u16;
                chunk_size = 128;
            } else if sw == 0x6282 {
                // EOF - append data e termina
                data.extend_from_slice(&chunk);
                break;
            } else if sw == 0x6B00 {
                // Wrong P1/P2 (offset oltre fine file)
                break;
            } else {
                // Altro errore
                debug!("Errore lettura: {:04X}", sw);
                break;
            }
        }

        Ok(data)
    }

    /// Legge dati binari dal file corrente (per compatibilità)
    pub fn read_binary(&self, offset: u16, length: u8) -> Result<Vec<u8>> {
        let apdu = CieApdu::read_binary(offset, length);
        self.card.transmit(&apdu)
    }

    /// Verifica se il PIN è stato inserito
    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    /// Legge il numero seriale (ID_SERVIZI - 0x1001)
    pub fn read_id_servizi(&self) -> Result<Vec<u8>> {
        self.read_file(CieFileId::IdServizi)
    }

    /// Legge il seriale CIE (0x1002)
    pub fn read_seriale(&self) -> Result<Vec<u8>> {
        self.read_file(CieFileId::SerialeCIE)
    }

    /// Legge il certificato CIE (0x1003)
    pub fn read_cert_cie(&self) -> Result<Vec<u8>> {
        self.read_file(CieFileId::CertCIE)
    }

    /// Legge il certificato CA (0x1004)
    pub fn read_cert_ca(&self) -> Result<Vec<u8>> {
        self.read_file(CieFileId::CertificateAuthority)
    }

    /// Legge il PAN (Primary Account Number - 0xD003)
    /// Contiene il numero carta. Il numero seriale visualizzato è: PAN[5..11] in hex
    pub fn read_pan(&self) -> Result<Vec<u8>> {
        self.read_file(CieFileId::PAN)
    }

    /// Estrae il numero seriale dal PAN (bytes 5-10, 6 bytes = 12 cifre hex)
    /// Questo è il numero stampato sulla carta fisica
    pub fn get_serial_from_pan(pan: &[u8]) -> Option<String> {
        if pan.len() >= 11 {
            // Prendi bytes 5-10 (6 bytes) e converti in hex
            let serial_bytes = &pan[5..11];
            Some(hex::encode(serial_bytes).to_uppercase())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_ias() {
        let apdu = CieApdu::select_ias();
        assert_eq!(apdu[0], 0x00); // CLA
        assert_eq!(apdu[1], 0xA4); // INS
        assert_eq!(apdu[2], 0x04); // P1
        assert_eq!(apdu[3], 0x0C); // P2
        assert_eq!(apdu[4] as usize, IAS_AID.len()); // Lc
        assert_eq!(&apdu[5..], IAS_AID);
    }

    #[test]
    fn test_select_cie() {
        let apdu = CieApdu::select_cie();
        assert_eq!(apdu[0], 0x00);
        assert_eq!(apdu[1], 0xA4);
        assert_eq!(apdu[2], 0x04);
        assert_eq!(apdu[3], 0x0C);
        assert_eq!(&apdu[5..], CIE_AID);
    }

    #[test]
    fn test_read_binary() {
        let apdu = CieApdu::read_binary(0, 10);
        assert_eq!(apdu[0], 0x00); // CLA
        assert_eq!(apdu[1], 0xB0); // INS
        assert_eq!(apdu[4], 10);   // Le
    }

    #[test]
    fn test_select_file() {
        let apdu = CieApdu::select_file(0x1001);
        assert_eq!(apdu[0], 0x00); // CLA
        assert_eq!(apdu[1], 0xA4); // INS
        assert_eq!(apdu[2], 0x02); // P1
        assert_eq!(apdu[3], 0x04); // P2
        assert_eq!(apdu[5], 0x10); // File ID MSB
        assert_eq!(apdu[6], 0x01); // File ID LSB
    }
}
