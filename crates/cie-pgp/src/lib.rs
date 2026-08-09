//! Certificazione OpenPGP con la chiave di firma della CIE.
//!
//! Sta sopra `cie-firma` e sotto `cie-cli`. Vede la carta solo attraverso il
//! trait [`CryptoSigner`](cie_firma::CryptoSigner): nessuna APDU, nessun Secure
//! Messaging, nessun PIN passa di qui. Simmetricamente, nessun packet OpenPGP
//! entra nei livelli sottostanti.
//!
//! Crate a sé e non modulo di `cie-firma` per una ragione pratica: `cie-gui`
//! dipende da `cie-firma`, e Sequoia lì dentro finirebbe nella build della GUI
//! senza servire a niente.
//!
//! Due artefatti, due significati diversi:
//!
//! - [`certifier_cert`] — la chiave della CIE come certificato OpenPGP
//!   autoportante, auto-firmato dalla carta. Senza questo GnuPG non ha con cosa
//!   verificare la certification, e la mostra come `sig?  [no public key]`.
//! - [`certify`] — la certification vera e propria, aggiunta alla chiave altrui.

mod signer;

use anyhow::{Context, Result};
use cie_lib::DatiTitolare;
use sequoia_openpgp::cert::Cert;
use sequoia_openpgp::crypto::Signer;
use sequoia_openpgp::packet::signature::subpacket::NotationDataFlags;
use sequoia_openpgp::packet::{signature::SignatureBuilder, Packet, UserID};
use sequoia_openpgp::serialize::SerializeInto;
use sequoia_openpgp::types::{HashAlgorithm, KeyFlags, SignatureType};
use sha2::{Digest, Sha256};

pub use sequoia_openpgp::cert::Cert as OpenPgpCert;
pub use sequoia_openpgp::packet::UserID as OpenPgpUserID;
pub use sequoia_openpgp::parse::Parse;
pub use signer::CieSequoiaSigner;

/// Versione del profilo di certificazione, dichiarata nella firma.
///
/// `v1` significa: verificato il nome del titolare contro i dati della CIE.
/// Nient'altro. In particolare **non** l'indirizzo email.
const PROFILO: &str = "v1";

/// La chiave della CIE come certificato OpenPGP autoportante.
///
/// Primary key presa dal certificato X.509, una User ID con i dati anagrafici
/// letti dalla carta, e la self-signature prodotta dalla carta stessa.
///
/// La fingerprint è stabile: l'istante di creazione viene dal `notBefore` del
/// certificato e non dall'ora corrente, quindi rieseguire il comando non crea
/// una seconda chiave che confonde chi verifica. I byte invece cambiano a ogni
/// esecuzione — Sequoia mette un salt casuale in ogni firma v4 apposta, per
/// renderla imprevedibile — e va bene così: le due self-signature si fondono.
///
/// Key flags: solo certification. Questa chiave non cifra e non firma
/// documenti, e dirlo evita che qualcuno ci provi.
pub fn certifier_cert(signer: &mut CieSequoiaSigner<'_>, card_id: Option<&str>) -> Result<Cert> {
    let uid = UserID::from(certifier_userid(signer.certificate_der(), card_id));
    let primary = signer.public().clone().role_into_primary();
    let creata = primary.creation_time();

    let sig = SignatureBuilder::new(SignatureType::PositiveCertification)
        .set_hash_algo(HashAlgorithm::SHA256)
        .set_signature_creation_time(creata)?
        .set_key_flags(KeyFlags::empty().set_certification())?
        .set_preferred_hash_algorithms(vec![HashAlgorithm::SHA256])?
        .set_primary_userid(true)?
        .sign_userid_binding(signer, &primary, &uid)?;

    Cert::from_packets(vec![Packet::from(primary), Packet::from(uid), Packet::from(sig)].into_iter())
        .context("costruzione del certificato OpenPGP della CIE")
}

