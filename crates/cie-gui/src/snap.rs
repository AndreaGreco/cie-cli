//! Latching pieces on to each other, and on to the box around them.
//!
//! Composing a signature box by eye means nudging a line a pixel at a time and
//! still ending up with two left edges that are almost the same. The anchors
//! here are the almost: every edge and every middle, of the box and of the
//! pieces already in it, is a line a dragged piece will latch on to when it
//! comes near enough.
//!
//! Everything is in fractions of the box, `[x0, y0, x1, y1]` with y downwards,
//! the same as `Placed::rect` - with one exception, [`REACH_PX`], and the
//! reason is in its own doc comment.
//!
//! No widgets here. The whole point is that the arithmetic can be checked
//! without opening a window.

/// How near a piece has to come before it latches on, in **pixels**.
///
/// A gesture, not a measurement. The hand has to be within the same distance
/// of a line however large the box is drawn on screen, so this is converted
/// into fractions against the box's size on screen, not stored as one.
///
/// Four rather than six, and the difference is all on the vertical axis. Five
/// stacked lines put twelve anchors down a box only a couple of hundred pixels
/// tall - a median gap of about sixteen pixels - so at six the reaches almost
/// touch and less than half the box is somewhere a piece can simply be left.
/// At four it is over half. Across the box it hardly matters either way: the
/// lines there share edges, so they collapse to five anchors and nine tenths
/// of the width is free at any of these numbers.
pub const REACH_PX: f32 = 4.0;

/// How far in from the paper's edge the margin lines sit.
///
/// Nobody puts a signature flush against the edge of the sheet, so the edges
/// alone are poor anchors. Twenty millimetres is the margin most documents
/// that arrive here turn out to have; it is a line offered, not imposed, and
/// Alt or simply staying away from it ignores it.
const MARGIN_MM: f32 = 20.0;

/// Two coordinates are the same line when they are this close, in fractions.
///
/// Loose enough to survive the rounding that clamping a rectangle does on the
/// way through, tight enough to be under half a pixel on any box worth
/// composing in.
const SAME: f32 = 1e-3;

/// The lines a piece can latch on to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Anchors {
    pub x: Vec<f32>,
    pub y: Vec<f32>,
}

impl Anchors {
    /// The box's edges and middle, plus the edges and middles of the pieces
    /// that are not the one being dragged.
    ///
    /// The dragged piece is left out, and it has to be: a piece that could
    /// latch on to its own edges would never come off them.
    pub fn around(rects: &[[f32; 4]], dragged: Option<usize>) -> Self {
        // The box first, so it wins ties against a piece sitting on the same
        // line - a guide on the box edge says more than one on a neighbour
        // that happens to be there too.
        let mut anchors = Self {
            x: vec![0.0, 0.5, 1.0],
            y: vec![0.0, 0.5, 1.0],
        };

        for (index, [x0, y0, x1, y1]) in rects.iter().enumerate() {
            if Some(index) == dragged {
                continue;
            }
            anchors.x.extend([*x0, (x0 + x1) / 2.0, *x1]);
            anchors.y.extend([*y0, (y0 + y1) / 2.0, *y1]);
        }

        anchors.x = collapse(anchors.x);
        anchors.y = collapse(anchors.y);
        anchors
    }
}

impl Anchors {
    /// The lines a signature box can latch on to on a page: the edges of the
    /// paper, its middle, and a margin's width in from each edge.
    ///
    /// The margin is worked out from the page's real size, so it is the same
    /// distance on an A4 and on a US Letter rather than the same fraction of
    /// two different sheets.
    pub fn page(size_pt: (f32, f32)) -> Self {
        let margin = MARGIN_MM * 72.0 / 25.4;
        let (mx, my) = (
            margin / size_pt.0.max(1.0),
            margin / size_pt.1.max(1.0),
        );

        // A margin wider than half the sheet is not a margin. Better to offer
        // the edges alone than lines that cross each other.
        let inset = |m: f32| if m < 0.5 { vec![m, 1.0 - m] } else { Vec::new() };

        let mut anchors = Self {
            x: vec![0.0, 0.5, 1.0],
            y: vec![0.0, 0.5, 1.0],
        };
        anchors.x.extend(inset(mx));
        anchors.y.extend(inset(my));

        anchors.x = collapse(anchors.x);
        anchors.y = collapse(anchors.y);
        anchors
    }
}

/// Drops coordinates that name a line already in the list, keeping the first.
///
/// Without this, three pieces sharing a left edge would draw three guides on
/// top of each other and the topmost would be the only one anybody saw.
fn collapse(coordinates: Vec<f32>) -> Vec<f32> {
    let mut kept: Vec<f32> = Vec::with_capacity(coordinates.len());
    for c in coordinates {
        if !kept.iter().any(|k| (k - c).abs() < SAME) {
            kept.push(c);
        }
    }
    kept
}

