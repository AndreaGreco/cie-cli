//! Certificazione OpenPGP con la chiave della CIE.
//!
//! Qui dentro non c'e' logica OpenPGP: la costruisce `cie-pgp`. Questo livello
//! sceglie i file, chiede il PIN, decide cosa mostrare e cosa chiedere.

use anyhow::{bail, Context, Result};
use cie_firma::CryptoSigner;
use cie_lib::{DatiTitolare, PcscContext};
use cie_pgp::{
    armored, certifier_cert, certify, holder_matches, CieSequoiaSigner, OpenPgpCert, OpenPgpUserID,
    Parse,
};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Certifica con la CIE una User ID di una chiave OpenPGP.
pub fn certify_key(
    percorso: String,
    uid: Option<usize>,
    pin: Option<String>,
    uscita: Option<String>,
    outdir: Option<String>,
    conferma_automatica: bool,
) -> Result<()> {
    // Con la chiave in arrivo da stdin non si puo' anche chiedere a video: lo
    // stesso descrittore serve gia' a leggere la chiave.
    let da_stdin = percorso == "-";
    let chiave = if da_stdin {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf).context("lettura della chiave OpenPGP da stdin")?;
        OpenPgpCert::from_bytes(&buf).context("lettura della chiave OpenPGP da stdin")?
    } else {
        OpenPgpCert::from_file(&percorso)
            .with_context(|| format!("lettura della chiave OpenPGP {percorso}"))?
    };

    // Tutto il discorsivo va su stderr: stdout puo' essere la chiave stessa,
    // dentro una pipe verso `gpg --import`.
    eprintln!("\n╔══════════════════════════════════════╗");
    eprintln!("║      CERTIFICAZIONE OPENPGP          ║");
    eprintln!("╚══════════════════════════════════════╝\n");
    eprintln!("Chiave:      {}", chiave.fingerprint());

    let da_certificare = scegli_uid(&chiave, uid, !da_stdin)?;
    eprintln!("User ID:     {}", stringa(&da_certificare));

    let ctx = PcscContext::new()?;
    // Il numero della carta si legge senza PIN, e finisce nella User ID del
    // certificatore. Va letto prima di aprire il canale sicuro: una seconda
    // connessione mentre quello e' aperto selezionerebbe l'applet sotto di lui.
    let numero_carta = cie_firma::identify(&ctx).ok();

    let carta = super::open_card(&ctx, pin)?;
    let dati = DatiTitolare::from_certificate_der(carta.certificate_der()).unwrap_or_default();

    if !conferma_automatica {
        conferma_identita(&dati, &da_certificare, !da_stdin)?;
    }

    eprintln!("\n═══ Certificazione ═══");
    let mut firmatario = CieSequoiaSigner::new(&carta)?;

    let certificatore = certifier_cert(&mut firmatario, numero_carta.as_deref())?;
    eprintln!("  Certificatore CIE:  {}", certificatore.fingerprint());

    let certificata = certify(&chiave, &da_certificare, &mut firmatario)?;
    eprintln!("  ✓ Certification prodotta dalla chiave della carta");

    // Senza un file di uscita la chiave esce su stdout; gli altri due artefatti
    // non ci stanno, e finiscono comunque su disco.
    let out = match uscita.as_deref() {
        Some("-") => None,
        Some(f) => Some(PathBuf::from(f)),
        None if da_stdin => None,
        None => Some(accanto(&percorso)),
    };
    let dir = match (&outdir, &out) {
        (Some(d), _) => PathBuf::from(d),
        (None, Some(o)) => o.parent().unwrap_or(Path::new(".")).to_path_buf(),
        (None, None) => PathBuf::from("."),
    };
    let out_certificatore = dir.join("cie-certifier.asc");
    let out_x509 = dir.join("cie-certifier.cer");

    let armor = armored(&certificata)?;
    match &out {
        Some(o) => scrivi(o, &armor)?,
        None => io::stdout().write_all(&armor).context("scrittura della chiave su stdout")?,
    }
    scrivi(&out_certificatore, &armored(&certificatore)?)?;
    scrivi(&out_x509, carta.certificate_der())?;

    let dove = match &out {
        Some(o) => o.display().to_string(),
        None => "(stdout)".into(),
    };
    eprintln!("\n  Chiave certificata:  {dove}");
    eprintln!("  Certificatore CIE:   {}", out_certificatore.display());
    eprintln!("  Certificato X.509:   {}", out_x509.display());

    eprintln!("\n  Il .cer contiene il tuo codice fiscale in chiaro: non pubblicarlo.");
    eprintln!("  Serve solo a chi deve risalire dalla chiave alla PKI del Ministero,");
    eprintln!("  e glielo consegni tu. Gli altri due file sono pubblicabili.");

    eprintln!("\nVerifica con GnuPG:");
    match &out {
        Some(o) => {
            eprintln!("  gpg --import {} {}", out_certificatore.display(), o.display());
        }
        // Nella pipe la chiave l'ha gia' presa `gpg --import`: manca il certificatore,
        // senza il quale la certification resta un `sig?  [no public key]`.
        None => eprintln!("  gpg --import {}", out_certificatore.display()),
    }
    eprintln!("  gpg --check-sigs {}", certificata.fingerprint());

    Ok(())
}

