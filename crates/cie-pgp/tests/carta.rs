//! La stessa certificazione, con la carta vera.
//!
//! Ignorato di default: serve un lettore, una CIE inserita e il PIN in
//! `CIE_PIN`. È il solo test che dimostra che la firma esce davvero dalla carta
//! e non da un firmatario software.
//!
//!     CIE_PIN=12345678 cargo test -p cie-pgp --test carta -- --ignored --nocapture

use cie_firma::CryptoSigner;
use cie_lib::PcscContext;
use cie_pgp::{certifier_cert, certify, CieSequoiaSigner, OpenPgpCert, Parse};
use sequoia_openpgp::cert::CertBuilder;

#[test]
#[ignore = "richiede una CIE nel lettore e CIE_PIN nell'ambiente"]
fn la_carta_produce_una_certification_valida() {
    let pin = std::env::var("CIE_PIN").expect("serve CIE_PIN");

    let ctx = PcscContext::new().expect("nessun contesto PC/SC");
    let numero = cie_firma::identify(&ctx).ok();
    let carta = cie_firma::open_and_authenticate(&ctx, &pin, &|_| {}).expect("apertura della carta");

    let mut firmatario = CieSequoiaSigner::new(&carta).unwrap();
    let certificatore = certifier_cert(&mut firmatario, numero.as_deref()).unwrap();
    println!("certificatore: {}", certificatore.fingerprint());
    println!(
        "user id:       {}",
        String::from_utf8_lossy(certificatore.userids().next().unwrap().userid().value())
    );

    // La chiave della CIE deve essere la stessa che c'e' nel certificato X.509:
    // se l'adapter avesse sbagliato a importare modulo o esponente, la firma
    // seguente verificherebbe comunque contro la chiave sbagliata. Questo lo
    // esclude alla radice.
    let dal_certificato =
        cie_lib::titolare::ChiavePubblicaRsa::from_certificate_der(carta.certificate_der()).unwrap();
    let atteso = cie_lib::crypto::sha256(b"cie-pgp");
    assert!(
        dal_certificato.verifica_sha256(&atteso, &carta.sign_hash(&atteso).unwrap()),
        "la firma della carta non verifica con la pubblica del suo certificato"
    );

    let utente = CertBuilder::new()
        .add_userid("Prova CIE <prova@example.invalid>")
        .generate()
        .unwrap()
        .0;
    let uid = utente.userids().next().unwrap().userid().clone();

    let firmato = certify(&utente, &uid, &mut firmatario).unwrap();

    let sig = firmato
        .userids()
        .next()
        .unwrap()
        .certifications()
        .next()
        .expect("nessuna certification");

    sig.verify_userid_binding(
        certificatore.primary_key().key(),
        firmato.primary_key().key(),
        &uid,
    )
    .expect("la certification firmata dalla carta non verifica");

    // E deve sopravvivere alla serializzazione, come qualunque altra chiave.
    let riletto = OpenPgpCert::from_bytes(&cie_pgp::armored(&firmato).unwrap()).unwrap();
    assert_eq!(riletto.fingerprint(), utente.fingerprint());
}
