//! CAdES enveloping: il file *e* la firma insieme in un unico DER.
//!
//! E' il `.p7m` della prassi italiana. A differenza del PAdES vale per qualunque
//! file, non solo per i PDF.
//!
//! Livello CAdES B-B. Niente marca temporale, per lo stesso motivo del PAdES.

use anyhow::{Context, Result};
use underskrift::{cades, CryptoSigner};

/// Impacchetta contenuto e firma in una busta CAdES enveloping.
pub fn firma_p7m(contenuto: &[u8], firmatario: &dyn CryptoSigner) -> Result<Vec<u8>> {
    // Il CAdES tiene il signingTime dentro il CMS, dove il PAdES lo vieta.
    let adesso = chrono::Local::now().naive_local();

    let busta = cades::sign_attached(contenuto, firmatario, Some(adesso)).context("firma CAdES")?;

    // La busta deve restituire esattamente cio' che e' entrato, altrimenti la
    // firma copre qualcosa che il destinatario non vedra' mai.
    // ponytail: qui si ricontrolla solo l'incapsulamento, non la crittografia:
    // per quella la feature `verify` di underskrift non compila senza `tsp`
    // (che riporta dentro reqwest). I test verificano la firma con OpenSSL.
    let recuperato = cades::extract_content(&busta).context("rilettura della busta")?;
    anyhow::ensure!(
        recuperato == contenuto,
        "la busta non restituisce il contenuto originale"
    );

    Ok(busta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firmatario::finto::firmatario_finto;
    use std::process::Command;

    /// Verifica la busta con OpenSSL: un giudizio indipendente dalla libreria
    /// che l'ha prodotta. `-noverify` salta la catena (il certificato di prova
    /// e' autofirmato), non la firma.
    fn openssl_verifica(busta: &[u8]) -> bool {
        let dir = std::env::temp_dir().join(format!("cie-p7m-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(format!("{:x}.p7m", busta.len()));
        std::fs::write(&file, busta).unwrap();

        let esito = Command::new("openssl")
            .args(["cms", "-verify", "-inform", "DER", "-noverify", "-in"])
            .arg(&file)
            .arg("-out")
            .arg(dir.join("contenuto"))
            .output()
            .expect("openssl non trovato: serve per verificare la busta");

        let _ = std::fs::remove_dir_all(&dir);
        esito.status.success()
    }

    #[test]
    fn la_busta_contiene_il_file_e_openssl_ne_verifica_la_firma() {
        let contenuto = b"contratto qualunque, non serve che sia un PDF";
        let busta = firma_p7m(contenuto, &firmatario_finto()).unwrap();

        // Enveloping, non detached: il contenuto sta dentro la busta.
        assert_eq!(cades::extract_content(&busta).unwrap(), contenuto);
        assert!(openssl_verifica(&busta), "OpenSSL rifiuta la busta");
    }

    /// Un byte cambiato nel contenuto incapsulato deve far cadere la verifica:
    /// e' l'unica cosa che distingue una firma da un blocco di byte in coda.
    #[test]
    fn una_busta_alterata_non_verifica_piu() {
        let contenuto = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_vec();
        let mut busta = firma_p7m(&contenuto, &firmatario_finto()).unwrap();

        let pos = busta
            .windows(contenuto.len())
            .position(|w| w == contenuto.as_slice())
            .expect("contenuto incapsulato");
        busta[pos] = b'B';

        assert!(!openssl_verifica(&busta), "OpenSSL accetta una busta alterata");
    }
}
