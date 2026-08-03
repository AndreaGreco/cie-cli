//! CAdES enveloping: the file *and* the signature together in one DER.
//!
//! This is the `.p7m` of Italian practice. Unlike PAdES it works on any file,
//! not just PDFs.
//!
//! Level CAdES B-B. No timestamp, for the same reason as PAdES.

use anyhow::{Context, Result};
use underskrift::{cades, CryptoSigner};

/// Wraps content and signature into a CAdES enveloping structure.
pub fn sign_p7m(content: &[u8], signer: &dyn CryptoSigner) -> Result<Vec<u8>> {
    // CAdES keeps signingTime inside the CMS, where PAdES forbids it.
    let now = chrono::Local::now().naive_local();

    let envelope = cades::sign_attached(content, signer, Some(now)).context("CAdES signing")?;

    // The envelope must give back exactly what went in, otherwise the
    // signature covers something the recipient will never see.
    // ponytail: this only rechecks the encapsulation, not the crypto: for that
    // underskrift's `verify` feature does not compile without `tsp` (which
    // drags reqwest back in). The tests verify the signature with OpenSSL.
    let recovered = cades::extract_content(&envelope).context("reading the envelope back")?;
    anyhow::ensure!(
        recovered == content,
        "the envelope does not return the original content"
    );

    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signer::fake::fake_signer;
    use std::process::Command;

    /// Verifies the envelope with OpenSSL: a verdict independent of the
    /// library that produced it. `-noverify` skips the chain (the test
    /// certificate is self-signed), not the signature.
    fn openssl_accepts(envelope: &[u8]) -> bool {
        let dir = std::env::temp_dir().join(format!("cie-p7m-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(format!("{:x}.p7m", envelope.len()));
        std::fs::write(&file, envelope).unwrap();

        let outcome = Command::new("openssl")
            .args(["cms", "-verify", "-inform", "DER", "-noverify", "-in"])
            .arg(&file)
            .arg("-out")
            .arg(dir.join("content"))
            .output()
            .expect("openssl not found: needed to verify the envelope");

        let _ = std::fs::remove_dir_all(&dir);
        outcome.status.success()
    }

    #[test]
    fn the_envelope_holds_the_file_and_openssl_verifies_its_signature() {
        let content = b"any contract, it does not have to be a PDF";
        let envelope = sign_p7m(content, &fake_signer()).unwrap();

        // Enveloping, not detached: the content lives inside.
        assert_eq!(cades::extract_content(&envelope).unwrap(), content);
        assert!(openssl_accepts(&envelope), "OpenSSL rejects the envelope");
    }

    /// One changed byte in the encapsulated content must break verification:
    /// it is the only thing separating a signature from trailing bytes.
    #[test]
    fn a_tampered_envelope_no_longer_verifies() {
        let content = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_vec();
        let mut envelope = sign_p7m(&content, &fake_signer()).unwrap();

        let at = envelope
            .windows(content.len())
            .position(|w| w == content.as_slice())
            .expect("encapsulated content");
        envelope[at] = b'B';

        assert!(!openssl_accepts(&envelope), "OpenSSL accepts a tampered envelope");
    }
}
