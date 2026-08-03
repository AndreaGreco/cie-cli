//! PAdES: la firma finisce *dentro* il PDF.
//!
//! Il file resta un PDF, si apre normalmente, e i lettori che sanno leggere le
//! firme la mostrano.
//!
//! Livello PAdES B-B. Niente marca temporale: sarebbe una chiamata di rete a una
//! TSA, e il progetto per ora parla solo con la carta.

use anyhow::{Context, Result};
use cie_lib::titolare::DatiTitolare;
use underskrift::{
    Border, Color, CryptoSigner, DocMdpPermissions, PdfSigner, SignatureLayout, SignatureRect,
    SigningOptions, TextConfig, TextLine, VisibleSignatureConfig,
};

/// Dove va il riquadro e cosa ci scriviamo dentro.
pub struct OpzioniPades {
    /// Pagina su cui disegnare il riquadro, contata da 0.
    pub pagina: u32,
    /// `[llx, lly, urx, ury]` in punti PDF (1 pt = 1/72"), origine in basso a
    /// sinistra: e' lo stesso sistema del `/Rect` dell'annotazione.
    pub riquadro: [f32; 4],
    /// Motivo della firma, mostrato nel riquadro e nel dizionario.
    pub motivo: Option<String>,
    /// Firma di certificazione (DocMDP): 1 = nessuna modifica, 2 = moduli e
    /// altre firme, 3 = anche annotazioni. `None` = normale firma di
    /// approvazione.
    pub certifica: Option<u8>,
}

impl Default for OpzioniPades {
    fn default() -> Self {
        Self {
            pagina: 0,
            // In basso a destra su una A4. E' la posizione che `cie sign-pdf`
            // usava quando il riquadro era cablato nel sorgente.
            riquadro: [330.0, 40.0, 560.0, 110.0],
            motivo: None,
            certifica: None,
        }
    }
}

/// Le righe del riquadro visibile, ricavate dal certificato.
fn righe_riquadro(certificato: &[u8], motivo: Option<&str>, quando: &str) -> Vec<TextLine> {
    let dati = DatiTitolare::from_certificate_der(certificato).unwrap_or_default();

    let intestatario = dati
        .nome_completo
        .or_else(|| match (dati.nome, dati.cognome) {
            (Some(n), Some(c)) => Some(format!("{} {}", n, c)),
            _ => None,
        })
        .unwrap_or_else(|| "titolare non identificato".to_string());

    let mut righe = vec![
        TextLine::new("Firmato digitalmente da").size(7.0),
        TextLine::new(intestatario).bold().size(9.0),
    ];
    if let Some(cf) = dati.codice_fiscale {
        righe.push(TextLine::new(format!("C.F. {}", cf)).size(7.0));
    }
    righe.push(TextLine::new(format!("Data: {}", quando)).size(7.0));
    if let Some(m) = motivo {
        righe.push(TextLine::new(format!("Motivo: {}", m)).size(7.0));
    }
    righe
}

