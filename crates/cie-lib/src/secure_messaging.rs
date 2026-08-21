//! Secure Messaging per CIE
//!
//! Implementa il protocollo di Secure Messaging della CIE che include:
//! - Scambio chiavi Diffie-Hellman
//! - Autenticazione mutua (DAPP)
//! - Cifratura/decifratura APDU con 3DES
//! - Calcolo MAC per integrità

use crate::error::{CieError, Result};
use crate::crypto::{TripleDes, RetailMac, derive_session_keys, iso_pad, sha256, rsa_raw};
use crate::asn1::{Asn1Node, asn1_tag};
use crate::pcsc_direct::PcscCard;
use crate::apdu::CieType;
use tracing::{debug, info};

/// Modulo RSA del certificato end-entity DAPP.
/// Generata da cie-middleware-linux/cie-pkcs11/CSP/IAS.cpp (256 byte).
const DEF_MODULE: &[u8] = &[
    0xBA, 0x28, 0x37, 0xAB, 0x4C, 0x6B, 0xB8, 0x27, 0x57, 0x7B, 0xFF, 0x4E, 0xB7, 0xB1, 0xE4, 0x9C,
    0xDD, 0xE0, 0xF1, 0x66, 0x14, 0xD1, 0xEF, 0x24, 0xC1, 0xB7, 0x5C, 0xF7, 0x0F, 0xB1, 0x2C, 0xD1,
    0x8F, 0x4D, 0x14, 0xE2, 0x81, 0x4B, 0xA4, 0x87, 0x7E, 0xA8, 0x00, 0xE1, 0x75, 0x90, 0x60, 0x76,
    0xB5, 0x62, 0xBA, 0x53, 0x59, 0x73, 0xC5, 0xD8, 0xB3, 0x78, 0x05, 0x1D, 0x8A, 0xFC, 0x74, 0x07,
    0xA1, 0xD9, 0x19, 0x52, 0x9E, 0x03, 0xC1, 0x06, 0xCD, 0xA1, 0x8D, 0x69, 0x9A, 0xFB, 0x0D, 0x8A,
    0xB4, 0xFD, 0xDD, 0x9D, 0xC7, 0x19, 0x15, 0x9A, 0x50, 0xDE, 0x94, 0x68, 0xF0, 0x2A, 0xB1, 0x03,
    0xE2, 0x82, 0xA5, 0x0E, 0x71, 0x6E, 0xC2, 0x3C, 0xDA, 0x5B, 0xFC, 0x4A, 0x23, 0x2B, 0x09, 0xA4,
    0xB2, 0xC7, 0x07, 0x45, 0x93, 0x95, 0x49, 0x09, 0x9B, 0x44, 0x83, 0xCB, 0xAE, 0x62, 0xD0, 0x09,
    0x96, 0x74, 0xDB, 0xF6, 0xF3, 0x9B, 0x72, 0x23, 0xA9, 0x9D, 0x88, 0xE3, 0x3F, 0x1A, 0x0C, 0xDE,
    0xDE, 0xEB, 0xBD, 0xC3, 0x55, 0x17, 0xAB, 0xE9, 0x88, 0x0A, 0xAB, 0x24, 0x0E, 0x1E, 0xA1, 0x66,
    0x28, 0x3A, 0x27, 0x4A, 0x9A, 0xD9, 0x3B, 0x4B, 0x1D, 0x19, 0xF3, 0x67, 0x9F, 0x3E, 0x8B, 0x5F,
    0xF6, 0xA1, 0xE0, 0xED, 0x73, 0x6E, 0x84, 0xD5, 0xAB, 0xE0, 0x3C, 0x59, 0xE7, 0x34, 0x6B, 0x42,
    0x18, 0x75, 0x5D, 0x75, 0x36, 0x6C, 0xBF, 0x41, 0x36, 0xF0, 0xA2, 0x6C, 0x3D, 0xC7, 0x0A, 0x69,
    0xAB, 0xAA, 0xF6, 0x6E, 0x13, 0xA1, 0xB2, 0xFA, 0xAD, 0x05, 0x2C, 0xA6, 0xEC, 0x9C, 0x51, 0xE2,
    0xAE, 0xD1, 0x4D, 0x16, 0xE0, 0x90, 0x25, 0x4D, 0xC3, 0xF6, 0x4E, 0xA2, 0xBD, 0x8A, 0x83, 0x6B,
    0xBA, 0x99, 0xDE, 0xFA, 0xCB, 0xA3, 0xA6, 0x13, 0xAE, 0xED, 0xD9, 0x3A, 0x96, 0x15, 0x27, 0x3D,
];

/// Esponente pubblico (65537).
/// Generata da cie-middleware-linux/cie-pkcs11/CSP/IAS.cpp (4 byte).
const DEF_PUBEXP: &[u8] = &[
    0x00, 0x01, 0x00, 0x01,
];

/// Esponente privato del certificato end-entity DAPP.
/// Generata da cie-middleware-linux/cie-pkcs11/CSP/IAS.cpp (256 byte).
const DEF_PRIVEXP: &[u8] = &[
    0x47, 0x16, 0xC2, 0xA3, 0x8C, 0xCC, 0x7A, 0x07, 0xB4, 0x15, 0xEB, 0x1A, 0x61, 0x75, 0xF2, 0xAA,
    0xA0, 0xE4, 0x9C, 0xEA, 0xF1, 0xBA, 0x75, 0xCB, 0xA0, 0x9A, 0x68, 0x4B, 0x04, 0xD8, 0x11, 0x18,
    0x79, 0xD3, 0xE2, 0xCC, 0xD8, 0xB9, 0x4D, 0x3C, 0x5C, 0xF6, 0xC5, 0x57, 0x53, 0xF0, 0xED, 0x95,
    0x87, 0x91, 0x0B, 0x3C, 0x77, 0x25, 0x8A, 0x01, 0x46, 0x0F, 0xE8, 0x4C, 0x2E, 0xDE, 0x57, 0x64,
    0xEE, 0xBE, 0x9C, 0x37, 0xFB, 0x95, 0xCD, 0x69, 0xCE, 0xAF, 0x09, 0xF4, 0xB1, 0x35, 0x7C, 0x27,
    0x63, 0x14, 0xAB, 0x43, 0xEC, 0x5B, 0x3C, 0xEF, 0xB0, 0x40, 0x3F, 0x86, 0x8F, 0x68, 0x8E, 0x2E,
    0xC0, 0x9A, 0x49, 0x73, 0xE9, 0x87, 0x75, 0x6F, 0x8D, 0xA7, 0xA1, 0x01, 0xA2, 0xCA, 0x75, 0xA5,
    0x4A, 0x8C, 0x4C, 0xCF, 0x9A, 0x1B, 0x61, 0x47, 0xE4, 0xDE, 0x56, 0x42, 0x3A, 0xF7, 0x0B, 0x20,
    0x67, 0x17, 0x9C, 0x5E, 0xEB, 0x64, 0x68, 0x67, 0x86, 0x34, 0x78, 0xD7, 0x52, 0xC7, 0xF4, 0x12,
    0xDB, 0x27, 0x75, 0x41, 0x57, 0x5A, 0xA0, 0x61, 0x9D, 0x30, 0xBC, 0xCC, 0x8D, 0x87, 0xE6, 0x17,
    0x0B, 0x33, 0x43, 0x9A, 0x2C, 0x93, 0xF2, 0xD9, 0x7E, 0x18, 0xC0, 0xA8, 0x23, 0x43, 0xA6, 0x01,
    0x2A, 0x5B, 0xB1, 0x82, 0x28, 0x08, 0xF0, 0x1B, 0x5C, 0xFD, 0x85, 0x67, 0x3A, 0xC0, 0x96, 0x4C,
    0x5F, 0x3C, 0xFD, 0x2D, 0xAF, 0x81, 0x42, 0x35, 0x97, 0x64, 0xA9, 0xAD, 0xB9, 0xE3, 0xF7, 0x6D,
    0xB6, 0x13, 0x46, 0x1C, 0x1B, 0xC9, 0x13, 0xDC, 0x9A, 0xC0, 0xAB, 0x50, 0xD3, 0x65, 0xF7, 0x7C,
    0xB9, 0x31, 0x94, 0xC9, 0x8A, 0xA9, 0x66, 0xD8, 0x9C, 0xDD, 0x55, 0x51, 0x25, 0xA5, 0xE5, 0x9E,
    0xCF, 0x4F, 0xA3, 0xF0, 0xC3, 0xFD, 0x61, 0x0C, 0xD3, 0xD0, 0x56, 0x43, 0x93, 0x38, 0xFD, 0x81,
];

