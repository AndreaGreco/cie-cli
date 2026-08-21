//! The signature box composed piece by piece.
//!
//! The other way of filling the box is [`crate::stamp`]: a block of lines that
//! stack themselves and centre themselves, which is what the command line has
//! always produced and still does. A template is the opposite arrangement —
//! every piece sits exactly where it was put, in fractions of the box — and
//! the two are kept apart on purpose. Flow and absolute are different layouts,
//! not two spellings of one, and the day a box is dragged twice as tall they
//! have to behave differently or one of them is wrong.
//!
//! What the user places here is what underskrift draws: the renderer at the
//! bottom of this file emits the PDF content stream itself, through
//! `SignatureLayout::Custom`. The preview in the GUI reads the same pieces, so
//! there is nothing between what is on screen and what lands in the document.
//!
//! Coordinates are `[x0, y0, x1, y1]` in fractions of the box with **y
//! downwards**, the way the preview measures a page. Turning them the right way
//! up for the PDF happens in one place, [`to_points`], and nowhere else.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use underskrift::error::VisualError;
use underskrift::visual::{encode_pdf_text, FontMetrics};
use underskrift::{
    AppearanceContext, AppearanceRenderer, CustomAppearanceResult, ImageConfig, ImageScale,
    SignatureLayout, Standard14Font,
};

use cie_lib::titolare::DatiTitolare;

use crate::stamp::StampImage;

/// The ink. One colour for the whole stamp: a signature block is not a place
/// to be expressive, and a colour per piece is a setting nobody would use
/// twice.
const INK: (f32, f32, f32) = (0.0, 0.0, 0.0);

/// Resource names in the content stream. Only one picture can be in a box, so
/// the image name is a constant rather than a counter.
const REGULAR: &str = "F1";
const BOLD: &str = "F2";
const PICTURE: &str = "Im1";

/// One thing that can appear in the box.
///
/// Everything but [`Piece::Text`] is read off the card or the clock at signing
/// time: the template says *that* the tax code appears and where, never what
/// it says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Piece {
    /// "Firmato digitalmente da" — the line that says what this block is.
    Caption,
    /// Who signed, off the certificate.
    Holder,
    /// `C.F.` and the tax code.
    TaxCode,
    /// When it was signed.
    Date,
    /// Why, when a reason was given. Nothing is drawn when there is none.
    Reason,
    /// Whatever the user typed, drawn as it stands.
    Text { content: String },
    /// The scan of a handwritten signature.
    Image,
}

/// Where something sits across the room it was given.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Align {
    #[default]
    Left,
    Centre,
    Right,
}

/// Where something sits up and down the room it was given.
///
/// Only the picture uses this, and the reason is worth stating: a line of text
/// has no slack in its rectangle — it hangs from the top and you move the
/// rectangle to move the line. A picture is fitted inside its rectangle
/// keeping its proportions, so unless the two happen to have the same shape
/// there is always room left over on one axis, and something has to say which
/// end of it the picture goes.
///
/// `Middle` is the default because that is where pictures were before anyone
/// could say otherwise: reading back a template written then must not move
/// anybody's signature.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Vertical {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// A piece, and the room it takes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placed {
    pub piece: Piece,
    /// Fractions of the box, `[x0, y0, x1, y1]`, y downwards.
    pub rect: [f32; 4],
    /// Point size. The picture ignores it — it is sized by its rectangle.
    pub size: f32,
    pub bold: bool,
    pub align: Align,
    /// Only the picture reads this. See [`Vertical`].
    #[serde(default)]
    pub vertical: Vertical,
}

impl Placed {
    /// How wide this piece's text comes out in the PDF, in points.
    ///
    /// Public because the preview needs it. It draws in the interface's
    /// typeface, not Helvetica, so measuring with its own font would put
    /// centred and right-aligned lines somewhere the PDF will not — a small
    /// lie, but exactly the kind this preview exists to not tell.
    pub fn width_of(&self, text: &str) -> f32 {
        FontMetrics::string_width(self.font(), text, self.size)
    }

    /// A line of text, with the sizes the built-in templates use.
    fn line(piece: Piece, rect: [f32; 4], size: f32) -> Self {
        Self {
            piece,
            rect,
            size,
            bold: false,
            align: Align::Left,
            vertical: Vertical::default(),
        }
    }