/// Firma un PDF in PAdES B-B e restituisce il PDF firmato.
///
/// Non tocca il disco: il PAdES si appende in coda, quindi il risultato inizia
/// esattamente con i byte in ingresso.
pub fn firma_pdf(
    pdf: &[u8],
    firmatario: &dyn CryptoSigner,
    opzioni: &OpzioniPades,
) -> Result<Vec<u8>> {
    let adesso = chrono::Local::now();
    let quando = adesso.format("%d/%m/%Y %H:%M:%S").to_string();

    let testo = TextConfig {
        lines: righe_riquadro(
            firmatario.certificate_der(),
            opzioni.motivo.as_deref(),
            &quando,
        ),
        ..Default::default()
    };

    let permessi = match opzioni.certifica {
        Some(1) => DocMdpPermissions::NoChanges,
        Some(3) => DocMdpPermissions::FormFillingSigningAndAnnotation,
        _ => DocMdpPermissions::FormFillingAndSigning,
    };

    let [llx, lly, urx, ury] = opzioni.riquadro;

    let opts = SigningOptions {
        page: opzioni.pagina,
        reason: opzioni.motivo.clone(),
        cms_signing_time: Some(adesso.naive_local()),
        certify: opzioni.certifica.is_some(),
        certify_permissions: permessi,
        visible_signature: Some(VisibleSignatureConfig {
            page: opzioni.pagina,
            rect: SignatureRect::Absolute { llx, lly, urx, ury },
            layout: SignatureLayout::TextOnly(testo),
            background_color: Some(Color::white()),
            border: Some(Border::default()),
        }),
        ..Default::default()
    };

    // `sign` e' async ma senza la feature `tsp` non fa I/O: un runtime a thread
    // singolo basta e avanza.
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .context("creazione del runtime")?;

    rt.block_on(PdfSigner::new().options(opts).sign(pdf, firmatario))
        .context("firma PAdES")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::firmatario::finto::firmatario_finto;

    /// Una pagina A4 vuota, xref incluso: 324 byte di PDF valido.
    const PDF_MINIMO: &[u8] = include_bytes!("../tests/fixtures/minimo.pdf");

    fn con(motivo: Option<&str>, certifica: Option<u8>) -> OpzioniPades {
        OpzioniPades {
            motivo: motivo.map(str::to_string),
            certifica,
            ..Default::default()
        }
    }

    #[test]
    fn firma_un_pdf_con_un_firmatario_che_vede_solo_il_digest() {
        let firmatario = firmatario_finto();
        let firmato = firma_pdf(PDF_MINIMO, &firmatario, &con(Some("prova"), None)).unwrap();

        // Il PAdES si appende in coda: l'originale deve restare intatto.
        assert!(firmato.starts_with(PDF_MINIMO));
        assert!(firmato.len() > PDF_MINIMO.len());

        // Le strutture che rendono la firma una firma, non solo byte in coda.
        let testo = String::from_utf8_lossy(&firmato);
        assert!(testo.contains("/ETSI.CAdES.detached"), "manca il SubFilter PAdES");
        assert!(testo.contains("/ByteRange"), "manca il ByteRange");
        assert!(testo.contains("/Widget"), "manca l'annotazione del riquadro");
    }

    /// Senza `certifica` il PDF non deve contenere niente di DocMDP: una
    /// certificazione non voluta bloccherebbe le firme successive.
    #[test]
    fn senza_certifica_non_ce_nessun_docmdp() {
        let firmato = firma_pdf(PDF_MINIMO, &firmatario_finto(), &con(None, None)).unwrap();
        let testo = String::from_utf8_lossy(&firmato);
        assert!(!testo.contains("/DocMDP"));
        assert!(!testo.contains("/Perms"));
    }

    #[test]
    fn certifica_produce_docmdp_col_livello_richiesto() {
        for (livello, atteso) in [(1u8, "/P 1"), (2, "/P 2"), (3, "/P 3")] {
            let firmato =
                firma_pdf(PDF_MINIMO, &firmatario_finto(), &con(None, Some(livello))).unwrap();
            let testo = String::from_utf8_lossy(&firmato);

            assert!(testo.contains("/TransformMethod /DocMDP"), "livello {livello}");
            assert!(testo.contains("/V /1.2"), "livello {livello}");
            assert!(testo.contains(atteso), "livello {livello}: manca {atteso}");
            // Il /Reference da solo non basta: senza /Perms nel catalogo i
            // lettori lo ignorano e la firma resta di semplice approvazione.
            assert!(testo.contains("/Perms"), "livello {livello}: manca /Perms");
        }
    }

    #[test]
    fn il_riquadro_riporta_il_titolare_del_certificato() {
        let firmatario = firmatario_finto();
        let righe = righe_riquadro(firmatario.certificate_der(), Some("test"), "01/01/2026 10:00:00");
        let testo: Vec<&str> = righe.iter().map(|r| r.text.as_str()).collect();

        assert!(testo.iter().any(|r| r.contains("MARIO ROSSI")), "{:?}", testo);
        assert!(testo.iter().any(|r| r.contains("Motivo: test")), "{:?}", testo);
    }

    /// Il riquadro passato dal chiamante deve finire nel `/Rect`
    /// dell'annotazione: e' l'unico modo che ha la GUI di piazzare la firma.
    #[test]
    fn il_riquadro_richiesto_finisce_nel_rect_dellannotazione() {
        let opzioni = OpzioniPades {
            riquadro: [72.0, 144.0, 288.0, 216.0],
            ..Default::default()
        };
        let firmato = firma_pdf(PDF_MINIMO, &firmatario_finto(), &opzioni).unwrap();
        let testo = String::from_utf8_lossy(&firmato);

        assert!(
            testo.contains("/Rect [72 144 288 216]"),
            "il /Rect non riporta il riquadro richiesto"
        );
    }
}