/// Mostra chi ha certificato cosa, e con quale esito.
pub fn verify_key(percorso: String, certificatore: Option<String>) -> Result<()> {
    let chiave = OpenPgpCert::from_file(&percorso)
        .with_context(|| format!("lettura della chiave OpenPGP {percorso}"))?;

    println!("\nChiave: {}", chiave.fingerprint());

    let Some(percorso_cert) = certificatore else {
        println!("\n  Senza --certifier si possono solo elencare le certification:");
        for uid in chiave.userids() {
            println!("\n  {}", stringa(uid.userid()));
            for sig in uid.certifications() {
                let chi = sig
                    .issuers()
                    .next()
                    .map(|k| k.to_hex())
                    .unwrap_or_else(|| "sconosciuto".into());
                println!("    certificata da {chi} (non verificata)");
            }
        }
        println!("\n  Per verificarle davvero: --certifier cie-certifier.asc");
        return Ok(());
    };

    let cie = OpenPgpCert::from_file(&percorso_cert)
        .with_context(|| format!("lettura del certificatore {percorso_cert}"))?;
    println!("Certificatore: {} - {}", cie.fingerprint(), primo_uid(&cie));

    let mut verificate = 0;
    for uid in chiave.userids() {
        for sig in uid.certifications() {
            if sig
                .verify_userid_binding(cie.primary_key().key(), chiave.primary_key().key(), uid.userid())
                .is_err()
            {
                continue;
            }
            verificate += 1;
            println!("\n  ✓ {}", stringa(uid.userid()));
            for notation in sig.notation_data() {
                println!(
                    "      {} = {}",
                    notation.name(),
                    String::from_utf8_lossy(notation.value())
                );
            }
        }
    }

    if verificate == 0 {
        bail!("nessuna certification di questo certificatore su questa chiave");
    }

    println!("\n  Verificato: la chiave della CIE ha firmato quella User ID.");
    println!("  Il nome del titolare e' cio' che la CIE attesta; l'email no.");
    println!("\n  Che quella chiave sia davvero una CIE lo dice solo il certificato");
    println!("  X.509. Se lo hai ricevuto, controlla che sia quello legato alla firma");
    println!("  (notation cie-x509-sha256) e validalo con la CA del Ministero:");
    println!("    sha256sum cie-certifier.cer");
    println!("    openssl verify -CAfile ministero.pem -inform DER cie-certifier.cer");

    Ok(())
}

