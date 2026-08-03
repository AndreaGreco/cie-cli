//! PAdES: the signature goes *inside* the PDF.
//!
//! The file stays a PDF, opens normally, and readers that understand
//! signatures show it.
//!
//! Level PAdES B-B. No timestamp: that would be a network call to a TSA, and
//! this project only talks to the card.

use anyhow::{Context, Result};
use cie_lib::titolare::DatiTitolare;
use underskrift::{
    Border, Color, CryptoSigner, DocMdpPermissions, PdfSigner, SignatureLayout, SignatureRect,
    SigningOptions, TextConfig, TextLine, VisibleSignatureConfig,
};

/// Where the stamp goes and what goes in it.
pub struct PadesOptions {
    /// Page to draw the stamp on, counted from 0.
    pub page: u32,
    /// `[llx, lly, urx, ury]` in PDF points (1 pt = 1/72"), origin bottom left:
    /// the same system as the annotation's `/Rect`.
    pub rect: [f32; 4],
    /// Reason for signing, shown in the stamp and in the dictionary.
    pub reason: Option<String>,
    /// Certification signature (DocMDP): 1 = no changes, 2 = form filling and
    /// further signatures, 3 = annotations too. `None` = plain approval
    /// signature.
    pub certify: Option<u8>,
}

impl Default for PadesOptions {
    fn default() -> Self {
        Self {
            page: 0,
            // Bottom right on A4. This is where `cie sign-pdf` put the stamp
            // back when it was hard-coded.
            rect: [330.0, 40.0, 560.0, 110.0],
            reason: None,
            certify: None,
        }
    }
}

/// The lines of the visible stamp, taken from the certificate.
fn stamp_lines(certificate: &[u8], reason: Option<&str>, when: &str) -> Vec<TextLine> {
    let holder = DatiTitolare::from_certificate_der(certificate).unwrap_or_default();

    let name = holder
        .nome_completo
        .or_else(|| match (holder.nome, holder.cognome) {
            (Some(n), Some(c)) => Some(format!("{} {}", n, c)),
            _ => None,
        })
        .unwrap_or_else(|| "titolare non identificato".to_string());

    let mut lines = vec![
        TextLine::new("Firmato digitalmente da").size(7.0),
        TextLine::new(name).bold().size(9.0),
    ];
    if let Some(cf) = holder.codice_fiscale {
        lines.push(TextLine::new(format!("C.F. {}", cf)).size(7.0));
    }
    lines.push(TextLine::new(format!("Data: {}", when)).size(7.0));
    if let Some(r) = reason {
        lines.push(TextLine::new(format!("Motivo: {}", r)).size(7.0));
    }
    lines
}

