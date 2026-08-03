//! Il firmatario che vive sulla carta, condiviso da tutti i contenitori di
//! firma (PAdES, CAdES, detached) e da tutte le interfacce.

use anyhow::Result;
use cie_lib::{secure_messaging, CieCard, CieType, PcscCard, PcscContext, SecureMessaging};
use std::sync::Mutex;
use underskrift::error::CryptoError;
use underskrift::{CryptoSigner, DigestAlgorithm, SignatureAlgorithm};

/// Firmatario che vive sulla carta.
///
/// underskrift non vede mai una chiave: gli passiamo questo, lui ci consegna un
/// digest e si riprende 256 byte di firma. E' lo stesso schema che usano gli HSM
/// e la firma remota, ed e' il motivo per cui questa libreria e' utilizzabile
/// con una chiave che non e' estraibile.
///
/// Il `Mutex` c'e' perche' il tratto chiede `&self` mentre `sign_sha256` vuole
/// `&mut SecureMessaging` (l'SSC avanza a ogni comando). `pcsc::Card` e' gia'
/// `Send + Sync`, quindi non serve altro.
pub struct FirmatarioCie {
    sm: Mutex<SecureMessaging>,
    card: PcscCard,
    certificato: Vec<u8>,
}

/// A che punto e' l'apertura della carta.
///
/// Serve a chi ha una faccia da aggiornare: la sequenza costa circa un secondo
/// e restare muti tutto quel tempo sembra un blocco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passo {
    /// Connessione al lettore e riconoscimento della carta.
    Connessione,
    /// Diffie-Hellman e autenticazione mutua DAPP.
    CanaleSicuro,
    /// VERIFY PIN dentro il canale.
    Pin,
    /// Lettura del certificato (file 0x1003). Se arriva, il PIN era giusto.
    Certificato,
}

impl CryptoSigner for FirmatarioCie {
    fn sign_hash(&self, hash: &[u8]) -> std::result::Result<Vec<u8>, CryptoError> {
        let hash: &[u8; 32] = hash.try_into().map_err(|_| {
            CryptoError::SigningFailed(format!("atteso SHA-256, ricevuti {} byte", hash.len()))
        })?;

        self.sm
            .lock()
            .expect("mutex del canale sicuro avvelenato")
            .sign_sha256(&self.card, hash)
            .map_err(|e| CryptoError::SigningFailed(e.to_string()))
    }

    fn certificate_der(&self) -> &[u8] {
        &self.certificato
    }

    fn certificate_chain_der(&self) -> Vec<&[u8]> {
        // La carta espone solo la propria foglia: le CA del Ministero non ci
        // sono. Basta per il livello B-B; per LTV servirebbero anche quelle.
        vec![&self.certificato]
    }

    fn digest_algorithm(&self) -> DigestAlgorithm {
        DigestAlgorithm::Sha256
    }

    fn signature_algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::RsaPkcs1v15
    }
}

/// Canale sicuro, PIN e certificato: tutto quello che serve prima di firmare.
///
/// Il contesto resta al chiamante perche' chiudendolo si invalida l'handle
/// della carta.
///
/// `avanzamento` viene chiamato *prima* di ogni fase, mai dopo: chi stampa a
/// terminale e chi muove una barra vogliono sapere cosa sta per succedere, non
/// cosa e' gia' successo.
pub fn apri_e_autentica(
    ctx: &PcscContext,
    pin: &str,
    avanzamento: &dyn Fn(Passo),
) -> Result<FirmatarioCie> {
    avanzamento(Passo::Connessione);
    let card = ctx.connect_to_cie()?;
    let cie_type = CieType::from_atr(&card.get_atr()?);
    CieCard::new(&card)?;

    avanzamento(Passo::CanaleSicuro);
    let mut sm = secure_messaging::establish_secure_messaging(&card, cie_type)?;

    avanzamento(Passo::Pin);
    sm.verify_pin(&card, pin)?;

    avanzamento(Passo::Certificato);
    let certificato = sm.read_file(&card, 0x1003)?;

    Ok(FirmatarioCie {
        sm: Mutex::new(sm),
        card,
        certificato,
    })
}

/// Sta al posto della carta: riceve un hash, costruisce il DigestInfo e applica
/// padding PKCS#1 v1.5 + RSA. E' esattamente cio' che fa `sign_sha256`, quindi
/// il percorso di firma esercitato dai test e' quello vero.
#[cfg(test)]
pub mod finto {
    use super::*;

    pub struct FirmatarioFinto {
        key: rsa::RsaPrivateKey,
        cert: Vec<u8>,
    }

    const PREFISSO_DIGESTINFO_SHA256: [u8; 19] = [
        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
        0x05, 0x00, 0x04, 0x20,
    ];

    impl CryptoSigner for FirmatarioFinto {
        fn sign_hash(&self, hash: &[u8]) -> std::result::Result<Vec<u8>, CryptoError> {
            let mut digest_info = PREFISSO_DIGESTINFO_SHA256.to_vec();
            digest_info.extend_from_slice(hash);
            self.key
                .sign(rsa::Pkcs1v15Sign::new_unprefixed(), &digest_info)
                .map_err(|e| CryptoError::SigningFailed(e.to_string()))
        }
        fn certificate_der(&self) -> &[u8] {
            &self.cert
        }
        fn certificate_chain_der(&self) -> Vec<&[u8]> {
            vec![&self.cert]
        }
        fn digest_algorithm(&self) -> DigestAlgorithm {
            DigestAlgorithm::Sha256
        }
        fn signature_algorithm(&self) -> SignatureAlgorithm {
            SignatureAlgorithm::RsaPkcs1v15
        }
    }

    pub fn firmatario_finto() -> FirmatarioFinto {
        use der::{Decode, Encode};
        use rsa::pkcs1v15::SigningKey;
        use sha2::Sha256;
        use signature::Keypair;
        use spki::{EncodePublicKey, SubjectPublicKeyInfoOwned};
        use x509_cert::builder::{Builder, CertificateBuilder, Profile};
        use x509_cert::{name::RdnSequence, serial_number::SerialNumber, time::Validity};

        let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
        let sk = SigningKey::<Sha256>::new(key.clone());
        let spki = SubjectPublicKeyInfoOwned::from_der(
            sk.verifying_key().to_public_key_der().unwrap().as_bytes(),
        )
        .unwrap();
        let subject: RdnSequence = "CN=MARIO ROSSI".parse().unwrap();
        let cert = CertificateBuilder::new(
            Profile::Root,
            SerialNumber::from(1u32),
            Validity::from_now(std::time::Duration::from_secs(86_400)).unwrap(),
            subject,
            spki,
            &sk,
        )
        .unwrap()
        .build::<rsa::pkcs1v15::Signature>()
        .unwrap()
        .to_der()
        .unwrap();

        FirmatarioFinto { key, cert }
    }
}
