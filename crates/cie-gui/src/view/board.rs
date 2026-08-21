//! The signature box in the editor, and the dragging of the pieces in it.
//!
//! The same arithmetic as `view::stamp`, one level down: there the whole box
//! is dragged over a page, here each piece is dragged inside the box. Both
//! measure in fractions with y downwards, so both go through
//! `preview::{move_stamp, resize_stamp}` — the rules about not falling off the
//! edge and not shrinking to nothing are written once and apply to both.

use iced::keyboard;
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, LineDash, Path, Stroke, Text};
use iced::widget::image;
use iced::{Color, Element, Fill, Pixels, Point, Rectangle, Renderer, Size, Theme};

use cie_firma::{Drawn, Ink, Vertical};

use crate::preview::{move_stamp, resize_stamp, Grab};
use crate::snap::{self, Anchors};
use crate::state::Message;
use crate::theme;

/// Side of the corner grips, in pixels, and the slack on hitting one.
const GRIP: f32 = 6.0;
const SLACK: f32 = 1.8;

/// The picture, as much of it as the board needs.
#[derive(Debug, Clone)]
pub struct Art {
    pub handle: image::Handle,
    /// Width over height. Without it the board would stretch the picture to
    /// its rectangle while the renderer fits it, and the editor would be
    /// showing a signature nobody is going to get.
    pub ratio: f32,
}

/// What the template puts behind the stamp.
#[derive(Debug, Clone, Copy)]
pub struct Paper {
    /// White paper, or the document showing through.
    pub background: bool,
    /// A hairline around the box.
    pub border: bool,
}

/// The box as the editor shows it.
pub fn canvas(
    pieces: Vec<Drawn>,
    selected: Option<usize>,
    picture: Option<Art>,
    size_pt: (f32, f32),
    paper: Paper,
) -> Element<'static, Message> {
    canvas::Canvas::new(Board {
        pieces,
        selected,
        picture,
        size_pt,
        paper,
    })
    .width(Fill)
    .height(Fill)
    .into()
}

struct Board {
    /// Already resolved: the editor shows the words that will be printed, not
    /// the names of the pieces that produce them.
    pieces: Vec<Drawn>,
    selected: Option<usize>,
    picture: Option<Art>,
    /// The box in PDF points: its shape, and the scale every type size is
    /// measured against.
    size_pt: (f32, f32),
    paper: Paper,
}

#[derive(Default)]
struct Dragging {
    grab: Option<Grab>,
    last: Option<Point>,
    /// The lines the piece is currently sitting on, to draw while it is held.
    /// Kept here rather than worked out again in `draw`, which does not know
    /// what was dragged or where it came from.
    guides: (Vec<f32>, Vec<f32>),
    /// Alt lets a piece go where it is put. Every editor that snaps needs a
    /// way not to, or the one place you cannot put something is a hair off a
    /// line — which is sometimes exactly where it belongs.
    free: bool,
}

impl Board {
    /// Where the box lands inside the canvas: as wide as it can be, centred,
    /// keeping the proportions the box will really have.
    fn box_rect(&self, bounds: Rectangle) -> Rectangle {
        let ratio = (self.size_pt.0 / self.size_pt.1).max(0.01);

        let mut width = bounds.width;
        let mut height = width / ratio;
        if height > bounds.height {
            height = bounds.height;
            width = height * ratio;
        }
        Rectangle {
            x: (bounds.width - width) / 2.0,
            y: (bounds.height - height) / 2.0,
            width,
            height,
        }
    }

    fn piece_rect(&self, boxed: Rectangle, index: usize) -> Rectangle {
        share(boxed, self.pieces[index].placed.rect)
    }

    fn corners(&self, boxed: Rectangle, index: usize) -> [Point; 4] {
        let r = self.piece_rect(boxed, index);
        [
            Point::new(r.x, r.y),
            Point::new(r.x + r.width, r.y),
            Point::new(r.x, r.y + r.height),
            Point::new(r.x + r.width, r.y + r.height),
        ]
    }

    /// The lines everything but the piece in hand offers.
    fn anchors(&self, dragged: Option<usize>) -> Anchors {
        let rects: Vec<[f32; 4]> = self.pieces.iter().map(|d| d.placed.rect).collect();
        Anchors::around(&rects, dragged)
    }

    /// How far the reach is in fractions of the box, an axis at a time. The
    /// gesture is a distance on screen, so a box drawn wide has a narrower
    /// reach in fractions than the same box drawn small.
    fn reach(&self, boxed: Rectangle) -> (f32, f32) {
        (
            snap::REACH_PX / boxed.width.max(1.0),
            snap::REACH_PX / boxed.height.max(1.0),
        )
    }

    /// What is under a point: a grip of the selected piece first, then any
    /// piece's body, topmost last-drawn first.
    ///
    /// Only the selected piece shows grips, so only it can be resized. That is
    /// what stops a click meant to select a piece from resizing its neighbour.
    fn under(&self, boxed: Rectangle, p: Point) -> Option<(usize, Grab)> {
        if let Some(index) = self.selected.filter(|i| *i < self.pieces.len())
            && let Some(corner) = self
                .corners(boxed, index)
                .iter()
                .position(|c| c.distance(p) <= GRIP * SLACK)
        {
            return Some((index, Grab::Corner(corner)));
        }

        (0..self.pieces.len())
            .rev()
            .find(|i| self.piece_rect(boxed, *i).contains(p))
            .map(|i| (i, Grab::Body))
    }
}

