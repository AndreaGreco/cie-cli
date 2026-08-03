//! Raw RSA signature, kept apart from the document.
//!
//! Not a container: 256 bytes of PKCS#1 v1.5 over the file's SHA-256 digest.
//! It is the brick PAdES and CAdES are built from, and the only form that can
//! be checked with two OpenSSL commands and nothing else.

use anyhow::{anyhow, ensure, Result};
use cie_lib::titolare::ChiavePubblicaRsa;
use underskrift::CryptoSigner;

/// Signs the content's SHA-256 digest and rechecks the result locally.
///
/// Verifying against the certificate's public key is not ceremony: if it does
/// not add up the problem is ours, and there is no point sending the signature
/// to an external validator. It costs microseconds and the card has already
/// done its part.
pub fn sign_detached(content: &[u8], signer: &dyn CryptoSigner) -> Result<Vec<u8>> {
    let hash = cie_lib::crypto::sha256(content);

    let signature = signer
        .sign_hash(&hash)
        .map_err(|e| anyhow!("signing on the card: {e}"))?;

    let public = ChiavePubblicaRsa::from_certificate_der(signer.certificate_der())?;
    ensure!(
        public.verifica_sha256(&hash, &signature),
        "la firma non verifica con la chiave pubblica del certificato: \
         qualcosa non torna nella catena"
    );

    Ok(signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signer::fake::fake_signer;

    #[test]
    fn the_signature_verifies_against_the_certificate_public_key() {
        let signer = fake_signer();
        let signature = sign_detached(b"any content", &signer).unwrap();
        assert_eq!(signature.len(), 256, "RSA 2048 produces 256 bytes");
    }

    /// The internal check has to bite: a signer handing back random bytes must
    /// not pass for a valid signature.
    #[test]
    fn a_signature_that_does_not_verify_is_an_error() {
        struct Liar(Vec<u8>);
        impl CryptoSigner for Liar {
            fn sign_hash(
                &self,
                _: &[u8],
            ) -> std::result::Result<Vec<u8>, underskrift::error::CryptoError> {
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

        let cert = fake_signer().certificate_der().to_vec();
        assert!(sign_detached(b"content", &Liar(cert)).is_err());
    }
}