/// Esponente privato della CA per il certificato end-entity DAPP.
/// Generata da cie-middleware-linux/cie-pkcs11/CSP/ExtAuthKey.cpp (256 byte).
/// Il modulo e l'esponente pubblico corrispondenti si leggono dalla carta
/// ([`SecureMessaging::init_ext_auth_key_param`]), questa e' l'unica costante CA.
const EXT_AUTH_PRIVEXP: &[u8] = &[
    0x18, 0x6B, 0x31, 0x48, 0x8C, 0x25, 0xDC, 0xF8, 0x5D, 0x95, 0x3D, 0x36, 0x30, 0xC0, 0xD0, 0x73,
    0xBA, 0x1C, 0x6A, 0xA2, 0x45, 0x81, 0xAD, 0x25, 0x4F, 0x3B, 0x67, 0x19, 0xC5, 0xD7, 0x2C, 0xCA,
    0x3E, 0x5C, 0xDC, 0x5A, 0x1E, 0x53, 0x16, 0x57, 0x8D, 0x75, 0x95, 0x4F, 0xF7, 0x3B, 0x23, 0x7B,
    0x53, 0x2C, 0x9F, 0x8D, 0xE4, 0xA2, 0xC4, 0xC9, 0x11, 0x38, 0x5A, 0x23, 0xE6, 0x3E, 0x33, 0xE4,
    0x7E, 0xE4, 0x5E, 0x66, 0xEF, 0xD4, 0x9B, 0x18, 0xE0, 0x2C, 0xFF, 0x87, 0x59, 0x8C, 0x39, 0x10,
    0x9E, 0x8F, 0x86, 0xA6, 0x6B, 0xC3, 0x30, 0x24, 0x9C, 0xE3, 0xFC, 0xAD, 0x65, 0x5D, 0xCD, 0xBF,
    0x98, 0xC9, 0xC5, 0xE4, 0x79, 0x32, 0x1A, 0xF5, 0x3B, 0x51, 0x7D, 0x04, 0x10, 0x61, 0x88, 0x0A,
    0x64, 0x7B, 0xBE, 0x0F, 0xF8, 0x13, 0x68, 0x34, 0x70, 0xE6, 0xC5, 0x00, 0x94, 0xCE, 0x81, 0xD0,
    0x64, 0xE2, 0x04, 0xE3, 0x51, 0xBD, 0x3A, 0xE0, 0xA7, 0x94, 0x7D, 0x8E, 0x91, 0xC3, 0xFD, 0x5C,
    0x0A, 0x15, 0x23, 0x3C, 0x34, 0x9A, 0x52, 0x15, 0xA4, 0xE6, 0x6E, 0x21, 0xC5, 0xD3, 0x34, 0x98,
    0xE7, 0x19, 0x91, 0xEA, 0x24, 0x47, 0x3B, 0x29, 0xF1, 0x47, 0x5F, 0x6F, 0xD9, 0xBE, 0x39, 0x96,
    0xE1, 0x9B, 0xD4, 0x74, 0xFA, 0xD1, 0xB4, 0x1E, 0xA0, 0xDC, 0xD2, 0xFC, 0x16, 0xC9, 0xBF, 0xFA,
    0x07, 0x1B, 0xFE, 0xC1, 0xB2, 0x24, 0x15, 0x18, 0x48, 0x11, 0xC1, 0x98, 0x5F, 0xBF, 0xE3, 0xE7,
    0xB4, 0xF4, 0x4A, 0x4B, 0x3C, 0x8D, 0xFA, 0xB4, 0xD9, 0x0C, 0xEC, 0xFC, 0x5E, 0x60, 0x8D, 0x67,
    0x3E, 0x67, 0x62, 0xC6, 0x2C, 0xB7, 0x98, 0x34, 0x12, 0x71, 0x14, 0x9B, 0xA6, 0x88, 0x16, 0x2E,
    0xC7, 0xD0, 0xE3, 0x46, 0x8F, 0x65, 0xA9, 0x4A, 0xB4, 0xAD, 0x1A, 0xB6, 0x7E, 0x37, 0xBF, 0xC1,
];

/// Key id della chiave di external authentication (CIE_KEY_ExtAuth_ID).
const CIE_KEY_EXT_AUTH_ID: u8 = 0x84;

/// Key id della chiave di firma on-card (CIE_KEY_Sign_ID).
const CIE_KEY_SIGN_ID: u8 = 0x81;

/// Contesto Secure Messaging
pub struct SecureMessaging {
    /// Chiave di sessione per cifratura (16 bytes)
    sess_enc: [u8; 16],
    /// Chiave di sessione per MAC (16 bytes)
    sess_mac: [u8; 16],
    /// Sequence Counter (8 bytes, incrementato ad ogni APDU)
    ssc: [u8; 8],
    /// SM attivo
    active: bool,
    /// Tipo CIE (per variazioni nel protocollo)
    cie_type: CieType,
    /// Parametri DH dalla carta
    dh_g: Vec<u8>,
    dh_p: Vec<u8>,
    dh_q: Vec<u8>,
    /// Chiave pubblica DAPP dalla carta
    dapp_pub_key: Vec<u8>,
    /// Modulo RSA del chip, letto insieme alla chiave DAPP dal file 0x1004.
    dapp_module: Vec<u8>,
    /// La nostra chiave pubblica DH, serve nell'hash del DAPP.
    dh_pub_key: Vec<u8>,
    /// Chiave pubblica DH del chip, idem.
    dh_icc_pub_key: Vec<u8>,
    /// Parametri della CA di external authentication, letti dalla carta.
    ca_module: Vec<u8>,
    ca_pubexp: Vec<u8>,
    ca_car: Vec<u8>,
    ca_aid: Vec<u8>,
}

impl SecureMessaging {
    /// Crea un nuovo contesto SM non inizializzato
    pub fn new(cie_type: CieType) -> Self {
        Self {
            sess_enc: [0u8; 16],
            sess_mac: [0u8; 16],
            ssc: [0u8; 8],
            active: false,
            cie_type,
            dh_g: Vec::new(),
            dh_p: Vec::new(),
            dh_q: Vec::new(),
            dapp_pub_key: Vec::new(),
            dapp_module: Vec::new(),
            dh_pub_key: Vec::new(),
            dh_icc_pub_key: Vec::new(),
            ca_module: Vec::new(),
            ca_pubexp: Vec::new(),
            ca_car: Vec::new(),
            ca_aid: Vec::new(),
        }
    }

