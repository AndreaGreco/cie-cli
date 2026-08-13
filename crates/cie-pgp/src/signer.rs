//! L'adapter fra la carta e Sequoia.
//!
//! È l'unico punto in cui i due mondi si toccano, e fa una cosa sola: prendere
//! il digest che Sequoia ha calcolato, passarlo alla carta, reimpacchettare i
//! byte che tornano.
//!
//! La conversione è banale perché i due formati coincidono già: la CIE riceve un
//! `DigestInfo(SHA-256)` e restituisce PKCS#1 v1.5, che è esattamente com'è
//! fatta una firma RSA OpenPGP. Non c'è nessuna traduzione da fare, solo
//! l'involucro da cambiare.

use anyhow::{anyhow, Result};
use cie_firma::CryptoSigner;
use cie_lib::titolare::{inizio_validita, ChiavePubblicaRsa};
use sequoia_openpgp::crypto::{mpi, Signer};
use sequoia_openpgp::packet::key::{Key4, PublicParts, UnspecifiedRole};
use sequoia_openpgp::packet::Key;
use sequoia_openpgp::types::HashAlgorithm;

/// La chiave della CIE vista come firmatario OpenPGP.
///
/// Non possiede la carta: la prende in prestito per il tempo della
/// certificazione, canale sicuro e PIN già a posto.
pub struct CieSequoiaSigner<'a> {
    card: &'a dyn CryptoSigner,
    key: Key<PublicParts, UnspecifiedRole>,
}

impl<'a> CieSequoiaSigner<'a> {
    /// Costruisce il firmatario dalla chiave pubblica del certificato X.509.
    ///
    /// L'istante di creazione è il `notBefore` del certificato, non l'ora
    /// corrente: la fingerprint OpenPGP dipende da quel valore, e con l'ora
    /// corrente ogni esecuzione produrrebbe una chiave diversa. Con `notBefore`
    /// la stessa carta dà sempre la stessa chiave.
    pub fn new(card: &'a dyn CryptoSigner) -> Result<Self> {
        let der = card.certificate_der();
        let rsa = ChiavePubblicaRsa::from_certificate_der(der)?;
        let key: Key<PublicParts, UnspecifiedRole> =
            Key4::import_public_rsa(&rsa.esponente, &rsa.modulo, inizio_validita(der)?)?.into();

        Ok(Self { card, key })
    }

    /// Il certificato X.509 della carta, per chi deve risalire alla PKI.
    pub fn certificate_der(&self) -> &[u8] {
        self.card.certificate_der()
    }
}

impl Signer for CieSequoiaSigner<'_> {
    fn public(&self) -> &Key<PublicParts, UnspecifiedRole> {
        &self.key
    }

    /// La carta sa fare solo SHA-256.
    ///
    /// Attenzione: `SignatureBuilder` non guarda questo elenco - lo consulta
    /// solo il firmatario di streaming - e da solo sceglierebbe SHA-512. Chi
    /// costruisce una firma deve quindi imporre l'algoritmo con
    /// `set_hash_algo`, e il controllo in `sign` è la rete che se ne accorge.
    fn acceptable_hashes(&self) -> &[HashAlgorithm] {
        &[HashAlgorithm::SHA256]
    }

    fn sign(&mut self, hash_algo: HashAlgorithm, digest: &[u8]) -> Result<mpi::Signature> {
        if hash_algo != HashAlgorithm::SHA256 {
            return Err(anyhow!("la CIE firma solo digest SHA-256, richiesto {hash_algo}"));
        }

        let firma = self
            .card
            .sign_hash(digest)
            .map_err(|e| anyhow!("firma sulla carta fallita: {e}"))?;

        Ok(mpi::Signature::RSA {
            s: mpi::MPI::new(&firma),
        })
    }
}
