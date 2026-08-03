//! PDF preview and conversion of the stamp rectangle into PDF points.
//!
//! This is the only non-trivial calculation in the GUI. The user drags the
//! rectangle over the image they see, but underskrift's
//! `SignatureRect::Absolute` goes straight into the annotation's `/Rect`:
//! **unrotated user space, origin bottom left**. The image, on the other hand,
//! is rotated per `/Rotate`, cropped to the crop box, and has its origin top
//! left.
//!
//! Ignoring rotation and crop box puts the signature in the wrong place with
//! no visible error — and scanned PDFs often carry both.

use anyhow::{anyhow, Result};
use eframe::egui;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::page::Rotation;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{RenderCache, RenderSettings};
use std::sync::Arc;

/// An open PDF, ready to be previewed.
///
/// It does not own the bytes: those are the same ones that go to the card, and
/// the wizard holds them in an `Arc` that hayro shares without copying.
pub struct Document {
    pdf: Pdf,
}

/// What it takes to map a point of the image back into PDF points.
#[derive(Debug, Clone, Copy)]
pub struct Geometry {
    /// The effective crop box in PDF points: `[x0, y0, x1, y1]`, y upwards.
    pub crop: [f32; 4],
    /// The rotation the page declares, clockwise.
    pub rotation: Rotation,
}

impl Document {
    pub fn open(bytes: &Arc<Vec<u8>>) -> Result<Self> {
        let pdf = Pdf::new(bytes.clone()).map_err(|e| anyhow!("PDF illeggibile: {e:?}"))?;
        if pdf.pages().is_empty() {
            return Err(anyhow!("il PDF non ha pagine"));
        }
        Ok(Self { pdf })
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    pub fn geometry(&self, page: usize) -> Geometry {
        let p = &self.pdf.pages()[page];
        let c = p.intersected_crop_box();
        Geometry {
            crop: [c.x0 as f32, c.y0 as f32, c.x1 as f32, c.y1 as f32],
            rotation: p.rotation(),
        }
    }

    /// Size of the page as displayed, in PDF points, rotation included.
    pub fn display_size(&self, page: usize) -> (f32, f32) {
        self.pdf.pages()[page].render_dimensions()
    }

    /// Aspect ratio of the page as displayed, rotation included.
    pub fn aspect_ratio(&self, page: usize) -> f32 {
        let (w, h) = self.display_size(page);
        if h > 0.0 { w / h } else { 1.0 }
    }

    /// Rasterizes a page at the given width, in pixels.
    pub fn render(&self, page: usize, width_px: f32) -> egui::ColorImage {
        let p = &self.pdf.pages()[page];
        let (width_pt, _) = p.render_dimensions();
        let scale = (width_px / width_pt.max(1.0)).clamp(0.05, 8.0);

        let pixmap = hayro::render(
            p,
            &RenderCache::new(),
            &InterpreterSettings::default(),
            &RenderSettings {
                x_scale: scale,
                y_scale: scale,
                // Without a background the page comes out transparent and on a
                // dark theme the document is unreadable.
                bg_color: WHITE,
                ..Default::default()
            },
        );

        egui::ColorImage::from_rgba_premultiplied(
            [pixmap.width() as usize, pixmap.height() as usize],
            pixmap.data_as_u8_slice(),
        )
    }
}

/// Converts a rectangle given as fractions of the image (`0..1`, y **going
/// down**) into the `[llx, lly, urx, ury]` in PDF points the annotation wants.
pub fn to_pdf_rect(selection: [f32; 4], g: &Geometry) -> [f32; 4] {
    let (ax, ay) = to_pdf_point(selection[0], selection[1], g);
    let (bx, by) = to_pdf_point(selection[2], selection[3], g);

    // Rotation and the y flip can swap the corners: the /Rect still wants the
    // bottom-left one first.
    [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)]
}

/// A single point: undo the rotation, then step out of the crop box.
fn to_pdf_point(fx: f32, fy: f32, g: &Geometry) -> (f32, f32) {
    // (u, v) are fractions on the *unrotated* page, v still measured downwards.
    let (u, v) = match g.rotation {
        Rotation::None => (fx, fy),
        Rotation::Horizontal => (fy, 1.0 - fx),
        Rotation::Flipped => (1.0 - fx, 1.0 - fy),
        Rotation::FlippedHorizontal => (1.0 - fy, fx),
    };

    let [x0, y0, x1, y1] = g.crop;
    (x0 + u * (x1 - x0), y1 - v * (y1 - y0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shifted crop box: anyone forgetting the origin shows up here.
    fn geometry(rotation: Rotation) -> Geometry {
        Geometry {
            crop: [10.0, 20.0, 605.0, 862.0], // 595 × 842, shifted by (10, 20)
            rotation,
        }
    }

    fn close(got: [f32; 4], want: [f32; 4]) {
        for (i, (a, b)) in got.iter().zip(want.iter()).enumerate() {
            assert!((a - b).abs() < 0.01, "component {i}: {got:?} instead of {want:?}");
        }
    }

    #[test]
    fn the_whole_page_is_the_whole_crop_box_at_every_rotation() {
        for rot in [
            Rotation::None,
            Rotation::Horizontal,
            Rotation::Flipped,
            Rotation::FlippedHorizontal,
        ] {
            close(
                to_pdf_rect([0.0, 0.0, 1.0, 1.0], &geometry(rot)),
                [10.0, 20.0, 605.0, 862.0],
            );
        }
    }

    /// The top-left quarter of the image, traced by hand for each of the four
    /// clockwise rotations.
    #[test]
    fn the_top_left_quarter_lands_where_it_should() {
        let sel = [0.0, 0.0, 0.5, 0.5];

        // No rotation: the top-left quarter of the page, and in PDF
        // coordinates "top" means large y.
        close(
            to_pdf_rect(sel, &geometry(Rotation::None)),
            [10.0, 441.0, 307.5, 862.0],
        );

        // 90° clockwise: what shows up top left is the original page's
        // bottom-left corner.
        close(
            to_pdf_rect(sel, &geometry(Rotation::Horizontal)),
            [10.0, 20.0, 307.5, 441.0],
        );

        // 180°: the opposite corner.
        close(
            to_pdf_rect(sel, &geometry(Rotation::Flipped)),
            [307.5, 20.0, 605.0, 441.0],
        );

        // 270° clockwise: the top-right corner.
        close(
            to_pdf_rect(sel, &geometry(Rotation::FlippedHorizontal)),
            [307.5, 441.0, 605.0, 862.0],
        );
    }

    /// A rectangle dragged bottom-up arrives with its corners swapped: the
    /// `/Rect` must come out normalised anyway.
    #[test]
    fn corners_always_come_out_ordered() {
        let forwards = to_pdf_rect([0.2, 0.3, 0.6, 0.8], &geometry(Rotation::None));
        let backwards = to_pdf_rect([0.6, 0.8, 0.2, 0.3], &geometry(Rotation::None));
        assert_eq!(forwards, backwards);
        assert!(forwards[0] < forwards[2] && forwards[1] < forwards[3]);
    }
}