    /// The picture, which is placed rather than typeset.
    fn picture_at(rect: [f32; 4]) -> Self {
        Self {
            piece: Piece::Image,
            rect,
            size: 0.0,
            bold: false,
            // Middle of its rectangle both ways, which is where a picture in a
            // stamp has always sat.
            align: Align::Centre,
            vertical: Vertical::Middle,
        }
    }

    fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    fn font(&self) -> Standard14Font {
        if self.bold {
            Standard14Font::HelveticaBold
        } else {
            Standard14Font::Helvetica
        }
    }
}

/// A signature box the user composed, under a name they chose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub pieces: Vec<Placed>,
    /// The picture, kept by path so re-scanning over the same file updates
    /// every template that uses it. Read when the template is loaded, not
    /// here: this type stays plain data that survives a settings file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<PathBuf>,

    /// White paper behind the stamp.
    ///
    /// Off, the document shows through, which is what a scan of a handwritten
    /// signature usually wants: ink on the page rather than a label stuck over
    /// it. It also means whatever the box covers stays readable — worth
    /// knowing before putting one over a paragraph.
    #[serde(default = "yes")]
    pub background: bool,
    /// A hairline around the box. Separate from the paper: a stamp can want
    /// the document showing through and still want an edge, or neither.
    #[serde(default = "yes")]
    pub border: bool,
}

/// Templates written before there was anything to say about paper had both.
/// Defaulting these to `false` would quietly strip the box off every stamp
/// anybody had already composed.
fn yes() -> bool {
    true
}

impl Template {
    /// The lines alone, laid out the way the stamp has always looked.
    ///
    /// Not a reproduction of [`crate::stamp`] to the point — that one centres
    /// a block of stacked lines and this one cannot, being absolute — but the
    /// same lines in the same order at the same sizes, which is what somebody
    /// opening the editor for the first time expects to find in it.
    pub fn classic(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            pieces: vec![
                Placed::line(Piece::Caption, [0.04, 0.06, 0.96, 0.22], 7.0),
                Placed::line(Piece::Holder, [0.04, 0.22, 0.96, 0.44], 9.0).bold(),
                Placed::line(Piece::TaxCode, [0.04, 0.44, 0.96, 0.60], 7.0),
                Placed::line(Piece::Date, [0.04, 0.60, 0.96, 0.76], 7.0),
                Placed::line(Piece::Reason, [0.04, 0.76, 0.96, 0.92], 7.0),
            ],
            image: None,
            background: true,
            border: true,
        }
    }

    /// The handwritten signature on the left, the lines beside it.
    ///
    /// The shape most people picture when they say they want their signature
    /// on the document, and the reason the picture support exists at all.
    pub fn handwritten(name: impl Into<String>, image: PathBuf) -> Self {
        Self {
            name: name.into(),
            pieces: vec![
                Placed::picture_at([0.02, 0.08, 0.40, 0.92]),
                Placed::line(Piece::Caption, [0.44, 0.10, 0.98, 0.28], 6.5),
                Placed::line(Piece::Holder, [0.44, 0.28, 0.98, 0.52], 9.0).bold(),
                Placed::line(Piece::TaxCode, [0.44, 0.52, 0.98, 0.70], 6.5),
                Placed::line(Piece::Date, [0.44, 0.70, 0.98, 0.88], 6.5),
            ],
            image: Some(image),
            background: true,
            border: true,
        }
    }

    /// True when a piece of this kind is somewhere in the box.
    pub fn holds(&self, piece: &Piece) -> bool {
        self.pieces.iter().any(|p| &p.piece == piece)
    }

    /// What each piece says, once the card and the clock have been consulted.
    ///
    /// This is the whole of the resolution: the renderer runs inside
    /// underskrift, long after the certificate has been read and dropped, so
    /// it can carry text but never go and look for it.
    pub fn resolve(&self, facts: &Facts) -> Vec<Drawn> {
        self.pieces
            .iter()
            .filter_map(|placed| {
                let ink = match &placed.piece {
                    Piece::Image => Ink::Picture,
                    Piece::Caption => Ink::Line("Firmato digitalmente da".to_string()),
                    Piece::Holder => Ink::Line(facts.holder.clone()),
                    Piece::TaxCode => Ink::Line(format!("C.F. {}", facts.tax_code.as_ref()?)),
                    Piece::Date => Ink::Line(format!("Data: {}", facts.when)),
                    Piece::Reason => Ink::Line(format!("Motivo: {}", facts.reason.as_ref()?)),
                    Piece::Text { content } => Ink::Line(content.clone()),
                };
                Some(Drawn {
                    placed: placed.clone(),
                    ink,
                })
            })
            .collect()
    }
}

