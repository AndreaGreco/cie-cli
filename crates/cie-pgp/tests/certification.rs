//! La certificazione OpenPGP esercitata senza CIE.
//!
//! `fake::fake_signer` di `cie-firma` implementa lo stesso contratto della carta
//! - riceve un hash, costruisce il DigestInfo, applica PKCS#1 v1.5 - quindi il
//! percorso che questi test attraversano è lo stesso della carta vera, meno il
//! lettore.
//!
//! Il test di interoperabilità con GnuPG si salta da solo se `gpg` non c'è: un
//! ambiente senza GnuPG non è un fallimento, ma nemmeno un successo silenzioso.

use cie_firma::{fake, CryptoSigner};
use cie_lib::DatiTitolare;
use cie_pgp::{armored, certifier_cert, certify, holder_matches, CieSequoiaSigner, Parse};
use sequoia_openpgp::cert::{Cert, CertBuilder};
use sequoia_openpgp::packet::UserID;
use sequoia_openpgp::types::SignatureType;

const CARTA: &str = "1234567890";

/// La chiave di chi si fa certificare.
fn chiave_utente() -> Cert {
    CertBuilder::new()
        .add_userid("Mario Rossi <mario@example.com>")
        .add_signing_subkey()
        .generate()
        .unwrap()
        .0
}

fn certificatore(card: &dyn CryptoSigner) -> (CieSequoiaSigner<'_>, Cert) {
    let mut signer = CieSequoiaSigner::new(card).unwrap();
    let cert = certifier_cert(&mut signer, Some(CARTA)).unwrap();
    (signer, cert)
}

#[test]
fn il_certificatore_e_autoportante() {
    let card = fake::fake_signer();
    let (_, cert) = certificatore(&card);

    // Round-trip attraverso il parser: se GnuPG lo rifiuta, lo rifiuta qui.
    let riletto = Cert::from_bytes(&armored(&cert).unwrap()).unwrap();
    assert_eq!(riletto.fingerprint(), cert.fingerprint());

    let uid = riletto.userids().next().expect("nessuna User ID");
    assert!(
        uid.userid().value().ends_with(format!("(CIE {CARTA})").as_bytes()),
        "la User ID deve portare il numero della carta: {:?}",
        String::from_utf8_lossy(uid.userid().value())
    );

    // La self-signature deve verificare contro la chiave stessa.
    let sig = uid.self_signatures().next().expect("nessuna self-signature");
    sig.verify_userid_binding(riletto.primary_key().key(), riletto.primary_key().key(), uid.userid())
        .expect("self-signature non valida");
}

#[test]
fn la_user_id_del_certificatore_non_espone_dati_riservati() {
    let card = fake::fake_signer();
    let (_, cert) = certificatore(&card);

    let uid = String::from_utf8(cert.userids().next().unwrap().userid().value().to_vec()).unwrap();

    assert!(!uid.contains('@'), "nessuna email: la CIE non ne attesta {uid:?}");
    // Il codice fiscale del certificato di prova, che non deve trapelare qui.
    let cf = DatiTitolare::from_certificate_der(card.certificate_der())
        .and_then(|d| d.codice_fiscale);
    if let Some(cf) = cf {
        assert!(!uid.contains(&cf), "il codice fiscale non va nella User ID: {uid:?}");
    }
}

#[test]
fn il_certificatore_e_deterministico() {
    let card = fake::fake_signer();
    let (_, primo) = certificatore(&card);
    let (_, secondo) = certificatore(&card);

    // Creation time dal notBefore invece che dall'ora corrente: rieseguire il
    // comando non deve creare una seconda chiave che confonde chi verifica.
    //
    // I byte no: Sequoia mette un salt casuale in ogni firma v4 apposta, per
    // renderla imprevedibile. E' l'identita' della chiave a dover essere
    // stabile, non la serializzazione.
    assert_eq!(primo.fingerprint(), secondo.fingerprint());
    assert_eq!(
        primo.userids().map(|u| u.userid().clone()).collect::<Vec<_>>(),
        secondo.userids().map(|u| u.userid().clone()).collect::<Vec<_>>()
    );
}

