//! Firma RSA grezza, separata dal documento.
//!
//! Non e' un contenitore: sono 256 byte di PKCS#1 v1.5 sul digest SHA-256 del
//! file. E' il mattone su cui PAdES e CAdES si costruiscono, ed e' anche l'unica
//! forma che si verifica con due comandi di OpenSSL e niente altro.

use anyhow::{anyhow, ensure, Result};
use cie_lib::titolare::ChiavePubblicaRsa;
use underskrift::CryptoSigner;

/// Firma il digest SHA-256 del contenuto e ricontrolla il risultato in locale.
///
/// La verifica con la pubblica del certificato non e' cerimonia: se non torna,
/// il problema e' nostro e non ha senso mandare la firma a un validatore
/// esterno. Costa qualche microsecondo e la carta ha gia' fatto la sua parte.
pub fn firma_detached(contenuto: &[u8], firmatario: &dyn CryptoSigner) -> Result<Vec<u8>> {
    let hash = cie_lib::crypto::sha256(contenuto);

    let firma = firmatario
        .sign_hash(&hash)
        .map_err(|e| anyhow!("firma sulla carta: {e}"))?;

    let pubblica = ChiavePubblicaRsa::from_certificate_der(firmatario.certificate_der())?;
    ensure!(
        pubblica.verifica_sha256(&hash, &firma),
        "la firma non verifica con la chiave pubblica del certificato: \
         qualcosa non torna nella catena"
    );

    Ok(firma)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firmatario::finto::firmatario_finto;

    #[test]
    fn la_firma_verifica_con_la_pubblica_del_certificato() {
        let firmatario = firmatario_finto();
        let firma = firma_detached(b"contenuto qualunque", &firmatario).unwrap();
        assert_eq!(firma.len(), 256, "RSA 2048 produce 256 byte");
    }

    /// La verifica interna deve mordere: un firmatario che restituisce byte a
    /// caso non deve poter passare per una firma valida.
    #[test]
    fn una_firma_che_non_verifica_e_un_errore() {
        struct Bugiardo(Vec<u8>);
        impl CryptoSigner for Bugiardo {
            fn sign_hash(&self, _: &[u8]) -> std::result::Result<Vec<u8>, underskrift::error::CryptoError> {
                Ok(vec![0x42; 256])
            }
            fn certificate_der(&self) -> &[u8] {
                &self.0
            }
            fn certificate_chain_der(&self) -> Vec<&[u8]> {
                vec![&self.0]
            }
            fn digest_algorithm(&self) -> underskrift::DigestAlgorithm {
                underskrift::DigestAlgorithm::Sha256
            }
            fn signature_algorithm(&self) -> underskrift::SignatureAlgorithm {
                underskrift::SignatureAlgorithm::RsaPkcs1v15
            }
        }

        let cert = firmatario_finto().certificate_der().to_vec();
        assert!(firma_detached(b"contenuto", &Bugiardo(cert)).is_err());
    }
}