/// How far a rectangle has to slide for one of its edges - or its middle - to
/// land on an anchor.
///
/// `(0.0, 0.0)` when nothing is within reach. The two axes are worked out
/// apart, so a piece can latch on horizontally while still moving freely up
/// and down, which is what makes the gesture feel like guidance rather than a
/// grid.
pub fn slide(rect: [f32; 4], anchors: &Anchors, reach: (f32, f32)) -> (f32, f32) {
    let [x0, y0, x1, y1] = rect;
    (
        nearest(&[x0, (x0 + x1) / 2.0, x1], &anchors.x, reach.0),
        nearest(&[y0, (y0 + y1) / 2.0, y1], &anchors.y, reach.1),
    )
}

/// Pulls a single point on to the nearest anchor within reach, an axis at a
/// time. This is the one a corner grip uses.
pub fn point(at: (f32, f32), anchors: &Anchors, reach: (f32, f32)) -> (f32, f32) {
    (
        at.0 + nearest(&[at.0], &anchors.x, reach.0),
        at.1 + nearest(&[at.1], &anchors.y, reach.1),
    )
}

/// The smallest move that puts any of `coordinates` on any of `anchors`.
fn nearest(coordinates: &[f32], anchors: &[f32], reach: f32) -> f32 {
    let mut best = 0.0;
    let mut distance = reach;

    for c in coordinates {
        for a in anchors {
            let delta = a - c;
            if delta.abs() < distance {
                distance = delta.abs();
                best = delta;
            }
        }
    }
    best
}

/// A rectangle being dragged, and where it would be if nothing had latched
/// on to anything.
///
/// The two positions are the whole of being able to come off a line again.
/// Sliding the *latched* rectangle and latching the result means every small
/// move is undone by the same line that caught it: the pointer walks away and
/// the rectangle stays, and it only ever comes off if a single frame of the
/// drag clears the reach in one go - which, with a hand on a mouse, it never
/// does. So the drag is accumulated on a position nothing has touched, and the
/// latching is worked out from that each time.
#[derive(Debug, Clone, Copy)]
pub struct Drag {
    free: [f32; 4],
}

impl Drag {
    /// Starts a drag from where the rectangle is now.
    pub fn from(rect: [f32; 4]) -> Self {
        Self { free: rect }
    }

    /// Slides by a fraction of the box and says where the rectangle should be
    /// drawn: on a line when one is within reach of where it truly is, and
    /// where the hand put it once it is not.
    ///
    /// `keep` is what stops a rectangle leaving the box - `preview::move_stamp`
    /// in every caller, passed in so this module keeps knowing nothing about
    /// pages and stamps.
    pub fn slide(
        &mut self,
        (dx, dy): (f32, f32),
        anchors: &Anchors,
        reach: (f32, f32),
        keep: impl Fn([f32; 4], f32, f32) -> [f32; 4],
    ) -> [f32; 4] {
        self.free = keep(self.free, dx, dy);

        let (sx, sy) = slide(self.free, anchors, reach);
        keep(self.free, sx, sy)
    }
}

/// The anchors a rectangle is actually sitting on.
///
/// Worked out from where the piece ended up rather than from where the snap
/// meant to put it: clamping happens after, and a guide drawn on a line the
/// piece was pulled back off would be pointing at nothing.
pub fn touching(rect: [f32; 4], anchors: &Anchors) -> (Vec<f32>, Vec<f32>) {
    let [x0, y0, x1, y1] = rect;
    (
        sitting_on(&[x0, (x0 + x1) / 2.0, x1], &anchors.x),
        sitting_on(&[y0, (y0 + y1) / 2.0, y1], &anchors.y),
    )
}

