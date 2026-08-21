//! The signature box drawn over the page, and the dragging of it.
//!
//! Everything here is in fractions of the page (`0..1`, y downwards). Turning
//! those into a `/Rect` in PDF points is `preview::to_pdf_rect`'s job, and it
//! is the one place that knows about `/Rotate` and the crop box.

use iced::keyboard;
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Element, Fill, Point, Rectangle, Renderer, Size, Theme};

use cie_firma::Drawn;

use crate::preview::{Document, Grab, move_stamp, resize_stamp};
use crate::snap::{self, Anchors, Drag};
use crate::state::Message;
use crate::view::board::{self, Art};

/// The program owns everything it needs, so the widget borrows nothing from
/// the document — it only had to measure it.
pub fn canvas(
    document: &Document,
    page: usize,
    stamp: [f32; 4],
    pieces: Vec<Drawn>,
    art: Option<Art>,
    paper: board::Paper,
    snapping: bool,
) -> Element<'static, Message> {
    canvas::Canvas::new(Stamp {
        ratio: document.aspect_ratio(page),
        page_size_pt: document.display_size(page),
        stamp,
        pieces,
        art,
        paper,
        snapping,
    })
    .width(Fill)
    .height(Fill)
    .into()
}

struct Stamp {
    ratio: f32,
    /// The page as displayed, in PDF points. Both numbers earn their keep: the
    /// width scales the type, and the margins the box latches on to are a real
    /// distance rather than a fraction of whatever sheet this is.
    page_size_pt: (f32, f32),
    stamp: [f32; 4],
    /// The active template, already resolved: the words that will be printed,
    /// not the names of the pieces that produce them.
    pieces: Vec<Drawn>,
    /// The scan of a handwritten signature the template carries, if any.
    art: Option<Art>,
    /// What the template says goes behind the stamp.
    paper: board::Paper,
    /// Whether the box latches on to the page's lines. Alt turns it round.
    snapping: bool,
}

/// What the pointer is holding, and where it was last seen.
#[derive(Default)]
struct Dragging {
    grab: Option<Grab>,
    last: Option<Point>,
    /// Where the box would be with nothing to latch on to. See `snap::Drag`.
    travel: Option<Drag>,
    /// The lines the box is sitting on, to draw while it is held.
    guides: (Vec<f32>, Vec<f32>),
    /// Alt, held. It inverts the setting rather than only switching latching
    /// off. See the same field on the editor's board.
    alt: bool,
}

impl Dragging {
    /// Lets go of whatever was being dragged, guides and all: left on screen
    /// with nothing in hand they are lines pointing at nothing.
    fn let_go(&mut self) {
        self.last = None;
        self.travel = None;
        self.guides = (Vec::new(), Vec::new());
    }
}

impl Stamp {
    /// Where the page actually lands inside the canvas.
    ///
    /// `image` fits the page inside its bounds and centres it; the box has to
    /// land on the same rectangle or it points at the wrong part of the page.
    fn page_rect(&self, bounds: Rectangle) -> Rectangle {
        let mut width = bounds.width;
        let mut height = width / self.ratio;
        if height > bounds.height {
            height = bounds.height;
            width = height * self.ratio;
        }
        Rectangle {
            x: (bounds.width - width) / 2.0,
            y: (bounds.height - height) / 2.0,
            width,
            height,
        }
    }

    fn box_rect(&self, page: Rectangle) -> Rectangle {
        let [x0, y0, x1, y1] = self.stamp;
        Rectangle {
            x: page.x + x0 * page.width,
            y: page.y + y0 * page.height,
            width: (x1 - x0) * page.width,
            height: (y1 - y0) * page.height,
        }
    }

    /// What sits under a point. Corners win over the body: they are drawn on
    /// top of it, so they have to be grabbed before it too — but never so much
    /// of it that there is no body left to drag. See [`board::on_corner`].
    fn under(&self, page: Rectangle, p: Point) -> Option<Grab> {
        let boxed = self.box_rect(page);

        board::on_corner(boxed, p)
            .map(Grab::Corner)
            .or_else(|| boxed.contains(p).then_some(Grab::Body))
    }
}

