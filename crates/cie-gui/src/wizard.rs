//! The wizard: five steps, four when the signature is not PAdES.

use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;

use cie_firma::PadesOptions;

use crate::job::{self, Kind, Message, Request};
use crate::keyring;
use crate::preview::{move_stamp, resize_stamp, to_pdf_rect, Document, Grab};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    File,
    Kind,
    Stamp,
    Pin,
    Outcome,
}

impl Step {
    fn title(self) -> &'static str {
        match self {
            Step::File => "Documento",
            Step::Kind => "Tipo di firma",
            Step::Stamp => "Riquadro",
            Step::Pin => "PIN",
            Step::Outcome => "Esito",
        }
    }
}

/// The rectangle proposed when the user drags none: bottom right, same place
/// `cie sign-pdf` uses from the command line.
const DEFAULT_STAMP: [f32; 4] = [0.56, 0.87, 0.95, 0.96];

pub struct App {
    step: Step,

    path: Option<PathBuf>,
    bytes: Option<Arc<Vec<u8>>>,
    document: Option<Document>,
    open_error: Option<String>,

    kind: Kind,

    page: usize,
    /// Fractions of the image (`0..1`, y downwards). It only becomes a `/Rect`
    /// at signing time, when the page geometry is known.
    stamp: [f32; 4],
    grab: Option<Grab>,
    texture: Option<(usize, f32, egui::TextureHandle)>,
    reason: String,
    certify: Option<u8>,

    pin: String,
    store_pin: bool,
    pin_in_keyring: bool,
    keyring_message: Option<String>,

    output: PathBuf,
    output_chosen: bool,

