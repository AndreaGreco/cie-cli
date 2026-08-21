//! The window's state and everything that changes it.
//!
//! No widget is built here. That is deliberate: `update` stays a plain function
//! over plain data, so the behaviour that matters — which signature kind a file
//! lands on, when signing is allowed, what the panel shows after a failure —
//! can be tested without opening a window.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use anyhow::Result;
use cie_firma::keyring::{self, CardSecret};
use cie_firma::{Align, PadesOptions, Piece, Placed, StampImage, Template, Vertical};
use rust_i18n::t;

use crate::job::{self, Kind, Request};
use crate::preview::{Document, to_pdf_rect};
use crate::settings::{self, Settings};
use iced::widget::image;
use crate::theme::Mode;

/// Where the stamp is proposed: bottom right, the same place `cie sign-pdf`
/// uses from the command line.
pub const DEFAULT_STAMP: [f32; 4] = [0.56, 0.87, 0.95, 0.96];

/// Zoom stops. `1.0` means the page fits the window; the rest are steps a
/// button press moves between, so the zoom never lands on an odd number.
const ZOOM_STOPS: [f32; 8] = [1.0, 1.25, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];

/// Width the page is rasterised at when zoom is `1.0`. Comfortably above a
/// typical window so the page stays sharp without a re-render on every resize.
const BASE_RENDER_PX: f32 = 1600.0;

/// Ceiling on the bitmap, at roughly 220 dpi for A4.
///
/// Past this the page is rasterised once and the GPU scales it, so deep zoom
/// goes slightly soft instead of freezing the window: hayro is synchronous and
/// runs in `update`. ponytail: the real fix is rendering only the visible tile,
/// which is worth it the day somebody zooms to 8x to read a signature.
const MAX_RENDER_PX: f32 = 2600.0;

/// A box's size as it goes in its fields: a tenth of a millimetre if there is
/// one, and a whole number when there is not.
///
/// `81` rather than `81.0`, because a field showing a decimal point nobody
/// asked for is a field somebody has to delete a character out of the middle
/// of before they can type their own number in.
fn written(size_mm: [f32; 2]) -> [String; 2] {
    let word = |mm: f32| {
        if (mm - mm.round()).abs() < 0.05 {
            format!("{:.0}", mm.round())
        } else {
            format!("{mm:.1}")
        }
    };
    [word(size_mm[0]), word(size_mm[1])]
}

fn next_zoom(current: f32, direction: i32) -> f32 {
    let here = ZOOM_STOPS
        .iter()
        .position(|z| (z - current).abs() < 0.01)
        .unwrap_or(0) as i32;
    let next = (here + direction).clamp(0, ZOOM_STOPS.len() as i32 - 1);
    ZOOM_STOPS[next as usize]
}

#[derive(Debug, Clone)]
pub enum Message {
    // Document
    ChooseDocument,
    DocumentChosen(Option<PathBuf>),
    FileDropped(PathBuf),

    // Signature options
    KindChanged(Kind),
    PageBack,
    PageForward,
    StampReset,
    /// The box was dragged or resized: fractions of the page, already
    /// normalised by `preview`.
    StampChanged([f32; 4]),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// The wheel turned over something that did not want it.
    Wheel { up: bool },
    ReasonChanged(String),
    CertifyToggled(bool),
    CertifyLevel(u8),

    // Templates. These outlast the document, and the window.
    /// Which template signs. Also which one the editor is showing.
    TemplateChosen(usize),
    TemplateAdded,
    TemplateDuplicated,
    TemplateDeleted,
    TemplateRenamed(String),
    /// One of the box's two size fields was typed into: which side, and the
    /// words now in it.
    BoxSized { side: usize, mm: String },
    /// The box was taken by one of its own edges in the editor and pulled to
    /// this size, `[width, height]` in millimetres.
    BoxStretched([f32; 2]),
    SnappingToggled(bool),
    TemplateBackgroundToggled(bool),
    TemplateBorderToggled(bool),
    /// Pick the picture for the template being edited.
    ChooseSignatureImage,
    SignatureImageChosen(Option<PathBuf>),
    RemoveSignatureImage,

    // The piece under the pointer in the editor
    PieceAdded(Piece),
    PieceSelected(Option<usize>),
    /// Dragged or resized: fractions of the box, already normalised.
    PieceMoved([f32; 4]),
    PieceResized(f32),
    PieceBoldToggled(bool),
    PieceAligned(Align),
    PieceRaised(Vertical),
    PieceTextChanged(String),
    PieceRemoved,
    /// A drag over either canvas has ended.
    ///
    /// Dragging changes the template on every frame, and writing the settings
    /// out on every frame is a file rewritten a hundred times over one
    /// gesture — enough, on a slow disk, for the box to lag behind the hand
    /// moving it. The move happens as it is made and is written down once,
    /// when the button comes up.
    DragEnded,

    // Destination
    ChooseOutput,
    OutputChosen(Option<PathBuf>),

    // Signing
    SignPressed,
    PinChanged(String),
    RememberToggled(bool),
    PinConfirmed,
    SignCancelled,
    OutcomeDismissed,
    OpenResult,
    OpenFolder,

    // Menus and the panels they open
    MenuToggled(Menu),
    MenuClosed,
    ShowAbout,
    ShowTemplates,
    OverlayDismissed,
    Quit,

    // Background
    Tick,
    ForgetCard,
    ThemeChanged(Mode),
}

impl Message {
    /// Moving around the document, as opposed to acting on it. These are the
    /// ones the signing panel swallows.
    fn is_navigation(&self) -> bool {
        matches!(
            self,
            Message::PageBack
                | Message::PageForward
                | Message::Wheel { .. }
                | Message::ZoomIn
                | Message::ZoomOut
                | Message::ZoomReset
                | Message::StampChanged(_)
                | Message::StampReset
        )
    }
}

/// How far the signing panel has got.
#[derive(Debug, Default)]
pub enum Phase {
    /// No panel: the window is being filled in.
    #[default]
    Idle,
    /// Asking for the PIN, because the keyring did not have it.
    Pin { pin: String, remember: bool },
    /// The card is working. `step` counts the steps already ticked off.
    Running { step: usize, label: String },
    Done(PathBuf),
    Failed(String),
}

/// A menu in the application bar. `impl` in `view::menu`, which is the only
/// thing that has an opinion about what they are called and how wide they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    File,
    Settings,
}

/// A panel laid over the window that has nothing to do with the card.
///
/// Kept apart from `Phase`, which is the signing story from the PIN to the
/// outcome. Folding the two together would give the signing state machine an
/// About box as one of its steps, and the two can never be up at once anyway.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    #[default]
    None,
    About,
    /// The template editor. What it edits is the active template in
    /// `settings`, changed in place and saved as it goes — so the only thing
    /// the overlay itself has to remember is which piece is selected.
    Templates {
        selected: Option<usize>,
    },
}

/// The user's handwritten signature, read off disk and ready for both the
/// canvas and the card.
#[derive(Debug, Clone)]
pub struct Signature {
    /// The bytes and their pixel size, in the shape `cie-firma` wants.
    pub image: StampImage,
    /// The same picture, in the shape the canvas wants. Reference counted, so
    /// handing it to a widget on every frame costs nothing.
    pub handle: image::Handle,
}

