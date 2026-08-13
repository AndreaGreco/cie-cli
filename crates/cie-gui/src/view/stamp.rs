//! The signature box drawn over the page, and the dragging of it.
//!
//! Everything here is in fractions of the page (`0..1`, y downwards). Turning
//! those into a `/Rect` in PDF points is `preview::to_pdf_rect`'s job, and it
//! is the one place that knows about `/Rotate` and the crop box.

use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Element, Fill, Pixels, Point, Rectangle, Renderer, Size, Theme};

use cie_lib::DatiTitolare;

use crate::preview::{Document, Grab, move_stamp, resize_stamp};
use crate::state::Message;
use crate::theme;

/// Side of the corner grips, in pixels. Also the hit radius, times the slack
/// below: a grip you cannot hit is not a grip.
const GRIP: f32 = 7.0;
const SLACK: f32 = 1.6;

/// The program owns everything it needs, so the widget borrows nothing from
/// the document - it only had to measure it.
pub fn canvas(
    document: &Document,
    page: usize,
    stamp: [f32; 4],
    reason: &str,
    holder: Option<DatiTitolare>,
) -> Element<'static, Message> {
    canvas::Canvas::new(Stamp {
        ratio: document.aspect_ratio(page),
        page_width_pt: document.display_size(page).0,
        stamp,
        reason: reason.trim().to_string(),
        holder,
    })
    .width(Fill)
    .height(Fill)
    .into()
}

struct Stamp {
    ratio: f32,
    page_width_pt: f32,
    stamp: [f32; 4],
    reason: String,
    /// Who the keyring says this card belongs to. With it the mock-up stops
    /// being a mock-up: what is drawn is what will be stamped.
    holder: Option<DatiTitolare>,
}

/// What the pointer is holding, and where it was last seen.
#[derive(Default)]
struct Dragging {
    grab: Option<Grab>,
    last: Option<Point>,
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

    fn corners(&self, page: Rectangle) -> [Point; 4] {
        let b = self.box_rect(page);
        [
            Point::new(b.x, b.y),
            Point::new(b.x + b.width, b.y),
            Point::new(b.x, b.y + b.height),
            Point::new(b.x + b.width, b.y + b.height),
        ]
    }

    /// What sits under a point. Corners win over the body: they are drawn on
    /// top of it, so they have to be grabbed before it too.
    fn under(&self, page: Rectangle, p: Point) -> Option<Grab> {
        self.corners(page)
            .iter()
            .position(|c| c.distance(p) <= GRIP * SLACK)
            .map(Grab::Corner)
            .or_else(|| self.box_rect(page).contains(p).then_some(Grab::Body))
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
                state.grab.is_some().then(canvas::Action::capture)
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let grab = state.grab?;
                let p = local(cursor.position()?);
                let previous = state.last.replace(p)?;

                let moved = match grab {
                    Grab::Body => move_stamp(
                        self.stamp,
                        (p.x - previous.x) / page.width,
                        (p.y - previous.y) / page.height,
                    ),
                    Grab::Corner(i) => resize_stamp(
                        self.stamp,
                        i,
                        ((p.x - page.x) / page.width, (p.y - page.y) / page.height),
                    ),
                };
                Some(canvas::Action::publish(Message::StampChanged(moved)))
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.grab = None;
                state.last = None;
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
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

        let path = Path::rectangle(Point::new(boxed.x, boxed.y), boxed.size());
        // Nearly opaque rather than opaque: enough to read the mock-up, enough
        // to still see what of the page is about to be covered.
        frame.fill(&path, Color { a: 0.92, ..Color::WHITE });
        frame.stroke(&path, Stroke::default().with_width(1.5).with_color(accent));

        for c in self.corners(page) {
            frame.fill(
                &Path::rectangle(
                    Point::new(c.x - GRIP / 2.0, c.y - GRIP / 2.0),
                    Size::new(GRIP, GRIP),
                ),
                accent,
            );
        }

        self.draw_mockup(&mut frame, boxed, page);
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

impl Stamp {
    /// Draws what the signature will look like, at the size it will have.
    ///
    /// The lines come from `cie_firma::preview_stamp_lines`, the very function
    /// that composes the real stamp: no second copy of the layout to keep in
    /// step. The holder's name and tax code are placeholders - they live in a
    /// certificate that stays behind the PIN until signing time.
    fn draw_mockup(&self, frame: &mut Frame, boxed: Rectangle, page: Rectangle) {
        // One PDF point is this many pixels on screen. Without it the mock-up
        // would stop predicting anything as soon as the window is resized.
        let px_per_pt = page.width / self.page_width_pt.max(1.0);

        let lines = cie_firma::preview_stamp_lines(
            self.holder.as_ref(),
            (!self.reason.is_empty()).then_some(self.reason.as_str()),
        );

        let pad = 3.0 * px_per_pt;
        let mut y = boxed.y + pad;

        for line in &lines {
            let size = line.font_size.unwrap_or(7.0) * px_per_pt;
            // Under about 5 px text is grey mush: better to stop than to fill
            // the box with noise.
            if size < 5.0 {
                break;
            }
            let leading = size * 1.25;
            if y + leading > boxed.y + boxed.height {
                break;
            }

            frame.fill_text(Text {
                content: line.text.clone(),
                position: Point::new(boxed.x + pad, y),
                color: Color::from_rgb(0.09, 0.20, 0.30),
                size: Pixels(size),
                font: if line.bold { theme::UI_STRONG } else { theme::UI },
                ..Text::default()
            });
            y += leading;
        }
    }
}
