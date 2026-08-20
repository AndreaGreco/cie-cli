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
    Border, Color, CryptoSigner, DocMdpPermissions, PdfSigner, SignatureRect, SigningOptions,
    TextConfig, VisibleSignatureConfig,
};

use crate::stamp::{self, StampImage, StampLayout};
use crate::template::{self, Facts, Template};

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
    /// What goes in the box.
    pub stamp: Stamp,
}

/// The two ways the signature box can be filled.
///
/// They are different layouts, not two spellings of one. [`Stamp::Lines`] is
/// what the command line has always produced: the lines stack and the block
/// centres itself, so it fits whatever box it is given. A [`Template`] is
/// absolute — every piece where the user dragged it — which is the only thing
/// an editor showing a live preview can honestly promise.
pub enum Stamp {
    /// The block of lines, with an optional picture beside them.
    Lines {
        /// A scan of the holder's handwritten signature. `None` leaves the box
        /// to the text.
        image: Option<StampImage>,
        /// Where that picture sits. Ignored while there is no picture.
        layout: StampLayout,
    },
    /// A box the user composed piece by piece.
    Composed {
        template: Template,
        /// The template's picture, already read. `None` when it has no picture
        /// piece, or when the file it names could not be opened — in which
        /// case the rest of the box is still drawn.
        image: Option<StampImage>,
    },
}

impl Default for Stamp {
    fn default() -> Self {
        Stamp::Lines {
            image: None,
            layout: StampLayout::default(),
        }
    }
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
            stamp: Stamp::default(),
        }
    }
}

/// Signs a PDF at PAdES B-B and returns the signed PDF.
///
/// Touches no disk: PAdES appends, so the result starts with exactly the bytes
/// that went in.
pub fn sign_pdf(pdf: &[u8], signer: &dyn CryptoSigner, options: &PadesOptions) -> Result<Vec<u8>> {
    let now = chrono::Local::now();
    let when = now.format("%d/%m/%Y %H:%M:%S").to_string();

    // An unparseable certificate still has a holder — we just cannot read it —
    // so this stays `Some` and the stamp says as much rather than falling back
    // to the preview's placeholders.
    let holder =
        DatiTitolare::from_certificate_der(signer.certificate_der()).unwrap_or_default();

    let layout = match &options.stamp {
        Stamp::Lines { image, layout } => {
            let text = TextConfig {
                lines: stamp::stamp_lines(Some(&holder), options.reason.as_deref(), &when),
                ..Default::default()
            };
            stamp::compose(image.as_ref(), *layout, text)
        }
        Stamp::Composed { template, image } => template::layout(
            template,
            &Facts::signing(&holder, options.reason.as_deref(), &when),
            image.clone(),
        ),
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
            layout,
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

    /// A picture in the options has to reach the PDF as an image XObject.
    /// Without this the setting is a checkbox that changes nothing.
    #[test]
    fn a_picture_in_the_options_lands_in_the_pdf() {
        let options = PadesOptions {
            stamp: Stamp::Lines {
                image: Some(
                    crate::stamp::StampImage::decode(
                        include_bytes!("../tests/fixtures/firma.png").to_vec(),
                    )
                    .unwrap(),
                ),
                layout: StampLayout::default(),
            },
            ..Default::default()
        };
        let signed = sign_pdf(MINIMAL_PDF, &fake_signer(), &options).unwrap();
        let text = String::from_utf8_lossy(&signed);

        assert!(text.contains("/Subtype /Image"), "no image in the PDF");
        // The default layout keeps the lines beside the picture, so both have
        // to be there: an image that replaced the text would be a regression.
        assert!(text.contains("/BaseFont"), "the lines went missing");
    }

    /// A template reaches the PDF through `SignatureLayout::Custom`, picture
    /// and all. The picture is the part that only works because of the patch
    /// in `patches/BRIEF-underskrift-custom-images.md`.
    #[test]
    fn a_template_lands_in_the_pdf_with_its_picture() {
        let picture = StampImage::decode(
            include_bytes!("../tests/fixtures/firma.png").to_vec(),
        )
        .unwrap();

        let options = PadesOptions {
            stamp: Stamp::Composed {
                template: Template::handwritten("prova", "firma.png".into()),
                image: Some(picture),
            },
            ..Default::default()
        };
        let signed = sign_pdf(MINIMAL_PDF, &fake_signer(), &options).unwrap();
        let text = String::from_utf8_lossy(&signed);

        assert!(text.contains("/Subtype /Image"), "no picture in the PDF");
        assert!(text.contains("/BaseFont"), "no text in the PDF");
        assert!(text.contains("/Widget"), "no stamp annotation");
    }

    /// A template whose picture could not be read still signs, and still
    /// draws everything else. Losing a scan must not cost a signature.
    #[test]
    fn a_template_without_its_picture_still_signs() {
        let options = PadesOptions {
            stamp: Stamp::Composed {
                template: Template::handwritten("prova", "sparita.png".into()),
                image: None,
            },
            ..Default::default()
        };
        let signed = sign_pdf(MINIMAL_PDF, &fake_signer(), &options).unwrap();
        let text = String::from_utf8_lossy(&signed);

        assert!(!text.contains("/Subtype /Image"));
        assert!(text.contains("/BaseFont"), "the lines went with the picture");
    }

    /// Picture-only means picture only: none of the text belongs in a stamp
    /// the user asked to be nothing but their handwriting.
    #[test]
    fn a_picture_only_stamp_carries_no_text() {
        let picture = crate::stamp::StampImage::decode(
            include_bytes!("../tests/fixtures/firma.png").to_vec(),
        )
        .unwrap();

        let options = PadesOptions {
            stamp: Stamp::Lines {
                image: Some(picture),
                layout: StampLayout::ImageOnly,
            },
            reason: Some("prova".to_string()),
            ..Default::default()
        };
        let signed = sign_pdf(MINIMAL_PDF, &fake_signer(), &options).unwrap();
        let text = String::from_utf8_lossy(&signed);

        assert!(text.contains("/Subtype /Image"));
        // The reason still belongs in the signature dictionary; what must not
        // be there is a font, which only a drawn line of text would need.
        assert!(!text.contains("/BaseFont"), "the picture-only stamp drew text");
    }
}