impl Signature {
    fn load(path: &Path) -> Result<Self> {
        let image = StampImage::read(path)?;
        Ok(Self {
            handle: image::Handle::from_bytes(image.data.clone()),
            image,
        })
    }
}

/// The card on the reader, as far as we know.
#[derive(Debug, Default)]
pub struct Card {
    pub id: Option<String>,
    pub stored: Option<CardSecret>,
}

pub struct State {
    // Document
    pub path: Option<PathBuf>,
    pub bytes: Option<Arc<Vec<u8>>>,
    pub document: Option<Document>,
    pub open_error: Option<String>,

    // Options
    pub kind: Kind,
    pub page: usize,
    /// Fractions of the page image (`0..1`, y downwards). It only becomes a
    /// `/Rect` at signing time, when the page geometry is known.
    pub stamp: [f32; 4],
    pub reason: String,
    pub certify: Option<u8>,
    /// `1.0` fits the page to the window; above that the desk scrolls.
    pub zoom: f32,
    /// The current page, already rasterised.
    ///
    /// Kept here rather than rebuilt in `view`: hayro takes tens of
    /// milliseconds on a real page, and `view` runs on every frame. The handle
    /// itself is cheap to clone — it is reference counted.
    pub page_image: Option<image::Handle>,

    // Destination
    pub output: PathBuf,
    output_chosen: bool,

    // Card and signing
    pub card: Card,
    pub phase: Phase,
    /// The menu whose list is down, if any.
    pub menu: Option<Menu>,
    pub overlay: Overlay,
    pub notice: Option<String>,
    pub mode: Mode,

    // User settings
    pub settings: Settings,
    /// What the box's two size fields are showing.
    ///
    /// The number in the template would do, and does not: a field that
    /// reformatted what it was handed on every keystroke could never be
    /// cleared to type a new number into — take the last digit off `81.0` and
    /// `8.0` comes straight back with the cursor somewhere in the middle of
    /// it. So the words stay as they were typed, and the box follows them
    /// whenever they say a size a box can be. Anything that changes the box
    /// from elsewhere — another template, a corner dragged over the page —
    /// writes them again.
    pub box_size: [String; 2],
    /// The picture the settings name, once it has been read. `None` when there
    /// is none, and also when the one named could not be opened.
    pub signature: Option<Signature>,
    /// Where the settings are written back. `None` in tests and wherever the
    /// platform gave us no configuration directory: then nothing is kept,
    /// rather than something being written to a guess.
    settings_path: Option<PathBuf>,

    identifying: Option<Receiver<Option<String>>>,
    job: Option<Receiver<job::Message>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            path: None,
            bytes: None,
            document: None,
            open_error: None,
            kind: Kind::Pades,
            page: 0,
            stamp: DEFAULT_STAMP,
            reason: String::new(),
            certify: None,
            zoom: 1.0,
            page_image: None,
            output: PathBuf::new(),
            output_chosen: false,
            card: Card::default(),
            phase: Phase::Idle,
            menu: None,
            overlay: Overlay::None,
            notice: None,
            mode: Mode::default(),
            settings: Settings::default(),
            box_size: written(cie_firma::DEFAULT_SIZE_MM),
            signature: None,
            settings_path: None,
            identifying: None,
            job: None,
        }
    }
}

impl State {
    /// Starts asking the reader who is on it, and picks up what the user
    /// settled on last time. No PIN involved in either.
    pub fn new() -> Self {
        let settings_path = settings::path();
        let settings = settings_path
            .as_deref()
            .map(Settings::load)
            .unwrap_or_default();

        let mut state = Self {
            identifying: Some(job::identify()),
            settings,
            settings_path,
            ..Self::default()
        };
        state.show_box_size();
        state.load_signature();
        state
    }

    /// True while something in the background needs draining.
    pub fn busy(&self) -> bool {
        self.identifying.is_some() || self.job.is_some()
    }

    /// A document is enough to press Firma: the PIN is asked afterwards.
    pub fn can_sign(&self) -> bool {
        self.bytes.is_some() && self.job.is_none()
    }

    pub fn shows_preview(&self) -> bool {
        self.kind == Kind::Pades && self.document.is_some()
    }

    pub fn page_count(&self) -> usize {
        self.document.as_ref().map_or(0, Document::page_count)
    }

