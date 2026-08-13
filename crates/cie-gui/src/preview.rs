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
//! no visible error - and scanned PDFs often carry both.

use anyhow::{anyhow, Result};
use iced::widget::image;
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
    ///
    /// Waiting for the draggable stamp: the mock-up has to be drawn at the same
    /// scale it will have in the PDF, which is this over the on-screen width.
    pub fn display_size(&self, page: usize) -> (f32, f32) {
        self.pdf.pages()[page].render_dimensions()
    }

    /// Aspect ratio of the page as displayed, rotation included.
    pub fn aspect_ratio(&self, page: usize) -> f32 {
        let (w, h) = self.display_size(page);
        if h > 0.0 { w / h } else { 1.0 }
    }

    /// Rasterizes a page at the given width, in pixels.
    pub fn render(&self, page: usize, width_px: f32) -> image::Handle {
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

        // hayro hands back premultiplied RGBA, which would need dividing back
        // out - except `bg_color` above makes every pixel opaque, and at alpha
        // 255 premultiplied and straight are the same bytes. Walking a hundred
        // megabytes to multiply by one is not free at these sizes.
        debug_assert!(
            pixmap.data_as_u8_slice().chunks_exact(4).all(|p| p[3] == 255),
            "an opaque background should leave no alpha to undo"
        );

        image::Handle::from_rgba(
            pixmap.width() as u32,
            pixmap.height() as u32,
            pixmap.data_as_u8_slice().to_vec(),
        )
    }
}

/// Smallest side the stamp may be shrunk to, as a fraction of the page.
///
/// Below this the mock-up stops being legible and, more to the point, a stamp
/// you cannot grab again is a stamp you cannot fix.

const MIN_SIDE: f32 = 0.03;

/// Which grip on the stamp is being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]

pub enum Grab {
    /// The body: the whole rectangle travels.
    Body,
    /// A corner, indexed 0 = top left, 1 = top right, 2 = bottom left,
    /// 3 = bottom right.
    Corner(usize),
}

/// Puts a stamp back in order: corners sorted, minimum size enforced, and the
/// whole thing kept on the page.
///
/// Everything that touches the stamp goes through here, so no other code has
/// to remember that a rectangle dragged upwards arrives inside out.

pub fn normalise(stamp: [f32; 4]) -> [f32; 4] {
    let [a, b, c, d] = stamp;
    let (mut x0, mut x1) = (a.min(c), a.max(c));
    let (mut y0, mut y1) = (b.min(d), b.max(d));

    // Grow to the minimum away from the edge that has room, so a stamp pinned
    // against the right margin grows leftwards instead of off the page.
    if x1 - x0 < MIN_SIDE {
        if x0 + MIN_SIDE <= 1.0 {
            x1 = x0 + MIN_SIDE;
        } else {
            x0 = x1 - MIN_SIDE;
        }
    }
    if y1 - y0 < MIN_SIDE {
        if y0 + MIN_SIDE <= 1.0 {
            y1 = y0 + MIN_SIDE;
        } else {
            y0 = y1 - MIN_SIDE;
        }
    }

    [
        x0.clamp(0.0, 1.0),
        y0.clamp(0.0, 1.0),
        x1.clamp(0.0, 1.0),
        y1.clamp(0.0, 1.0),
    ]
}

/// Slides the stamp by a fraction of the page, keeping its size and keeping it
/// on the page.

pub fn move_stamp(stamp: [f32; 4], dx: f32, dy: f32) -> [f32; 4] {
    let [x0, y0, x1, y1] = normalise(stamp);
    let (w, h) = (x1 - x0, y1 - y0);
    // Clamping the origin against `1 - size` is what stops a drag past the edge
    // from squashing the stamp instead of stopping it.
    let nx = (x0 + dx).clamp(0.0, (1.0 - w).max(0.0));
    let ny = (y0 + dy).clamp(0.0, (1.0 - h).max(0.0));
    [nx, ny, nx + w, ny + h]
}

/// Drags one corner to a point, leaving the opposite corner where it is.

pub fn resize_stamp(stamp: [f32; 4], corner: usize, to: (f32, f32)) -> [f32; 4] {
    let [mut x0, mut y0, mut x1, mut y1] = normalise(stamp);
    let (x, y) = (to.0.clamp(0.0, 1.0), to.1.clamp(0.0, 1.0));
    match corner {
        0 => (x0, y0) = (x, y),
        1 => (x1, y0) = (x, y),
        2 => (x0, y1) = (x, y),
        _ => (x1, y1) = (x, y),
    }
    normalise([x0, y0, x1, y1])
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

    #[test]
    fn normalise_orders_the_corners_and_keeps_a_minimum() {
        assert_eq!(normalise([0.6, 0.8, 0.2, 0.3]), [0.2, 0.3, 0.6, 0.8]);

        // `0.5 + MIN_SIDE - 0.5` is not exactly MIN_SIDE in f32, and a few
        // billionths of a page are nobody's problem.
        let big_enough = |r: [f32; 4]| {
            r[2] - r[0] >= MIN_SIDE - 1e-6 && r[3] - r[1] >= MIN_SIDE - 1e-6
        };

        // Collapsed to nothing: it must come back grabbable.
        let r = normalise([0.5, 0.5, 0.5, 0.5]);
        assert!(big_enough(r), "{r:?}");

        // Collapsed against the far edge: it has to grow inwards, not off the
        // page.
        let r = normalise([1.0, 1.0, 1.0, 1.0]);
        assert!(r[2] <= 1.0 && r[3] <= 1.0, "{r:?} runs off the page");
        assert!(big_enough(r), "{r:?}");
    }

    #[test]
    fn moving_keeps_the_size_and_stops_at_the_edge() {
        let stamp = [0.1, 0.1, 0.3, 0.2];

        let moved = move_stamp(stamp, 0.2, 0.1);
        assert_eq!(moved, [0.3, 0.2, 0.5, 0.3]);

        // Shoved well past the right and bottom edges: it stops flush, same
        // size. Squashing it here would be the easy bug.
        let shoved = move_stamp(stamp, 5.0, 5.0);
        assert!((shoved[2] - 1.0).abs() < 1e-6 && (shoved[3] - 1.0).abs() < 1e-6, "{shoved:?}");
        assert!((shoved[2] - shoved[0] - 0.2).abs() < 1e-6, "width changed: {shoved:?}");
        assert!((shoved[3] - shoved[1] - 0.1).abs() < 1e-6, "height changed: {shoved:?}");
    }

    #[test]
    fn resizing_moves_one_corner_and_pins_the_other() {
        let stamp = [0.2, 0.2, 0.8, 0.6];

        // Top left in: the bottom right must not budge.
        let r = resize_stamp(stamp, 0, (0.4, 0.3));
        assert_eq!(r, [0.4, 0.3, 0.8, 0.6]);

        // Bottom right out: the top left must not budge.
        let r = resize_stamp(stamp, 3, (0.9, 0.7));
        assert_eq!(r, [0.2, 0.2, 0.9, 0.7]);

        // Dragged past the opposite corner: it flips instead of inverting.
        let r = resize_stamp(stamp, 3, (0.1, 0.1));
        assert!(r[0] < r[2] && r[1] < r[3], "inside out: {r:?}");
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