    /// Verifica se SM è attivo
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Inizializza parametri DH dalla carta
    pub fn init_dh_params(&mut self, card: &PcscCard) -> Result<()> {
        info!("Inizializzazione parametri DH...");

        match self.cie_type {
            CieType::Gemalto => {
                // Gemalto: un singolo GET DATA per tutti i parametri
                let get_dh_data = vec![0x4D, 0x08, 0x70, 0x06, 0xBF, 0xA1, 0x01, 0x02, 0xA3, 0x80];
                let mut apdu = vec![0x00, 0xCB, 0x3F, 0xFF, get_dh_data.len() as u8];
                apdu.extend_from_slice(&get_dh_data);
                apdu.push(0x00);

                let (resp, sw) = card.transmit_raw(&apdu)?;
                if sw != 0x9000 {
                    return Err(CieError::Card { sw });
                }

                let asn = Asn1Node::parse(&resp)?;
                if let Some(node) = asn.find_tag_recursive(0xA3) {
                    for child in &node.children {
                        match child.tag {
                            0x81 => self.dh_g = child.content.clone(),
                            0x82 => self.dh_p = child.content.clone(),
                            0x83 => self.dh_q = child.content.clone(),
                            _ => {}
                        }
                    }
                }
            }
            _ => {
                // NXP, STM, Bit4Id, Actalis: tre GET DATA separati
                // GET g (generatore) - tag 0x97
                let get_g = vec![0x4D, 0x0A, 0x70, 0x08, 0xBF, 0xA1, 0x01, 0x04, 0xA3, 0x02, 0x97, 0x00];
                let mut apdu_g = vec![0x00, 0xCB, 0x3F, 0xFF, get_g.len() as u8];
                apdu_g.extend_from_slice(&get_g);
                apdu_g.push(0x00);

                let (resp_g, sw) = card.transmit_auto(&apdu_g)?;
                if sw != 0x9000 {
                    return Err(CieError::Card { sw });
                }
                self.dh_g = self.extract_dh_param_tag(&resp_g, 0x97)?;
                debug!("DH g: {} bytes", self.dh_g.len());

                // GET p (modulo) - tag 0x98
                let get_p = vec![0x4D, 0x0A, 0x70, 0x08, 0xBF, 0xA1, 0x01, 0x04, 0xA3, 0x02, 0x98, 0x00];
                let mut apdu_p = vec![0x00, 0xCB, 0x3F, 0xFF, get_p.len() as u8];
                apdu_p.extend_from_slice(&get_p);
                apdu_p.push(0x00);

                let (resp_p, sw) = card.transmit_auto(&apdu_p)?;
                if sw != 0x9000 {
                    return Err(CieError::Card { sw });
                }
                self.dh_p = self.extract_dh_param_tag(&resp_p, 0x98)?;
                debug!("DH p: {} bytes", self.dh_p.len());

                // GET q (ordine) - tag 0x99
                let get_q = vec![0x4D, 0x0A, 0x70, 0x08, 0xBF, 0xA1, 0x01, 0x04, 0xA3, 0x02, 0x99, 0x00];
                let mut apdu_q = vec![0x00, 0xCB, 0x3F, 0xFF, get_q.len() as u8];
                apdu_q.extend_from_slice(&get_q);
                apdu_q.push(0x00);

                let (resp_q, sw) = card.transmit_auto(&apdu_q)?;
                if sw != 0x9000 {
                    return Err(CieError::Card { sw });
                }
                self.dh_q = self.extract_dh_param_tag(&resp_q, 0x99)?;
                debug!("DH q: {} bytes", self.dh_q.len());
            }
        }

        info!("DH params: g={} bytes, p={} bytes, q={} bytes",
            self.dh_g.len(), self.dh_p.len(), self.dh_q.len());

        if self.dh_p.is_empty() {
            return Err(CieError::Protocol("Parametri DH non trovati nella risposta".into()));
        }

        Ok(())
    }

    /// Estrae un parametro DH dalla risposta a GET DATA.
    ///
    /// Il C++ naviga per posizione (`tags[0]->tags[0]->tags[0]->tags[0]`, IAS.cpp:1058):
    /// il valore sta in fondo a una catena di tag costruiti. Qui si cerca prima il
    /// tag atteso e si ricade sulla discesa posizionale, cosi' la struttura esatta
    /// dell'incapsulamento non e' vincolante.
    ///
    /// La versione precedente cercava il byte del tag con una scansione lineare sui
    /// dati grezzi: su un modulo da 256 byte casuali un falso positivo era quasi
    /// certo, e produceva parametri DH silenziosamente sbagliati.
    fn extract_dh_param_tag(&self, data: &[u8], target_tag: u32) -> Result<Vec<u8>> {
        let asn = Asn1Node::parse(data)?;

        if let Some(node) = asn.find_tag_recursive(target_tag)
            && !node.content.is_empty() {
                return Ok(node.content.clone());
            }

        // Fallback: scendi sempre sul primo figlio finche' ce n'e' uno.
        let mut nodo = &asn;
        while let Some(primo) = nodo.children.first() {
            nodo = primo;
        }
        if nodo.content.is_empty() {
            return Err(CieError::Protocol(format!(
                "Parametro DH {:04X} non trovato nella risposta", target_tag
            )));
        }
        Ok(nodo.content.clone())
    }

    /// Legge modulo e chiave pubblica DAPP del chip dal file 0x1004.
    ///
    /// Va chiamata **dopo** [`Self::dh_key_exchange`]: il file si legge dentro il
    /// canale SM (`IAS::ReadDappPubKey` -> `readfile` -> `SendAPDU_SM`).
    /// Il contenuto e' una SEQUENCE di due INTEGER: modulo ed esponente.
    pub fn read_dapp_pub_key(&mut self, card: &PcscCard) -> Result<()> {
        info!("Lettura chiave pubblica DAPP (file 0x1004)...");

        let data = self.read_file(card, 0x1004)?;
        let asn = Asn1Node::parse(&data)?;

        if asn.children.len() < 2 {
            return Err(CieError::Protocol(
                "File 0x1004: attesa una SEQUENCE con modulo ed esponente".into(),
            ));
        }

        // as_integer() toglie gli zeri di padding ASN.1, come il while del C++.
        self.dapp_module = asn.children[0].as_integer();
        self.dapp_pub_key = asn.children[1].as_integer();

        if self.dapp_module.is_empty() || self.dapp_pub_key.is_empty() {
            return Err(CieError::Protocol("Chiave DAPP vuota".into()));
        }

        debug!("DAPP: modulo {} byte, esponente {} byte",
            self.dapp_module.len(), self.dapp_pub_key.len());
        Ok(())
    }