    pub fn update(&mut self, message: Message) {
        // While the signing panel is up the keyboard belongs to the PIN field
        // and the document underneath is not going anywhere.
        if !matches!(self.phase, Phase::Idle) && message.is_navigation() {
            return;
        }

        // Anything that is not the menu itself closes it. A list left hanging
        // over the window after the thing it asked for has happened is a list
        // the user has to dismiss before they can see what they did.
        if !matches!(message, Message::MenuToggled(_) | Message::Tick) {
            self.menu = None;
        }

        match message {
            Message::MenuToggled(menu) => {
                // The same title again puts the list away, which is what a
                // second click on an open menu means everywhere else.
                self.menu = (self.menu != Some(menu)).then_some(menu);
            }
            Message::MenuClosed => {}

            Message::DocumentChosen(Some(p)) | Message::FileDropped(p) => self.open(p),
            Message::DocumentChosen(None) | Message::ChooseDocument => {}

            Message::KindChanged(kind) => {
                self.kind = kind;
                self.output_chosen = false;
                self.refresh_output();
            }
            Message::PageBack => {
                self.page = self.page.saturating_sub(1);
                // Pages in one document need not be the same size, and the box
                // is a real measurement: the same fraction of a different
                // sheet would be a different stamp.
                self.fit_stamp();
                self.rasterise();
            }
            Message::PageForward => {
                if self.page + 1 < self.page_count() {
                    self.page += 1;
                    self.fit_stamp();
                    self.rasterise();
                }
            }
            // At 1:1 the page fits and nothing scrolls, so the wheel is free
            // to turn pages. Zoomed in it is the scrollbars' to take, and the
            // ones that reach here are the ones they did not want.
            Message::Wheel { up } => {
                if self.zoom <= 1.0 {
                    self.update(if up { Message::PageBack } else { Message::PageForward });
                }
            }
            Message::StampReset => self.propose_stamp(),
            // Dragging the box over the page is the other way of saying how
            // big it is, so what it comes out measuring goes back to the
            // template. Written down when the button comes up, not here.
            Message::StampChanged(stamp) => {
                self.stamp = stamp;
                if let Some(size) = self.stamp_size_pt() {
                    self.settings.active_mut().set_size_pt(size);
                    self.show_box_size();
                }
            }
            Message::ZoomIn | Message::ZoomOut | Message::ZoomReset => {
                self.zoom = match message {
                    Message::ZoomIn => next_zoom(self.zoom, 1),
                    Message::ZoomOut => next_zoom(self.zoom, -1),
                    _ => 1.0,
                };
                self.rasterise();
            }
            Message::ReasonChanged(r) => self.reason = r,
            Message::CertifyToggled(on) => self.certify = on.then_some(2),
            Message::CertifyLevel(l) => self.certify = Some(l),

            Message::TemplateChosen(index) => {
                self.settings.active = index.min(self.settings.templates.len() - 1);
                self.select(None);
                self.load_signature();
                self.fit_stamp();
                self.show_box_size();
                self.save_settings();
            }
            Message::TemplateAdded => {
                self.settings.templates.push(Template::classic(t!("template.new")));
                self.settings.active = self.settings.templates.len() - 1;
                self.select(None);
                self.load_signature();
                self.fit_stamp();
                self.show_box_size();
                self.save_settings();
            }
            Message::TemplateDuplicated => {
                let mut copy = self.settings.active().clone();
                copy.name = t!("template.copy_of", name = copy.name).to_string();
                self.settings.templates.push(copy);
                self.settings.active = self.settings.templates.len() - 1;
                self.select(None);
                self.fit_stamp();
                self.show_box_size();
                self.save_settings();
            }
            // The last one never goes: with no templates there is nothing to
            // sign with, and an editor opening on an empty list is an editor
            // nobody knows how to start.
            Message::TemplateDeleted if self.settings.templates.len() > 1 => {
                let gone = self.settings.active.min(self.settings.templates.len() - 1);
                self.settings.templates.remove(gone);
                self.settings.active = gone.min(self.settings.templates.len() - 1);
                self.select(None);
                self.load_signature();
                self.fit_stamp();
                self.show_box_size();
                self.save_settings();
            }
            Message::TemplateDeleted => {}
            Message::TemplateRenamed(name) => {
                self.settings.active_mut().name = name;
                self.save_settings();
            }
            // What was typed stays typed. Out of range or halfway through a
            // number, it simply does not reach the box yet: the field is the
            // one place where an unfinished answer has to be allowed to sit.
            Message::BoxSized { side, mm } if side < 2 => {
                self.box_size[side] = mm;

                let typed = self.box_size[side].trim().replace(',', ".").parse::<f32>();
                let mut size = self.settings.active().size_mm;
                if let Ok(mm) = typed
                    && (cie_firma::MIN_SIDE_MM..=cie_firma::MAX_SIDE_MM).contains(&mm)
                {
                    size[side] = mm;
                    self.settings.active_mut().set_size_mm(size);
                    self.fit_stamp();
                    self.save_settings();
                }
            }
            Message::BoxSized { .. } => {}
            // The third way of saying how big the box is, after the two fields
            // and the rectangle on the page. All three end up in the same
            // place, and the other two have to show what this one did.
            Message::BoxStretched(size) => {
                self.settings.active_mut().set_size_mm(size);
                self.fit_stamp();
                self.show_box_size();
            }
            Message::SnappingToggled(on) => {
                self.settings.snapping = on;
                self.save_settings();
            }
            Message::TemplateBackgroundToggled(on) => {
                self.settings.active_mut().background = on;
                self.save_settings();
            }
            Message::TemplateBorderToggled(on) => {
                self.settings.active_mut().border = on;
                self.save_settings();
            }

            Message::SignatureImageChosen(Some(path)) => {
                self.notice = None;
                self.settings.active_mut().image = Some(path);
                self.load_signature();
                // A file that could not be read is not worth remembering: the
                // next run would only fail to open it again.
                if self.signature.is_none() {
                    self.settings.active_mut().image = None;
                } else if !self.settings.active().holds(&Piece::Image) {
                    // A picture nobody put in the box would be a picture that
                    // silently does nothing.
                    self.add_piece(Piece::Image);
                }
                self.save_settings();
            }
            Message::SignatureImageChosen(None) | Message::ChooseSignatureImage => {}
            Message::RemoveSignatureImage => {
                self.notice = None;
                self.settings.active_mut().image = None;
                self.signature = None;
                self.save_settings();
            }

            Message::PieceAdded(piece) => {
                self.add_piece(piece);
                self.save_settings();
            }
            Message::PieceSelected(which) => self.select(which),
            Message::PieceMoved(rect) => {
                if let Some(piece) = self.selected_piece_mut() {
                    piece.rect = rect;
                }
            }
            // What the drag has been doing all along, written down once.
            Message::DragEnded => self.save_settings(),
            Message::PieceResized(size) => {
                if let Some(piece) = self.selected_piece_mut() {
                    piece.size = size.clamp(4.0, 48.0);
                    self.save_settings();
                }
            }
            Message::PieceBoldToggled(bold) => {
                if let Some(piece) = self.selected_piece_mut() {
                    piece.bold = bold;
                    self.save_settings();
                }
            }
            Message::PieceAligned(align) => {
                if let Some(piece) = self.selected_piece_mut() {
                    piece.align = align;
                    self.save_settings();
                }
            }
            Message::PieceRaised(vertical) => {
                if let Some(piece) = self.selected_piece_mut() {
                    piece.vertical = vertical;
                    self.save_settings();
                }
            }
            Message::PieceTextChanged(content) => {
                if let Some(piece) = self.selected_piece_mut()
                    && let Piece::Text { content: slot } = &mut piece.piece
                {
                    *slot = content;
                    self.save_settings();
                }
            }
            Message::PieceRemoved => {
                if let Some(index) = self.selected() {
                    self.settings.active_mut().pieces.remove(index);
                    self.select(None);
                    self.save_settings();
                }
            }

            Message::OutputChosen(Some(p)) => {
                self.output = p;
                self.output_chosen = true;
            }
            Message::OutputChosen(None) | Message::ChooseOutput => {}

            Message::SignPressed => self.begin(),
            Message::PinChanged(pin) => {
                if let Phase::Pin { pin: slot, .. } = &mut self.phase {
                    // Only digits, never more than eight: the card allows three
                    // tries and must not burn one on a stray keystroke.
                    *slot = pin.chars().filter(char::is_ascii_digit).take(8).collect();
                }
            }
            Message::RememberToggled(on) => {
                if let Phase::Pin { remember, .. } = &mut self.phase {
                    *remember = on;
                }
            }
            Message::PinConfirmed => {
                if let Phase::Pin { pin, remember } = &self.phase
                    && pin.len() == 8
                {
                    let (pin, remember) = (pin.clone(), *remember);
                    self.start_signing(pin, remember);
                }
            }
            Message::SignCancelled | Message::OutcomeDismissed => {
                self.phase = Phase::Idle;
            }

            Message::ShowAbout => self.overlay = Overlay::About,
            Message::ShowTemplates => self.overlay = Overlay::Templates { selected: None },
            Message::OverlayDismissed => self.overlay = Overlay::None,

            Message::Tick => self.poll(),
            Message::ForgetCard => self.forget(),
            Message::ThemeChanged(mode) => self.mode = mode,

            // Side effects with nothing to change: the view turns these into
            // commands, `update` only has to not choke on them.
            Message::OpenResult | Message::OpenFolder | Message::Quit => {}
        }
    }

    fn open(&mut self, path: PathBuf) {
        self.open_error = None;
        self.document = None;
        self.page = 0;
        // Somewhere to be while there is no document. The box is a real size,
        // and a real size is only a fraction of a page once there is a page:
        // `propose_stamp`, at the end, once this one has been read.
        self.stamp = DEFAULT_STAMP;
        self.zoom = 1.0;
        self.output_chosen = false;
        self.phase = Phase::Idle;

        match std::fs::read(&path) {
            Ok(bytes) => {
                let is_pdf = bytes.starts_with(b"%PDF-");
                self.bytes = Some(Arc::new(bytes));

                if is_pdf {
                    match Document::open(self.bytes.as_ref().expect("just assigned")) {
                        Ok(d) => self.document = Some(d),
                        Err(e) => self.open_error = Some(format!("{e:#}")),
                    }
                }
                // A file that is not a PDF, or a PDF hayro cannot open, is
                // still signable — just not as PAdES.
                if self.document.is_none() && self.kind == Kind::Pades {
                    self.kind = Kind::P7m;
                }
                self.path = Some(path);
            }
            Err(e) => {
                self.bytes = None;
                self.path = None;
                self.open_error = Some(t!("document.read_failed", error = e.to_string()).to_string());
            }
        }
        self.propose_stamp();
        self.rasterise();
        self.refresh_output();
    }