impl canvas::Program<Message> for Board {
    type State = Dragging;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let local = |p: Point| Point::new(p.x - bounds.x, p.y - bounds.y);
        let boxed = self.box_rect(bounds);

        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = local(cursor.position_over(bounds)?);
                let found = self.under(boxed, p);

                state.grab = found.map(|(_, grab)| grab);
                state.last = Some(p);

                // A press on nothing clears the selection, which is how you
                // put the properties panel away without deleting anything.
                Some(canvas::Action::publish(Message::PieceSelected(
                    found.map(|(index, _)| index),
                )))
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let grab = state.grab?;
                let index = self.selected?;
                let rect = self.pieces.get(index)?.placed.rect;

                let p = local(cursor.position()?);
                let previous = state.last.replace(p)?;

                let anchors = self.anchors(Some(index));
                let reach = if state.free {
                    (0.0, 0.0)
                } else {
                    self.reach(boxed)
                };

                let moved = match grab {
                    Grab::Body => {
                        let dragged = move_stamp(
                            rect,
                            (p.x - previous.x) / boxed.width,
                            (p.y - previous.y) / boxed.height,
                        );
                        // Snapped after the drag rather than folded into it,
                        // so the clamping that keeps a piece in the box has
                        // the last word over the latching.
                        let (dx, dy) = snap::slide(dragged, &anchors, reach);
                        move_stamp(dragged, dx, dy)
                    }
                    Grab::Corner(corner) => {
                        let at = snap::point(
                            (
                                (p.x - boxed.x) / boxed.width,
                                (p.y - boxed.y) / boxed.height,
                            ),
                            &anchors,
                            reach,
                        );
                        resize_stamp(rect, corner, at)
                    }
                };

                state.guides = snap::touching(moved, &anchors);
                Some(canvas::Action::publish(Message::PieceMoved(moved)))
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.grab = None;
                state.last = None;
                // The guides belong to the drag. Left up, they would be a set
                // of lines pointing at nothing.
                state.guides = (Vec::new(), Vec::new());
                None
            }
            // The canvas is handed every event, modifiers included, so Alt can
            // be watched without the window having to route it here.
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.free = modifiers.alt();
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let boxed = self.box_rect(bounds);
        let accent = theme.extended_palette().primary.base.color;
        let mut frame = Frame::new(renderer, bounds.size());

        paper(&mut frame, boxed, self.paper);

        paint(&mut frame, boxed, &self.pieces, self.picture.as_ref(), self.size_pt.0);

        for (index, drawn) in self.pieces.iter().enumerate() {
            let rect = share(boxed, drawn.placed.rect);
            let outline = Path::rectangle(Point::new(rect.x, rect.y), rect.size());
            if self.selected == Some(index) {
                frame.stroke(&outline, Stroke::default().with_width(1.5).with_color(accent));
                for c in self.corners(boxed, index) {
                    frame.fill(
                        &Path::rectangle(
                            Point::new(c.x - GRIP / 2.0, c.y - GRIP / 2.0),
                            Size::new(GRIP, GRIP),
                        ),
                        accent,
                    );
                }
            } else {
                // Every piece keeps a faint outline: they are invisible
                // otherwise, and an empty one — a reason nobody gave — would
                // be impossible to find and click.
                frame.stroke(
                    &outline,
                    Stroke::default()
                        .with_width(1.0)
                        .with_color(Color { a: 0.25, ..accent }),
                );
            }
        }

        guides(&mut frame, boxed, &state.guides, accent);

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.grab.is_some() {
            return mouse::Interaction::Grabbing;
        }
        let Some(p) = cursor.position_over(bounds) else {
            return mouse::Interaction::default();
        };
        let boxed = self.box_rect(bounds);

        match self.under(boxed, Point::new(p.x - bounds.x, p.y - bounds.y)) {
            Some((_, Grab::Corner(0 | 3))) => mouse::Interaction::ResizingDiagonallyDown,
            Some((_, Grab::Corner(_))) => mouse::Interaction::ResizingDiagonallyUp,
            Some((_, Grab::Body)) => mouse::Interaction::Grab,
            None => mouse::Interaction::default(),
        }
    }
}

/// What goes behind the stamp, as the template asks for it.
///
/// Both halves are the document's, not the interface's: they are drawn into
/// the PDF by underskrift and have to look here the way they will look there.
/// A transparent stamp that the preview draws on white paper would hide the
/// one thing worth knowing before putting a box over a paragraph.
pub fn paper(frame: &mut Frame, boxed: Rectangle, paper: Paper) {
    let sheet = Path::rectangle(Point::new(boxed.x, boxed.y), boxed.size());

    if paper.background {
        frame.fill(&sheet, Color::WHITE);
    }
    if paper.border {
        frame.stroke(
            &sheet,
            Stroke::default()
                .with_width(1.0)
                .with_color(Color::from_rgb(0.2, 0.2, 0.2)),
        );
    }
}