/// La User ID del certificatore CIE.
///
/// Nome, cognome e numero della carta. Nessuna email, perché la CIE non ne
/// attesta nessuna e una User ID senza email non fa asserzioni implicite.
/// Nessun codice fiscale: questa stringa è la cosa più pubblica dell'intero
/// artefatto, finisce sui keyserver e da lì non si toglie più.
fn certifier_userid(der: &[u8], card_id: Option<&str>) -> String {
    let dati = DatiTitolare::from_certificate_der(der).unwrap_or_default();

    let nome = match (&dati.cognome, &dati.nome) {
        (Some(cognome), Some(nome)) => format!("{cognome} {nome}"),
        _ => dati
            .nome_completo
            .unwrap_or_else(|| "Titolare CIE".to_string()),
    };

    match card_id {
        Some(id) => format!("{nome} (CIE {id})"),
        None => format!("{nome} (CIE)"),
    }
}

/// Aggiunge alla chiave `target` la certification della User ID scelta.
///
/// Tipo 0x10 Generic, non 0x13 Positive. 0x13 dichiara che l'emittente ha
/// verificato accuratamente la User ID, e la User ID comprende l'email: la CIE
/// verifica il nome del titolare, dell'email non sa niente. 0x10 non fa
/// asserzioni sul grado di verifica, quindi non promette ciò che non possiamo
/// mantenere.
///
/// Una certification OpenPGP lega la User ID intera e non è divisibile: le
/// notation sono l'unico meccanismo standard per dire quale parte sia stata
/// verificata davvero.
pub fn certify(target: &Cert, uid: &UserID, signer: &mut CieSequoiaSigner<'_>) -> Result<Cert> {
    let testo = NotationDataFlags::empty().set_human_readable();

    let sig = SignatureBuilder::new(SignatureType::GenericCertification)
        // Senza questo il builder sceglierebbe SHA-512, che la carta non sa
        // fare: `acceptable_hashes` lo guarda solo il firmatario di streaming.
        .set_hash_algo(HashAlgorithm::SHA256)
        .set_exportable_certification(true)?
        // Ciò che non è elencato qui non è attestato dalla CIE.
        .add_notation("cie-verified@cie-cli", "holder-name", testo.clone(), false)?
        // Impegno vincolante sul certificato X.509: il DER non viene incorporato
        // (contiene il codice fiscale in chiaro), ma chi lo riceverà potrà
        // verificare che sia esattamente quello legato a questa firma.
        .add_notation(
            "cie-x509-sha256@cie-cli",
            hex::encode(Sha256::digest(signer.certificate_der())),
            testo.clone(),
            false,
        )?
        .add_notation("cie-profile@cie-cli", PROFILO, testo, false)?
        .sign_userid_binding(signer, target.primary_key().key(), uid)?;

    let (cert, aggiunta) = target.clone().insert_packets(sig)?;
    if !aggiunta {
        anyhow::bail!("questa User ID risulta già certificata da questa CIE");
    }
    Ok(cert)
}

/// Il nome sulla CIE e quello nella User ID indicano la stessa persona?
///
/// Confronto per insiemi di parole normalizzate, così l'ordine nome/cognome, le
/// maiuscole, gli accenti e la punteggiatura non contano. Un `false` non è un
/// errore — certificare uno pseudonimo è legittimo — ma deve essere una scelta
/// deliberata di chi usa il comando, non un caso.
pub fn holder_matches(dati: &DatiTitolare, uid: &UserID) -> bool {
    let Ok(Some(nome_uid)) = uid.name() else {
        return false;
    };

    // Il CN va usato come ripiego perche' non tutti i certificati CIE portano
    // givenName e surname separati.
    let cie = match (&dati.cognome, &dati.nome) {
        (Some(cognome), Some(nome)) => format!("{cognome} {nome}"),
        _ => dati.nome_completo.clone().unwrap_or_default(),
    };

    !cie.trim().is_empty() && parole(&cie) == parole(nome_uid)
}

/// Le parole di un nome, normalizzate e ordinate.
fn parole(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|p| !p.is_empty())
        .map(|p| p.chars().map(senza_accento).collect())
        .collect();
    v.sort();
    v
}

/// Minuscola e senza accento, per i soli caratteri latini che compaiono nei nomi
/// italiani. Una normalizzazione Unicode completa qui non servirebbe a nulla e
/// costerebbe una dipendenza.
fn senza_accento(c: char) -> char {
    let c = c.to_ascii_lowercase();
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        altro => altro.to_lowercase().next().unwrap_or(altro),
    }
}

/// Il certificato in armor ASCII, pronto per `gpg --import`.
pub fn armored(cert: &Cert) -> Result<Vec<u8>> {
    cert.armored().to_vec().context("serializzazione OpenPGP")
}