    /// Esegue lo scambio chiavi DH con la carta
    pub fn dh_key_exchange(&mut self, card: &PcscCard) -> Result<()> {
        use num_bigint::BigUint;
        use rand::RngExt;

        info!("Scambio chiavi DH...");

        if self.dh_p.is_empty() || self.dh_g.is_empty() {
            return Err(CieError::Protocol("Parametri DH non inizializzati: chiamare prima init_dh_params".into()));
        }

        // 1. Genera chiave privata DH casuale (stessa lunghezza di p)
        let mut rng = rand::rng();
        let mut dh_priv_key = vec![0u8; self.dh_p.len()];
        rng.fill(&mut dh_priv_key[..]);
        // Assicura che sia dispari (come fa il middleware C++)
        if let Some(last) = dh_priv_key.last_mut() {
            *last |= 1;
        }

        // 2. Calcola chiave pubblica: g^priv mod p
        let g = BigUint::from_bytes_be(&self.dh_g);
        let p = BigUint::from_bytes_be(&self.dh_p);
        let priv_key = BigUint::from_bytes_be(&dh_priv_key);
        let dh_pub_key = g.modpow(&priv_key, &p);
        let mut dh_pub_key_bytes = dh_pub_key.to_bytes_be();

        // Padding a sinistra per avere la stessa lunghezza di p
        while dh_pub_key_bytes.len() < self.dh_p.len() {
            dh_pub_key_bytes.insert(0, 0);
        }

        debug!("DH g: {} bytes", self.dh_g.len());
        debug!("DH p: {} bytes", self.dh_p.len());
        debug!("DH pub key generata: {} bytes", dh_pub_key_bytes.len());

        // 3. MSE SET per inviare la nostra chiave pubblica alla carta
        // Algoritmo: 0x9B (DH key agreement con 3DES)
        // Key ID: 0x81 (CIE_KEY_DH_ID)
        let algo = asn1_tag(0x80, &[0x9B]);
        let key_id = asn1_tag(0x83, &[0x81]);
        let pub_key_tag = asn1_tag(0x91, &dh_pub_key_bytes);

        let mut mse_data = Vec::new();
        mse_data.extend_from_slice(&algo);
        mse_data.extend_from_slice(&key_id);
        mse_data.extend_from_slice(&pub_key_tag);

        debug!("MSE data size: {} bytes", mse_data.len());

        // Costruisci APDU MSE SET
        let mut mse_apdu = vec![0x00, 0x22, 0x41, 0xA6];
        // Se i dati sono > 255, usa extended APDU
        if mse_data.len() > 255 {
            mse_apdu.push(0x00);
            mse_apdu.push((mse_data.len() >> 8) as u8);
            mse_apdu.push((mse_data.len() & 0xFF) as u8);
        } else {
            mse_apdu.push(mse_data.len() as u8);
        }
        mse_apdu.extend_from_slice(&mse_data);

        debug!("MSE APDU: {} bytes", mse_apdu.len());

        // Se la carta rifiuta la nostra chiave pubblica non c'e' canale: fermarsi qui
        // evita di proseguire con chiavi di sessione che la carta non conosce.
        let (_, sw) = card.transmit_auto(&mse_apdu)?;
        if sw != 0x9000 {
            return Err(CieError::Card { sw });
        }

        // 4. GET DATA per ottenere la chiave pubblica ICC
        let get_icc_key = vec![0x00, 0xCB, 0x3F, 0xFF, 0x06, 0x4D, 0x04, 0xA6, 0x02, 0x91, 0x00, 0x00];
        let (resp, sw) = card.transmit_auto(&get_icc_key)?;

        if sw != 0x9000 {
            return Err(CieError::Card { sw });
        }

        // 5. Parsa risposta per estrarre chiave pubblica ICC
        let icc_pub_key = if let Ok(asn) = Asn1Node::parse(&resp) {
            if let Some(node) = asn.find_tag_recursive(0x91) {
                node.content.clone()
            } else {
                resp.clone()
            }
        } else {
            resp.clone()
        };

        debug!("ICC pub key: {} bytes", icc_pub_key.len());

        // Servono entrambe nell'hash del DAPP: conservarle.
        self.dh_pub_key = dh_pub_key_bytes.clone();
        self.dh_icc_pub_key = icc_pub_key.clone();

        // 6. Calcola segreto condiviso: icc_pub_key^priv mod p
        let icc_pub = BigUint::from_bytes_be(&icc_pub_key);
        let shared_secret = icc_pub.modpow(&priv_key, &p);
        let mut secret_bytes = shared_secret.to_bytes_be();

        // Padding
        while secret_bytes.len() < self.dh_p.len() {
            secret_bytes.insert(0, 0);
        }

        debug!("Shared secret: {} bytes", secret_bytes.len());

        // 7. Deriva chiavi di sessione
        let (sess_enc, sess_mac) = derive_session_keys(&secret_bytes);
        self.sess_enc = sess_enc;
        self.sess_mac = sess_mac;

        // 8. Inizializza SSC
        self.ssc = [0u8; 8];
        self.ssc[7] = 1;

        self.active = true;

        info!("Scambio chiavi DH completato, SM attivo");
        Ok(())
    }

    /// Invia un APDU in SM componendo `head || Lc || data || Le`.
    ///
    /// Oltre 0xE7 byte di dati la carta richiede il **command chaining**: i blocchi
    /// intermedi viaggiano con CLA|0x10 e ognuno fa avanzare l'SSC. Il certificato
    /// DAPP supera abbondantemente quella soglia.
    fn send_sm(
        &mut self,
        card: &PcscCard,
        head: [u8; 4],
        data: &[u8],
        le: Option<u8>,
    ) -> Result<(Vec<u8>, u16)> {
        let blocchi = chain_apdu(head, data, le);
        let ultimo = blocchi.len() - 1;

        let mut esito = (Vec::new(), 0u16);
        for (i, apdu) in blocchi.iter().enumerate() {
            esito = self.send_apdu(card, apdu)?;
            // Un blocco intermedio che fallisce interrompe la catena: proseguire
            // manderebbe alla carta un comando a meta'.
            if i != ultimo && esito.1 != 0x9000 {
                return Err(CieError::Card { sw: esito.1 });
            }
        }
        Ok(esito)
    }

    /// Come [`Self::send_sm`] ma trasforma uno status word != 9000 in errore.
    fn send_sm_ok(&mut self, card: &PcscCard, head: [u8; 4], data: &[u8], le: Option<u8>) -> Result<Vec<u8>> {
        let (resp, sw) = self.send_sm(card, head, data, le)?;
        if sw != 0x9000 {
            return Err(CieError::Card { sw });
        }
        Ok(resp)
    }

    /// Legge dalla carta i parametri della CA di external authentication.
    ///
    /// Va chiamata **prima** di aprire il canale: usa APDU in chiaro
    /// (`IAS::InitExtAuthKeyParam`, IAS.cpp:1086). Di tutta la coppia CA solo
    /// l'esponente privato e' una costante (`EXT_AUTH_PRIVEXP`).
    pub fn init_ext_auth_key_param(&mut self, card: &PcscCard) -> Result<()> {
        info!("Lettura parametri CA di external authentication...");

        let dati = [0x4D, 0x09, 0x70, 0x07, 0xBF, 0xA0, CIE_KEY_EXT_AUTH_ID & 0x7F, 0x03, 0x7F, 0x49, 0x80];
        let mut apdu = vec![0x00, 0xCB, 0x3F, 0xFF, dati.len() as u8];
        apdu.extend_from_slice(&dati);
        apdu.push(0x00);

        let (resp, sw) = card.transmit_auto(&apdu)?;
        if sw != 0x9000 {
            return Err(CieError::Card { sw });
        }

        let asn = Asn1Node::parse(&resp)?;
        let prendi = |tag: u32| -> Result<Vec<u8>> {
            asn.find_tag_recursive(tag)
                .map(|n| n.content.clone())
                .ok_or_else(|| CieError::Protocol(format!("Tag {:04X} assente nei parametri CA", tag)))
        };

        self.ca_module = prendi(0x81)?;
        self.ca_pubexp = prendi(0x82)?;
        let chr = prendi(0x5F20)?;
        let cha = prendi(0x5F4C)?;

        if chr.len() < 4 || cha.len() < 6 {
            return Err(CieError::Protocol("CHR/CHA della CA troppo corti".into()));
        }
        self.ca_car = chr[4..].to_vec();
        self.ca_aid = cha[..6].to_vec();

        debug!("CA module {} byte, CAR {} byte, AID {} byte",
            self.ca_module.len(), self.ca_car.len(), self.ca_aid.len());
        Ok(())
    }