#[test]
fn la_certification_verifica() {
    let card = fake::fake_signer();
    let (mut signer, certificatore) = certificatore(&card);
    let utente = chiave_utente();
    let uid = utente.userids().next().unwrap().userid().clone();

    let firmato = certify(&utente, &uid, &mut signer).unwrap();

    let amalgama = firmato
        .userids()
        .find(|u| u.userid() == &uid)
        .expect("User ID sparita");
    let sig = amalgama
        .certifications()
        .next()
        .expect("nessuna certification di terzi");

    assert_eq!(sig.typ(), SignatureType::GenericCertification);
    sig.verify_userid_binding(
        certificatore.primary_key().key(),
        firmato.primary_key().key(),
        &uid,
    )
    .expect("certification non valida");
}

#[test]
fn la_certification_dichiara_cosa_e_stato_verificato() {
    let card = fake::fake_signer();
    let (mut signer, _) = certificatore(&card);
    let utente = chiave_utente();
    let uid = utente.userids().next().unwrap().userid().clone();

    let firmato = certify(&utente, &uid, &mut signer).unwrap();
    let sig = firmato
        .userids()
        .next()
        .unwrap()
        .certifications()
        .next()
        .unwrap()
        .clone();

    let notation = |nome: &str| {
        sig.hashed_area()
            .subpackets(sequoia_openpgp::packet::signature::subpacket::SubpacketTag::NotationData)
            .filter_map(|s| match s.value() {
                sequoia_openpgp::packet::signature::subpacket::SubpacketValue::NotationData(n) => {
                    Some(n)
                }
                _ => None,
            })
            .find(|n| n.name() == nome)
            .map(|n| String::from_utf8_lossy(n.value()).into_owned())
    };

    assert_eq!(notation("cie-verified@cie-cli").as_deref(), Some("holder-name"));
    assert_eq!(notation("cie-profile@cie-cli").as_deref(), Some("v1"));

    // Impegno vincolante sul certificato X.509, che non viene incorporato.
    use sha2::{Digest, Sha256};
    let atteso = hex::encode(Sha256::digest(card.certificate_der()));
    assert_eq!(notation("cie-x509-sha256@cie-cli").as_deref(), Some(atteso.as_str()));

    // Il DER stesso non deve essere finito nella firma: contiene il CF.
    let byte = armored(&firmato).unwrap();
    assert!(
        !byte.windows(64).any(|w| w == &card.certificate_der()[..64]),
        "il certificato X.509 non va incorporato"
    );
}

#[test]
fn la_chiave_originale_resta_intatta() {
    let card = fake::fake_signer();
    let (mut signer, _) = certificatore(&card);
    let utente = chiave_utente();
    let uid = utente.userids().next().unwrap().userid().clone();

    let firmato = certify(&utente, &uid, &mut signer).unwrap();

    assert_eq!(firmato.fingerprint(), utente.fingerprint());
    assert_eq!(
        firmato.userids().map(|u| u.userid().clone()).collect::<Vec<_>>(),
        utente.userids().map(|u| u.userid().clone()).collect::<Vec<_>>()
    );
    assert_eq!(
        firmato.keys().subkeys().count(),
        utente.keys().subkeys().count(),
        "le subkey di firma devono sopravvivere"
    );
    assert_eq!(
        firmato.clone().into_packets().count(),
        utente.clone().into_packets().count() + 1,
        "deve essere stata aggiunta esattamente una signature"
    );
}

#[test]
fn la_carta_accetta_solo_sha256() {
    use sequoia_openpgp::crypto::Signer;
    use sequoia_openpgp::types::HashAlgorithm;

    let card = fake::fake_signer();
    let mut signer = CieSequoiaSigner::new(&card).unwrap();

    assert_eq!(signer.acceptable_hashes(), &[HashAlgorithm::SHA256]);
    assert!(signer.sign(HashAlgorithm::SHA512, &[0u8; 64]).is_err());
    assert!(signer.sign(HashAlgorithm::SHA256, &[0u8; 32]).is_ok());
}

#[test]
fn il_confronto_anagrafico_ignora_ordine_accenti_e_maiuscole() {
    let dati = |cognome: &str, nome: &str| DatiTitolare {
        cognome: Some(cognome.into()),
        nome: Some(nome.into()),
        ..Default::default()
    };

    let uid = UserID::from("Mario Rossi <mario@example.com>");
    assert!(holder_matches(&dati("ROSSI", "MARIO"), &uid));
    assert!(holder_matches(&dati("Rossi", "Mario"), &uid));
    assert!(!holder_matches(&dati("BIANCHI", "MARIO"), &uid));

    let accentato = UserID::from("Niccolò Nicolò <n@example.com>");
    assert!(holder_matches(&dati("NICOLO", "NICCOLO"), &accentato));

    // Nome mancante sulla carta: nessuna corrispondenza da affermare.
    assert!(!holder_matches(&DatiTitolare::default(), &uid));

    // Solo il CN, come su alcuni certificati.
    let solo_cn = DatiTitolare {
        nome_completo: Some("ROSSI/MARIO".into()),
        ..Default::default()
    };
    assert!(holder_matches(&solo_cn, &uid));
}

