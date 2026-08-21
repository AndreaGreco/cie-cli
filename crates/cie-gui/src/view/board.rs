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
use crate::snap::{self, Anchors, Drag};
use crate::state::Message;
use crate::theme;

/// Side of the corner grips, in pixels, and the slack on hitting one.
const GRIP: f32 = 6.0;
const SLACK: f32 = 1.8;

/// Air left around the box inside the canvas.
///
/// Not decoration. The box's own edges are what you take hold of to make it
/// bigger, and they can only be had from outside — inside belongs to the
/// pieces — so an edge flush against the side of the canvas is an edge with
/// nowhere to reach it from. Ten pixels is room for a grip and little enough
/// that the box still gets nearly all the space there is.
const MARGIN: f32 = 10.0;

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
    snapping: bool,
) -> Element<'static, Message> {
    canvas::Canvas::new(Board {
        pieces,
        selected,
        picture,
        size_pt,
        paper,
        snapping,
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
    /// Whether pieces latch on at all. Alt turns it round for as long as it is
    /// held, so there is a way to do the other thing without going to look for
    /// the setting.
    snapping: bool,
}

/// What the pointer has hold of on the board.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Hold {
    /// A piece, by its body or by one of its corners.
    Piece(usize, Grab),
    /// The box itself, by an edge or a corner of it, and which of its two
    /// sides that edge changes.
    Box { wide: bool, tall: bool },
}

/// The box as it was at the moment an edge of it was taken hold of.
///
/// The scale is the whole reason any of this is remembered rather than
/// measured again every frame. The board draws the box as large as it will
/// go, so a box made wider in millimetres is not drawn wider by the same
/// amount — it is drawn again at a new scale. Measure the drag against that
/// new scale and the drawing feeds back into the size: half a box away from
/// the middle would halve it, again, and again, and the box would run away in
/// the hand. The scale at the press is a constant, and the millimetres follow
/// the pointer and nothing else.
#[derive(Debug, Clone, Copy)]
struct Stretch {
    /// Pixels to the millimetre when the edge was taken hold of.
    scale: f32,
    /// The middle of the box, which every redraw keeps in the middle of the
    /// canvas however the box is resized.
    centre: Point,
    /// What it measured then, for the side this drag does not touch.
    size_mm: [f32; 2],
}

impl Stretch {
    /// How big the box is with its edge pulled to this point.
    ///
    /// Twice the distance from the middle, because the box is drawn around the
    /// middle of the canvas: an edge is half a box away from it, whichever
    /// edge it is, so pulling one out by a millimetre puts a millimetre on
    /// each side.
    fn pulled_to(&self, p: Point, (wide, tall): (bool, bool)) -> [f32; 2] {
        let mut size = self.size_mm;
        if wide {
            size[0] = 2.0 * (p.x - self.centre.x).abs() / self.scale;
        }
        if tall {
            size[1] = 2.0 * (p.y - self.centre.y).abs() / self.scale;
        }
        size
    }
}

#[derive(Default)]
struct Dragging {
    hold: Option<Hold>,
    last: Option<Point>,
    /// Where the piece would be with nothing to latch on to. Only a body drag
    /// needs it: a corner already follows the pointer's own position, so it
    /// was never glued the way a body was.
    travel: Option<Drag>,
    /// The box as it was when an edge of it was taken hold of. See [`Stretch`].
    stretch: Option<Stretch>,
    /// The lines the piece is currently sitting on, to draw while it is held.
    /// Kept here rather than worked out again in `draw`, which does not know
    /// what was dragged or where it came from.
    guides: (Vec<f32>, Vec<f32>),
    /// Alt, held. It inverts the setting rather than only switching latching
    /// off: with latching already off, the momentary gesture has to be able to
    /// reach for a line.
    alt: bool,
}

impl Dragging {
    /// Lets go of whatever was being dragged.
    ///
    /// The guides go with it: they belong to the gesture, and left on screen
    /// with nothing in hand they are lines pointing at nothing.
    /// Where the box is drawn while one of its edges is being pulled.
    ///
    /// At the scale it had when it was taken hold of, so it grows under the
    /// hand: the edge stays exactly where the pointer is, the type inside
    /// stays the size it will really be printed at, and a box pulled bigger
    /// than the room simply runs past the sides until it is let go. Drawn to
    /// fit instead, a box already as wide as the canvas could not be made any
    /// wider — it would only get shorter, which is a strange answer to being
    /// pulled outwards.
    ///
    /// `None` when nothing is being pulled, and then the fit has it.
    fn stretching(&self, size_mm: [f32; 2]) -> Option<Rectangle> {
        let stretch = self
            .stretch
            .filter(|_| matches!(self.hold, Some(Hold::Box { .. })))?;

        let (width, height) = (size_mm[0] * stretch.scale, size_mm[1] * stretch.scale);
        Some(Rectangle {
            x: stretch.centre.x - width / 2.0,
            y: stretch.centre.y - height / 2.0,
            width,
            height,
        })
    }