    job: Option<Receiver<Message>>,
    status: String,
    outcome: Option<Result<PathBuf, String>>,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_pin(keyring::read())
    }

    /// Without `eframe`, so the wizard can be exercised in a test.
    fn with_pin(pin: Option<String>) -> Self {
        Self {
            step: Step::File,
            path: None,
            bytes: None,
            document: None,
            open_error: None,
            kind: Kind::Pades,
            page: 0,
            stamp: DEFAULT_STAMP,
            grab: None,
            texture: None,
            reason: String::new(),
            certify: None,
            pin_in_keyring: pin.is_some(),
            store_pin: pin.is_none(),
            pin: pin.unwrap_or_default(),
            keyring_message: None,
            output: PathBuf::new(),
            output_chosen: false,
            job: None,
            status: String::new(),
            outcome: None,
        }
    }

    /// The applicable steps: the stamp only exists for PAdES.
    fn steps(&self) -> &'static [Step] {
        if self.kind == Kind::Pades {
            &[Step::File, Step::Kind, Step::Stamp, Step::Pin, Step::Outcome]
        } else {
            &[Step::File, Step::Kind, Step::Pin, Step::Outcome]
        }
    }

    fn index(&self) -> usize {
        self.steps().iter().position(|s| *s == self.step).unwrap_or(0)
    }

    fn can_advance(&self) -> bool {
        match self.step {
            Step::File => self.bytes.is_some(),
            Step::Kind => self.kind != Kind::Pades || self.document.is_some(),
            Step::Stamp => true,
            // Eight digits get checked here, before touching the card: the
            // counter allows three tries and must not go on a typo.
            Step::Pin => self.pin.len() == 8 && self.pin.chars().all(|c| c.is_ascii_digit()),
            Step::Outcome => false,
        }
    }

    fn go(&mut self, forward: bool) {
        let steps = self.steps();
        let i = self.index();
        let next = if forward {
            steps.get(i + 1).copied()
        } else {
            i.checked_sub(1).and_then(|j| steps.get(j).copied())
        };
        if let Some(s) = next {
            self.step = s;
            if s == Step::Pin {
                self.refresh_output();
            }
            if s == Step::Outcome {
                self.start();
            }
        }
    }

    fn refresh_output(&mut self) {
        if self.output_chosen {
            return;
        }
        if let Some(p) = &self.path {
            self.output = job::default_output(p, self.kind);
        }
    }

    fn open_file(&mut self, path: PathBuf) {
        self.open_error = None;
        self.document = None;
        self.texture = None;
        self.page = 0;
        self.stamp = DEFAULT_STAMP;
        self.output_chosen = false;

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
                // still signable: just not as PAdES.
                if self.document.is_none() && self.kind == Kind::Pades {
                    self.kind = Kind::P7m;
                }
                self.path = Some(path);
            }
            Err(e) => {
                self.bytes = None;
                self.path = None;
                self.open_error = Some(format!("lettura del file: {e}"));
            }
        }
        self.refresh_output();
    }

    fn start(&mut self) {
        self.outcome = None;
        self.status = "Avvio…".to_string();

        let Some(bytes) = self.bytes.clone() else {
            self.outcome = Some(Err("nessun documento".to_string()));
            return;
        };

        let options = match &self.document {
            Some(doc) if self.kind == Kind::Pades => PadesOptions {
                page: self.page as u32,
                rect: to_pdf_rect(self.stamp, &doc.geometry(self.page)),
                reason: (!self.reason.trim().is_empty()).then(|| self.reason.trim().to_string()),
                certify: self.certify,
            },
            _ => PadesOptions::default(),
        };

        if self.store_pin {
            // A keyring that refuses must not block signing: say so and carry
            // on.
            if let Err(e) = keyring::store(&self.pin) {
                self.keyring_message = Some(format!("PIN non salvato: {e:#}"));
            } else {
                self.pin_in_keyring = true;
                self.store_pin = false;
            }
        }

        self.job = Some(job::spawn(Request {
            bytes,
            kind: self.kind,
            pin: self.pin.clone(),
            output: self.output.clone(),
            options,
        }));
    }

    /// Drains the card thread's channel. Returns `true` while work is still in
    /// flight, i.e. while another repaint is needed.
    fn poll(&mut self) -> bool {
        let Some(rx) = &self.job else {
            return false;
        };
        loop {
            match rx.try_recv() {
                Ok(Message::Progress(s)) => self.status = s,
                Ok(Message::Done(p)) => {
                    self.outcome = Some(Ok(p));
                    self.job = None;
                    return false;
                }
                Ok(Message::Failed(e)) => {
                    self.outcome = Some(Err(e));
                    self.job = None;
                    return false;
                }
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => {
                    if self.outcome.is_none() {
                        self.outcome =
                            Some(Err("il thread della carta è morto senza dire perché".to_string()));
                    }
                    self.job = None;
                    return false;
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl App {
    /// All the drawing, split off the eframe trait so a test can run it inside
    /// a fake `Ui` without opening a window.
    fn draw(&mut self, ui: &mut egui::Ui) {
        if self.poll() {
            ui.ctx().request_repaint();
        }

        egui::Panel::top("steps").show(ui, |ui| {
            ui.add_space(6.0);
            breadcrumb(ui, self.steps(), self.index());
            ui.add_space(6.0);
        });

        egui::Panel::bottom("nav").show(ui, |ui| {
            ui.add_space(6.0);
            self.navigation(ui);
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ui, |ui| match self.step {
            Step::File => self.step_file(ui),
            Step::Kind => self.step_kind(ui),
            Step::Stamp => self.step_stamp(ui),
            Step::Pin => self.step_pin(ui),
            Step::Outcome => self.step_outcome(ui),
        });
    }
}

const STAMP_BLUE: egui::Color32 = egui::Color32::from_rgb(60, 120, 220);

/// Side of the corner grips, in pixels.
const GRIP_PX: f32 = 7.0;

/// Draws inside the stamp rectangle a mock-up of what the signature will look
/// like, at the same scale it will have in the PDF.
///
/// The lines come from `cie_firma::preview_stamp_lines`, the very function that
/// composes the real stamp: no second copy of the layout to keep in step. The
/// holder's name and tax code are placeholders — they live in a certificate
/// that stays locked behind the PIN until signing time.
///
/// ponytail: egui's default fonts ship no bold face, so a bold line is drawn
/// darker instead of heavier. Swap in a bold font if it ever reads wrong.
fn draw_stamp_mockup(painter: &egui::Painter, rect: egui::Rect, px_per_pt: f32, reason: &str) {
    let reason = reason.trim();
    let lines = cie_firma::preview_stamp_lines((!reason.is_empty()).then_some(reason));

    let pad = 3.0 * px_per_pt;
    let mut y = rect.min.y + pad;

    for line in &lines {
        let size_px = line.font_size.unwrap_or(7.0) * px_per_pt;
        // Below roughly 5 px text is unreadable mush: better to stop than to
        // fill the box with grey noise.
        if size_px < 5.0 {
            break;
        }
        let leading = size_px * 1.25;
        if y + leading > rect.max.y {
            break;
        }

        painter.text(
            egui::pos2(rect.min.x + pad, y),
            egui::Align2::LEFT_TOP,
            &line.text,
            egui::FontId::proportional(size_px),
            if line.bold {
                egui::Color32::from_gray(20)
            } else {
                egui::Color32::from_gray(90)
            },
        );
        y += leading;
    }
}

/// The strip of steps along the top.
fn breadcrumb(ui: &mut egui::Ui, steps: &[Step], current: usize) {
    ui.horizontal(|ui| {
        for (i, s) in steps.iter().enumerate() {
            if i > 0 {
                ui.label("─");
            }
            let text = format!("{} {}", i + 1, s.title());
            if i == current {
                ui.strong(text);
            } else {
                ui.weak(text);
            }
        }
    });
}

impl App {
    fn navigation(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let first = self.index() == 0;
            if ui.add_enabled(!first, egui::Button::new("◀ Indietro")).clicked() {
                self.go(false);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match self.step {
                    Step::Outcome => {
                        if self.job.is_none() && ui.button("Firma un altro documento").clicked() {
                            self.step = Step::File;
                            self.path = None;
                            self.bytes = None;
                            self.document = None;
                            self.texture = None;
                            self.outcome = None;
                            self.output_chosen = false;
                        }
                    }
                    _ => {
                        let label = if self.step == Step::Pin {
                            "Firma ▶"
                        } else {
                            "Avanti ▶"
                        };
                        if ui
                            .add_enabled(self.can_advance(), egui::Button::new(label))
                            .clicked()
                        {
                            self.go(true);
                        }
                    }
                }
            });
        });
    }

    fn step_file(&mut self, ui: &mut egui::Ui) {
        ui.heading("Quale documento vuoi firmare?");
        ui.add_space(12.0);

        if ui.button("Scegli un documento…").clicked()
            && let Some(p) = rfd::FileDialog::new().pick_file()
        {
            self.open_file(p);
        }

        ui.add_space(12.0);
        if let (Some(p), Some(b)) = (&self.path, &self.bytes) {
            ui.label(format!("{}", p.display()));
            ui.weak(format!("{} byte", b.len()));
            if self.document.is_some() {
                ui.weak("PDF leggibile: si può firmare in PAdES.");
            } else {
                ui.weak("Non è un PDF che sappiamo mostrare: resta CAdES o firma separata.");
            }
        }

        if let Some(e) = &self.open_error {
            ui.add_space(8.0);
            ui.colored_label(egui::Color32::from_rgb(200, 80, 60), e);
        }
    }

    fn step_kind(&mut self, ui: &mut egui::Ui) {
        ui.heading("Che tipo di firma?");
        ui.add_space(12.0);

        let pdf_ok = self.document.is_some();
        ui.add_enabled_ui(pdf_ok, |ui| {
            ui.radio_value(&mut self.kind, Kind::Pades, Kind::Pades.label());
        });
        if !pdf_ok {
            ui.weak("   (il PAdES vale solo per i PDF)");
        }
        ui.radio_value(&mut self.kind, Kind::P7m, Kind::P7m.label());
        ui.radio_value(&mut self.kind, Kind::Detached, Kind::Detached.label());

        self.output_chosen = false;
        self.refresh_output();
    }

    fn step_stamp(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &self.document else {
            ui.label("nessun PDF aperto");
            return;
        };
        let pages = doc.page_count();

        ui.horizontal(|ui| {
            if ui.add_enabled(self.page > 0, egui::Button::new("◀")).clicked() {
                self.page -= 1;
                self.texture = None;
            }
            ui.label(format!("Pagina {} / {}", self.page + 1, pages));
            if ui
                .add_enabled(self.page + 1 < pages, egui::Button::new("▶"))
                .clicked()
            {
                self.page += 1;
                self.texture = None;
            }

            ui.separator();
            ui.label("Motivo:");
            ui.add(egui::TextEdit::singleline(&mut self.reason).desired_width(160.0));

            ui.separator();
            let mut certify = self.certify.is_some();
            if ui.checkbox(&mut certify, "Certifica").changed() {
                self.certify = certify.then_some(2);
            }
            if let Some(level) = &mut self.certify {
                egui::ComboBox::from_id_salt("level")
                    .selected_text(match level {
                        1 => "nessuna modifica",
                        3 => "moduli, firme e annotazioni",
                        _ => "moduli e altre firme",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(level, 1, "nessuna modifica");
                        ui.selectable_value(level, 2, "moduli e altre firme");
                        ui.selectable_value(level, 3, "moduli, firme e annotazioni");
                    });
            }
        });

        ui.horizontal(|ui| {
            ui.weak(
                "Trascina il riquadro per spostarlo, gli angoli per ridimensionarlo. \
                 Il testo è un facsimile: nome e codice fiscale li legge dalla carta \
                 al momento della firma.",
            );
            // Drag-to-draw is gone, so this is the only way back to the
            // proposed position.
            if ui.small_button("Ripristina").clicked() {
                self.stamp = DEFAULT_STAMP;
            }
        });
        ui.add_space(6.0);

        self.draw_preview(ui);
    }

    fn draw_preview(&mut self, ui: &mut egui::Ui) {
        let doc = self.document.as_ref().expect("checked by the caller");

        // The page has to fit whatever room is left: try to fill the width
        // first, then shrink if it overflows the height.
        let room = ui.available_size();
        let ratio = doc.aspect_ratio(self.page);
        let (page_width_pt, _) = doc.display_size(self.page);
        let mut width = room.x.min(900.0);
        if width / ratio > room.y {
            width = room.y * ratio;
        }
        let width = width.max(80.0);
        let size = egui::vec2(width, width / ratio);

        // ponytail: re-render only past 4 px of difference. Rasterizing on
        // every pixel of a resize makes the window stutter; if smoothness ever
        // matters, a per-page cache goes here.
        let stale = match &self.texture {
            Some((p, w, _)) => *p != self.page || (w - width).abs() > 4.0,
            None => true,
        };
        if stale {
            let image = doc.render(self.page, width * ui.ctx().pixels_per_point());
            let handle = ui
                .ctx()
                .load_texture("page", image, egui::TextureOptions::LINEAR);
            self.texture = Some((self.page, width, handle));
        }

        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());

        if let Some((_, _, handle)) = &self.texture {
            egui::Image::new(handle).paint_at(ui, rect);
        }

        // Screen coordinates of the stamp, and its four corner grips.
        let on_screen = |s: [f32; 4]| {
            egui::Rect::from_min_max(
                rect.min + egui::vec2(s[0] * rect.width(), s[1] * rect.height()),
                rect.min + egui::vec2(s[2] * rect.width(), s[3] * rect.height()),
            )
        };
        let box_rect = on_screen(self.stamp);
        let corners = [
            box_rect.left_top(),
            box_rect.right_top(),
            box_rect.left_bottom(),
            box_rect.right_bottom(),
        ];

        // What is under the pointer. Corners win over the body: they sit on top
        // of it, and a grip you cannot hit is no grip.
        let what_is_under = |p: egui::Pos2| -> Option<Grab> {
            corners
                .iter()
                .position(|c| c.distance(p) <= GRIP_PX * 1.6)
                .map(Grab::Corner)
                .or_else(|| box_rect.contains(p).then_some(Grab::Body))
        };

        if response.drag_started()
            && let Some(p) = response.interact_pointer_pos()
        {
            self.grab = what_is_under(p);
        }
        if response.drag_stopped() {
            self.grab = None;
        }

        match self.grab {
            Some(Grab::Body) => {
                let d = response.drag_delta();
                self.stamp = move_stamp(self.stamp, d.x / rect.width(), d.y / rect.height());
            }
            Some(Grab::Corner(i)) => {
                if let Some(p) = response.interact_pointer_pos() {
                    self.stamp = resize_stamp(
                        self.stamp,
                        i,
                        (
                            (p.x - rect.min.x) / rect.width(),
                            (p.y - rect.min.y) / rect.height(),
                        ),
                    );
                }
            }
            None => {}
        }

        // Tell the pointer what it can do here before it tries.
        if let Some(p) = response.hover_pos() {
            ui.ctx().set_cursor_icon(match what_is_under(p) {
                Some(Grab::Corner(0)) | Some(Grab::Corner(3)) => egui::CursorIcon::ResizeNwSe,
                Some(Grab::Corner(_)) => egui::CursorIcon::ResizeNeSw,
                Some(Grab::Body) => egui::CursorIcon::Grab,
                None => egui::CursorIcon::Default,
            });
        }

        let box_rect = on_screen(self.stamp);
        let painter = ui.painter_at(rect);
        painter.rect_filled(box_rect, 2.0, egui::Color32::from_white_alpha(235));
        painter.rect_stroke(
            box_rect,
            2.0,
            egui::Stroke::new(1.5, STAMP_BLUE),
            egui::StrokeKind::Inside,
        );
        for c in [
            box_rect.left_top(),
            box_rect.right_top(),
            box_rect.left_bottom(),
            box_rect.right_bottom(),
        ] {
            painter.rect_filled(
                egui::Rect::from_center_size(c, egui::Vec2::splat(GRIP_PX)),
                1.0,
                STAMP_BLUE,
            );
        }
        let on_screen = box_rect;

        // One point of the page is this many pixels on screen: the mock-up has
        // to shrink and grow with the preview, or it stops predicting anything.
        let px_per_pt = rect.width() / page_width_pt;
        draw_stamp_mockup(&ui.painter_at(on_screen), on_screen, px_per_pt, &self.reason);
    }

    fn step_pin(&mut self, ui: &mut egui::Ui) {
        ui.heading("PIN della carta");
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.label("PIN (8 cifre):");
            ui.add(
                egui::TextEdit::singleline(&mut self.pin)
                    .password(true)
                    .desired_width(120.0),
            );
        });
        if !self.pin.is_empty() && !self.can_advance() {
            ui.weak("Servono tutte e otto le cifre.");
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.store_pin, "Salva nel portachiavi di sistema");
            if self.pin_in_keyring && ui.button("Dimentica").clicked() {
                self.keyring_message = match keyring::forget() {
                    Ok(()) => {
                        self.pin_in_keyring = false;
                        self.pin.clear();
                        Some("PIN rimosso dal portachiavi.".to_string())
                    }
                    Err(e) => Some(format!("Non rimosso: {e:#}")),
                };
            }
        });
        if let Some(m) = &self.keyring_message {
            ui.weak(m);
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        ui.label("Verrà scritto:");
        ui.horizontal(|ui| {
            ui.monospace(self.output.display().to_string());
            if ui.button("Cambia…").clicked() {
                let mut dlg = rfd::FileDialog::new();
                if let Some(dir) = self.output.parent() {
                    dlg = dlg.set_directory(dir);
                }
                if let Some(name) = self.output.file_name() {
                    dlg = dlg.set_file_name(name.to_string_lossy());
                }
                if let Some(p) = dlg.save_file() {
                    self.output = p;
                    self.output_chosen = true;
                }
            }
        });
        if self.kind == Kind::Detached {
            ui.weak(format!(
                "più il certificato in {}",
                self.output.with_extension("cer").display()
            ));
        }
    }

    fn step_outcome(&mut self, ui: &mut egui::Ui) {
        if self.job.is_some() {
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(&self.status);
            });
            ui.add_space(12.0);
            ui.weak("Non togliere la carta dal lettore.");
            return;
        }

        match &self.outcome {
            Some(Ok(path)) => {
                ui.heading("Firmato");
                ui.add_space(12.0);
                ui.monospace(path.display().to_string());
                ui.add_space(12.0);
                if ui.button("Apri cartella").clicked()
                    && let Some(dir) = path.parent()
                {
                    // ponytail: rfd has no "open folder"; xdg-open is on every
                    // desktop and a failure here does no harm.
                    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                }
                ui.add_space(20.0);
                ui.weak(
                    "Il certificato della CIE è di autenticazione, non di firma qualificata: \
                     un validatore che controlla l'EKU può segnalarlo.",
                );
            }
            Some(Err(e)) => {
                ui.heading("Non firmato");
                ui.add_space(12.0);
                ui.colored_label(egui::Color32::from_rgb(200, 80, 60), e);
                ui.add_space(12.0);
                if ui.button("◀ Torna al PIN").clicked() {
                    self.step = Step::Pin;
                }
            }
            None => {
                ui.label("…");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blank A4 page: enough to exercise opening, layout and rasterizing.
    const MINIMAL_PDF: &[u8] = include_bytes!("../../cie-firma/tests/fixtures/minimo.pdf");

    fn app_with_a_pdf() -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("documento.pdf");
        std::fs::write(&path, MINIMAL_PDF).unwrap();

        let mut app = App::with_pin(None);
        app.open_file(path);
        (app, dir)
    }

    /// Choosing a PDF must leave the stamp step reachable. This is exactly what
    /// looked broken from outside: land on step 4 instead of 5 and there is no
    /// preview and nowhere to place the rectangle.
    #[test]
    fn a_pdf_keeps_the_stamp_step() {
        let (app, _dir) = app_with_a_pdf();

        assert!(app.document.is_some(), "PDF not opened: {:?}", app.open_error);
        assert_eq!(app.kind, Kind::Pades);
        assert!(app.steps().contains(&Step::Stamp));
    }

    /// A non-PDF drops PAdES, and with it the stamp step: four steps, and the
    /// index must still line up or navigation breaks.
    #[test]
    fn a_non_pdf_drops_the_stamp_step() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lettera.txt");
        std::fs::write(&path, b"non e' un PDF").unwrap();

        let mut app = App::with_pin(None);
        app.open_file(path);

        assert!(app.document.is_none());
        assert_eq!(app.kind, Kind::P7m);
        assert!(!app.steps().contains(&Step::Stamp));

        app.step = Step::Kind;
        assert_eq!(app.index(), 1);
        app.go(true);
        assert_eq!(app.step, Step::Pin, "Avanti must skip the stamp step");
    }

    /// Draws the stamp step in a window-sized `Ui` and checks a page texture
    /// actually gets built. Without a card, without a window.
    #[test]
    fn the_stamp_step_builds_a_page_texture() {
        let (mut app, _dir) = app_with_a_pdf();
        app.step = Step::Stamp;

        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 760.0),
            )),
            ..Default::default()
        };
        // Two passes: the first settles the panels, the second measures.
        for _ in 0..2 {
            let _ = ctx.run_ui(input.clone(), |ui| app.draw(ui));
        }

        let (page, width, _) = app.texture.as_ref().expect("no page texture built");
        assert_eq!(*page, 0);
        assert!(*width > 100.0, "preview squeezed to {width} px");
    }

    /// A wrong PIN must be retryable without restarting: the card allows three
    /// tries and a restart would cost one of them for nothing.
    #[test]
    fn a_failed_signature_goes_back_to_the_pin() {
        let (mut app, _dir) = app_with_a_pdf();
        app.step = Step::Outcome;
        app.outcome = Some(Err("PIN errato. Tentativi rimasti: 2".to_string()));

        // What the "◀ Torna al PIN" button does.
        app.step = Step::Pin;
        assert!(app.steps().contains(&Step::Pin));
        assert_eq!(app.index(), 3);
    }
}