/// GnuPG deve verificare la certification e continuare a usare la chiave.
///
/// La chiave la genera GnuPG, non Sequoia: cosi' il test copre anche il caso che
/// conta davvero per chi la usa - la chiave con cui si firmano i commit deve
/// restare utilizzabile dopo essere passata di qui.
///
/// Si salta se `gpg` non e' installato: un ambiente senza GnuPG non e' un
/// fallimento, ma non deve nemmeno passare in silenzio.
#[test]
fn gnupg_verifica_la_certification_e_continua_a_firmare() {
    let Some(gpg) = Gpg::nuovo() else {
        eprintln!("gpg non installato: test di interoperabilita' saltato");
        return;
    };

    let out = gpg.esegui(
        &[
            "--passphrase", "", "--quick-gen-key",
            "Mario Rossi <mario@example.com>", "default", "default", "never",
        ],
        None,
    );
    assert!(out.status.success(), "generazione: {}", String::from_utf8_lossy(&out.stderr));

    let esportata = gpg.esegui(&["--armor", "--export"], None);
    let utente = Cert::from_bytes(&esportata.stdout).unwrap();
    let uid = utente.userids().next().unwrap().userid().clone();

    // La firma prima della certificazione: serve come metro di paragone.
    let prima = gpg.esegui(&["--clearsign", "--pinentry-mode", "loopback"], Some(b"commit"));
    assert!(prima.status.success(), "la chiave non firmava nemmeno prima");

    let card = fake::fake_signer();
    let (mut signer, certificatore) = certificatore(&card);
    let firmato = certify(&utente, &uid, &mut signer).unwrap();

    for cert in [&certificatore, &firmato] {
        let out = gpg.esegui(&["--import"], Some(&armored(cert).unwrap()));
        assert!(out.status.success(), "import: {}", String::from_utf8_lossy(&out.stderr));
    }

    // `sig:!` e' GnuPG che dice di aver verificato la firma, non solo di averla vista.
    let out = gpg.esegui(&["--check-sigs", "--with-colons", &firmato.fingerprint().to_hex()], None);
    let testo = String::from_utf8_lossy(&out.stdout);
    let keyid = certificatore.keyid().to_hex();
    assert!(
        testo.lines().any(|l| l.starts_with("sig:!") && l.contains(&keyid)),
        "GnuPG non ha verificato la certification.\nkeyid atteso: {keyid}\n{testo}"
    );

    // E la chiave deve restare quella di prima: firma ancora, e la firma verifica.
    let dopo = gpg.esegui(&["--clearsign", "--pinentry-mode", "loopback"], Some(b"commit"));
    assert!(
        dopo.status.success(),
        "la chiave non firma piu' dopo la certificazione: {}",
        String::from_utf8_lossy(&dopo.stderr)
    );
    let verifica = gpg.esegui(&["--verify"], Some(&dopo.stdout));
    assert!(
        verifica.status.success(),
        "firma non verificabile dopo la certificazione: {}",
        String::from_utf8_lossy(&verifica.stderr)
    );
}

/// Un GnuPG usa e getta, con la sua casa in una directory temporanea.
struct Gpg(std::path::PathBuf);

impl Gpg {
    fn nuovo() -> Option<Self> {
        use std::process::Command;
        Command::new("gpg").arg("--version").output().ok()?;

        let home = std::env::temp_dir().join(format!("cie-pgp-{}", std::process::id()));
        std::fs::create_dir_all(&home).ok()?;
        // GnuPG si rifiuta di lavorare in una directory leggibile da altri.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).ok()?;
        }
        Some(Self(home))
    }

    fn esegui(&self, args: &[&str], stdin: Option<&[u8]>) -> std::process::Output {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut figlio = Command::new("gpg")
            .env("GNUPGHOME", &self.0)
            .args(["--batch", "--no-tty", "--yes"])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("gpg non eseguibile");

        if let Some(dati) = stdin {
            figlio.stdin.as_mut().unwrap().write_all(dati).unwrap();
        }
        figlio.wait_with_output().unwrap()
    }
}

impl Drop for Gpg {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
