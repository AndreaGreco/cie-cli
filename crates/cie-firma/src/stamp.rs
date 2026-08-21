//! The block of lines the command line puts in the signature box.
//!
//! Who signed, their tax code, when and why, stacked and centred in whatever
//! box they are given, with an optional picture beside them in one of
//! underskrift's fixed arrangements. The box composed piece by piece - which is
//! what the window offers - is [`crate::template`] instead.
//!
//! [`StampImage`] serves both: a scan of a handwritten signature is the one
//! part of either stamp that is a choice rather than a fact off the card.

use std::path::Path;

use anyhow::{bail, Context, Result};
use cie_lib::titolare::DatiTitolare;
use serde::{Deserialize, Serialize};
use underskrift::{
    Arrangement, ImageConfig, ImageFormat, ImageScale, SignatureLayout, TextConfig, TextLine,
};

/// Where the picture sits in the box, relative to the text.
///
/// Serialised: this is a user setting, and it has to survive a restart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StampLayout {
    /// Picture on the left, the lines beside it. The shape most people expect
    /// of a signed document, so it is the one we propose.
    #[default]
    ImageLeft,
    ImageRight,
    ImageTop,
    ImageBottom,
    /// The picture alone: no name, no tax code, no date. The signature is
    /// still in the PDF, it just is not written on the page.
    ImageOnly,
}

impl StampLayout {
    fn arrangement(self) -> Arrangement {
        match self {
            StampLayout::ImageRight => Arrangement::ImageRightTextLeft,
            StampLayout::ImageTop => Arrangement::ImageTopTextBottom,
            StampLayout::ImageBottom => Arrangement::ImageBottomTextTop,
            // `ImageOnly` never gets here: it has no text to arrange around.
            _ => Arrangement::ImageLeftTextRight,
        }
    }
}

/// The picture formats a PDF can carry without us re-encoding anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StampFormat {
    Png,
    Jpeg,
}

impl StampFormat {
    /// What the first bytes say the file is.
    ///
    /// The extension is not asked: a `.png` that is really a JPEG would be
    /// embedded with the wrong filter and come out as a grey rectangle.
    fn sniff(data: &[u8]) -> Option<Self> {
        if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(StampFormat::Png)
        } else if data.starts_with(b"\xFF\xD8\xFF") {
            Some(StampFormat::Jpeg)
        } else {
            None
        }
    }

    fn to_underskrift(self) -> ImageFormat {
        match self {
            StampFormat::Png => ImageFormat::Png,
            StampFormat::Jpeg => ImageFormat::Jpeg,
        }
    }
}

/// A picture to draw in the stamp, already known to be one.
///
/// Built by [`StampImage::decode`], which is also where a file that is not a
/// usable picture is turned away - at the moment it is chosen, not halfway
/// through a signature with the card on the reader and the PIN spent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StampImage {
    /// The file as it was read. underskrift wants the encoded bytes, not our
    /// decoding of them.
    pub data: Vec<u8>,
    pub format: StampFormat,
    /// Pixel size, learnt while checking the file could be read at all. The
    /// preview needs it: without the aspect ratio it cannot draw the picture
    /// where underskrift will put it.
    pub width: u32,
    pub height: u32,
}

impl StampImage {
    /// Reads a picture off disk.
    pub fn read(path: &Path) -> Result<Self> {
        let data =
            std::fs::read(path).with_context(|| format!("lettura di {}", path.display()))?;
        Self::decode(data).with_context(|| format!("{}", path.display()))
    }

    /// Checks some bytes really are a picture we can embed, and measures them.
    pub fn decode(data: Vec<u8>) -> Result<Self> {
        let Some(format) = StampFormat::sniff(&data) else {
            bail!("formato non riconosciuto: servono un PNG o un JPEG");
        };

        // Going through underskrift rather than a decoder of our own: whatever
        // it refuses at signing time has to be refused here too, or the check
        // is worth nothing.
        let embedded = underskrift::prepare_image(&data, format.to_underskrift())
            .map_err(|e| anyhow::anyhow!("immagine illeggibile: {e}"))?;

        Ok(Self {
            data,
            format,
            width: embedded.width,
            height: embedded.height,
        })
    }