    /// Autenticazione mutua DAPP (`IAS::DAPP`, IAS.cpp:426).
    ///
    /// Presenta alla carta un certificato end-entity firmato dalla CA, dimostra
    /// di possederne la chiave privata rispondendo a una sfida, e **verifica a sua
    /// volta la firma del chip**: e' autenticazione mutua, non una formalita'.
    ///
    /// Al termine l'SSC riparte da `challenge[4..] || rndIFD[4..]` (IAS.cpp:569).
    pub fn dapp(&mut self, card: &PcscCard) -> Result<()> {
        use rand::RngExt;

        info!("DAPP: autenticazione mutua...");

        if self.dapp_pub_key.is_empty() || self.dapp_module.is_empty() {
            return Err(CieError::Protocol("Chiave pubblica DAPP non letta".into()));
        }
        if self.ca_module.is_empty() {
            return Err(CieError::Protocol("Parametri CA non inizializzati".into()));
        }

        const SHA: usize = 32;
        let sn_ifd: [u8; 8] = [0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];

        // --- Certificato end-entity: CPI | CAR | CHR | CHA | OID | modulo | pubexp
        let mut chr = vec![0u8; 4];
        chr.extend_from_slice(&sn_ifd);
        let mut cha = self.ca_aid.clone();
        cha.push(0x01);
        let oid = [0x2A, 0x81, 0x22, 0xF4, 0x2A, 0x02, 0x04, 0x01, 0x04];

        let mut cert_body = vec![0x8A]; // CPI
        cert_body.extend_from_slice(&self.ca_car);
        cert_body.extend_from_slice(&chr);
        cert_body.extend_from_slice(&cha);
        cert_body.extend_from_slice(&oid);
        cert_body.extend_from_slice(DEF_MODULE);
        cert_body.extend_from_slice(DEF_PUBEXP);

        // Il certificato non entra intero nella firma: la coda viaggia in chiaro (PkRem).
        let taglio = self.ca_module.len().checked_sub(SHA + 2)
            .ok_or_else(|| CieError::Protocol("Modulo CA troppo corto".into()))?;
        if cert_body.len() < taglio {
            return Err(CieError::Protocol("Certificato end-entity piu' corto del previsto".into()));
        }

        let mut to_sign = vec![0x6A];
        to_sign.extend_from_slice(&cert_body[..taglio]);
        to_sign.extend_from_slice(&sha256(&cert_body));
        to_sign.push(0xBC);

        let cert_sign = rsa_raw(&to_sign, EXT_AUTH_PRIVEXP, &self.ca_module);
        // Se la costante EXT_AUTH_PRIVEXP non corrisponde al modulo letto dalla
        // carta, se ne accorge qui invece che dopo, con un 6982 opaco.
        if rsa_raw(&cert_sign, &self.ca_pubexp, &self.ca_module) != to_sign {
            return Err(CieError::Protocol(
                "La firma del certificato end-entity non si verifica con la chiave CA della carta".into(),
            ));
        }

        let mut corpo_cert = asn1_tag(0x5F37, &cert_sign);
        corpo_cert.extend_from_slice(&asn1_tag(0x5F38, &cert_body[taglio..]));
        corpo_cert.extend_from_slice(&asn1_tag(0x42, &self.ca_car));
        let cert = asn1_tag(0x7F21, &corpo_cert);

        // 1. MSE SET: algoritmo di verifica PSO + key id della CA
        let mut mse = asn1_tag(0x80, &[0x41]);
        mse.extend_from_slice(&asn1_tag(0x83, &[CIE_KEY_EXT_AUTH_ID]));
        self.send_sm_ok(card, [0x00, 0x22, 0x81, 0xB6], &mse, Some(0))?;

        // 2. PSO VERIFY CERTIFICATE (va in chaining: il certificato e' grande)
        self.send_sm_ok(card, [0x00, 0x2A, 0x00, 0xAE], &cert, None)?;

        // 3. MSE SET CHR
        self.send_sm_ok(card, [0x00, 0x22, 0x81, 0xA4], &asn1_tag(0x83, &chr), None)?;

        // 4. GET CHALLENGE
        let challenge = self.send_sm_ok(card, [0x00, 0x84, 0x00, 0x00], &[], Some(8))?;
        if challenge.len() < 8 {
            return Err(CieError::Protocol("Challenge piu' corta di 8 byte".into()));
        }

        // 5. EXTERNAL AUTHENTICATE: dimostriamo di avere la privata del certificato
        let pad = DEF_MODULE.len() - SHA - 2;
        let mut prnd = vec![0u8; pad];
        rand::rng().fill(&mut prnd[..]);

        let mut da_hashare = prnd.clone();
        da_hashare.extend_from_slice(&self.dh_pub_key);
        da_hashare.extend_from_slice(&sn_ifd);
        da_hashare.extend_from_slice(&challenge);
        da_hashare.extend_from_slice(&self.dh_icc_pub_key);
        da_hashare.extend_from_slice(&self.dh_g);
        da_hashare.extend_from_slice(&self.dh_p);
        da_hashare.extend_from_slice(&self.dh_q);

        let mut firma_in = vec![0x6A];
        firma_in.extend_from_slice(&prnd);
        firma_in.extend_from_slice(&sha256(&da_hashare));
        firma_in.push(0xBC);

        let mut risposta = sn_ifd.to_vec();
        risposta.extend_from_slice(&rsa_raw(&firma_in, DEF_PRIVEXP, DEF_MODULE));
        self.send_sm_ok(card, [0x00, 0x82, 0x00, 0x00], &risposta, None)?;

        // 6. MSE SET per l'internal authenticate
        let mut mse_int = asn1_tag(0x84, &[0x82]);
        mse_int.extend_from_slice(&asn1_tag(0x80, &[0x9B]));
        self.send_sm_ok(card, [0x00, 0x22, 0x41, 0xA4], &mse_int, None)?;

        // 7. GIVE RANDOM: ora tocca al chip dimostrare chi e'
        let mut rnd_ifd = [0u8; 8];
        rand::rng().fill(&mut rnd_ifd);
        let resp = self.send_sm_ok(card, [0x00, 0x88, 0x00, 0x00], &rnd_ifd, None)?;

        if resp.len() <= 8 {
            return Err(CieError::Protocol("Risposta internal authenticate troppo corta".into()));
        }
        let sn_icc = &resp[..8];
        let firma_chip = rsa_raw(&resp[8..], &self.dapp_pub_key, &self.dapp_module);

        if firma_chip.len() < SHA + 2 || firma_chip[0] != 0x6A || *firma_chip.last().unwrap() != 0xBC {
            return Err(CieError::ChipAuthFailed);
        }
        let prnd2 = &firma_chip[1..firma_chip.len() - SHA - 1];
        let hash_icc = &firma_chip[1 + prnd2.len()..1 + prnd2.len() + SHA];

        let mut verifica = prnd2.to_vec();
        verifica.extend_from_slice(&self.dh_icc_pub_key);
        verifica.extend_from_slice(sn_icc);
        verifica.extend_from_slice(&rnd_ifd);
        verifica.extend_from_slice(&self.dh_pub_key);
        verifica.extend_from_slice(&self.dh_g);
        verifica.extend_from_slice(&self.dh_p);
        verifica.extend_from_slice(&self.dh_q);

        if sha256(&verifica) != hash_icc {
            return Err(CieError::ChipAuthFailed);
        }

        // L'SSC riparte dai due nonce: 4 byte della challenge + 4 di rndIFD.
        self.ssc[..4].copy_from_slice(&challenge[challenge.len() - 4..]);
        self.ssc[4..].copy_from_slice(&rnd_ifd[4..]);
        self.active = true;

        info!("DAPP completato: chip autenticato, canale SM attivo");
        Ok(())
    }

    /// Incrementa il Sequence Counter
    fn increment_ssc(&mut self) {
        for i in (0..8).rev() {
            self.ssc[i] = self.ssc[i].wrapping_add(1);
            if self.ssc[i] != 0 {
                break;
            }
        }
    }

    /// Codifica un APDU con Secure Messaging.
    ///
    /// Segue `IAS::SM` del middleware C++ (cie-pkcs11/CSP/IAS.cpp):
    /// `calcMac = ISOPad(SSC || header)`, poi i TLV vengono concatenati **grezzi**
    /// e si applica **un solo** ISOPad finale prima del MAC.
    pub fn encode_apdu(&mut self, apdu: &[u8]) -> Result<Vec<u8>> {
        if !self.active {
            return Err(CieError::SmNotActive);
        }
        if apdu.len() < 4 {
            return Err(CieError::Protocol("APDU troppo corto".into()));
        }

        self.increment_ssc();

        // Header con i bit di secure messaging accesi
        let head = [apdu[0] | 0x0C, apdu[1], apdu[2], apdu[3]];

        // calcMac parte da ISOPad(SSC || header); i TLV si accodano grezzi.
        let mut calc_mac = iso_pad(&[&self.ssc[..], &head[..]].concat());
        let mut datafield = Vec::new();

        let (data, le) = parse_apdu_body(apdu);

        if !data.is_empty() {
            let des3 = TripleDes::new(&self.sess_enc, &[0u8; 8])?;
            let enc = des3.encrypt(&data)?;

            // INS pari -> tag 0x87 con padding indicator 0x01; INS dispari -> tag 0x85 nudo.
            let doob = if apdu[1] & 1 == 0 {
                let mut v = vec![0x01];
                v.extend_from_slice(&enc);
                asn1_tag(0x87, &v)
            } else {
                asn1_tag(0x85, &enc)
            };
            calc_mac.extend_from_slice(&doob);
            datafield.extend_from_slice(&doob);
        }

        if let Some(le_val) = le {
            let doob = asn1_tag(0x97, &[le_val]);
            calc_mac.extend_from_slice(&doob);
            datafield.extend_from_slice(&doob);
        }

        // Un unico ISOPad finale, poi Retail MAC.
        let mac = RetailMac::new(&self.sess_mac, &[0u8; 8])?;
        let mac_value = mac.compute_raw(&iso_pad(&calc_mac));
        datafield.extend_from_slice(&asn1_tag(0x8E, &mac_value));

        // Assemblaggio: Lc corto o esteso a seconda della dimensione.
        let mut out = head.to_vec();
        if datafield.len() < 0x100 {
            out.push(datafield.len() as u8);
            out.extend_from_slice(&datafield);
            out.push(0x00);
        } else {
            out.push(0x00);
            out.push((datafield.len() >> 8) as u8);
            out.push((datafield.len() & 0xFF) as u8);
            out.extend_from_slice(&datafield);
            out.push(0x00);
            out.push(0x00);
        }

        Ok(out)
    }

