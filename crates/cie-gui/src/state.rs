//! The window's state and everything that changes it.
//!
//! No widget is built here. That is deliberate: `update` stays a plain function
//! over plain data, so the behaviour that matters — which signature kind a file
//! lands on, when signing is allowed, what the panel shows after a failure —
//! can be tested without opening a window.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use cie_firma::PadesOptions;
use cie_firma::keyring::{self, CardSecret};
use cie_lib::CardStatus;
use rust_i18n::t;

use crate::job::{self, Kind, Request};
use crate::preview::{Document, to_pdf_rect};
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

    // Background
    Tick,
    /// The reader gained or lost something.
    CardChanged(CardStatus),
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

/// The card on the reader, as far as we know.
#[derive(Debug, Default)]
pub struct Card {
    pub status: CardStatus,
    pub id: Option<String>,
    pub stored: Option<CardSecret>,
    /// The card has already been asked who it is. `id` stays `None` when it
    /// would not say, and without this we would keep asking forever.
    pub asked: bool,
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
    pub notice: Option<String>,
    pub mode: Mode,

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
            notice: None,
            mode: Mode::default(),
            identifying: None,
            job: None,
        }
    }
}

impl State {
    /// True while something in the background needs draining.
    pub fn busy(&self) -> bool {
        self.identifying.is_some() || self.job.is_some()
    }

    /// A document and a card on the reader are enough to press Firma: the PIN
    /// is asked afterwards.
    ///
    /// Not while that card is being identified, though: that thread holds the
    /// reader, and signing alongside it would make one of the two fail.
    pub fn can_sign(&self) -> bool {
        self.bytes.is_some()
            && self.card.status == CardStatus::Present
            && self.job.is_none()
            && self.identifying.is_none()
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

        match message {
            Message::DocumentChosen(Some(p)) | Message::FileDropped(p) => self.open(p),
            Message::DocumentChosen(None) | Message::ChooseDocument => {}

            Message::KindChanged(kind) => {
                self.kind = kind;
                self.output_chosen = false;
                self.refresh_output();
            }
            Message::PageBack => {
                self.page = self.page.saturating_sub(1);
                self.rasterise();
            }
            Message::PageForward => {
                if self.page + 1 < self.page_count() {
                    self.page += 1;
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
            Message::StampReset => self.stamp = DEFAULT_STAMP,
            Message::StampChanged(stamp) => self.stamp = stamp,
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

            Message::Tick => self.poll(),
            Message::CardChanged(status) => {
                self.set_status(status);
                self.identify_if_needed();
            }
            Message::ForgetCard => self.forget(),
            Message::ThemeChanged(mode) => self.mode = mode,

            // Side effects with nothing to change: the view turns these into
            // commands, `update` only has to not choke on them.
            Message::OpenResult | Message::OpenFolder => {}
        }
    }

    fn open(&mut self, path: PathBuf) {
        self.open_error = None;
        self.document = None;
        self.page = 0;
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

        let options = match &self.document {
            Some(doc) if self.kind == Kind::Pades => PadesOptions {
                page: self.page as u32,
                rect: to_pdf_rect(self.stamp, &doc.geometry(self.page)),
                reason: (!self.reason.trim().is_empty()).then(|| self.reason.trim().to_string()),
                certify: self.certify,
            },
            _ => PadesOptions::default(),
        };

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
            self.card.asked = true;
            self.identifying = None;
        }

        if self.job.is_some() {
            self.poll_job();
            // The reader may have come free, with a card on it nobody has
            // asked about yet.
            self.identify_if_needed();
        }
    }

    fn poll_job(&mut self) {
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

    /// The reader changed. Anything read off the old card stops being true the
    /// moment it leaves — including the PIN the keyring handed over for it.
    fn set_status(&mut self, status: CardStatus) {
        if status == self.card.status {
            return;
        }
        self.card = Card {
            status,
            ..Card::default()
        };
        self.identifying = None;
    }

    /// Asks the card who it is, unless somebody else is on the reader.
    ///
    /// Called again once a signature ends: while one is running the reader is
    /// taken, and the card that was in it never got asked.
    fn identify_if_needed(&mut self) {
        if self.card.status == CardStatus::Present
            && !self.card.asked
            && self.identifying.is_none()
            && self.job.is_none()
        {
            self.identifying = Some(job::identify());
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

    /// Firma stays shut until there is something to sign and a card to sign it
    /// with: without both, the press could only end in an error panel.
    #[test]
    fn signing_needs_a_document_and_a_card() {
        assert!(!State::default().can_sign());

        let (mut state, _dir) = with_a_pdf();
        assert!(!state.can_sign(), "no reader to sign with");

        state.set_status(CardStatus::NoCard);
        assert!(!state.can_sign(), "a reader on its own signs nothing");

        state.set_status(CardStatus::Present);
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

    /// Taking the card out has to take everything read off it with it: the id
    /// on the bar, and the PIN the keyring handed over for that card.
    #[test]
    fn removing_the_card_forgets_it() {
        let mut state = State::default();
        state.set_status(CardStatus::Present);
        state.card.id = Some("1234".into());
        state.card.stored = Some(CardSecret::default());
        state.card.asked = true;

        state.set_status(CardStatus::NoCard);
        assert_eq!(state.card.status, CardStatus::NoCard);
        assert!(state.card.id.is_none());
        assert!(state.card.stored.is_none());

        // ...and the next card gets asked who it is, rather than inheriting the
        // answer the last one gave.
        state.set_status(CardStatus::Present);
        assert!(!state.card.asked);
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
}