    /// Rasterises the current page at a resolution that suits the zoom.
    ///
    /// Above `1.0` the bitmap has to grow too, or zooming in would just
    /// magnify the same pixels and the whole point would be lost.
    fn rasterise(&mut self) {
        self.page_image = self
            .document
            .as_ref()
            .map(|d| d.render(self.page, (BASE_RENDER_PX * self.zoom).min(MAX_RENDER_PX)));
    }

    fn refresh_output(&mut self) {
        if self.output_chosen {
            return;
        }
        if let Some(p) = &self.path {
            self.output = job::default_output(p, self.kind);
        }
    }

    /// Firma was pressed: skip the PIN panel when the keyring already knows
    /// this card, otherwise ask.
    fn begin(&mut self) {
        self.notice = None;
        match self.card.stored.as_ref().map(|s| s.pin.clone()) {
            Some(pin) => self.start_signing(pin, false),
            None => {
                self.phase = Phase::Pin {
                    pin: String::new(),
                    remember: true,
                }
            }
        }
    }

    fn start_signing(&mut self, pin: String, remember: bool) {
        let Some(bytes) = self.bytes.clone() else {
            self.phase = Phase::Failed(t!("sign.no_document").to_string());
            return;
        };

        if remember {
            self.remember(&pin);
        }

        let options = self.pades_options();

        self.phase = Phase::Running {
            step: 0,
            label: t!("sign.step_connecting").to_string(),
        };
        self.job = Some(job::spawn(Request {
            bytes,
            kind: self.kind,
            pin,
            output: self.output.clone(),
            options,
        }));
    }

    /// What the PAdES layer is being asked for.
    ///
    /// Gathered in one place, and out of `start_signing`, so what ends up in
    /// the stamp can be checked without a card on the reader.
    fn pades_options(&self) -> PadesOptions {
        match &self.document {
            Some(doc) if self.kind == Kind::Pades => PadesOptions {
                page: self.page as u32,
                rect: to_pdf_rect(self.stamp, &doc.geometry(self.page)),
                reason: (!self.reason.trim().is_empty()).then(|| self.reason.trim().to_string()),
                certify: self.certify,
                // The box the user composed, and the picture in it: the two
                // settings that outlast this document.
                stamp: cie_firma::Stamp::Composed {
                    template: self.settings.active().clone(),
                    image: self.signature.as_ref().map(|s| s.image.clone()),
                },
            },
            _ => PadesOptions::default(),
        }
    }

    /// How big the signature box is, in PDF points.
    ///
    /// The editor needs both numbers, and for different reasons: the shape, so
    /// what is composed has the proportions it will really have, and the width,
    /// because that is what turns a type size in points into pixels on screen.
    /// Compose against the wrong width and every size in the editor is a lie.
    ///
    /// The template's own, not the document's: the box is a size somebody
    /// chose, and dragging the rectangle over a page writes that choice back
    /// here, so the two can never drift apart. It is therefore an answer even
    /// with nothing open, which is what lets a box be composed before there is
    /// a document to put it on.
    pub fn box_size_pt(&self) -> (f32, f32) {
        self.settings.active().size_pt()
    }

    /// What the rectangle on the page currently measures, in points.
    ///
    /// `None` with no document open, and with a page of no size: both are the
    /// cases where a fraction of the sheet says nothing about millimetres.
    fn stamp_size_pt(&self) -> Option<(f32, f32)> {
        let doc = self.document.as_ref()?;
        let [x0, y0, x1, y1] = self.stamp;
        let (page_w, page_h) = doc.display_size(self.page);

        let (width, height) = ((x1 - x0) * page_w, (y1 - y0) * page_h);
        (width > 0.0 && height > 0.0).then_some((width, height))
    }

    /// Puts the template's box back in its two fields.
    fn show_box_size(&mut self) {
        self.box_size = written(self.settings.active().size_mm);
    }

    /// The template's box as a fraction of the page now showing.
    ///
    /// Capped at the whole sheet: a box wider than the paper cannot be put on
    /// it, and a rectangle that ran off the edge would be one whose far corner
    /// there is no way to grab.
    fn box_share(&self) -> Option<(f32, f32)> {
        let doc = self.document.as_ref()?;
        let (page_w, page_h) = doc.display_size(self.page);
        let (width, height) = self.settings.active().size_pt();

        (page_w > 0.0 && page_h > 0.0)
            .then(|| ((width / page_w).min(1.0), (height / page_h).min(1.0)))
    }

    /// Puts the rectangle back to the size the template says, leaving its top
    /// left corner where it is.
    ///
    /// The corner rather than the middle: what somebody positions is the
    /// beginning of the box, and a box that drifted sideways every time its
    /// width was typed into would be impossible to place.
    fn fit_stamp(&mut self) {
        let Some((width, height)) = self.box_share() else {
            return;
        };
        let [x0, y0, ..] = self.stamp;
        let x0 = x0.clamp(0.0, (1.0 - width).max(0.0));
        let y0 = y0.clamp(0.0, (1.0 - height).max(0.0));

        self.stamp = [x0, y0, (x0 + width).min(1.0), (y0 + height).min(1.0)];
    }

    /// Where the box is proposed on a document just opened: the corner
    /// [`DEFAULT_STAMP`] has always put it in, at the size the template asks
    /// for.
    ///
    /// Anchored on the far corner, not the near one, because the proposal is
    /// "bottom right of the page": a taller box has to grow up and to the
    /// left, away from the margin, rather than off the sheet.
    fn propose_stamp(&mut self) {
        self.stamp = DEFAULT_STAMP;
        let Some((width, height)) = self.box_share() else {
            return;
        };
        let [.., x1, y1] = DEFAULT_STAMP;
        let x0 = (x1 - width).max(0.0);
        let y0 = (y1 - height).max(0.0);

        self.stamp = [x0, y0, (x0 + width).min(1.0), (y0 + height).min(1.0)];
    }

    /// Which piece the editor has selected, if it is even open.
    pub fn selected(&self) -> Option<usize> {
        match self.overlay {
            Overlay::Templates { selected } => selected,
            _ => None,
        }
    }

    fn select(&mut self, which: Option<usize>) {
        if let Overlay::Templates { selected } = &mut self.overlay {
            *selected = which;
        }
    }

    /// The piece being edited, to be changed.
    fn selected_piece_mut(&mut self) -> Option<&mut Placed> {
        let index = self.selected()?;
        self.settings.active_mut().pieces.get_mut(index)
    }

    /// Puts a piece in the box and selects it.
    ///
    /// It lands in the middle rather than under the pointer: the palette is a
    /// list of buttons, not a drag source, so there is no pointer to land
    /// under. Selected, because the next thing anybody does with a piece they
    /// just added is drag it somewhere.
    fn add_piece(&mut self, piece: Piece) {
        // A picture wants room; a line wants a line's worth of it.
        let rect = match piece {
            Piece::Image => [0.30, 0.25, 0.70, 0.75],
            _ => [0.20, 0.40, 0.80, 0.60],
        };

        // A picture sits in the middle of its rectangle both ways, which is
        // where one in a stamp has always sat; a line starts at the left of
        // its own, where a line starts.
        let picture = piece == Piece::Image;

        let template = self.settings.active_mut();
        template.pieces.push(Placed {
            piece,
            rect,
            size: 7.0,
            bold: false,
            align: if picture { Align::Centre } else { Align::Left },
            vertical: Vertical::Middle,
        });
        let last = template.pieces.len() - 1;

        self.select(Some(last));
    }

