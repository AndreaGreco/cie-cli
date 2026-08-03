//! The signer that lives on the card, shared by every signature container
//! (PAdES, CAdES, detached) and by every front end.

use anyhow::Result;
use cie_lib::{secure_messaging, CieCard, CieType, PcscCard, PcscContext, SecureMessaging};
use std::sync::Mutex;
use underskrift::error::CryptoError;
use underskrift::{CryptoSigner, DigestAlgorithm, SignatureAlgorithm};

/// A signer backed by the card.
///
/// underskrift never sees a key: we hand it this, it hands us a digest and
/// takes back 256 bytes of signature. Same scheme HSMs and remote signing use,
/// and the reason that library works at all with a non-extractable key.
///
/// The `Mutex` is there because the trait asks for `&self` while `sign_sha256`
/// wants `&mut SecureMessaging` (the SSC advances on every command).
/// `pcsc::Card` is already `Send + Sync`, so nothing else is needed.
pub struct CieSigner {
    sm: Mutex<SecureMessaging>,
    card: PcscCard,
    certificate: Vec<u8>,
}

/// How far along opening the card is.
///
/// Meant for anything with a face to update: the sequence costs about a second
/// and staying silent that long reads as a freeze.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Connecting to the reader and identifying the card.
    Connecting,
    /// Diffie-Hellman and mutual DAPP authentication.
    SecureChannel,
    /// VERIFY PIN inside the channel.
    Pin,
    /// Reading the certificate (file 0x1003). If we get here, the PIN was good.
    Certificate,
}

impl CryptoSigner for CieSigner {
    fn sign_hash(&self, hash: &[u8]) -> std::result::Result<Vec<u8>, CryptoError> {
        let hash: &[u8; 32] = hash.try_into().map_err(|_| {
            CryptoError::SigningFailed(format!("expected SHA-256, got {} bytes", hash.len()))
        })?;

        self.sm
            .lock()
            .expect("secure channel mutex poisoned")
            .sign_sha256(&self.card, hash)
            .map_err(|e| CryptoError::SigningFailed(e.to_string()))
    }

    fn certificate_der(&self) -> &[u8] {
        &self.certificate
    }

    fn certificate_chain_der(&self) -> Vec<&[u8]> {
        // The card only exposes its own leaf: the Ministry CAs are not there.
        // Enough for level B-B; LTV would need them too.
        vec![&self.certificate]
    }

    fn digest_algorithm(&self) -> DigestAlgorithm {
        DigestAlgorithm::Sha256
    }

    fn signature_algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::RsaPkcs1v15
    }
}

/// Secure channel, PIN and certificate: everything needed before signing.
///
/// The context stays with the caller because dropping it invalidates the card
/// handle.
///
/// `progress` is called *before* each phase, never after: whoever prints to a
/// terminal and whoever moves a spinner both want to know what is about to
/// happen, not what already did.
pub fn open_and_authenticate(
    ctx: &PcscContext,
    pin: &str,
    progress: &dyn Fn(Step),
) -> Result<CieSigner> {
    progress(Step::Connecting);
    let card = ctx.connect_to_cie()?;
    let cie_type = CieType::from_atr(&card.get_atr()?);
    CieCard::new(&card)?;

    progress(Step::SecureChannel);
    let mut sm = secure_messaging::establish_secure_messaging(&card, cie_type)?;

    progress(Step::Pin);
    sm.verify_pin(&card, pin)?;

    progress(Step::Certificate);
    let certificate = sm.read_file(&card, 0x1003)?;

    Ok(CieSigner {
        sm: Mutex::new(sm),
        card,
        certificate,
    })
}

/// Stands in for the card: takes a hash, builds the DigestInfo, applies PKCS#1
/// v1.5 padding and RSA. Exactly what `sign_sha256` does, so the signing path
/// the tests exercise is the real one.
#[cfg(test)]
pub mod fake {
    use super::*;

    pub struct FakeSigner {
        key: rsa::RsaPrivateKey,
        cert: Vec<u8>,
    }

    const SHA256_DIGESTINFO_PREFIX: [u8; 19] = [
        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
        0x05, 0x00, 0x04, 0x20,
    ];

    impl CryptoSigner for FakeSigner {
        fn sign_hash(&self, hash: &[u8]) -> std::result::Result<Vec<u8>, CryptoError> {
            let mut digest_info = SHA256_DIGESTINFO_PREFIX.to_vec();
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

    pub fn fake_signer() -> FakeSigner {
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

        FakeSigner { key, cert }
    }
}