/// Signs a PDF at PAdES B-B and returns the signed PDF.
///
/// Touches no disk: PAdES appends, so the result starts with exactly the bytes
/// that went in.
pub fn sign_pdf(pdf: &[u8], signer: &dyn CryptoSigner, options: &PadesOptions) -> Result<Vec<u8>> {
    let now = chrono::Local::now();
    let when = now.format("%d/%m/%Y %H:%M:%S").to_string();

    let text = TextConfig {
        lines: stamp_lines(signer.certificate_der(), options.reason.as_deref(), &when),
        ..Default::default()
    };

    let permissions = match options.certify {
        Some(1) => DocMdpPermissions::NoChanges,
        Some(3) => DocMdpPermissions::FormFillingSigningAndAnnotation,
        _ => DocMdpPermissions::FormFillingAndSigning,
    };

    let [llx, lly, urx, ury] = options.rect;

    let opts = SigningOptions {
        page: options.page,
        reason: options.reason.clone(),
        cms_signing_time: Some(now.naive_local()),
        certify: options.certify.is_some(),
        certify_permissions: permissions,
        visible_signature: Some(VisibleSignatureConfig {
            page: options.page,
            rect: SignatureRect::Absolute { llx, lly, urx, ury },
            layout: SignatureLayout::TextOnly(text),
            background_color: Some(Color::white()),
            border: Some(Border::default()),
        }),
        ..Default::default()
    };

    // `sign` is async but without the `tsp` feature it does no I/O: a
    // single-threaded runtime is plenty.
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .context("creating the runtime")?;

    rt.block_on(PdfSigner::new().options(opts).sign(pdf, signer))
        .context("PAdES signing")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::signer::fake::fake_signer;

    /// A blank A4 page, xref included: 324 bytes of valid PDF.
    const MINIMAL_PDF: &[u8] = include_bytes!("../tests/fixtures/minimo.pdf");

    fn with(reason: Option<&str>, certify: Option<u8>) -> PadesOptions {
        PadesOptions {
            reason: reason.map(str::to_string),
            certify,
            ..Default::default()
        }
    }

    #[test]
    fn signs_a_pdf_with_a_signer_that_only_sees_the_digest() {
        let signer = fake_signer();
        let signed = sign_pdf(MINIMAL_PDF, &signer, &with(Some("prova"), None)).unwrap();

        // PAdES appends: the original must survive byte for byte.
        assert!(signed.starts_with(MINIMAL_PDF));
        assert!(signed.len() > MINIMAL_PDF.len());

        // The structures that make it a signature rather than trailing bytes.
        let text = String::from_utf8_lossy(&signed);
        assert!(text.contains("/ETSI.CAdES.detached"), "missing PAdES SubFilter");
        assert!(text.contains("/ByteRange"), "missing ByteRange");
        assert!(text.contains("/Widget"), "missing stamp annotation");
    }

    /// Without `certify` the PDF must contain nothing of DocMDP: an unwanted
    /// certification would block every later signature.
    #[test]
    fn without_certify_there_is_no_docmdp() {
        let signed = sign_pdf(MINIMAL_PDF, &fake_signer(), &with(None, None)).unwrap();
        let text = String::from_utf8_lossy(&signed);
        assert!(!text.contains("/DocMDP"));
        assert!(!text.contains("/Perms"));
    }

    #[test]
    fn certify_produces_docmdp_at_the_requested_level() {
        for (level, expected) in [(1u8, "/P 1"), (2, "/P 2"), (3, "/P 3")] {
            let signed = sign_pdf(MINIMAL_PDF, &fake_signer(), &with(None, Some(level))).unwrap();
            let text = String::from_utf8_lossy(&signed);

            assert!(text.contains("/TransformMethod /DocMDP"), "level {level}");
            assert!(text.contains("/V /1.2"), "level {level}");
            assert!(text.contains(expected), "level {level}: missing {expected}");
            // /Reference alone is not enough: without /Perms in the catalog
            // readers ignore it and the signature stays plain approval.
            assert!(text.contains("/Perms"), "level {level}: missing /Perms");
        }
    }

    #[test]
    fn the_stamp_carries_the_certificate_holder() {
        let signer = fake_signer();
        let lines = stamp_lines(signer.certificate_der(), Some("test"), "01/01/2026 10:00:00");
        let text: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();

        assert!(text.iter().any(|l| l.contains("MARIO ROSSI")), "{:?}", text);
        assert!(text.iter().any(|l| l.contains("Motivo: test")), "{:?}", text);
    }

    /// The rect the caller asks for must land in the annotation's `/Rect`:
    /// it is the only way the GUI can place the signature.
    #[test]
    fn the_requested_rect_lands_in_the_annotation_rect() {
        let options = PadesOptions {
            rect: [72.0, 144.0, 288.0, 216.0],
            ..Default::default()
        };
        let signed = sign_pdf(MINIMAL_PDF, &fake_signer(), &options).unwrap();
        let text = String::from_utf8_lossy(&signed);

        assert!(
            text.contains("/Rect [72 144 288 216]"),
            "the /Rect does not carry the requested rectangle"
        );
    }
}