    /// Decodifica una risposta in Secure Messaging, **verificando il MAC**.
    ///
    /// Segue `IAS::respSM`: il MAC si calcola su `SSC || <TLV grezzi nell'ordine
    /// in cui arrivano>` con un ISOPad finale, e va confrontato con il tag 0x8E.
    pub fn decode_response(&mut self, response: &[u8]) -> Result<Vec<u8>> {
        if !self.active {
            return Ok(response.to_vec());
        }
        if response.is_empty() {
            return Ok(Vec::new());
        }

        self.increment_ssc();

        let mut calc_mac = self.ssc.to_vec();
        let mut enc_data: Option<Vec<u8>> = None;
        let mut resp_mac: Option<Vec<u8>> = None;

        for tlv in walk_tlv(response)? {
            match tlv.tag {
                // Processing status: entra nel MAC ma non porta dati.
                0x99 => calc_mac.extend_from_slice(tlv.raw),
                // Dati cifrati senza padding indicator.
                0x85 => {
                    enc_data = Some(tlv.content.to_vec());
                    calc_mac.extend_from_slice(tlv.raw);
                }
                // Dati cifrati con padding indicator da scartare.
                0x87 => {
                    if tlv.content.is_empty() {
                        return Err(CieError::Protocol("TLV 0x87 vuoto".into()));
                    }
                    enc_data = Some(tlv.content[1..].to_vec());
                    calc_mac.extend_from_slice(tlv.raw);
                }
                // Il MAC stesso non entra nel proprio calcolo.
                0x8E => {
                    if tlv.content.len() != 8 {
                        return Err(CieError::Protocol("Lunghezza MAC non valida".into()));
                    }
                    resp_mac = Some(tlv.content.to_vec());
                }
                t => return Err(CieError::Protocol(format!("Tag SM non riconosciuto: {:02X}", t))),
            }
        }

        let resp_mac = resp_mac.ok_or_else(|| CieError::Protocol("MAC assente nella risposta".into()))?;
        let mac = RetailMac::new(&self.sess_mac, &[0u8; 8])?;
        if mac.compute_raw(&iso_pad(&calc_mac)) != resp_mac {
            return Err(CieError::MacMismatch);
        }

        match enc_data {
            Some(enc) => TripleDes::new(&self.sess_enc, &[0u8; 8])?.decrypt(&enc),
            None => Ok(Vec::new()),
        }
    }

    /// Invia un APDU in Secure Messaging: cifra, trasmette, riassembla e decifra.
    ///
    /// È l'unico punto in cui l'SSC viene fatto avanzare (una volta per il comando,
    /// una per la risposta): usare `encode_apdu`/`decode_response` separatamente
    /// disallinea il contatore con la carta.
    ///
    /// Le risposte lunghe arrivano spezzate con 61xx e vanno ricomposte **prima**
    /// di decodificare: le GET RESPONSE sono comandi di trasporto, non viaggiano
    /// in SM e non fanno avanzare l'SSC (`IAS::getResp_SM`, IAS.cpp:880).
    pub fn send_apdu(&mut self, card: &PcscCard, apdu: &[u8]) -> Result<(Vec<u8>, u16)> {
        let sm_apdu = self.encode_apdu(apdu)?;
        debug!("SM TX: {}", hex::encode(&sm_apdu));

        let (mut resp, mut sw) = card.transmit_raw(&sm_apdu)?;

        loop {
            if (sw >> 8) == 0x61 {
                let ln = (sw & 0xFF) as u8;
                // Con ln==0 la carta non dice quanto resta: il middleware chiede
                // il massimo, e con CLA 0x0C.
                let get = if ln != 0 {
                    vec![0x00, 0xC0, 0x00, 0x00, ln]
                } else {
                    vec![0x0C, 0xC0, 0x00, 0x00, 0x00]
                };

                let (pezzo, nuovo_sw) = card.transmit_raw(&get)?;
                resp.extend_from_slice(&pezzo);
                sw = nuovo_sw;

                if ln != 0 {
                    if sw == 0x9000 {
                        break;
                    }
                    if (sw >> 8) != 0x61 {
                        return Err(CieError::Card { sw });
                    }
                }
            } else if sw == 0x9000 || sw == 0x6B00 || sw == 0x6282 {
                break;
            } else {
                // Errore secco senza payload in SM: la carta non ha prodotto una
                // risposta protetta, quindi il suo SSC non e' avanzato. Non
                // decodificare, o il contatore si sfasa.
                return Ok((Vec::new(), sw));
            }
        }

        let data = self.decode_response(&resp)?;
        Ok((data, sw))
    }


    /// Legge il numero di tentativi PIN rimasti **senza consumarne uno**.
    ///
    /// VERIFY con campo dati vuoto: la carta risponde 63Cx senza valutare il PIN.
    /// Da chiamare prima di ogni tentativo per non arrivare al blocco inavvertitamente.
    pub fn pin_retries_left(&mut self, card: &PcscCard) -> Result<u8> {
        let (_, sw) = self.send_apdu(card, &[0x00, 0x20, 0x00, 0x81])?;
        match sw {
            sw if (sw & 0xFFF0) == 0x63C0 => Ok((sw & 0x0F) as u8),
            0x9000 => Ok(3), // PIN già verificato in questa sessione
            0x6983 => Err(CieError::PinBlocked),
            sw => Err(CieError::Card { sw }),
        }
    }

    /// Verifica il PIN con Secure Messaging
    pub fn verify_pin(&mut self, card: &PcscCard, pin: &str) -> Result<()> {
        if !self.active {
            return Err(CieError::SmNotActive);
        }

        let pin_bytes = pin.as_bytes();
        if pin_bytes.len() != 8 || !pin_bytes.iter().all(|b| b.is_ascii_digit()) {
            return Err(CieError::InvalidInput("Il PIN deve essere 8 cifre".into()));
        }

        // CLA=00 INS=20 P1=00 P2=81 (PIN ID della CIE)
        let mut apdu = vec![0x00, 0x20, 0x00, 0x81, 0x08];
        apdu.extend_from_slice(pin_bytes);

        let (_, sw) = self.send_apdu(card, &apdu)?;

        match sw {
            0x9000 => {
                info!("PIN verificato correttamente");
                Ok(())
            }
            sw if (sw & 0xFFF0) == 0x63C0 => Err(CieError::PinIncorrect((sw & 0x0F) as u32)),
            0x6983 => Err(CieError::PinBlocked),
            sw => Err(CieError::Card { sw }),
        }
    }

    /// Firma un digest SHA-256 con la chiave di firma della carta.
    ///
    /// Richiede il PIN gia' verificato: la chiave e' protetta.
    ///
    /// Segue `IAS::Sign` (IAS.cpp:122): MSE SET dell'algoritmo e della chiave,
    /// poi `INTERNAL AUTHENTICATE`. Alla carta non si manda l'hash nudo ma il
    /// **DigestInfo** PKCS#1 v1.5 (`CCIESigner::Sign` -> `makeDigestInfo`); il
    /// padding PKCS#1 e la RSA li fa il chip.
    ///
    /// Restituisce la firma grezza, lunga quanto il modulo (256 byte, RSA-2048).
    pub fn sign_sha256(&mut self, card: &PcscCard, hash: &[u8; 32]) -> Result<Vec<u8>> {
        info!("Firma on-card di un digest SHA-256...");

        let mut mse = asn1_tag(0x80, &[0x02]);
        mse.extend_from_slice(&asn1_tag(0x84, &[CIE_KEY_SIGN_ID]));
        self.send_sm_ok(card, [0x00, 0x22, 0x41, 0xA4], &mse, None)?;

        let firma = self.send_sm_ok(card, [0x00, 0x88, 0x00, 0x00], &digest_info_sha256(hash), None)?;

        debug!("firma: {} byte", firma.len());
        Ok(firma)
    }