    /// Aspect ratio, width over height. `1.0` for a picture of no size, which
    /// cannot happen but must not divide by zero if it does.
    pub fn aspect_ratio(&self) -> f32 {
        match (self.width, self.height) {
            (_, 0) => 1.0,
            (w, h) => w as f32 / h as f32,
        }
    }

    /// The picture as underskrift wants it.
    ///
    /// The scale is the caller's: the built-in layouts let underskrift fit the
    /// picture into the share it worked out, while a template has already
    /// worked out the rectangle itself.
    pub(crate) fn config(&self, scale: ImageScale) -> ImageConfig {
        ImageConfig {
            data: self.data.clone(),
            format: self.format.to_underskrift(),
            scale,
        }
    }
}

/// The holder as one line, for the stamp.
///
/// Given name plus surname first, and the common name only as a fallback. On a
/// CIE the CN is not a name: it carries the tax code, sometimes with the card
/// number attached. Preferring it puts `RSSMRA80A01H501U` where the reader
/// expects to see who signed.
pub(crate) fn holder_name(holder: &DatiTitolare) -> Option<String> {
    match (&holder.nome, &holder.cognome) {
        (Some(n), Some(c)) => Some(format!("{n} {c}")),
        (Some(one), None) | (None, Some(one)) => Some(one.clone()),
        (None, None) => holder.nome_completo.clone(),
    }
}

/// The lines of the visible stamp.
///
/// `holder` is `None` while nobody knows whose card this is - the preview has
/// to draw the stamp before the certificate has been read. In that case the
/// card-derived lines carry placeholders of realistic width, so what is on
/// screen has the shape and size of what will end up in the PDF.
pub fn stamp_lines(
    holder: Option<&DatiTitolare>,
    reason: Option<&str>,
    when: &str,
) -> Vec<TextLine> {
    let name = holder
        .and_then(holder_name)
        .unwrap_or_else(|| match holder {
            Some(_) => "titolare non identificato".to_string(),
            None => "NOME DEL TITOLARE".to_string(),
        });

    let mut lines = vec![
        TextLine::new("Firmato digitalmente da").size(7.0),
        TextLine::new(name).bold().size(9.0),
    ];
    match holder.and_then(|h| h.codice_fiscale.clone()) {
        Some(cf) => lines.push(TextLine::new(format!("C.F. {cf}")).size(7.0)),
        // Nobody known yet: keep the line, it takes up room in the box.
        None if holder.is_none() => {
            lines.push(TextLine::new("C.F. XXXXXXXXXXXXXXXX").size(7.0))
        }
        None => {}
    }
    lines.push(TextLine::new(format!("Data: {when}")).size(7.0));
    if let Some(r) = reason {
        lines.push(TextLine::new(format!("Motivo: {r}")).size(7.0));
    }
    lines
}