    /// Reads the picture the settings name, if they name one.
    ///
    /// A path that no longer resolves — the scan was moved, or the settings
    /// came from another machine — is said out loud. Silently falling back to
    /// a text-only stamp would be a signature that quietly changed shape.
    fn load_signature(&mut self) {
        let Some(path) = self.settings.active().image.clone() else {
            self.signature = None;
            return;
        };

        match Signature::load(&path) {
            Ok(signature) => self.signature = Some(signature),
            Err(e) => {
                self.signature = None;
                self.notice =
                    Some(t!("signature.unusable", error = format!("{e:#}")).to_string());
            }
        }
    }

    /// Writes the settings back, if there is anywhere to write them.
    ///
    /// A settings file that cannot be written must not stop anything: the
    /// choice still holds for this session, it just will not be there tomorrow.
    fn save_settings(&mut self) {
        let Some(path) = &self.settings_path else {
            return;
        };
        if let Err(e) = self.settings.save(path) {
            self.notice = Some(t!("settings.not_saved", error = format!("{e:#}")).to_string());
        }
    }

    /// Writes the PIN under this card's id. Without an id there is nowhere to
    /// put it, and a keyring that refuses must not stop the signature.
    fn remember(&mut self, pin: &str) {
        let Some(id) = self.card.id.clone() else {
            return;
        };
        let entry = CardSecret {
            pin: pin.to_string(),
            ..self.card.stored.clone().unwrap_or_default()
        };
        match keyring::store(&id, &entry) {
            Ok(()) => {
                self.card.stored = Some(entry);
                self.notice = Some(t!("keyring.saved").to_string());
            }
            Err(e) => {
                self.notice = Some(t!("keyring.not_saved", error = format!("{e:#}")).to_string())
            }
        }
    }

    fn forget(&mut self) {
        let Some(id) = &self.card.id else { return };
        if keyring::forget(id).is_ok() {
            self.card.stored = None;
            self.notice = Some(t!("keyring.forgotten").to_string());
        }
    }

    /// Drains whatever the background threads have to say.
    fn poll(&mut self) {
        if let Some(rx) = &self.identifying
            && let Ok(id) = rx.try_recv()
        {
            self.card.stored = id.as_deref().and_then(keyring::read);
            self.card.id = id;
            self.identifying = None;
        }

        let Some(rx) = &self.job else { return };
        loop {
            match rx.try_recv() {
                Ok(job::Message::Progress(label)) => {
                    let step = match &self.phase {
                        Phase::Running { step, .. } => step + 1,
                        _ => 1,
                    };
                    self.phase = Phase::Running { step, label };
                }
                Ok(job::Message::Done { path, holder }) => {
                    self.learn(holder);
                    self.phase = Phase::Done(path);
                    self.job = None;
                    return;
                }
                Ok(job::Message::Failed(e)) => {
                    self.phase = Phase::Failed(e);
                    self.job = None;
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.job = None;
                    return;
                }
            }
        }
    }

    /// A signature just succeeded, so the certificate has been read: this is
    /// the one moment the holder's name can be learnt.
    ///
    /// Only worth writing when the keyring already holds this card — there is
    /// no PIN to file it under otherwise, and a name on its own would be
    /// personal data stored for nothing.
    fn learn(&mut self, holder: Option<cie_lib::DatiTitolare>) {
        let (Some(holder), Some(id), Some(stored)) =
            (holder, self.card.id.clone(), self.card.stored.clone())
        else {
            return;
        };

        let learnt = CardSecret {
            pin: stored.pin.clone(),
            name: holder.nome,
            surname: holder.cognome,
            tax_code: holder.codice_fiscale,
        };
        if learnt == stored {
            return;
        }
        if keyring::store(&id, &learnt).is_ok() {
            self.card.stored = Some(learnt);
        }
    }

