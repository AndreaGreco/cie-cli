//! Latching pieces on to each other, and on to the box around them.
//!
//! Composing a signature box by eye means nudging a line a pixel at a time and
//! still ending up with two left edges that are almost the same. The anchors
//! here are the almost: every edge and every middle, of the box and of the
//! pieces already in it, is a line a dragged piece will latch on to when it
//! comes near enough.
//!
//! Everything is in fractions of the box, `[x0, y0, x1, y1]` with y downwards,
//! the same as `Placed::rect` — with one exception, [`REACH_PX`], and the
//! reason is in its own doc comment.
//!
//! No widgets here. The whole point is that the arithmetic can be checked
//! without opening a window.

/// How near a piece has to come before it latches on, in **pixels**.
///
/// A gesture, not a measurement. The hand has to be within the same distance
/// of a line however large the box is drawn on screen, so this is converted
/// into fractions against the box's size on screen, not stored as one.
pub const REACH_PX: f32 = 6.0;

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
        // line — a guide on the box edge says more than one on a neighbour
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

/// How far a rectangle has to slide for one of its edges — or its middle — to
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
    /// it — a piece exactly as wide as the box would otherwise draw the same
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
}