/// What the card and the clock had to say.
///
/// Built once, by whoever knows: [`Facts::preview`] before the PIN, from
/// the certificate at signing time.
#[derive(Debug, Clone)]
pub struct Facts {
    pub holder: String,
    pub tax_code: Option<String>,
    pub reason: Option<String>,
    pub when: String,
}

impl Facts {
    /// What is known at signing time, when the certificate has been read.
    pub fn signing(holder: &DatiTitolare, reason: Option<&str>, when: &str) -> Self {
        Self {
            holder: crate::stamp::holder_name(holder)
                .unwrap_or_else(|| "titolare non identificato".to_string()),
            tax_code: holder.codice_fiscale.clone(),
            reason: reason.map(str::to_string),
            when: when.to_string(),
        }
    }

    /// What the preview has to work with before the PIN.
    ///
    /// A card the keyring has already seen gives up its holder; otherwise the
    /// placeholders stand in, at a realistic width so the box on screen is the
    /// size the box in the document will be.
    pub fn preview(holder: Option<&DatiTitolare>, reason: Option<&str>) -> Self {
        let when = chrono::Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
        match holder {
            Some(holder) => Self::signing(holder, reason, &when),
            None => Self {
                holder: "NOME DEL TITOLARE".to_string(),
                tax_code: Some("XXXXXXXXXXXXXXXX".to_string()),
                reason: reason.map(str::to_string),
                when,
            },
        }
    }
}

/// A piece with the words it turned out to carry.
#[derive(Debug, Clone)]
pub struct Drawn {
    pub placed: Placed,
    pub ink: Ink,
}

/// What actually gets painted.
#[derive(Debug, Clone)]
pub enum Ink {
    Line(String),
    Picture,
}

/// A rectangle of the box, in PDF points with the origin at the bottom left.
///
/// The one place the preview's y-downwards fractions become a PDF's
/// y-upwards points. Everything else on both sides stays in its own system.
fn to_points([x0, y0, x1, y1]: [f32; 4], width: f32, height: f32) -> (f32, f32, f32, f32) {
    let llx = x0 * width;
    let urx = x1 * width;
    // y0 is the *top* of the piece, so it becomes the *upper* edge.
    let ury = (1.0 - y0) * height;
    let lly = (1.0 - y1) * height;
    (llx, lly, urx, ury)
}

/// Draws a template into a PDF content stream.
///
/// Handed to underskrift as `SignatureLayout::Custom`, which is the only
/// layout that lets a caller place things itself. It can carry a picture
/// because of the patch in `patches/BRIEF-underskrift-custom-images.md`;
/// without it `CustomAppearanceResult` has nowhere to put one and a template
/// with a handwritten signature in it would come out as text alone.
pub struct Renderer {
    pieces: Vec<Drawn>,
    image: Option<StampImage>,
}

impl Renderer {
    pub fn new(pieces: Vec<Drawn>, image: Option<StampImage>) -> Self {
        Self { pieces, image }
    }
}

impl AppearanceRenderer for Renderer {
    fn render(&self, ctx: &AppearanceContext) -> Result<CustomAppearanceResult, VisualError> {
        let mut content = Vec::new();
        let mut fonts: Vec<(String, String)> = Vec::new();
        let mut images = Vec::new();

        for drawn in &self.pieces {
            let rect = to_points(drawn.placed.rect, ctx.width, ctx.height);

            match &drawn.ink {
                Ink::Line(text) if !text.is_empty() => {
                    line(&mut content, &mut fonts, &drawn.placed, text, rect)
                }
                Ink::Line(_) => {}
                Ink::Picture => {
                    if let Some(image) = &self.image {
                        picture(&mut content, &mut images, image, &drawn.placed, rect);
                    }
                }
            }
        }

        Ok(CustomAppearanceResult {
            content,
            font_resources: fonts,
            image_resources: images,
        })
    }
}