    /// What the stamp should say, as far as anything knows before the PIN.
    ///
    /// `None` means nobody knows yet and the preview draws placeholders.
    pub fn known_holder(&self) -> Option<cie_lib::DatiTitolare> {
        let stored = self.card.stored.as_ref()?;
        (stored.name.is_some() || stored.surname.is_some()).then(|| cie_lib::DatiTitolare {
            nome: stored.name.clone(),
            cognome: stored.surname.clone(),
            codice_fiscale: stored.tax_code.clone(),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_PDF: &[u8] = include_bytes!("../../cie-firma/tests/fixtures/minimo.pdf");

    fn with_a_pdf() -> (State, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("documento.pdf");
        std::fs::write(&path, MINIMAL_PDF).unwrap();

        let mut state = State::default();
        state.update(Message::DocumentChosen(Some(path)));
        (state, dir)
    }

    /// A PDF has to land on PAdES with the preview showing: no preview and
    /// nowhere to put the rectangle is what looked broken from outside.
    #[test]
    fn a_pdf_lands_on_pades() {
        let (state, _dir) = with_a_pdf();

        assert!(state.document.is_some(), "PDF not opened: {:?}", state.open_error);
        assert_eq!(state.kind, Kind::Pades);
        assert!(state.shows_preview());
    }

    /// Anything that is not a PDF drops PAdES, and the preview with it.
    #[test]
    fn a_non_pdf_falls_back_to_p7m() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lettera.txt");
        std::fs::write(&path, b"non e' un PDF").unwrap();

        let mut state = State::default();
        state.update(Message::DocumentChosen(Some(path)));

        assert!(state.document.is_none());
        assert_eq!(state.kind, Kind::P7m);
        assert!(!state.shows_preview());
        assert_eq!(state.output.extension().unwrap(), "p7m");
    }

    /// Firma stays shut until there is something to sign.
    #[test]
    fn signing_needs_a_document() {
        assert!(!State::default().can_sign());

        let (state, _dir) = with_a_pdf();
        assert!(state.can_sign());
    }

    /// With no PIN in the keyring, Firma opens the panel rather than going
    /// straight at the card.
    #[test]
    fn firma_asks_for_the_pin_when_the_keyring_is_empty() {
        let (mut state, _dir) = with_a_pdf();
        state.update(Message::SignPressed);

        assert!(matches!(state.phase, Phase::Pin { .. }));
    }

    /// The field refuses anything the card would reject, so a typo cannot
    /// spend one of the three tries.
    #[test]
    fn the_pin_field_takes_eight_digits_and_no_more() {
        let (mut state, _dir) = with_a_pdf();
        state.update(Message::SignPressed);

        state.update(Message::PinChanged("12ab34-56789".to_string()));
        let Phase::Pin { pin, .. } = &state.phase else {
            panic!("not asking for a PIN");
        };
        assert_eq!(pin, "12345678");
    }

    /// A short PIN must not start anything.
    #[test]
    fn a_short_pin_does_not_start_the_card() {
        let (mut state, _dir) = with_a_pdf();
        state.update(Message::SignPressed);
        state.update(Message::PinChanged("1234".to_string()));
        state.update(Message::PinConfirmed);

        assert!(matches!(state.phase, Phase::Pin { .. }), "should still be asking");
        assert!(state.job.is_none());
    }

    /// The destination follows the kind, and keeps following it even after one
    /// was picked by hand.
    ///
    /// The suffix *is* the signature kind, so keeping `altrove.p7m` after a
    /// switch to detached would write a raw `.sig` into a file whose name
    /// claims to be an envelope. A lost folder choice is the cheaper mistake.
    #[test]
    fn the_destination_follows_the_kind() {
        let (mut state, dir) = with_a_pdf();
        assert_eq!(state.output.file_name().unwrap(), "documento-firmato.pdf");

        state.update(Message::KindChanged(Kind::P7m));
        assert_eq!(state.output.file_name().unwrap(), "documento.pdf.p7m");

        let chosen = dir.path().join("altrove.p7m");
        state.update(Message::OutputChosen(Some(chosen)));
        assert_eq!(state.output.file_name().unwrap(), "altrove.p7m");

        state.update(Message::KindChanged(Kind::Detached));
        assert_eq!(
            state.output.file_name().unwrap(),
            "documento.pdf.sig",
            "the suffix has to match the kind that will actually be written"
        );
    }

    /// At 1:1 the wheel turns pages; zoomed in it belongs to the scrollbars,
    /// and anything that reaches us is left alone.
    #[test]
    fn the_wheel_turns_pages_only_at_one_to_one() {
        let (mut state, _dir) = with_a_pdf();
        assert_eq!(state.zoom, 1.0);

        // One page in this fixture, so forward is a no-op; back off page 0 too.
        state.update(Message::Wheel { up: false });
        assert_eq!(state.page, 0);

        state.update(Message::ZoomIn);
        assert!(state.zoom > 1.0);
        state.update(Message::Wheel { up: false });
        assert_eq!(state.page, 0, "zoomed in, the wheel must not page");
    }

    /// Zoom walks its stops and stops at the ends rather than running off.
    #[test]
    fn zoom_walks_its_stops() {
        let (mut state, _dir) = with_a_pdf();

        state.update(Message::ZoomIn);
        assert_eq!(state.zoom, 1.25);
        state.update(Message::ZoomIn);
        assert_eq!(state.zoom, 1.5);
        state.update(Message::ZoomReset);
        assert_eq!(state.zoom, 1.0);

        // Below the first stop there is nothing to go to.
        state.update(Message::ZoomOut);
        assert_eq!(state.zoom, 1.0);

        for _ in 0..20 {
            state.update(Message::ZoomIn);
        }
        assert_eq!(state.zoom, 8.0, "must stop at the last stop");
    }

    /// While the panel is up, the document underneath must hold still — a
    /// stray arrow key must not move the box that is about to be signed.
    #[test]
    fn the_panel_swallows_navigation() {
        let (mut state, _dir) = with_a_pdf();
        state.update(Message::ZoomIn);
        let zoom = state.zoom;

        state.update(Message::SignPressed);
        assert!(matches!(state.phase, Phase::Pin { .. }));

        state.update(Message::ZoomIn);
        state.update(Message::StampReset);
        state.update(Message::Wheel { up: true });

        assert_eq!(state.zoom, zoom, "zoom moved while signing");
    }

    /// Opening a document rasterises it: without this the page area is blank
    /// until something else happens to trigger a redraw.
    #[test]
    fn opening_a_document_rasterises_it() {
        let (state, _dir) = with_a_pdf();
        assert!(state.page_image.is_some());
    }

    /// Paging stops at both ends instead of panicking on the index.
    #[test]
    fn paging_stays_on_the_document() {
        let (mut state, _dir) = with_a_pdf();
        assert_eq!(state.page_count(), 1);

        state.update(Message::PageForward);
        assert_eq!(state.page, 0);
        state.update(Message::PageBack);
        assert_eq!(state.page, 0);
    }

    /// A 2x1 PNG: the smallest thing that is still a picture with a shape.
    const SIGNATURE_PNG: &[u8] = include_bytes!("../../cie-firma/tests/fixtures/firma.png");

    fn with_a_signature() -> (State, tempfile::TempDir, PathBuf) {
        let (mut state, dir) = with_a_pdf();
        let picture = dir.path().join("firma.png");
        std::fs::write(&picture, SIGNATURE_PNG).unwrap();

        state.update(Message::ShowTemplates);
        state.update(Message::SignatureImageChosen(Some(picture.clone())));
        (state, dir, picture)
    }

    /// The template in `settings` is the one that signs, picture and all.
    #[test]
    fn the_active_template_is_the_one_that_signs() {
        let (state, _dir, picture) = with_a_signature();

        assert_eq!(state.settings.active().image.as_ref(), Some(&picture));
        assert!(state.signature.is_some(), "the picture was never read");
        assert_eq!(state.notice, None, "nothing went wrong, so nothing to say");

        let cie_firma::Stamp::Composed { template, image } = state.pades_options().stamp else {
            panic!("the GUI must sign with a template");
        };
        assert_eq!(&template, state.settings.active());
        assert_eq!(image.map(|i| (i.width, i.height)), Some((2, 1)));
    }

    /// Choosing a picture puts it in the box. A picture the template carries
    /// but never draws would be a setting that silently does nothing.
    #[test]
    fn a_chosen_picture_lands_in_the_box() {
        let (state, _dir, _) = with_a_signature();
        assert!(state.settings.active().holds(&Piece::Image));
    }

    /// ...and only once, however many times it is changed.
    #[test]
    fn changing_the_picture_does_not_add_it_again() {
        let (mut state, dir, _) = with_a_signature();
        let other = dir.path().join("altra.png");
        std::fs::write(&other, SIGNATURE_PNG).unwrap();

        state.update(Message::SignatureImageChosen(Some(other)));

        let pictures = state
            .settings
            .active()
            .pieces
            .iter()
            .filter(|p| p.piece == Piece::Image)
            .count();
        assert_eq!(pictures, 1);
    }

    /// A file that is not a picture must be turned away where it can still be
    /// re-chosen, and must not be written into the template: remembering it
    /// would only make the next run fail on startup instead.
    #[test]
    fn a_file_that_is_not_a_picture_is_refused_and_not_remembered() {
        let (mut state, dir) = with_a_pdf();
        let not_a_picture = dir.path().join("lettera.png");
        std::fs::write(&not_a_picture, b"questo non e' un PNG").unwrap();

        state.update(Message::SignatureImageChosen(Some(not_a_picture)));

        assert!(state.signature.is_none());
        assert_eq!(state.settings.active().image, None);
        assert!(state.notice.is_some(), "the refusal has to be visible");
    }

    /// A picture that has been moved or deleted since it was chosen must be
    /// said out loud, and must not cost the signature: the rest of the box is
    /// still drawn.
    #[test]
    fn a_picture_that_is_no_longer_there_is_reported() {
        let (mut state, dir) = with_a_pdf();
        state.settings.active_mut().image = Some(dir.path().join("sparita.png"));

        state.load_signature();

        assert!(state.signature.is_none());
        assert!(state.notice.is_some());

        let cie_firma::Stamp::Composed { template, image } = state.pades_options().stamp else {
            panic!("the GUI must sign with a template");
        };
        assert!(image.is_none());
        assert!(!template.pieces.is_empty(), "the lines went with the picture");
    }

    // --- the editor ------------------------------------------------------

    fn editing() -> (State, tempfile::TempDir) {
        let (mut state, dir) = with_a_pdf();
        state.update(Message::ShowTemplates);
        (state, dir)
    }

    /// The A4 fixture, in points, so a rectangle can be checked against a real
    /// size rather than against another fraction.
    const A4: (f32, f32) = (595.0, 842.0);

    /// The box is as big as the template says, and the rectangle on the page
    /// is that size worked out on this sheet.
    #[test]
    fn the_box_is_the_size_the_template_says() {
        let (mut state, _dir) = with_a_pdf();
        state.update(Message::BoxSized { side: 0, mm: "50".to_string() });
        state.update(Message::BoxSized { side: 1, mm: "20".to_string() });

        let (width, height) = state.box_size_pt();
        assert!((width - cie_firma::mm_to_pt(50.0)).abs() < 0.01, "{width}");

        let [x0, y0, x1, y1] = state.stamp;
        assert!(((x1 - x0) * A4.0 - width).abs() < 0.5, "{:?}", state.stamp);
        assert!(((y1 - y0) * A4.1 - height).abs() < 0.5, "{:?}", state.stamp);
        assert!(x0 >= 0.0 && y0 >= 0.0 && x1 <= 1.0 && y1 <= 1.0, "{:?}", state.stamp);
    }

    /// Resizing keeps the corner the box was put in, and never pushes it off
    /// the sheet however big it is asked to be.
    #[test]
    fn a_resized_box_stays_where_it_was_put_and_on_the_page() {
        let (mut state, _dir) = with_a_pdf();
        state.update(Message::StampChanged([0.10, 0.20, 0.30, 0.30]));

        state.update(Message::BoxSized { side: 0, mm: "40".to_string() });
        state.update(Message::BoxSized { side: 1, mm: "15".to_string() });
        assert!((state.stamp[0] - 0.10).abs() < 1e-6, "{:?}", state.stamp);
        assert!((state.stamp[1] - 0.20).abs() < 1e-6, "{:?}", state.stamp);

        // Wider than the paper: it can only be the paper, and it has to stay
        // on it or there is no corner left to take hold of. A4 is 210 mm
        // across.
        state.update(Message::BoxSized { side: 0, mm: "290".to_string() });
        let [x0, y0, x1, y1] = state.stamp;
        assert!(x0 >= 0.0 && y0 >= 0.0 && x1 <= 1.0 && y1 <= 1.0, "{:?}", state.stamp);
        assert!((x1 - x0 - 1.0).abs() < 1e-6, "should be the whole width: {:?}", state.stamp);
    }

    /// Dragging the rectangle over the page is the other way of saying how big
    /// the box is, so what it comes out measuring is what the template keeps.
    #[test]
    fn dragging_the_box_says_how_big_it_is() {
        let (mut state, _dir) = with_a_pdf();
        let (width, height) = (100.0, 50.0);

        state.update(Message::StampChanged([
            0.1,
            0.1,
            0.1 + width / A4.0,
            0.1 + height / A4.1,
        ]));

        let [w, h] = state.settings.active().size_mm;
        assert!((w - cie_firma::pt_to_mm(width)).abs() < 0.1, "{w}");
        assert!((h - cie_firma::pt_to_mm(height)).abs() < 0.1, "{h}");
    }

    /// A document opens with the box the template asks for, in the corner it
    /// has always been proposed in.
    #[test]
    fn an_opened_document_gets_the_template_box() {
        let (mut state, dir) = with_a_pdf();
        state.update(Message::BoxSized { side: 0, mm: "40".to_string() });
        state.update(Message::BoxSized { side: 1, mm: "15".to_string() });

        let path = dir.path().join("altro.pdf");
        std::fs::write(&path, MINIMAL_PDF).unwrap();
        state.update(Message::DocumentChosen(Some(path)));

        let [x0, y0, x1, y1] = state.stamp;
        assert!(((x1 - x0) * A4.0 - cie_firma::mm_to_pt(40.0)).abs() < 0.5, "{:?}", state.stamp);
        assert!(((y1 - y0) * A4.1 - cie_firma::mm_to_pt(15.0)).abs() < 0.5, "{:?}", state.stamp);
        // Bottom right, where `cie sign-pdf` has always put it.
        assert!((x1 - DEFAULT_STAMP[2]).abs() < 1e-6 && (y1 - DEFAULT_STAMP[3]).abs() < 1e-6);
    }

    /// Each template carries its own box, and switching to one brings its size
    /// with it.
    #[test]
    fn every_template_has_its_own_box() {
        let (mut state, _dir) = with_a_pdf();
        state.update(Message::BoxSized { side: 0, mm: "40".to_string() });
        state.update(Message::BoxSized { side: 1, mm: "15".to_string() });

        state.update(Message::TemplateAdded);
        assert_eq!(state.settings.active().size_mm, cie_firma::DEFAULT_SIZE_MM);
        let fresh = state.stamp;

        state.update(Message::TemplateChosen(0));
        assert_eq!(state.settings.active().size_mm, [40.0, 15.0]);
        assert_ne!(state.stamp, fresh, "the box did not follow the template");
    }

    /// The fields keep what was typed into them, and the box takes it as soon
    /// as it is a size a box can be.
    ///
    /// A field that answered back on every keystroke could not be typed into
    /// at all: clear `81` to put `60` in and the `8` would arrive back before
    /// the `6` did.
    #[test]
    fn a_size_field_holds_what_was_typed_into_it() {
        let (mut state, _dir) = with_a_pdf();
        let before = state.settings.active().size_mm;

        // Cleared to type into: the words go, the box stays.
        state.update(Message::BoxSized { side: 0, mm: String::new() });
        assert_eq!(state.box_size[0], "");
        assert_eq!(state.settings.active().size_mm, before);

        // Halfway through a number, and below anything a box can be: still
        // only words.
        state.update(Message::BoxSized { side: 0, mm: "6".to_string() });
        assert_eq!(state.box_size[0], "6");
        assert_eq!(state.settings.active().size_mm, before);

        // And there it is.
        state.update(Message::BoxSized { side: 0, mm: "60".to_string() });
        assert_eq!(state.settings.active().size_mm[0], 60.0);

        // The keyboard this is used on has a comma on it.
        state.update(Message::BoxSized { side: 1, mm: "24,5".to_string() });
        assert_eq!(state.settings.active().size_mm[1], 24.5);
    }

    /// Dragging the box is the other way of saying how big it is, so the
    /// fields have to say what the drag made it.
    #[test]
    fn the_fields_follow_the_box_around() {
        let (mut state, _dir) = with_a_pdf();
        state.update(Message::BoxSized { side: 0, mm: "60".to_string() });
        assert_eq!(state.box_size[0], "60");

        state.update(Message::StampChanged([0.1, 0.1, 0.1 + 100.0 / A4.0, 0.2]));
        assert_eq!(state.box_size[0], "35.3", "{:?}", state.box_size);

        // A template of its own, and the fields say so the moment it is
        // switched to.
        state.update(Message::TemplateAdded);
        assert_eq!(state.box_size, ["81".to_string(), "24.7".to_string()]);
    }

    /// Pulling the box's own edge in the editor is the third way of saying how
    /// big it is, and the other two have to show what it did.
    #[test]
    fn the_box_can_be_pulled_bigger_by_its_edge() {
        let (mut state, _dir) = with_a_pdf();
        state.update(Message::ShowTemplates);

        state.update(Message::BoxStretched([100.0, 40.0]));

        assert_eq!(state.settings.active().size_mm, [100.0, 40.0]);
        assert_eq!(state.box_size, ["100".to_string(), "40".to_string()]);

        let [x0, y0, x1, y1] = state.stamp;
        assert!(((x1 - x0) * A4.0 - cie_firma::mm_to_pt(100.0)).abs() < 0.5, "{:?}", state.stamp);
        assert!(((y1 - y0) * A4.1 - cie_firma::mm_to_pt(40.0)).abs() < 0.5, "{:?}", state.stamp);

        // Pulled past what a box can be, it stops at what a box can be rather
        // than at nothing.
        state.update(Message::BoxStretched([0.0, 0.0]));
        assert_eq!(
            state.settings.active().size_mm,
            [cie_firma::MIN_SIDE_MM, cie_firma::MIN_SIDE_MM]
        );
    }

    /// A piece lands in the box selected, because the next thing anybody does
    /// with a piece they just added is drag it somewhere.
    #[test]
    fn an_added_piece_arrives_selected() {
        let (mut state, _dir) = editing();
        let before = state.settings.active().pieces.len();

        state.update(Message::PieceAdded(Piece::Text {
            content: "ciao".to_string(),
        }));

        assert_eq!(state.settings.active().pieces.len(), before + 1);
        assert_eq!(state.selected(), Some(before));
    }

    /// Dragging changes the piece the editor has selected, and only that one.
    #[test]
    fn dragging_moves_the_selected_piece_alone() {
        let (mut state, _dir) = editing();
        let untouched = state.settings.active().pieces[1].rect;

        state.update(Message::PieceSelected(Some(0)));
        state.update(Message::PieceMoved([0.5, 0.5, 0.9, 0.7]));

        assert_eq!(state.settings.active().pieces[0].rect, [0.5, 0.5, 0.9, 0.7]);
        assert_eq!(state.settings.active().pieces[1].rect, untouched);
    }

    /// With nothing selected there is nothing to change, and a stray drag must
    /// not land on whatever happens to be first.
    #[test]
    fn nothing_selected_means_nothing_changes() {
        let (mut state, _dir) = editing();
        let before = state.settings.active().clone();

        state.update(Message::PieceSelected(None));
        state.update(Message::PieceMoved([0.0, 0.0, 1.0, 1.0]));
        state.update(Message::PieceResized(30.0));
        state.update(Message::PieceRemoved);

        assert_eq!(state.settings.active(), &before);
    }

    /// The type size is clamped: a box is a few dozen points tall, and a
    /// mistyped 500 would draw one letter and lose the rest.
    #[test]
    fn the_type_size_stays_within_reason() {
        let (mut state, _dir) = editing();
        state.update(Message::PieceSelected(Some(0)));

        state.update(Message::PieceResized(500.0));
        assert_eq!(state.settings.active().pieces[0].size, 48.0);

        state.update(Message::PieceResized(0.0));
        assert_eq!(state.settings.active().pieces[0].size, 4.0);
    }

    /// Free text is the one piece whose words are the user's. Changing it must
    /// reach the piece, and must not touch a piece that has no text of its own.
    #[test]
    fn free_text_can_be_typed_and_the_rest_cannot() {
        let (mut state, _dir) = editing();

        state.update(Message::PieceAdded(Piece::Text {
            content: "prima".to_string(),
        }));
        state.update(Message::PieceTextChanged("dopo".to_string()));

        let last = state.settings.active().pieces.last().unwrap();
        assert_eq!(last.piece, Piece::Text { content: "dopo".to_string() });

        // The date says what the clock says, whatever anybody types.
        state.update(Message::PieceSelected(Some(3)));
        let before = state.settings.active().pieces[3].piece.clone();
        state.update(Message::PieceTextChanged("il mio compleanno".to_string()));
        assert_eq!(state.settings.active().pieces[3].piece, before);
    }

    /// Removing takes the selection with it: leaving it pointing at an index
    /// that has shifted under it would edit the wrong piece next.
    #[test]
    fn removing_a_piece_clears_the_selection() {
        let (mut state, _dir) = editing();
        let before = state.settings.active().pieces.len();

        state.update(Message::PieceSelected(Some(0)));
        state.update(Message::PieceRemoved);

        assert_eq!(state.settings.active().pieces.len(), before - 1);
        assert_eq!(state.selected(), None);
    }

    /// New, duplicate, delete — and never down to nothing. With no templates
    /// there is nothing to sign with.
    #[test]
    fn templates_can_be_added_copied_and_removed_but_never_all() {
        let (mut state, _dir) = editing();
        assert_eq!(state.settings.templates.len(), 1);

        state.update(Message::TemplateAdded);
        assert_eq!(state.settings.templates.len(), 2);
        assert_eq!(state.settings.active, 1, "the new one is the one you edit");

        state.update(Message::TemplateRenamed("il mio".to_string()));
        state.update(Message::TemplateDuplicated);
        assert_eq!(state.settings.templates.len(), 3);
        assert!(
            state.settings.active().name.contains("il mio"),
            "a copy should say what it is a copy of: {}",
            state.settings.active().name
        );

        for _ in 0..5 {
            state.update(Message::TemplateDeleted);
        }
        assert_eq!(state.settings.templates.len(), 1, "the last one has to stay");
    }

    /// Switching template switches the picture with it: each one carries its
    /// own, and showing the previous one's would be a lie about what will sign.
    #[test]
    fn switching_template_switches_the_picture() {
        let (mut state, _dir, _) = with_a_signature();
        assert!(state.signature.is_some());

        state.update(Message::TemplateAdded);
        assert!(state.signature.is_none(), "a fresh template has no picture");

        state.update(Message::TemplateChosen(0));
        assert!(state.signature.is_some(), "the first one still has one");
    }

    /// A title opens its list, the same title again puts it away. Anything
    /// else the user does closes it: a list left hanging over the window after
    /// its command has run is one more click before you can see what happened.
    #[test]
    fn a_menu_opens_toggles_and_gets_out_of_the_way() {
        let mut state = State::default();
        assert_eq!(state.menu, None);

        state.update(Message::MenuToggled(Menu::File));
        assert_eq!(state.menu, Some(Menu::File));

        state.update(Message::MenuToggled(Menu::File));
        assert_eq!(state.menu, None, "the same title should have closed it");

        state.update(Message::MenuToggled(Menu::File));
        state.update(Message::ChooseDocument);
        assert_eq!(state.menu, None, "running a command should have closed it");
    }

    /// The card thread ticks every 100 ms while it is working. If that closed
    /// the menu, no menu could be opened during a signature at all.
    #[test]
    fn the_card_ticking_does_not_shut_the_menu() {
        let mut state = State::default();
        state.update(Message::MenuToggled(Menu::File));

        state.update(Message::Tick);

        assert_eq!(state.menu, Some(Menu::File));
    }

    /// About goes up and comes back down, and does not disturb the signing
    /// state machine on its way past.
    #[test]
    fn about_opens_and_closes_without_touching_the_card() {
        let mut state = State::default();

        state.update(Message::ShowAbout);
        assert_eq!(state.overlay, Overlay::About);
        assert!(matches!(state.phase, Phase::Idle));

        state.update(Message::OverlayDismissed);
        assert_eq!(state.overlay, Overlay::None);
    }
}