/// Quale User ID certificare: quella indicata, l'unica presente, o una scelta.
fn scegli_uid(
    chiave: &OpenPgpCert,
    scelta: Option<usize>,
    interattivo: bool,
) -> Result<OpenPgpUserID> {
    let uids: Vec<_> = chiave.userids().map(|u| u.userid().clone()).collect();

    match (scelta, uids.len()) {
        (_, 0) => bail!("la chiave non ha nessuna User ID da certificare"),
        (Some(n), len) => uids
            .into_iter()
            .nth(n.checked_sub(1).context("le User ID si contano da 1")?)
            .with_context(|| format!("la chiave ha {len} User ID, richiesta la {n}")),
        (None, 1) => Ok(uids.into_iter().next().expect("appena contata")),
        (None, len) if !interattivo => {
            bail!("la chiave ha {len} User ID e la scelta non si puo' chiedere: indica --uid N")
        }
        (None, _) => {
            eprintln!("\nQuale User ID certificare?");
            for (i, uid) in uids.iter().enumerate() {
                eprintln!("  {}. {}", i + 1, stringa(uid));
            }
            eprint!("  Scelta [1-{}]: ", uids.len());
            io::stderr().flush()?;

            let mut riga = String::new();
            io::stdin().read_line(&mut riga)?;
            let n: usize = riga.trim().parse().context("serve un numero")?;

            uids.into_iter()
                .nth(n.wrapping_sub(1))
                .context("scelta fuori intervallo")
        }
    }
}

/// Il nome sulla carta e quello nella User ID indicano la stessa persona?
///
/// Una risposta negativa non blocca: certificare uno pseudonimo e' legittimo.
/// Deve pero' essere una scelta, non una distrazione.
fn conferma_identita(dati: &DatiTitolare, uid: &OpenPgpUserID, interattivo: bool) -> Result<()> {
    if holder_matches(dati, uid) {
        return Ok(());
    }
    if !interattivo {
        bail!(
            "il nome della User ID non corrisponde a quello sulla CIE e la conferma \
             non si puo' chiedere: rilancia con --yes se e' voluto"
        );
    }

    let sulla_carta = match (&dati.cognome, &dati.nome) {
        (Some(c), Some(n)) => format!("{c} {n}"),
        _ => dati.nome_completo.clone().unwrap_or_else(|| "(assente)".into()),
    };

    eprintln!("\n  ⚠ Il nome della User ID non corrisponde a quello sulla CIE.");
    eprintln!("      CIE:      {sulla_carta}");
    eprintln!("      User ID:  {}", stringa(uid));
    eprint!("\n  Certificare comunque questa User ID? [s/N] ");
    io::stderr().flush()?;

    let mut risposta = String::new();
    io::stdin().read_line(&mut risposta)?;
    if !matches!(risposta.trim(), "s" | "S" | "si" | "sì" | "y" | "Y") {
        bail!("annullato");
    }
    Ok(())
}

fn stringa(uid: &OpenPgpUserID) -> String {
    String::from_utf8_lossy(uid.value()).into_owned()
}

fn primo_uid(cert: &OpenPgpCert) -> String {
    cert.userids()
        .next()
        .map(|u| stringa(u.userid()))
        .unwrap_or_else(|| "(senza User ID)".into())
}

/// `chiave.asc` → `chiave-certificata.asc`, accanto all'originale.
fn accanto(percorso: &str) -> PathBuf {
    let p = Path::new(percorso);
    let stelo = p.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
    let est = p.extension().map(|e| e.to_string_lossy().into_owned());

    let nome = match est {
        Some(e) => format!("{stelo}-certificata.{e}"),
        None => format!("{stelo}-certificata.asc"),
    };
    p.with_file_name(nome)
}

fn scrivi(dove: &Path, cosa: &[u8]) -> Result<()> {
    std::fs::write(dove, cosa).with_context(|| format!("scrittura di {}", dove.display()))
}