fn sitting_on(coordinates: &[f32], anchors: &[f32]) -> Vec<f32> {
    let mut found: Vec<f32> = Vec::new();
    for a in anchors {
        if coordinates.iter().any(|c| (c - a).abs() < SAME)
            && !found.iter().any(|f| (f - a).abs() < SAME)
        {
            found.push(*a);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two pieces, the second one wider and offset.
    fn two() -> Vec<[f32; 4]> {
        vec![[0.10, 0.10, 0.30, 0.20], [0.50, 0.60, 0.90, 0.80]]
    }

    /// The box is always there to latch on to, even in an empty template.
    #[test]
    fn an_empty_box_still_has_its_own_lines() {
        let anchors = Anchors::around(&[], None);
        assert_eq!(anchors.x, vec![0.0, 0.5, 1.0]);
        assert_eq!(anchors.y, vec![0.0, 0.5, 1.0]);
    }

    /// Every edge and every middle of the other pieces, and nothing of the one
    /// in hand: a piece that could latch on to itself would never come off.
    #[test]
    fn the_piece_in_hand_offers_no_lines_of_its_own() {
        let anchors = Anchors::around(&two(), Some(0));

        for line in [0.50, 0.70, 0.90] {
            assert!(anchors.x.iter().any(|x| (x - line).abs() < SAME), "{line}");
        }
        for line in [0.10, 0.20, 0.30] {
            assert!(
                !anchors.x.iter().any(|x| (x - line).abs() < SAME),
                "the dragged piece put {line} on the list"
            );
        }
    }

    /// A line named twice is one line. Three pieces sharing a left edge would
    /// otherwise draw three guides on top of each other.
    #[test]
    fn the_same_line_is_only_listed_once() {
        let stacked = vec![
            [0.10, 0.10, 0.40, 0.20],
            [0.10, 0.30, 0.40, 0.40],
            [0.10, 0.50, 0.40, 0.60],
        ];
        let anchors = Anchors::around(&stacked, None);

        assert_eq!(anchors.x.iter().filter(|x| (**x - 0.10).abs() < SAME).count(), 1);
        // 0, 0.5, 1 from the box, plus 0.10, 0.25 and 0.40 from the pieces.
        assert_eq!(anchors.x.len(), 6, "{:?}", anchors.x);
    }

    /// A left edge just short of a neighbour's is pulled on to it.
    #[test]
    fn an_edge_within_reach_latches_on() {
        let anchors = Anchors::around(&two(), Some(0));
        // x0 at 0.49 is 0.01 short of the other piece's left edge.
        let (dx, _) = slide([0.49, 0.30, 0.69, 0.40], &anchors, (0.02, 0.02));

        assert!((dx - 0.01).abs() < 1e-6, "{dx}");
    }

    /// Out of reach, nothing moves. Snapping that reached across the whole box
    /// would be a box you cannot put anything in the middle of.
    #[test]
    fn out_of_reach_nothing_moves() {
        let anchors = Anchors::around(&two(), Some(0));
        let (dx, dy) = slide([0.35, 0.35, 0.45, 0.45], &anchors, (0.02, 0.02));

        assert_eq!((dx, dy), (0.0, 0.0));
    }

    /// Nearest wins. With two lines in reach the piece must not jump past the
    /// close one to the far one.
    #[test]
    fn the_nearest_line_wins() {
        let anchors = Anchors {
            x: vec![0.30, 0.34],
            y: vec![],
        };
        let (dx, _) = slide([0.33, 0.0, 0.43, 0.1], &anchors, (0.05, 0.05));

        assert!((dx - 0.01).abs() < 1e-6, "should reach 0.34, not 0.30: {dx}");
    }

    /// The middle latches on too, which is the only way to centre a line in
    /// the box without arithmetic.
    #[test]
    fn a_middle_latches_on_to_a_middle() {
        let anchors = Anchors::around(&[], None);
        // A piece whose centre is at 0.49, a hair off the box's middle.
        let (dx, _) = slide([0.29, 0.0, 0.69, 0.1], &anchors, (0.02, 0.02));

        assert!((dx - 0.01).abs() < 1e-6, "{dx}");
    }

    /// The axes are worked out apart: latching on horizontally must not pin
    /// the piece vertically as well.
    #[test]
    fn one_axis_can_latch_while_the_other_stays_free() {
        let anchors = Anchors {
            x: vec![0.50],
            y: vec![0.90],
        };
        let (dx, dy) = slide([0.49, 0.20, 0.60, 0.30], &anchors, (0.02, 0.02));

        assert!((dx - 0.01).abs() < 1e-6);
        assert_eq!(dy, 0.0, "the far axis should have been left alone");
    }

    /// A corner grip pulls the point itself, not a whole rectangle.
    #[test]
    fn a_corner_is_pulled_on_to_a_line() {
        let anchors = Anchors::around(&two(), Some(0));
        let at = point((0.49, 0.61), &anchors, (0.02, 0.02));

        assert!((at.0 - 0.50).abs() < 1e-6, "{at:?}");
        assert!((at.1 - 0.60).abs() < 1e-6, "{at:?}");
    }

    /// The guides come from where the piece ended up. Anything else and a
    /// piece clamped back off a line would still be drawing one.
    #[test]
    fn the_guides_report_where_the_piece_really_is() {
        let anchors = Anchors::around(&two(), Some(0));
        let (x, y) = touching([0.50, 0.60, 0.70, 0.70], &anchors);

        assert!(x.contains(&0.50), "{x:?}");
        assert!(y.contains(&0.60), "{y:?}");

        let (x, y) = touching([0.33, 0.33, 0.43, 0.43], &anchors);
        assert!(x.is_empty() && y.is_empty(), "{x:?} {y:?}");
    }

    /// One guide per line, however many of the rectangle's own edges are on
    /// it - a piece exactly as wide as the box would otherwise draw the same
    /// line three times.
    #[test]
    fn a_line_is_guided_once() {
        let anchors = Anchors {
            x: vec![0.5],
            y: vec![],
        };
        let (x, _) = touching([0.5, 0.0, 0.5, 0.1], &anchors);

        assert_eq!(x, vec![0.5]);
    }

    /// A page offers its edges, its middle and its margins, and the margins
    /// are a real distance rather than a fraction: the same twenty
    /// millimetres on any sheet.
    #[test]
    fn a_page_offers_its_margins_in_millimetres() {
        // A4, in points.
        let a4 = Anchors::page((595.0, 842.0));
        let expected = 20.0 * 72.0 / 25.4;

        assert!((a4.x[3] * 595.0 - expected).abs() < 0.01, "{:?}", a4.x);
        assert!((a4.y[3] * 842.0 - expected).abs() < 0.01, "{:?}", a4.y);

        // The same margin on a taller sheet is a smaller fraction of it.
        let long = Anchors::page((595.0, 1684.0));
        assert!(long.y[3] < a4.y[3], "{:?} {:?}", long.y, a4.y);
    }

    /// A sheet too small to have that margin gets its edges and nothing else,
    /// rather than margin lines that have crossed over each other.
    #[test]
    fn a_sheet_smaller_than_its_margins_keeps_only_its_edges() {
        let stamp = Anchors::page((40.0, 40.0));
        assert_eq!(stamp.x, vec![0.0, 0.5, 1.0]);
        assert_eq!(stamp.y, vec![0.0, 0.5, 1.0]);
    }

    /// The bug this whole type exists for: a piece that latched on to a line
    /// could not be pulled off it. Sliding the latched rectangle and latching
    /// the result undid every small move, so the pointer walked away and the
    /// piece stayed put.
    #[test]
    fn a_piece_can_be_pulled_off_a_line() {
        let anchors = Anchors {
            x: vec![0.5],
            y: vec![],
        };
        let reach = (0.02, 0.02);
        let free = |r: [f32; 4], dx: f32, dy: f32| [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy];

        let mut drag = Drag::from([0.5, 0.0, 0.6, 0.1]);

        // A nudge well inside the reach: it stays on the line, as it should.
        assert_eq!(drag.slide((0.005, 0.0), &anchors, reach, free)[0], 0.5);

        // The same nudge over and over. Each one on its own is inside the
        // reach - that is exactly what used to glue it down - but they add up
        // on the free position, and past the reach the piece comes away.
        let mut at = [0.0; 4];
        for _ in 0..5 {
            at = drag.slide((0.005, 0.0), &anchors, reach, free);
        }
        assert!(at[0] > 0.52, "still stuck to the line at {}", at[0]);
    }

    /// And having come off, it goes on following the hand rather than being
    /// dragged back the moment it passes the line again from the far side.
    #[test]
    fn a_piece_that_came_off_keeps_going() {
        let anchors = Anchors {
            x: vec![0.5],
            y: vec![],
        };
        let reach = (0.02, 0.02);
        let free = |r: [f32; 4], dx: f32, dy: f32| [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy];

        let mut drag = Drag::from([0.5, 0.0, 0.6, 0.1]);
        let mut at = [0.0; 4];
        for _ in 0..20 {
            at = drag.slide((0.01, 0.0), &anchors, reach, free);
        }

        // Twenty of those is 0.2 away, which no reach of 0.02 should reel in.
        assert!((at[0] - 0.7).abs() < 1e-4, "{at:?}");
    }

    /// Coming back to a line still latches on: leaving has to stay possible
    /// without the latching stopping working.
    #[test]
    fn a_piece_still_latches_on_when_it_comes_back() {
        let anchors = Anchors {
            x: vec![0.5],
            y: vec![],
        };
        let reach = (0.02, 0.02);
        let free = |r: [f32; 4], dx: f32, dy: f32| [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy];

        let mut drag = Drag::from([0.3, 0.0, 0.4, 0.1]);
        let mut at = [0.0; 4];
        for _ in 0..20 {
            at = drag.slide((0.01, 0.0), &anchors, reach, free);
        }
        assert_eq!(at[0], 0.5, "walked past the line without catching: {at:?}");
    }

    /// With no reach at all - Alt held down - the rectangle goes exactly where
    /// it is put, however many lines it walks over.
    #[test]
    fn no_reach_means_no_latching() {
        let anchors = Anchors::around(&[], None);
        let free = |r: [f32; 4], dx: f32, dy: f32| [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy];

        let mut drag = Drag::from([0.1, 0.0, 0.2, 0.1]);
        let mut at = [0.0; 4];
        for _ in 0..40 {
            at = drag.slide((0.01, 0.0), &anchors, (0.0, 0.0), free);
        }
        assert!((at[0] - 0.5).abs() < 1e-4, "{at:?}");
    }
}