    /// Legge un file dalla carta usando Secure Messaging
    pub fn read_file(&mut self, card: &PcscCard, file_id: u16) -> Result<Vec<u8>> {
        if !self.is_active() {
            return Err(CieError::SmNotActive);
        }

        let select = [
            0x00, 0xA4, 0x02, 0x04, 0x02,
            (file_id >> 8) as u8, (file_id & 0xFF) as u8,
        ];
        let (_, sw) = self.send_apdu(card, &select)?;
        if sw != 0x9000 && (sw >> 8) != 0x61 {
            return Err(CieError::Card { sw });
        }

        let mut data = Vec::new();
        let mut offset = 0u16;

        loop {
            let read = [0x00, 0xB0, (offset >> 8) as u8, (offset & 0xFF) as u8, 0x00];
            let (chunk, sw) = self.send_apdu(card, &read)?;

            match sw {
                0x9000 => {
                    if chunk.is_empty() {
                        break;
                    }
                    data.extend_from_slice(&chunk);
                    // ponytail: offset u16 basta, i file CIE stanno sotto i 64KB
                    offset += chunk.len() as u16;
                }
                // Fine file: 6282 porta con sé l'ultimo blocco valido.
                0x6282 => {
                    data.extend_from_slice(&chunk);
                    break;
                }
                0x6B00 => break,
                sw => return Err(CieError::Card { sw }),
            }
        }

        Ok(data)
    }
}

/// Inizializza e stabilisce Secure Messaging con la carta
pub fn establish_secure_messaging(card: &PcscCard, cie_type: CieType) -> Result<SecureMessaging> {
    let mut sm = SecureMessaging::new(cie_type);

    // In chiaro: parametri DH e chiave della CA di external authentication.
    sm.init_dh_params(card)?;
    sm.init_ext_auth_key_param(card)?;

    // Apre il canale: da qui in poi tutto passa in Secure Messaging.
    sm.dh_key_exchange(card)?;

    // Dentro il canale: chiave del chip, poi autenticazione mutua.
    sm.read_dapp_pub_key(card)?;
    sm.dapp(card)?;

    Ok(sm)
}

/// Estrae campo dati ed Le da un APDU in chiaro.
///
/// Riproduce le condizioni di `IAS::SM`: caso corto (`Lc` su un byte) ed esteso
/// (`00 Lc_hi Lc_lo`). Le è presente solo se l'APDU è `header+Le` oppure
/// `header+Lc+data+Le`.
fn parse_apdu_body(apdu: &[u8]) -> (Vec<u8>, Option<u8>) {
    match apdu.len() {
        0..=4 => (Vec::new(), None),
        5 => (Vec::new(), Some(apdu[4])),
        _ => {
            if apdu[4] != 0 {
                let lc = apdu[4] as usize;
                let data = apdu.get(5..5 + lc).unwrap_or_default().to_vec();
                // header(4) + Lc(1) + data(lc) + Le(1)
                let le = if apdu.len() == lc + 6 { Some(apdu[apdu.len() - 1]) } else { None };
                (data, le)
            } else if apdu.len() > 7 {
                // APDU esteso: Lc su due byte dopo lo 0x00
                let lc = ((apdu[5] as usize) << 8) | apdu[6] as usize;
                (apdu.get(7..7 + lc).unwrap_or_default().to_vec(), None)
            } else {
                (Vec::new(), None)
            }
        }
    }
}

/// Costruisce il DigestInfo PKCS#1 v1.5 per un hash SHA-256.
///
/// `DigestInfo ::= SEQUENCE { AlgorithmIdentifier, OCTET STRING }`, dove
/// l'AlgorithmIdentifier e' l'OID di SHA-256 (2.16.840.1.101.3.4.2.1) seguito
/// da NULL. E' cio' che la carta si aspetta al posto dell'hash nudo.
pub(crate) fn digest_info_sha256_pub(hash: &[u8; 32]) -> Vec<u8> {
    digest_info_sha256(hash)
}

fn digest_info_sha256(hash: &[u8; 32]) -> Vec<u8> {
    const OID_SHA256: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];

    let mut algo = asn1_tag(0x06, OID_SHA256);
    algo.extend_from_slice(&asn1_tag(0x05, &[])); // NULL dei parametri

    let mut contenuto = asn1_tag(0x30, &algo);
    contenuto.extend_from_slice(&asn1_tag(0x04, hash));

    asn1_tag(0x30, &contenuto)
}

/// Compone gli APDU in chiaro di un comando, spezzandolo se serve.
///
/// Oltre 0xE7 byte di dati la carta richiede il **command chaining**: i blocchi
/// intermedi viaggiano con CLA|0x10 e solo l'ultimo porta l'Le. Il certificato
/// DAPP supera abbondantemente quella soglia.
fn chain_apdu(head: [u8; 4], data: &[u8], le: Option<u8>) -> Vec<Vec<u8>> {
    const MAX_CHUNK: usize = 0xE7;

    if data.len() < MAX_CHUNK {
        let mut apdu = head.to_vec();
        // Con campo dati vuoto il byte Lc non va emesso affatto (IAS::SendAPDU):
        // un "00" di troppo verrebbe letto come Lc=0 e sfaserebbe l'APDU.
        if !data.is_empty() {
            apdu.push(data.len() as u8);
            apdu.extend_from_slice(data);
        }
        if let Some(l) = le {
            apdu.push(l);
        }
        return vec![apdu];
    }

    let mut blocchi = Vec::new();
    let mut sent = 0;
    while sent < data.len() {
        let chunk = &data[sent..(sent + MAX_CHUNK).min(data.len())];
        sent += chunk.len();
        let ultimo = sent == data.len();

        let mut apdu = head.to_vec();
        if !ultimo {
            apdu[0] |= 0x10;
        }
        apdu.push(chunk.len() as u8);
        apdu.extend_from_slice(chunk);
        if let (true, Some(l)) = (ultimo, le) {
            apdu.push(l);
        }
        blocchi.push(apdu);
    }
    blocchi
}

/// Un TLV nella risposta SM, con i byte grezzi necessari al calcolo del MAC.
struct Tlv<'a> {
    tag: u8,
    /// Solo il contenuto, senza tag né lunghezza.
    content: &'a [u8],
    /// Tag + lunghezza + contenuto, esattamente come arrivati dalla carta:
    /// è su questi byte che si calcola il MAC.
    raw: &'a [u8],
}