/// One line of text, clipped to the room it was given.
///
/// Clipped rather than left to run: a piece that overflows its rectangle on
/// screen has to overflow it the same way in the document, and a preview that
/// shows a tidy box over a PDF that spills is worse than one that shows the
/// spill.
fn line(
    content: &mut Vec<u8>,
    fonts: &mut Vec<(String, String)>,
    placed: &Placed,
    text: &str,
    (llx, lly, urx, ury): (f32, f32, f32, f32),
) {
    let font = placed.font();
    let name = if placed.bold { BOLD } else { REGULAR };
    let pdf_name = if placed.bold {
        "Helvetica-Bold"
    } else {
        "Helvetica"
    };
    if !fonts.iter().any(|(n, _)| n == name) {
        fonts.push((name.to_string(), pdf_name.to_string()));
    }

    // The line hangs from the top of its rectangle: that is where the editor
    // draws it, and where a reader expects the first line of a block to start.
    let baseline = ury - placed.size * FontMetrics::ascent(font) as f32 / 1000.0;

    let room = urx - llx;
    let drawn = FontMetrics::string_width(font, text, placed.size);
    let x = llx
        + match placed.align {
            Align::Left => 0.0,
            Align::Centre => (room - drawn) / 2.0,
            Align::Right => room - drawn,
        };

    let (r, g, b) = INK;
    write(content, "q");
    write(
        content,
        &format!("{llx:.2} {lly:.2} {:.2} {:.2} re W n", room, ury - lly),
    );
    write(content, "BT");
    write(content, &format!("/{name} {:.2} Tf", placed.size));
    write(content, &format!("{r:.3} {g:.3} {b:.3} rg"));
    write(content, &format!("1 0 0 1 {x:.2} {baseline:.2} Tm"));
    write(content, &format!("{} Tj", encode_pdf_text(text)));
    write(content, "ET");
    write(content, "Q");
}

/// The picture, fitted inside its rectangle and centred there.
///
/// Fitted rather than stretched: a handwritten signature squeezed to fill a
/// rectangle is not the signature anybody gave.
fn picture(
    content: &mut Vec<u8>,
    images: &mut Vec<(String, ImageConfig)>,
    image: &StampImage,
    placed: &Placed,
    (llx, lly, urx, ury): (f32, f32, f32, f32),
) {
    if images.is_empty() {
        images.push((PICTURE.to_string(), image.config(ImageScale::FitPreserveAspect)));
    }

    let (width, height) = fit(image.aspect_ratio(), (urx - llx, ury - lly));
    let (slack_x, slack_y) = (urx - llx - width, ury - lly - height);

    let x = llx + across(placed.align, slack_x);
    // Up here the y axis is the PDF's, running upwards, so `Top` is the far
    // end of the slack rather than the near one.
    let y = lly
        + match placed.vertical {
            Vertical::Top => slack_y,
            Vertical::Middle => slack_y / 2.0,
            Vertical::Bottom => 0.0,
        };

    write(content, "q");
    write(content, &format!("{width:.4} 0 0 {height:.4} {x:.4} {y:.4} cm"));
    write(content, &format!("/{PICTURE} Do"));
    write(content, "Q");
}

/// The size a picture comes out at inside a rectangle, keeping its
/// proportions.
///
/// Public because the preview has to fit it the same way. Stretching a
/// handwritten signature to fill a rectangle is how you get a signature that
/// is not yours, and doing it differently in two places is how you get a
/// preview that lies about which one you will get.
pub fn fit(ratio: f32, room: (f32, f32)) -> (f32, f32) {
    let ratio = if ratio > 0.0 { ratio } else { 1.0 };

    let (mut width, mut height) = (room.0, room.0 / ratio);
    if height > room.1 {
        height = room.1;
        width = height * ratio;
    }
    (width, height)
}

/// How far in from the near edge something sits, given the room left over.
///
/// The x axis runs the same way in both worlds, so this one number serves the
/// PDF and the screen alike.
pub fn across(align: Align, slack: f32) -> f32 {
    match align {
        Align::Left => 0.0,
        Align::Centre => slack / 2.0,
        Align::Right => slack,
    }
}