/// The lines the piece in hand is sitting on, drawn right across the box.
///
/// Across the whole box rather than just between the two pieces: what the
/// guide is saying is "this line", and a stub between two rectangles leaves
/// you working out which line it meant.
pub fn guides(frame: &mut Frame, boxed: Rectangle, (xs, ys): &(Vec<f32>, Vec<f32>), accent: Color) {
    // Dashed, and that is the point: solid, a guide running along the selected
    // piece's own edge would be indistinguishable from its outline, which is
    // exactly where guides most often are. A dash reads as a guide without
    // needing a colour from outside the palette.
    let stroke = Stroke {
        line_dash: LineDash {
            segments: &[4.0, 3.0],
            offset: 0,
        },
        ..Stroke::default()
            .with_width(1.0)
            .with_color(Color { a: 0.9, ..accent })
    };

    for x in xs {
        let at = boxed.x + x * boxed.width;
        frame.stroke(
            &Path::line(Point::new(at, boxed.y), Point::new(at, boxed.y + boxed.height)),
            stroke,
        );
    }
    for y in ys {
        let at = boxed.y + y * boxed.height;
        frame.stroke(
            &Path::line(Point::new(boxed.x, at), Point::new(boxed.x + boxed.width, at)),
            stroke,
        );
    }
}

/// Paints the contents of a box: the lines where they were put, and the
/// picture fitted into its own rectangle.
///
/// The editor and the page draw through here, and that is the whole point.
/// Two copies of this would be two answers to "what will the stamp look
/// like", and the one on the page is the one people trust.
///
/// `box_width_pt` is what turns a type size in points into pixels. Off the
/// box's real width, so a box dragged twice as wide shows type at the size it
/// will really be printed at.
pub fn paint(
    frame: &mut Frame,
    boxed: Rectangle,
    pieces: &[Drawn],
    picture: Option<&Art>,
    box_width_pt: f32,
) {
    let px_per_pt = boxed.width / box_width_pt.max(1.0);

    for drawn in pieces {
        let area = share(boxed, drawn.placed.rect);
        match &drawn.ink {
            Ink::Picture => {
                if let Some(art) = picture {
                    draw_picture(frame, art, &drawn.placed, area);
                }
            }
            Ink::Line(text) => draw_line(frame, drawn, text, area, px_per_pt),
        }
    }
}

/// The picture, fitted and placed the way the renderer will place it.
///
/// Both the fit and the placing come from `cie_firma`, so there is no second
/// answer here to where a picture goes.
fn draw_picture(frame: &mut Frame, art: &Art, placed: &cie_firma::Placed, area: Rectangle) {
    let (width, height) = cie_firma::fit(art.ratio, (area.width, area.height));
    let (slack_x, slack_y) = (area.width - width, area.height - height);

    frame.draw_image(
        Rectangle {
            x: area.x + cie_firma::across(placed.align, slack_x),
            // Down here the y axis runs downwards, so `Top` is the near end of
            // the slack. The renderer, working in PDF space, has it the other
            // way round — the one place these two disagree, and only about
            // which way is up.
            y: area.y
                + match placed.vertical {
                    Vertical::Top => 0.0,
                    Vertical::Middle => slack_y / 2.0,
                    Vertical::Bottom => slack_y,
                },
            width,
            height,
        },
        &art.handle,
    );
}

/// A line, drawn where and how the renderer will draw it.
///
/// Two details are what make this a preview rather than an impression. The
/// line is placed by the width Helvetica will give it, not the width the
/// interface's typeface gives it here — the glyphs differ either way, but a
/// centred line lands where it will really land. And it is clipped to its own
/// rectangle, because the renderer clips too: a preview that shows a tidy box
/// over a PDF that spills is worse than one that shows the spill.
fn draw_line(frame: &mut Frame, drawn: &Drawn, text: &str, area: Rectangle, px_per_pt: f32) {
    let size = drawn.placed.size * px_per_pt;
    if size < 4.0 {
        return;
    }

    let drawn_width = drawn.placed.width_of(text) * px_per_pt;
    let x = area.x + cie_firma::across(drawn.placed.align, area.width - drawn_width);

    frame.with_clip(area, |frame| {
        frame.fill_text(Text {
            content: text.to_string(),
            position: Point::new(x, area.y),
            color: Color::BLACK,
            size: Pixels(size),
            font: if drawn.placed.bold {
                theme::UI_STRONG
            } else {
                theme::UI
            },
            ..Text::default()
        });
    });
}

/// A fraction of the box as a rectangle on screen.
pub fn share(boxed: Rectangle, [x0, y0, x1, y1]: [f32; 4]) -> Rectangle {
    Rectangle {
        x: boxed.x + x0 * boxed.width,
        y: boxed.y + y0 * boxed.height,
        width: (x1 - x0) * boxed.width,
        height: (y1 - y0) * boxed.height,
    }
}