/// Scorre i TLV di una risposta SM restituendo anche i byte grezzi.
///
/// Non riusa `Asn1Node` perché il MAC va calcolato sulla codifica originale:
/// ri-serializzare rischierebbe di cambiare la forma della lunghezza.
fn walk_tlv(data: &[u8]) -> Result<Vec<Tlv<'_>>> {
    let mut out = Vec::new();
    let mut i = 0;

    while i < data.len() {
        let tag = data[i];
        let len_pos = i + 1;
        if len_pos >= data.len() {
            return Err(CieError::Protocol("TLV troncato: manca la lunghezza".into()));
        }

        let first = data[len_pos];
        let (len, header) = if first < 0x80 {
            (first as usize, 2)
        } else {
            let n = (first & 0x7F) as usize;
            if n == 0 || n > 2 {
                return Err(CieError::Protocol(format!("Lunghezza TLV non valida: {:02X}", first)));
            }
            if len_pos + n >= data.len() {
                return Err(CieError::Protocol("TLV troncato: lunghezza incompleta".into()));
            }
            let mut v = 0usize;
            for k in 0..n {
                v = (v << 8) | data[len_pos + 1 + k] as usize;
            }
            (v, 2 + n)
        };

        let end = i + header + len;
        if end > data.len() {
            return Err(CieError::Protocol(format!(
                "TLV {:02X} dichiara {} byte ma ne restano {}",
                tag, len, data.len() - i - header
            )));
        }

        out.push(Tlv { tag, content: &data[i + header..end], raw: &data[i..end] });
        i = end;
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sessione SM con chiavi e SSC noti, per confronto con vettori calcolati
    /// indipendentemente (riferimento Python su pycryptodome, semantica IAS::SM).
    fn sm_di_prova() -> SecureMessaging {
        let mut sm = SecureMessaging::new(CieType::NXP);
        sm.sess_enc.copy_from_slice(&hex::decode("00112233445566778899AABBCCDDEEFF").unwrap());
        sm.sess_mac.copy_from_slice(&hex::decode("FFEEDDCCBBAA99887766554433221100").unwrap());
        sm.ssc = [0u8; 8]; // il primo increment_ssc lo porta a ...01
        sm.active = true;
        sm
    }

    /// Regressione del bug principale: i TLV vanno concatenati grezzi nel calcolo
    /// del MAC, con **un solo** ISOPad finale. Paddare ogni TLV separatamente
    /// (o saltare il pad finale) produce un MAC che la carta rifiuta.
    #[test]
    fn test_encode_apdu_verify_pin() {
        let mut sm = sm_di_prova();
        let mut apdu = vec![0x00, 0x20, 0x00, 0x81, 0x08];
        apdu.extend_from_slice(b"12345678");

        assert_eq!(
            hex::encode_upper(sm.encode_apdu(&apdu).unwrap()),
            "0C2000811D8711010B9F8563FBA46D179807B5BBC529CED08E084A2C3FD733D77AE500"
        );
        assert_eq!(sm.ssc[7], 1, "l'SSC deve avanzare di uno per comando");
    }

    /// Andata e ritorno: una risposta costruita con le stesse chiavi deve
    /// superare la verifica del MAC e decifrare al plaintext originale.
    #[test]
    fn test_decode_response_roundtrip() {
        let mut sm = sm_di_prova();
        sm.increment_ssc(); // simula il comando gia' inviato

        let atteso = b"CIE payload";

        // Costruisce la risposta come farebbe la carta, con l'SSC della risposta.
        let mut prova = sm_di_prova();
        prova.increment_ssc();
        prova.increment_ssc();
        let ssc_risposta = prova.ssc;

        let enc = TripleDes::new(&prova.sess_enc, &[0u8; 8]).unwrap().encrypt(atteso).unwrap();
        let mut cifrato = vec![0x01];
        cifrato.extend_from_slice(&enc);
        let tlv87 = asn1_tag(0x87, &cifrato);
        let tlv99 = asn1_tag(0x99, &[0x90, 0x00]);

        let mut calc = ssc_risposta.to_vec();
        calc.extend_from_slice(&tlv99);
        calc.extend_from_slice(&tlv87);
        let mac = RetailMac::new(&prova.sess_mac, &[0u8; 8]).unwrap().compute_raw(&iso_pad(&calc));

        let mut risposta = tlv99.clone();
        risposta.extend_from_slice(&tlv87);
        risposta.extend_from_slice(&asn1_tag(0x8E, &mac));

        assert_eq!(sm.decode_response(&risposta).unwrap(), atteso);
    }

    /// Un MAC alterato deve far fallire la decodifica, non passare inosservato.
    #[test]
    fn test_decode_response_mac_alterato() {
        let mut sm = sm_di_prova();
        sm.increment_ssc();

        let tlv99 = asn1_tag(0x99, &[0x90, 0x00]);
        let mut risposta = tlv99;
        risposta.extend_from_slice(&asn1_tag(0x8E, &[0u8; 8]));

        assert!(matches!(sm.decode_response(&risposta), Err(CieError::MacMismatch)));
    }

    #[test]
    fn test_walk_tlv_forme_di_lunghezza() {
        let lungo = vec![0xAA; 300];
        let mut data = asn1_tag(0x99, &[0x90, 0x00]);
        data.extend_from_slice(&asn1_tag(0x87, &lungo));

        let tlv = walk_tlv(&data).unwrap();
        assert_eq!(tlv.len(), 2);
        assert_eq!(tlv[0].tag, 0x99);
        assert_eq!(tlv[1].content.len(), 300, "lunghezza in forma 0x82");
        assert_eq!(tlv[1].raw.len(), 304, "raw include tag e lunghezza");
    }

    #[test]
    fn test_walk_tlv_rifiuta_troncato() {
        assert!(walk_tlv(&[0x87, 0x10, 0x01, 0x02]).is_err());
    }

    /// Il chaining e' il punto in cui il certificato DAPP puo' rompersi in
    /// silenzio: blocchi intermedi con CLA|0x10, Le solo sull'ultimo, e la
    /// concatenazione dei chunk deve restituire i dati originali.
    /// Il DigestInfo ha una codifica DER fissa e arcinota: se la costruzione
    /// ASN.1 sbaglia di un byte, la carta firma qualcosa che non verifica.
    #[test]
    fn test_digest_info_sha256() {
        let di = digest_info_sha256(&[0xAB; 32]);
        assert_eq!(
            hex::encode_upper(&di[..19]),
            "3031300D060960864801650304020105000420",
            "prefisso DigestInfo SHA-256 standard"
        );
        assert_eq!(di.len(), 51, "19 di prefisso + 32 di hash");
        assert_eq!(&di[19..], &[0xAB; 32]);
    }

    #[test]
    fn test_chain_apdu() {
        let head = [0x00, 0x2A, 0x00, 0xAE];

        // Sotto soglia: un blocco solo, CLA intatta.
        let corto = chain_apdu(head, &[0xAA; 10], None);
        assert_eq!(corto.len(), 1);
        assert_eq!(corto[0][0], 0x00);
        assert_eq!(corto[0][4], 10);

        // Campo dati vuoto: niente Lc, solo header (+ Le se richiesto).
        assert_eq!(chain_apdu([0x00, 0x84, 0x00, 0x00], &[], Some(8))[0],
                   vec![0x00, 0x84, 0x00, 0x00, 0x08]);
        assert_eq!(chain_apdu([0x00, 0xA4, 0x00, 0x00], &[], None)[0],
                   vec![0x00, 0xA4, 0x00, 0x00]);

        // Sopra soglia: piu' blocchi, CLA|0x10 su tutti tranne l'ultimo.
        let dati: Vec<u8> = (0..600u16).map(|i| i as u8).collect();
        let blocchi = chain_apdu(head, &dati, Some(0));
        assert_eq!(blocchi.len(), 3, "600 byte -> 231+231+138");

        let mut ricostruito = Vec::new();
        for (i, b) in blocchi.iter().enumerate() {
            let ultimo = i == blocchi.len() - 1;
            assert_eq!(b[0] & 0x10, if ultimo { 0x00 } else { 0x10 }, "blocco {}", i);
            let lc = b[1 + 3] as usize;
            ricostruito.extend_from_slice(&b[5..5 + lc]);
            // L'Le compare solo in coda all'ultimo blocco.
            assert_eq!(b.len(), 5 + lc + if ultimo { 1 } else { 0 }, "blocco {}", i);
        }
        assert_eq!(ricostruito, dati, "i chunk devono ricomporre i dati originali");
    }

    #[test]
    fn test_parse_apdu_body() {
        assert_eq!(parse_apdu_body(&[0x00, 0xA4, 0x02, 0x0C]), (vec![], None));
        assert_eq!(parse_apdu_body(&[0x00, 0xB0, 0x00, 0x00, 0x00]), (vec![], Some(0x00)));
        assert_eq!(
            parse_apdu_body(&[0x00, 0xA4, 0x02, 0x04, 0x02, 0x10, 0x03]),
            (vec![0x10, 0x03], None)
        );
        assert_eq!(
            parse_apdu_body(&[0x00, 0xA4, 0x02, 0x04, 0x02, 0x10, 0x03, 0x00]),
            (vec![0x10, 0x03], Some(0x00))
        );
    }
}