fn write(stream: &mut Vec<u8>, op: &str) {
    stream.extend_from_slice(op.as_bytes());
    stream.push(b'\n');
}

/// The layout underskrift is asked for.
pub fn layout(template: &Template, facts: &Facts, image: Option<StampImage>) -> SignatureLayout {
    SignatureLayout::Custom(std::sync::Arc::new(Renderer::new(
        template.resolve(facts),
        image,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = include_bytes!("../tests/fixtures/firma.png");

    fn facts() -> Facts {
        Facts {
            holder: "MARIO ROSSI".to_string(),
            tax_code: Some("RSSMRA80A01H501U".to_string()),
            reason: Some("approvazione".to_string()),
            when: "01/01/2026 10:00:00".to_string(),
        }
    }

    fn render(template: &Template, image: Option<StampImage>) -> CustomAppearanceResult {
        let renderer = Renderer::new(template.resolve(&facts()), image);
        renderer
            .render(&AppearanceContext {
                width: 230.0,
                height: 70.0,
                signer_name: None,
                reason: None,
                location: None,
                date: None,
                contact_info: None,
            })
            .unwrap()
    }

    /// The one conversion between the two coordinate systems. Getting it
    /// upside down would put every stamp on its head, and only in the PDF —
    /// the preview would go on looking right.
    #[test]
    fn the_top_of_a_piece_becomes_its_upper_edge() {
        // The top quarter of a 100pt-tall box.
        let (llx, lly, urx, ury) = to_points([0.0, 0.0, 1.0, 0.25], 200.0, 100.0);

        assert_eq!((llx, urx), (0.0, 200.0));
        assert_eq!(ury, 100.0, "the top of the box is the top of the page space");
        assert_eq!(lly, 75.0, "a quarter down from the top is 75 up from the bottom");
    }

    /// A piece with nothing to say draws nothing — and leaves its room empty
    /// rather than closing the gap. Absolute means absolute: the pieces below
    /// it must not walk up the box because the reason was left blank.
    #[test]
    fn a_piece_with_nothing_to_say_is_left_out() {
        let template = Template::classic("x");
        let bare = Facts {
            tax_code: None,
            reason: None,
            ..facts()
        };

        let drawn = template.resolve(&bare);
        let lines: Vec<&str> = drawn
            .iter()
            .filter_map(|d| match &d.ink {
                Ink::Line(text) => Some(text.as_str()),
                Ink::Picture => None,
            })
            .collect();

        assert!(!lines.iter().any(|l| l.starts_with("C.F.")), "{lines:?}");
        assert!(!lines.iter().any(|l| l.starts_with("Motivo")), "{lines:?}");
        assert!(lines.contains(&"MARIO ROSSI"), "{lines:?}");

        // The pieces that stayed kept the rectangles they were given.
        let holder = drawn.iter().find(|d| d.placed.piece == Piece::Holder).unwrap();
        assert_eq!(holder.placed.rect, [0.04, 0.22, 0.96, 0.44]);
    }

    /// What the card says goes in, not what the template says. A template that
    /// could carry a name would be a template that could carry the wrong one.
    #[test]
    fn the_card_fills_in_the_words() {
        let drawn = Template::classic("x").resolve(&facts());
        let lines: Vec<String> = drawn
            .iter()
            .filter_map(|d| match &d.ink {
                Ink::Line(text) => Some(text.clone()),
                Ink::Picture => None,
            })
            .collect();

        assert_eq!(lines[0], "Firmato digitalmente da");
        assert_eq!(lines[1], "MARIO ROSSI");
        assert_eq!(lines[2], "C.F. RSSMRA80A01H501U");
        assert_eq!(lines[3], "Data: 01/01/2026 10:00:00");
        assert_eq!(lines[4], "Motivo: approvazione");
    }

    /// Free text goes in as it stands: it is the one piece the card has no
    /// opinion about.
    #[test]
    fn free_text_goes_in_untouched() {
        let mut template = Template::classic("x");
        template.pieces.push(Placed::line(
            Piece::Text {
                content: "Visto, si approva".to_string(),
            },
            [0.0, 0.9, 1.0, 1.0],
            6.0,
        ));

        let content = String::from_utf8(render(&template, None).content).unwrap();
        assert!(content.contains("(Visto, si approva) Tj"), "{content}");
    }

    /// Each weight is declared once however many lines wear it, or the PDF
    /// carries the same font resource several times over.
    #[test]
    fn every_weight_is_declared_exactly_once() {
        let result = render(&Template::classic("x"), None);

        let names: Vec<&str> = result.font_resources.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names.len(), 2, "{:?}", result.font_resources);
        assert!(names.contains(&"F1") && names.contains(&"F2"));

        let content = String::from_utf8(result.content).unwrap();
        assert!(content.contains("/F2 9.00 Tf"), "the bold line lost its weight");
    }

    /// A template with no bold line must not declare a bold font it never
    /// uses: an unused resource is a resource a reader still has to load.
    #[test]
    fn an_unused_weight_is_not_declared() {
        let mut template = Template::classic("x");
        for piece in &mut template.pieces {
            piece.bold = false;
        }

        let result = render(&template, None);
        assert_eq!(result.font_resources.len(), 1);
        assert_eq!(result.font_resources[0].0, "F1");
    }

    /// The picture reaches the appearance, and the stream paints it. Both
    /// halves matter: the resource without the operator draws nothing, and the
    /// operator without the resource draws nothing either.
    #[test]
    fn a_picture_is_declared_and_painted() {
        let template = Template::handwritten("x", "firma.png".into());
        let result = render(&template, Some(StampImage::decode(PNG.to_vec()).unwrap()));

        assert_eq!(result.image_resources.len(), 1);
        assert_eq!(result.image_resources[0].0, "Im1");

        let content = String::from_utf8(result.content).unwrap();
        assert!(content.contains("/Im1 Do"), "{content}");
    }

    /// A template that wants a picture but was given none draws the rest and
    /// says nothing. A missing scan is not worth losing a signature over.
    #[test]
    fn a_missing_picture_costs_only_the_picture() {
        let template = Template::handwritten("x", "sparita.png".into());
        let result = render(&template, None);

        assert!(result.image_resources.is_empty());
        assert!(!String::from_utf8(result.content).unwrap().contains("Do"));
        assert!(!result.font_resources.is_empty(), "the lines went too");
    }

    /// The picture keeps its proportions. Squeezed to fill the rectangle it
    /// would be a signature nobody wrote.
    #[test]
    fn the_picture_keeps_its_shape() {
        // The fixture is 2:1 in a rectangle that is far taller than that.
        let mut template = Template::handwritten("x", "firma.png".into());
        template.pieces[0].rect = [0.0, 0.0, 1.0, 1.0];

        let result = render(&template, Some(StampImage::decode(PNG.to_vec()).unwrap()));
        let content = String::from_utf8(result.content).unwrap();

        // `w 0 0 h x y cm` — the first two numbers are the drawn size.
        let cm = content
            .lines()
            .find(|l| l.ends_with(" cm"))
            .expect("no placement in the stream");
        let numbers: Vec<f32> = cm
            .trim_end_matches(" cm")
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();

        let (width, height) = (numbers[0], numbers[3]);
        assert!(
            (width / height - 2.0).abs() < 0.01,
            "drawn {width}x{height}, which is not the picture's 2:1"
        );
        assert!(height <= 70.0 && width <= 230.0, "drawn outside the box");
    }

    /// Alignment moves the line inside its own rectangle and nowhere else.
    #[test]
    fn alignment_moves_the_line_within_its_room() {
        let x_of = |align: Align| {
            let template = Template {
                name: "x".to_string(),
                pieces: vec![Placed {
                    piece: Piece::Text {
                        content: "breve".to_string(),
                    },
                    rect: [0.0, 0.0, 1.0, 1.0],
                    size: 8.0,
                    bold: false,
                    align,
                    vertical: Vertical::default(),
                }],
                image: None,
                background: true,
                border: true,
            };
            let content = String::from_utf8(render(&template, None).content).unwrap();
            let tm = content.lines().find(|l| l.ends_with(" Tm")).unwrap().to_string();
            tm.split_whitespace().nth(4).unwrap().parse::<f32>().unwrap()
        };

        let (left, centre, right) = (x_of(Align::Left), x_of(Align::Centre), x_of(Align::Right));
        assert_eq!(left, 0.0);
        assert!(left < centre && centre < right, "{left} {centre} {right}");
        assert!(right < 230.0, "the line was pushed out of the box");
    }

    /// The two starting points have to be what they claim, or the editor opens
    /// on something nobody would have chosen.
    #[test]
    fn the_built_in_templates_hold_what_they_promise() {
        let classic = Template::classic("classico");
        assert!(!classic.holds(&Piece::Image), "the plain one has no picture");
        assert!(classic.holds(&Piece::Holder) && classic.holds(&Piece::Date));
        assert_eq!(classic.image, None);

        let handwritten = Template::handwritten("autografo", "firma.png".into());
        assert!(handwritten.holds(&Piece::Image));
        assert_eq!(handwritten.image.as_deref(), Some(std::path::Path::new("firma.png")));
    }

    /// A template is a user setting, so it has to survive the settings file.
    #[test]
    fn a_template_survives_the_round_trip() {
        let template = Template::handwritten("il mio", "firma.png".into());
        let raw = serde_json::to_string(&template).unwrap();

        assert_eq!(serde_json::from_str::<Template>(&raw).unwrap(), template);
    }

    /// The preview positions its lines with this, so it has to be the same
    /// number the renderer places them by. A width that disagreed would put
    /// every centred and right-aligned line somewhere the PDF does not.
    #[test]
    fn a_line_measures_the_same_as_it_is_drawn() {
        let placed = Placed::line(Piece::Caption, [0.0, 0.0, 1.0, 1.0], 10.0);
        let bold = placed.clone().bold();

        // Helvetica at 10pt: not zero, and bold is wider than regular.
        assert!(placed.width_of("Firmato digitalmente da") > 0.0);
        assert!(bold.width_of("MARIO ROSSI") > placed.width_of("MARIO ROSSI"));

        // And it scales with the type size, which is what makes it a measure
        // rather than a guess.
        let bigger = Placed::line(Piece::Caption, [0.0, 0.0, 1.0, 1.0], 20.0);
        let (small, large) = (placed.width_of("test"), bigger.width_of("test"));
        assert!((large / small - 2.0).abs() < 0.01, "{small} {large}");
    }

    /// Empty text has no width, and the renderer skips it: a piece that
    /// resolved to nothing must not push an alignment calculation around.
    #[test]
    fn nothing_is_no_wide() {
        let placed = Placed::line(Piece::Reason, [0.0, 0.0, 1.0, 1.0], 8.0);
        assert_eq!(placed.width_of(""), 0.0);
    }

    /// A template written before there was anything to say about paper had
    /// white paper and an edge. Reading one back must not quietly strip the
    /// box off a stamp somebody had already composed.
    #[test]
    fn a_template_from_before_the_paper_setting_keeps_its_box() {
        let old = r#"{
            "name": "vecchio",
            "pieces": [
                {"piece": {"kind": "holder"}, "rect": [0.0, 0.0, 1.0, 0.5],
                 "size": 9.0, "bold": true, "align": "left"}
            ]
        }"#;

        let template: Template = serde_json::from_str(old).unwrap();
        assert!(template.background, "the paper went missing");
        assert!(template.border, "the edge went missing");
    }

    /// Turning the paper off has to reach the PDF, not just the preview.
    #[test]
    fn a_transparent_template_says_so() {
        let mut template = Template::classic("x");
        assert!(template.background && template.border);

        template.background = false;
        template.border = false;

        let raw = serde_json::to_string(&template).unwrap();
        let back: Template = serde_json::from_str(&raw).unwrap();
        assert!(!back.background && !back.border);
    }

    /// The picture goes where it is told, on both axes. Before, it was pinned
    /// to the middle of its rectangle with no way to say otherwise — which on
    /// a rectangle that is not the picture's own shape means a signature
    /// floating somewhere nobody put it.
    #[test]
    fn the_picture_goes_where_it_is_told() {
        // A rectangle twice as tall as the 2:1 fixture needs, so the picture
        // is fitted by width and all the slack is vertical.
        let place = |align, vertical| {
            let template = Template {
                name: "x".to_string(),
                pieces: vec![Placed {
                    piece: Piece::Image,
                    rect: [0.0, 0.0, 1.0, 1.0],
                    size: 0.0,
                    bold: false,
                    align,
                    vertical,
                }],
                image: None,
                background: true,
                border: true,
            };
            let content =
                String::from_utf8(render(&template, Some(picture())).content).unwrap();
            let cm = content.lines().find(|l| l.ends_with(" cm")).unwrap();
            let n: Vec<f32> = cm
                .trim_end_matches(" cm")
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            (n[4], n[5])
        };

        // The box is 230 x 70 and the picture 2:1, so it comes out 230 x 115 —
        // taller than the box, so it is fitted by height instead: 140 x 70,
        // leaving 90 points of slack across and none up and down.
        let (left, _) = place(Align::Left, Vertical::Middle);
        let (centre, _) = place(Align::Centre, Vertical::Middle);
        let (right, _) = place(Align::Right, Vertical::Middle);

        assert_eq!(left, 0.0);
        assert!(left < centre && centre < right, "{left} {centre} {right}");
        assert!((right - 90.0).abs() < 0.5, "{right}");
        assert!((centre - 45.0).abs() < 0.5, "{centre}");
    }

    /// The two axes are told apart, and the vertical one runs the PDF's way:
    /// `Top` is the high end of the sheet, not the low one.
    #[test]
    fn the_picture_knows_which_way_is_up() {
        let place = |vertical| {
            let mut template = Template::handwritten("x", "firma.png".into());
            template.pieces = vec![Placed {
                piece: Piece::Image,
                // Tall and narrow, so the slack is vertical this time.
                rect: [0.0, 0.0, 0.2, 1.0],
                size: 0.0,
                bold: false,
                align: Align::Centre,
                vertical,
            }];
            let content =
                String::from_utf8(render(&template, Some(picture())).content).unwrap();
            let cm = content.lines().find(|l| l.ends_with(" cm")).unwrap();
            cm.trim_end_matches(" cm")
                .split_whitespace()
                .nth(5)
                .unwrap()
                .parse::<f32>()
                .unwrap()
        };

        let (top, middle, bottom) = (
            place(Vertical::Top),
            place(Vertical::Middle),
            place(Vertical::Bottom),
        );
        assert_eq!(bottom, 0.0, "the bottom of the box is y = 0 in PDF space");
        assert!(bottom < middle && middle < top, "{bottom} {middle} {top}");
    }

    /// A picture the same shape as its rectangle has nowhere to go, and every
    /// alignment has to put it in the same place rather than off an edge.
    #[test]
    fn a_picture_that_fills_its_rectangle_does_not_move() {
        // 230 x 70 box; a rectangle 2:1 like the fixture leaves no slack.
        let rect = [0.0, 0.0, 0.6087, 1.0];
        let place = |align| {
            let template = Template {
                name: "x".to_string(),
                pieces: vec![Placed {
                    piece: Piece::Image,
                    rect,
                    size: 0.0,
                    bold: false,
                    align,
                    vertical: Vertical::Middle,
                }],
                image: None,
                background: true,
                border: true,
            };
            let content =
                String::from_utf8(render(&template, Some(picture())).content).unwrap();
            let cm = content.lines().find(|l| l.ends_with(" cm")).unwrap();
            cm.trim_end_matches(" cm")
                .split_whitespace()
                .nth(4)
                .unwrap()
                .parse::<f32>()
                .unwrap()
        };

        assert!((place(Align::Left) - place(Align::Right)).abs() < 0.5);
    }

    /// A template written before the picture could be placed had it in the
    /// middle. Reading one back must leave it there.
    #[test]
    fn a_piece_from_before_the_vertical_setting_stays_in_the_middle() {
        let old = r#"{"piece": {"kind": "image"}, "rect": [0.0, 0.0, 1.0, 1.0],
                      "size": 0.0, "bold": false, "align": "centre"}"#;

        let placed: Placed = serde_json::from_str(old).unwrap();
        assert_eq!(placed.vertical, Vertical::Middle);
    }

    fn picture() -> StampImage {
        StampImage::decode(PNG.to_vec()).unwrap()
    }
}