impl canvas::Program<Message> for Stamp {
    type State = Dragging;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        // Positions arrive in window space; everything here is canvas-relative.
        let local = |p: Point| Point::new(p.x - bounds.x, p.y - bounds.y);
        let page = self.page_rect(bounds);

        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = local(cursor.position_over(bounds)?);
                state.grab = self.under(page, p);
                state.last = Some(p);
                state.travel = (state.grab == Some(Grab::Body)).then(|| Drag::from(self.stamp));
                state.grab.is_some().then(canvas::Action::capture)
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let grab = state.grab?;
                let p = local(cursor.position()?);
                let previous = state.last.replace(p)?;

                let anchors = Anchors::page(self.page_size_pt);
                // Alt inverts the setting rather than only switching latching
                // off, so there is a momentary gesture for both directions.
                let latching = self.snapping != state.alt;
                let reach = if latching {
                    (
                        snap::REACH_PX / page.width.max(1.0),
                        snap::REACH_PX / page.height.max(1.0),
                    )
                } else {
                    (0.0, 0.0)
                };

                let moved = match grab {
                    Grab::Body => state.travel.as_mut()?.slide(
                        (
                            (p.x - previous.x) / page.width,
                            (p.y - previous.y) / page.height,
                        ),
                        &anchors,
                        reach,
                        move_stamp,
                    ),
                    Grab::Corner(i) => {
                        let at = snap::point(
                            ((p.x - page.x) / page.width, (p.y - page.y) / page.height),
                            &anchors,
                            reach,
                        );
                        resize_stamp(self.stamp, i, at)
                    }
                };

                // Only while something is latching: a box put down by hand
                // lands on a line often enough, and a guide drawn then would be
                // the interface claiming to have done what it was told not to.
                state.guides = if latching {
                    snap::touching(moved, &anchors)
                } else {
                    (Vec::new(), Vec::new())
                };
                Some(canvas::Action::publish(Message::StampChanged(moved)))
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let dragged = state.grab.take().is_some();
                state.let_go();
                // The box was moved a frame at a time and never written down.
                // This is where the template keeps the size it ended up.
                dragged.then(|| canvas::Action::publish(Message::DragEnded))
            }
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.alt = modifiers.alt();
                None
            }
            // Alt-Tab is Alt held down and released somewhere else: the release
            // never arrives here, and the setting would stay inverted for good.
            // See the same handler on the editor's board.
            iced::Event::Window(iced::window::Event::Unfocused) => {
                state.alt = false;
                state.grab = None;
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
        let page = self.page_rect(bounds);
        let boxed = self.box_rect(page);
        let accent = theme.extended_palette().primary.base.color;

        // No cache: the box moves with the pointer, and a cache invalidated on
        // every frame costs more than it saves.
        let mut frame = Frame::new(renderer, bounds.size());

        // The paper first, exactly as the template asks for it: a transparent
        // stamp has to look transparent here, or the one thing this preview is
        // for — knowing what the page will look like — is the thing it gets
        // wrong.
        board::paper(&mut frame, boxed, self.paper);

        // Then the handle: the dashed-free outline and the grips are ours, not
        // the document's, and they are drawn whatever the paper is doing.
        let path = Path::rectangle(Point::new(boxed.x, boxed.y), boxed.size());
        frame.stroke(&path, Stroke::default().with_width(1.5).with_color(accent));

        let side = board::grip(boxed);
        for c in board::corners(boxed) {
            frame.fill(
                &Path::rectangle(
                    Point::new(c.x - side / 2.0, c.y - side / 2.0),
                    Size::new(side, side),
                ),
                accent,
            );
        }

        // The same painting the editor does, from the same pieces: what was
        // composed in the editor is what shows up here, and what shows up here
        // is what goes in the document.
        board::paint(
            &mut frame,
            boxed,
            &self.pieces,
            self.art.as_ref(),
            (self.stamp[2] - self.stamp[0]) * self.page_size_pt.0,
        );

        // The guides span the page, not the box: what they are saying is where
        // the box has landed on the sheet.
        board::guides(&mut frame, page, &state.guides, accent);

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
        let page = self.page_rect(bounds);

        match self.under(page, Point::new(p.x - bounds.x, p.y - bounds.y)) {
            Some(Grab::Corner(0)) | Some(Grab::Corner(3)) => mouse::Interaction::ResizingDiagonallyDown,
            Some(Grab::Corner(_)) => mouse::Interaction::ResizingDiagonallyUp,
            Some(Grab::Body) => mouse::Interaction::Grab,
            None => mouse::Interaction::default(),
        }
    }
}