    fn let_go(&mut self) {
        self.last = None;
        self.travel = None;
        self.stretch = None;
        self.guides = (Vec::new(), Vec::new());
    }
}

impl Board {
    /// Where the box lands inside the canvas: as wide as it can be, centred,
    /// keeping the proportions the box will really have, and never quite
    /// touching the sides. See [`MARGIN`].
    fn box_rect(&self, bounds: Rectangle) -> Rectangle {
        let ratio = (self.size_pt.0 / self.size_pt.1).max(0.01);
        let room = (
            (bounds.width - 2.0 * MARGIN).max(1.0),
            (bounds.height - 2.0 * MARGIN).max(1.0),
        );

        let mut width = room.0;
        let mut height = width / ratio;
        if height > room.1 {
            height = room.1;
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

    /// The box in millimetres, which is what its size is really kept in.
    fn size_mm(&self) -> [f32; 2] {
        [
            cie_firma::pt_to_mm(self.size_pt.0),
            cie_firma::pt_to_mm(self.size_pt.1),
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

    /// What is under a point: a grip of the selected piece first, then an edge
    /// of the box, then any piece's body, topmost last-drawn first.
    ///
    /// Only the selected piece shows grips, so only it can be resized. That is
    /// what stops a click meant to select a piece from resizing its neighbour.
    /// The box comes before the bodies and after the grips, and it can only be
    /// had from outside itself, so it never takes a gesture meant for anything
    /// in it.
    fn under(&self, boxed: Rectangle, p: Point) -> Option<Hold> {
        if let Some(index) = self.selected.filter(|i| *i < self.pieces.len())
            && let Some(corner) = on_corner(self.piece_rect(boxed, index), p)
        {
            return Some(Hold::Piece(index, Grab::Corner(corner)));
        }

        if let Some((wide, tall)) = on_box(boxed, p) {
            return Some(Hold::Box { wide, tall });
        }

        (0..self.pieces.len())
            .rev()
            .find(|i| self.piece_rect(boxed, *i).contains(p))
            .map(|i| Hold::Piece(i, Grab::Body))
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

                state.hold = found;
                state.last = Some(p);
                state.travel = match found {
                    Some(Hold::Piece(index, Grab::Body)) => {
                        self.pieces.get(index).map(|d| Drag::from(d.placed.rect))
                    }
                    _ => None,
                };
                state.stretch = matches!(found, Some(Hold::Box { .. })).then(|| Stretch {
                    scale: boxed.width / self.size_mm()[0].max(1.0),
                    centre: boxed.center(),
                    size_mm: self.size_mm(),
                });

                match found {
                    // Taking the box by an edge is not choosing a piece, and it
                    // must not put down the one already in hand: the size of
                    // the box is usually the thing you change *while* looking
                    // at a piece in it.
                    Some(Hold::Box { .. }) => Some(canvas::Action::capture()),
                    // A press on nothing clears the selection, which is how you
                    // put the properties away without deleting anything.
                    other => Some(canvas::Action::publish(Message::PieceSelected(
                        other.and_then(|hold| match hold {
                            Hold::Piece(index, _) => Some(index),
                            Hold::Box { .. } => None,
                        }),
                    ))),
                }
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let hold = state.hold?;

                // The box first, and it works from the pointer alone: an edge
                // is the far side of a rectangle that is always drawn around
                // the middle of the canvas, so how big the box is is twice how
                // far the hand is from that middle. See [`Stretch`].
                if let Hold::Box { wide, tall } = hold {
                    let stretch = state.stretch?;
                    let p = local(cursor.position()?);
                    state.last = Some(p);

                    let size = stretch.pulled_to(p, (wide, tall));
                    return Some(canvas::Action::publish(Message::BoxStretched(size)));
                }

                let Hold::Piece(_, grab) = hold else {
                    return None;
                };
                let index = self.selected?;
                let rect = self.pieces.get(index)?.placed.rect;

                let p = local(cursor.position()?);
                let previous = state.last.replace(p)?;

                let anchors = self.anchors(Some(index));
                // Alt inverts the setting rather than only switching latching
                // off, so there is a momentary gesture for both directions.
                let latching = self.snapping != state.alt;
                let reach = if latching { self.reach(boxed) } else { (0.0, 0.0) };

                let moved = match grab {
                    // `move_stamp` goes in as the thing that keeps a piece
                    // inside the box, so the clamping has the last word over
                    // the latching either way.
                    Grab::Body => state.travel.as_mut()?.slide(
                        (
                            (p.x - previous.x) / boxed.width,
                            (p.y - previous.y) / boxed.height,
                        ),
                        &anchors,
                        reach,
                        move_stamp,
                    ),
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

                // Only while something is latching. A piece put down by hand
                // lands on a line often enough — the box's own edges are lines,
                // and everything is clamped to them — and a guide drawn then
                // would be the interface claiming to have done something it was
                // told not to do.
                state.guides = if latching {
                    snap::touching(moved, &anchors)
                } else {
                    (Vec::new(), Vec::new())
                };
                Some(canvas::Action::publish(Message::PieceMoved(moved)))
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let dragged = state.hold.take().is_some();
                state.let_go();
                // Everything the drag did was done a frame at a time and never
                // written down; this is where it is kept.
                dragged.then(|| canvas::Action::publish(Message::DragEnded))
            }
            // The canvas is handed every event, modifiers included, so Alt can
            // be watched without the window having to route it here.
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.alt = modifiers.alt();
                None
            }
            // Alt-Tab is Alt held down, and the release of it belongs to
            // whatever the window manager gave the keyboard to next: it never
            // arrives here. Without this the setting stays inverted for good —
            // the magnet in the toolbar switches off and pieces go on latching
            // on — and there is nothing on screen to say why. The drag goes
            // with it: a button released over another window never comes back
            // up here either.
            iced::Event::Window(iced::window::Event::Unfocused) => {
                state.alt = false;
                state.hold = None;
                state.let_go();
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
        let boxed = state
            .stretching(self.size_mm())
            .unwrap_or_else(|| self.box_rect(bounds));
        let accent = theme.extended_palette().primary.base.color;
        let mut frame = Frame::new(renderer, bounds.size());

        paper(&mut frame, boxed, self.paper);

        // The border the template asks for is drawn by `paper`, the same one
        // the document will get. A template that asks for none still has to
        // show where its box is: you cannot compose inside edges you are
        // guessing at, and you certainly cannot take hold of one. Dashed says
        // both things at once — here is the box, and this line is not going
        // into the document.
        let edge = theme.extended_palette().background.base.text;
        if !self.paper.border {
            frame.stroke(
                &Path::rectangle(Point::new(boxed.x, boxed.y), boxed.size()),
                Stroke {
                    line_dash: LineDash {
                        segments: &[5.0, 4.0],
                        offset: 0,
                    },
                    ..Stroke::default()
                        .with_width(1.0)
                        .with_color(Color { a: 0.45, ..edge })
                },
            );
        }

        paint(&mut frame, boxed, &self.pieces, self.picture.as_ref(), self.size_pt.0);

        for (index, drawn) in self.pieces.iter().enumerate() {
            let rect = share(boxed, drawn.placed.rect);
            let outline = Path::rectangle(Point::new(rect.x, rect.y), rect.size());
            if self.selected == Some(index) {
                frame.stroke(&outline, Stroke::default().with_width(1.5).with_color(accent));
                let side = grip(rect);
                for c in corners(rect) {
                    frame.fill(
                        &Path::rectangle(
                            Point::new(c.x - side / 2.0, c.y - side / 2.0),
                            Size::new(side, side),
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

        // The box's own corners last, over everything: they are what says the
        // box can be made bigger, and a piece sitting in the corner must not
        // be able to hide the one grip that changes the box itself. Quiet,
        // though — the piece in hand has grips in the accent colour, and those
        // have to stay the brighter pair.
        let handle = grip(boxed).max(GRIP);
        for c in corners(boxed) {
            frame.fill(
                &Path::rectangle(
                    Point::new(c.x - handle / 2.0, c.y - handle / 2.0),
                    Size::new(handle, handle),
                ),
                Color { a: 0.45, ..edge },
            );
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
        let Some(p) = cursor.position_over(bounds) else {
            return mouse::Interaction::default();
        };
        let boxed = self.box_rect(bounds);
        let p = Point::new(p.x - bounds.x, p.y - bounds.y);

        // While the box is being stretched the cursor has to keep saying so,
        // even once the pointer has left the edge it started on.
        match state.hold.or_else(|| self.under(boxed, p)) {
            Some(Hold::Box { wide: true, tall: true }) => {
                // Down means the arrow points into the corner being dragged.
                let centre = boxed.center();
                if (p.x < centre.x) == (p.y < centre.y) {
                    mouse::Interaction::ResizingDiagonallyDown
                } else {
                    mouse::Interaction::ResizingDiagonallyUp
                }
            }
            Some(Hold::Box { wide: true, .. }) => mouse::Interaction::ResizingHorizontally,
            Some(Hold::Box { .. }) => mouse::Interaction::ResizingVertically,
            Some(Hold::Piece(_, Grab::Corner(0 | 3))) => {
                mouse::Interaction::ResizingDiagonallyDown
            }
            Some(Hold::Piece(_, Grab::Corner(_))) => mouse::Interaction::ResizingDiagonallyUp,
            Some(Hold::Piece(_, Grab::Body)) => {
                if state.hold.is_some() {
                    mouse::Interaction::Grabbing
                } else {
                    mouse::Interaction::Grab
                }
            }
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
            // A piece with nothing to say today keeps its rectangle and paints
            // nothing in it. The editor still outlines it, which is the only
            // way to find it and give it something to say.
            Ink::Nothing => {}
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

/// The four corners of a rectangle, in the order [`Grab::Corner`] counts them:
/// top left, top right, bottom left, bottom right.
pub fn corners(r: Rectangle) -> [Point; 4] {
    [
        Point::new(r.x, r.y),
        Point::new(r.x + r.width, r.y),
        Point::new(r.x, r.y + r.height),
        Point::new(r.x + r.width, r.y + r.height),
    ]
}

/// Which of the box's own sides a point has hold of: across, up and down, or
/// both at once at a corner.
///
/// Only from outside, and that is the rule that keeps the two gestures apart.
/// Everything within the box belongs to the pieces in it; a box edge that
/// could be taken hold of from inside would be a piece that could not be put
/// against the edge without becoming impossible to pick up.
pub fn on_box(boxed: Rectangle, p: Point) -> Option<(bool, bool)> {
    let reach = GRIP * SLACK;
    let (x0, y0) = (boxed.x, boxed.y);
    let (x1, y1) = (boxed.x + boxed.width, boxed.y + boxed.height);

    let past = p.x < x0 - reach || p.x > x1 + reach || p.y < y0 - reach || p.y > y1 + reach;
    let within = p.x > x0 + 1.0 && p.x < x1 - 1.0 && p.y > y0 + 1.0 && p.y < y1 - 1.0;
    if past || within {
        return None;
    }

    let wide = (p.x - x0).abs() <= reach || (p.x - x1).abs() <= reach;
    let tall = (p.y - y0).abs() <= reach || (p.y - y1).abs() <= reach;

    (wide || tall).then_some((wide, tall))
}

/// How big the corner grips are drawn on a rectangle this size.
///
/// [`GRIP`] on anything with room for it, and a third of the shorter side on
/// anything without. A piece can be dragged down to a few pixels, and grips
/// that stayed six pixels there would cover it twice over: every drag would
/// resize it and none would move it, which is a piece that can only be
/// deleted. A third each way always leaves a body in the middle to take hold
/// of, however small the rectangle has been made.
pub fn grip(r: Rectangle) -> f32 {
    GRIP.min(r.width / 3.0).min(r.height / 3.0)
}

/// Which corner of a rectangle a point has hold of, if any.
///
/// The reach is [`SLACK`] times what is drawn — a grip you cannot hit is not a
/// grip — and still never more than a third of the rectangle each way, for the
/// same reason the grip itself is not.
pub fn on_corner(r: Rectangle, p: Point) -> Option<usize> {
    let side = grip(r) * SLACK;
    let (x, y) = ((r.width / 3.0).min(side), (r.height / 3.0).min(side));

    corners(r)
        .iter()
        .position(|c| (c.x - p.x).abs() <= x && (c.y - p.y).abs() <= y)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A piece dragged down to a few pixels is still a piece.
    ///
    /// Grips that stayed their full size on one would cover it twice over:
    /// every drag on it would resize it, none would move it, and the only way
    /// out is to delete the piece and put a new one back.
    #[test]
    fn a_small_piece_keeps_a_body_to_take_hold_of() {
        let piece = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 6.0,
        };

        assert_eq!(
            on_corner(piece, Point::new(20.0, 3.0)),
            None,
            "the middle of a piece is not a corner of it"
        );
        // Along the whole of it, in fact: the body is what a drag needs.
        for x in [8.0, 16.0, 24.0, 32.0] {
            assert_eq!(on_corner(piece, Point::new(x, 3.0)), None, "at {x}");
        }

        // And the corners are still there for the hand that wants them.
        assert_eq!(on_corner(piece, Point::new(0.0, 0.0)), Some(0));
        assert_eq!(on_corner(piece, Point::new(40.0, 6.0)), Some(3));
    }

    /// The box is taken by its edges, and only from outside them: everything
    /// within the box belongs to the pieces in it.
    #[test]
    fn the_box_is_taken_by_its_edges_from_outside() {
        let boxed = Rectangle {
            x: 100.0,
            y: 50.0,
            width: 400.0,
            height: 120.0,
        };

        // The middle of a side, just outside it: that side alone.
        assert_eq!(on_box(boxed, Point::new(98.0, 110.0)), Some((true, false)));
        assert_eq!(on_box(boxed, Point::new(300.0, 48.0)), Some((false, true)));
        assert_eq!(on_box(boxed, Point::new(502.0, 110.0)), Some((true, false)));
        assert_eq!(on_box(boxed, Point::new(300.0, 172.0)), Some((false, true)));

        // A corner is both.
        assert_eq!(on_box(boxed, Point::new(98.0, 48.0)), Some((true, true)));

        // Inside is the pieces', and so is a piece put right against the edge.
        assert_eq!(on_box(boxed, Point::new(300.0, 110.0)), None);
        assert_eq!(on_box(boxed, Point::new(110.0, 60.0)), None);

        // And far enough out is nothing at all — that is how a click in the
        // room around the box still clears the selection.
        assert_eq!(on_box(boxed, Point::new(60.0, 110.0)), None);
    }

    /// Held by an edge, the box is drawn at the scale it was taken hold of at,
    /// so pulling outwards makes it bigger on screen instead of only changing
    /// its proportions.
    #[test]
    fn a_box_being_pulled_is_drawn_growing() {
        let held = Dragging {
            hold: Some(Hold::Box { wide: true, tall: false }),
            stretch: Some(Stretch {
                scale: 4.0,
                centre: Point::new(200.0, 100.0),
                size_mm: [80.0, 25.0],
            }),
            ..Dragging::default()
        };

        // Wider by 20 mm is wider by 80 pixels, around the same middle.
        let drawn = held.stretching([100.0, 25.0]).expect("held by an edge");
        assert_eq!(drawn.width, 400.0);
        assert_eq!(drawn.height, 100.0);
        assert_eq!(drawn.center(), Point::new(200.0, 100.0));

        // Let go, and the fit has it again.
        assert!(Dragging::default().stretching([100.0, 25.0]).is_none());
    }

    /// Pulling an edge measures the box against the scale it had when it was
    /// taken hold of, never the one it is being redrawn at.
    ///
    /// The board draws the box as large as it will go, so the drawing follows
    /// the size. Measure against the drawing and the size follows the drawing
    /// too, and the two chase each other out of the window.
    #[test]
    fn an_edge_follows_the_hand_and_not_the_drawing() {
        // 4 pixels to the millimetre, an 80 x 25 box centred at (200, 100).
        let stretch = Stretch {
            scale: 4.0,
            centre: Point::new(200.0, 100.0),
            size_mm: [80.0, 25.0],
        };

        // The right edge starts 160 pixels out. Pulled to 200 out, the box is
        // 100 mm across — and no taller than it was.
        assert_eq!(
            stretch.pulled_to(Point::new(400.0, 100.0), (true, false)),
            [100.0, 25.0]
        );
        // The same distance the other way is the same box: the left edge and
        // the right edge are the same gesture.
        assert_eq!(
            stretch.pulled_to(Point::new(0.0, 100.0), (true, false)),
            [100.0, 25.0]
        );
        // A corner moves both, and each side answers to its own axis.
        assert_eq!(
            stretch.pulled_to(Point::new(400.0, 20.0), (true, true)),
            [100.0, 40.0]
        );
    }

    /// On a piece with room, a grip reaches further than it is drawn — a grip
    /// you cannot hit is not a grip — but no further than that.
    #[test]
    fn a_grip_is_easier_to_hit_than_it_is_to_see() {
        let piece = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };
        assert_eq!(grip(piece), GRIP);

        assert_eq!(on_corner(piece, Point::new(GRIP * SLACK, 0.0)), Some(0));
        assert_eq!(on_corner(piece, Point::new(GRIP * SLACK + 1.0, 0.0)), None);
        assert_eq!(on_corner(piece, Point::new(200.0, 100.0)), Some(3));
    }
}