/// Puts picture and text together into the layout underskrift draws.
///
/// With no picture there is nothing to arrange and the text has the whole box -
/// which is also what a layout of `ImageOnly` has to fall back to, or asking
/// for a picture-only stamp and giving no picture would sign a blank rectangle.
pub(crate) fn compose(
    image: Option<&StampImage>,
    layout: StampLayout,
    text: TextConfig,
) -> SignatureLayout {
    match (image, layout) {
        (None, _) => SignatureLayout::TextOnly(text),
        (Some(image), StampLayout::ImageOnly) => {
            // Stretching a handwritten signature to fill a rectangle is how
            // you get a signature that is not yours.
            SignatureLayout::ImageOnly(image.config(ImageScale::FitPreserveAspect))
        }
        (Some(image), layout) => SignatureLayout::ImageAndText {
            image: image.config(ImageScale::FitPreserveAspect),
            text,
            arrangement: layout.arrangement(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use underskrift::CryptoSigner;

    /// A 2x1 PNG, the smallest thing that is still a picture with a shape.
    const PNG: &[u8] = include_bytes!("../tests/fixtures/firma.png");

    #[test]
    fn a_png_is_recognised_and_measured() {
        let image = StampImage::decode(PNG.to_vec()).unwrap();

        assert_eq!(image.format, StampFormat::Png);
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.aspect_ratio(), 2.0);
        // The bytes must survive untouched: they are what goes into the PDF.
        assert_eq!(image.data, PNG);
    }

    /// The extension is a claim, the first bytes are the evidence. A file that
    /// is not a picture has to be refused while it can still be re-chosen.
    #[test]
    fn anything_that_is_not_a_picture_is_refused() {
        let error = StampImage::decode(b"%PDF-1.7\n".to_vec()).unwrap_err();
        assert!(
            format!("{error:#}").contains("PNG"),
            "the error should say what is wanted: {error:#}"
        );
    }



    /// Without a picture nothing changes, whatever the layout says - including
    /// the one that would otherwise leave the box empty.
    #[test]
    fn no_picture_means_text_only() {
        for layout in [
            StampLayout::ImageLeft,
            StampLayout::ImageRight,
            StampLayout::ImageTop,
            StampLayout::ImageBottom,
            StampLayout::ImageOnly,
        ] {
            assert!(matches!(
                compose(None, layout, TextConfig::default()),
                SignatureLayout::TextOnly(_)
            ));
        }
    }

    #[test]
    fn a_picture_lands_in_the_layout_it_asked_for() {
        let image = StampImage::decode(PNG.to_vec()).unwrap();

        assert!(matches!(
            compose(Some(&image), StampLayout::ImageOnly, TextConfig::default()),
            SignatureLayout::ImageOnly(_)
        ));
        assert!(matches!(
            compose(Some(&image), StampLayout::ImageRight, TextConfig::default()),
            SignatureLayout::ImageAndText {
                arrangement: Arrangement::ImageRightTextLeft,
                ..
            }
        ));
    }

    #[test]
    fn the_stamp_carries_the_certificate_holder() {
        use crate::signer::fake::fake_signer;

        let signer = fake_signer();
        let holder = DatiTitolare::from_certificate_der(signer.certificate_der()).unwrap();
        let lines = stamp_lines(Some(&holder), Some("test"), "01/01/2026 10:00:00");
        let text: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();

        assert!(text.iter().any(|l| l.contains("MARIO ROSSI")), "{:?}", text);
        assert!(text.iter().any(|l| l.contains("Motivo: test")), "{:?}", text);
    }

    /// A CIE certificate subject, in the shape a real one has:
    ///
    /// ```text
    /// CN=<codice fiscale>/<numero carta>, serialNumber=IDCIT-<...>,
    /// GN=<nome>, SN=<cognome>, C=IT
    /// ```
    ///
    /// The CN is the tax code with the card number attached, which is why it
    /// must never be used as a name: doing so printed the tax code twice, once
    /// where the holder's name belongs and once on the `C.F.` line, and the
    /// name nowhere at all.
    #[test]
    fn a_cie_subject_puts_the_name_where_the_name_goes() {
        let holder = DatiTitolare {
            nome: Some("MARIO".to_string()),
            cognome: Some("ROSSI".to_string()),
            nome_completo: Some("RSSMRA80A01H501U/100000000001".to_string()),
            codice_fiscale: Some("RSSMRA80A01H501U".to_string()),
            ..Default::default()
        };

        let lines = stamp_lines(Some(&holder), None, "01/01/2026 10:00:00");
        let text: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();

        assert_eq!(text[1], "MARIO ROSSI", "{text:?}");
        assert!(
            text.iter().filter(|l| l.contains("RSSMRA80A01H501U")).count() == 1,
            "the tax code is in the stamp more than once: {text:?}"
        );
        assert!(
            !text.iter().any(|l| l.contains("100000000001")),
            "the card number has no business in the stamp: {text:?}"
        );
    }

}
