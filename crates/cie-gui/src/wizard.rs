//! Il wizard: cinque passi, quattro se la firma non e' PAdES.

use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;

use cie_firma::OpzioniPades;

use crate::anteprima::{punti_pdf, Documento};
use crate::lavoro::{self, Messaggio, Richiesta, Tipo};
use crate::portachiavi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fase {
    File,
    Tipo,
    Riquadro,
    Pin,
    Esito,
}

impl Fase {
    fn titolo(self) -> &'static str {
        match self {
            Fase::File => "Documento",
            Fase::Tipo => "Tipo di firma",
            Fase::Riquadro => "Riquadro",
            Fase::Pin => "PIN",
            Fase::Esito => "Esito",
        }
    }
}

/// Il riquadro proposto se l'utente non ne trascina uno: in basso a destra,
/// come quello che `cie sign-pdf` usa da riga di comando.
const RIQUADRO_PREDEFINITO: [f32; 4] = [0.56, 0.87, 0.95, 0.96];

pub struct App {
    fase: Fase,

    percorso: Option<PathBuf>,
    byte: Option<Arc<Vec<u8>>>,
    documento: Option<Documento>,
    errore_apertura: Option<String>,

    tipo: Tipo,

    pagina: usize,
    /// Frazioni dell'immagine (`0..1`, y dall'alto). Diventa un `/Rect` solo al
    /// momento della firma, quando si conosce la geometria della pagina.
    riquadro: [f32; 4],
    trascino: Option<egui::Pos2>,
    texture: Option<(usize, f32, egui::TextureHandle)>,
    motivo: String,
    certifica: Option<u8>,

    pin: String,
    salva_pin: bool,
    pin_nel_portachiavi: bool,
    messaggio_portachiavi: Option<String>,

    uscita: PathBuf,
    uscita_scelta: bool,

    lavoro: Option<Receiver<Messaggio>>,
    stato: String,
    esito: Option<Result<PathBuf, String>>,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let pin = portachiavi::leggi();
        Self {
            fase: Fase::File,
            percorso: None,
            byte: None,
            documento: None,
            errore_apertura: None,
            tipo: Tipo::Pades,
            pagina: 0,
            riquadro: RIQUADRO_PREDEFINITO,
            trascino: None,
            texture: None,
            motivo: String::new(),
            certifica: None,
            pin_nel_portachiavi: pin.is_some(),
            salva_pin: pin.is_none(),
            pin: pin.unwrap_or_default(),
            messaggio_portachiavi: None,
            uscita: PathBuf::new(),
            uscita_scelta: false,
            lavoro: None,
            stato: String::new(),
            esito: None,
        }
    }

    /// I passi applicabili: il riquadro esiste solo per il PAdES.
    fn fasi(&self) -> &'static [Fase] {
        if self.tipo == Tipo::Pades {
            &[Fase::File, Fase::Tipo, Fase::Riquadro, Fase::Pin, Fase::Esito]
        } else {
            &[Fase::File, Fase::Tipo, Fase::Pin, Fase::Esito]
        }
    }

    fn indice(&self) -> usize {
        self.fasi().iter().position(|f| *f == self.fase).unwrap_or(0)
    }

    fn puo_avanzare(&self) -> bool {
        match self.fase {
            Fase::File => self.byte.is_some(),
            Fase::Tipo => self.tipo != Tipo::Pades || self.documento.is_some(),
            Fase::Riquadro => true,
            // Otto cifre si controllano qui, prima di toccare la carta: il
            // contatore e' a tre tentativi e non va speso su una svista.
            Fase::Pin => self.pin.len() == 8 && self.pin.chars().all(|c| c.is_ascii_digit()),
            Fase::Esito => false,
        }
    }

    fn vai(&mut self, avanti: bool) {
        let fasi = self.fasi();
        let i = self.indice();
        let prossima = if avanti {
            fasi.get(i + 1).copied()
        } else {
            i.checked_sub(1).and_then(|j| fasi.get(j).copied())
        };
        if let Some(f) = prossima {
            self.fase = f;
            if f == Fase::Pin {
                self.aggiorna_uscita();
            }
            if f == Fase::Esito {
                self.parti();
            }
        }
    }

    fn aggiorna_uscita(&mut self) {
        if self.uscita_scelta {
            return;
        }
        if let Some(p) = &self.percorso {
            self.uscita = lavoro::uscita_predefinita(p, self.tipo);
        }
    }

    fn apri_file(&mut self, percorso: PathBuf) {
        self.errore_apertura = None;
        self.documento = None;
        self.texture = None;
        self.pagina = 0;
        self.riquadro = RIQUADRO_PREDEFINITO;
        self.uscita_scelta = false;

        match std::fs::read(&percorso) {
            Ok(byte) => {
                let e_pdf = byte.starts_with(b"%PDF-");
                self.byte = Some(Arc::new(byte));

                if e_pdf {
                    match Documento::apri(self.byte.as_ref().expect("appena assegnato")) {
                        Ok(d) => self.documento = Some(d),
                        Err(e) => self.errore_apertura = Some(format!("{e:#}")),
                    }
                }
                // Un file che non e' un PDF, o un PDF che hayro non sa aprire,
                // si firma comunque: solo non in PAdES.
                if self.documento.is_none() && self.tipo == Tipo::Pades {
                    self.tipo = Tipo::P7m;
                }
                self.percorso = Some(percorso);
            }
            Err(e) => {
                self.byte = None;
                self.percorso = None;
                self.errore_apertura = Some(format!("lettura del file: {e}"));
            }
        }
        self.aggiorna_uscita();
    }

    fn parti(&mut self) {
        self.esito = None;
        self.stato = "Avvio…".to_string();

        let Some(byte) = self.byte.clone() else {
            self.esito = Some(Err("nessun documento".to_string()));
            return;
        };

        let opzioni = match &self.documento {
            Some(doc) if self.tipo == Tipo::Pades => OpzioniPades {
                pagina: self.pagina as u32,
                riquadro: punti_pdf(self.riquadro, &doc.geometria(self.pagina)),
                motivo: (!self.motivo.trim().is_empty()).then(|| self.motivo.trim().to_string()),
                certifica: self.certifica,
            },
            _ => OpzioniPades::default(),
        };

        if self.salva_pin {
            // Un portachiavi che rifiuta non deve impedire la firma: si dice e
            // si tira dritto.
            if let Err(e) = portachiavi::salva(&self.pin) {
                self.messaggio_portachiavi = Some(format!("PIN non salvato: {e:#}"));
            } else {
                self.pin_nel_portachiavi = true;
                self.salva_pin = false;
            }
        }

        self.lavoro = Some(lavoro::avvia(Richiesta {
            byte,
            tipo: self.tipo,
            pin: self.pin.clone(),
            uscita: self.uscita.clone(),
            opzioni,
        }));
    }

    /// Svuota il canale del thread della carta. Restituisce `true` se c'e'
    /// ancora lavoro in corso, cioe' se serve un altro giro di disegno.
    fn raccogli(&mut self) -> bool {
        let Some(rx) = &self.lavoro else {
            return false;
        };
        loop {
            match rx.try_recv() {
                Ok(Messaggio::Passo(s)) => self.stato = s,
                Ok(Messaggio::Fatto(p)) => {
                    self.esito = Some(Ok(p));
                    self.lavoro = None;
                    return false;
                }
                Ok(Messaggio::Errore(e)) => {
                    self.esito = Some(Err(e));
                    self.lavoro = None;
                    return false;
                }
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => {
                    if self.esito.is_none() {
                        self.esito = Some(Err("il thread della carta e' morto senza dire perche'"
                            .to_string()));
                    }
                    self.lavoro = None;
                    return false;
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.raccogli() {
            ui.ctx().request_repaint();
        }

        egui::Panel::top("passi").show(ui, |ui| {
            ui.add_space(6.0);
            briciole(ui, self.fasi(), self.indice());
            ui.add_space(6.0);
        });

        egui::Panel::bottom("navigazione").show(ui, |ui| {
            ui.add_space(6.0);
            self.navigazione(ui);
            ui.add_space(6.0);
        });

        egui::CentralPanel::default().show(ui, |ui| match self.fase {
            Fase::File => self.passo_file(ui),
            Fase::Tipo => self.passo_tipo(ui),
            Fase::Riquadro => self.passo_riquadro(ui),
            Fase::Pin => self.passo_pin(ui),
            Fase::Esito => self.passo_esito(ui),
        });
    }
}

/// La striscia dei passi in cima.
fn briciole(ui: &mut egui::Ui, fasi: &[Fase], corrente: usize) {
    ui.horizontal(|ui| {
        for (i, f) in fasi.iter().enumerate() {
            if i > 0 {
                ui.label("─");
            }
            let testo = format!("{} {}", i + 1, f.titolo());
            if i == corrente {
                ui.strong(testo);
            } else {
                ui.weak(testo);
            }
        }
    });
}

impl App {
    fn navigazione(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let primo = self.indice() == 0;
            if ui.add_enabled(!primo, egui::Button::new("◀ Indietro")).clicked() {
                self.vai(false);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match self.fase {
                    Fase::Esito => {
                        if self.lavoro.is_none()
                            && ui.button("Firma un altro documento").clicked()
                        {
                            self.fase = Fase::File;
                            self.percorso = None;
                            self.byte = None;
                            self.documento = None;
                            self.texture = None;
                            self.esito = None;
                            self.uscita_scelta = false;
                        }
                    }
                    _ => {
                        let ultimo_prima_dellesito = self.fase == Fase::Pin;
                        let etichetta = if ultimo_prima_dellesito {
                            "Firma ▶"
                        } else {
                            "Avanti ▶"
                        };
                        if ui
                            .add_enabled(self.puo_avanzare(), egui::Button::new(etichetta))
                            .clicked()
                        {
                            self.vai(true);
                        }
                    }
                }
            });
        });
    }

    fn passo_file(&mut self, ui: &mut egui::Ui) {
        ui.heading("Quale documento vuoi firmare?");
        ui.add_space(12.0);

        if ui.button("Scegli un documento…").clicked()
            && let Some(p) = rfd::FileDialog::new().pick_file()
        {
            self.apri_file(p);
        }

        ui.add_space(12.0);
        if let (Some(p), Some(b)) = (&self.percorso, &self.byte) {
            ui.label(format!("{}", p.display()));
            ui.weak(format!("{} byte", b.len()));
            if self.documento.is_some() {
                ui.weak("PDF leggibile: si puo' firmare in PAdES.");
            } else {
                ui.weak("Non e' un PDF che sappiamo mostrare: resta CAdES o firma separata.");
            }
        }

        if let Some(e) = &self.errore_apertura {
            ui.add_space(8.0);
            ui.colored_label(egui::Color32::from_rgb(200, 80, 60), e);
        }
    }

    fn passo_tipo(&mut self, ui: &mut egui::Ui) {
        ui.heading("Che tipo di firma?");
        ui.add_space(12.0);

        let pdf_ok = self.documento.is_some();
        ui.add_enabled_ui(pdf_ok, |ui| {
            ui.radio_value(&mut self.tipo, Tipo::Pades, Tipo::Pades.etichetta());
        });
        if !pdf_ok {
            ui.weak("   (il PAdES vale solo per i PDF)");
        }
        ui.radio_value(&mut self.tipo, Tipo::P7m, Tipo::P7m.etichetta());
        ui.radio_value(&mut self.tipo, Tipo::Detached, Tipo::Detached.etichetta());

        self.uscita_scelta = false;
        self.aggiorna_uscita();
    }

    fn passo_riquadro(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &self.documento else {
            ui.label("nessun PDF aperto");
            return;
        };
        let n = doc.n_pagine();

        ui.horizontal(|ui| {
            if ui.add_enabled(self.pagina > 0, egui::Button::new("◀")).clicked() {
                self.pagina -= 1;
                self.texture = None;
            }
            ui.label(format!("Pagina {} / {}", self.pagina + 1, n));
            if ui.add_enabled(self.pagina + 1 < n, egui::Button::new("▶")).clicked() {
                self.pagina += 1;
                self.texture = None;
            }

            ui.separator();
            ui.label("Motivo:");
            ui.add(egui::TextEdit::singleline(&mut self.motivo).desired_width(160.0));

            ui.separator();
            let mut certifica = self.certifica.is_some();
            if ui.checkbox(&mut certifica, "Certifica").changed() {
                self.certifica = certifica.then_some(2);
            }
            if let Some(livello) = &mut self.certifica {
                egui::ComboBox::from_id_salt("livello")
                    .selected_text(match livello {
                        1 => "nessuna modifica",
                        3 => "moduli, firme e annotazioni",
                        _ => "moduli e altre firme",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(livello, 1, "nessuna modifica");
                        ui.selectable_value(livello, 2, "moduli e altre firme");
                        ui.selectable_value(livello, 3, "moduli, firme e annotazioni");
                    });
            }
        });

        ui.weak("Trascina sull'anteprima per spostare il riquadro della firma.");
        ui.add_space(6.0);

        self.disegna_anteprima(ui);
    }

    fn disegna_anteprima(&mut self, ui: &mut egui::Ui) {
        let doc = self.documento.as_ref().expect("controllato dal chiamante");

        // La pagina entra tutta nello spazio rimasto: prima si prova a
        // riempire la larghezza, poi si stringe se sfora in altezza.
        let spazio = ui.available_size();
        let proporzioni = doc.proporzioni(self.pagina);
        let mut larghezza = spazio.x.min(900.0);
        if larghezza / proporzioni > spazio.y {
            larghezza = spazio.y * proporzioni;
        }
        let larghezza = larghezza.max(80.0);
        let dimensioni = egui::vec2(larghezza, larghezza / proporzioni);

        // ponytail: si ridisegna solo oltre i 4 px di differenza. Rasterizzare
        // a ogni pixel di ridimensionamento fa scattare la finestra; se un
        // giorno servisse fluidita', qui va una cache per pagina.
        let da_rifare = match &self.texture {
            Some((p, l, _)) => *p != self.pagina || (l - larghezza).abs() > 4.0,
            None => true,
        };
        if da_rifare {
            let immagine = doc.rasterizza(self.pagina, larghezza * ui.ctx().pixels_per_point());
            let handle = ui.ctx().load_texture(
                "pagina",
                immagine,
                egui::TextureOptions::LINEAR,
            );
            self.texture = Some((self.pagina, larghezza, handle));
        }

        let (rect, risposta) = ui.allocate_exact_size(dimensioni, egui::Sense::drag());

        if let Some((_, _, handle)) = &self.texture {
            egui::Image::new(handle).paint_at(ui, rect);
        }

        // Dallo schermo alle frazioni dell'immagine, e ritorno.
        let frazione = |p: egui::Pos2| {
            egui::pos2(
                ((p.x - rect.min.x) / rect.width()).clamp(0.0, 1.0),
                ((p.y - rect.min.y) / rect.height()).clamp(0.0, 1.0),
            )
        };

        if risposta.drag_started()
            && let Some(p) = risposta.interact_pointer_pos()
        {
            self.trascino = Some(frazione(p));
        }
        if let (Some(inizio), Some(p)) = (self.trascino, risposta.interact_pointer_pos()) {
            let fine = frazione(p);
            self.riquadro = [inizio.x, inizio.y, fine.x, fine.y];
        }
        if risposta.drag_stopped() {
            self.trascino = None;
            // Un click senza trascinamento lascerebbe un riquadro di area zero,
            // che nessun lettore disegna: si torna a quello proposto.
            let [x0, y0, x1, y1] = self.riquadro;
            if (x1 - x0).abs() < 0.02 || (y1 - y0).abs() < 0.02 {
                self.riquadro = RIQUADRO_PREDEFINITO;
            }
        }

        let [x0, y0, x1, y1] = self.riquadro;
        let riquadro_schermo = egui::Rect::from_two_pos(
            rect.min + egui::vec2(x0 * rect.width(), y0 * rect.height()),
            rect.min + egui::vec2(x1 * rect.width(), y1 * rect.height()),
        );
        let pittore = ui.painter_at(rect);
        pittore.rect_filled(
            riquadro_schermo,
            2.0,
            egui::Color32::from_rgba_unmultiplied(60, 120, 220, 40),
        );
        pittore.rect_stroke(
            riquadro_schermo,
            2.0,
            egui::Stroke::new(1.5, egui::Color32::from_rgb(60, 120, 220)),
            egui::StrokeKind::Inside,
        );
    }

    fn passo_pin(&mut self, ui: &mut egui::Ui) {
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
        if !self.pin.is_empty() && !self.puo_avanzare() {
            ui.weak("Servono tutte e otto le cifre.");
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.salva_pin, "Salva nel portachiavi di sistema");
            if self.pin_nel_portachiavi && ui.button("Dimentica").clicked() {
                self.messaggio_portachiavi = match portachiavi::dimentica() {
                    Ok(()) => {
                        self.pin_nel_portachiavi = false;
                        self.pin.clear();
                        Some("PIN rimosso dal portachiavi.".to_string())
                    }
                    Err(e) => Some(format!("Non rimosso: {e:#}")),
                };
            }
        });
        if let Some(m) = &self.messaggio_portachiavi {
            ui.weak(m);
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        ui.label("Verra' scritto:");
        ui.horizontal(|ui| {
            ui.monospace(self.uscita.display().to_string());
            if ui.button("Cambia…").clicked() {
                let mut dlg = rfd::FileDialog::new();
                if let Some(dir) = self.uscita.parent() {
                    dlg = dlg.set_directory(dir);
                }
                if let Some(nome) = self.uscita.file_name() {
                    dlg = dlg.set_file_name(nome.to_string_lossy());
                }
                if let Some(p) = dlg.save_file() {
                    self.uscita = p;
                    self.uscita_scelta = true;
                }
            }
        });
        if self.tipo == Tipo::Detached {
            ui.weak(format!(
                "piu' il certificato in {}",
                self.uscita.with_extension("cer").display()
            ));
        }
    }

    fn passo_esito(&mut self, ui: &mut egui::Ui) {
        if self.lavoro.is_some() {
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(&self.stato);
            });
            ui.add_space(12.0);
            ui.weak("Non togliere la carta dal lettore.");
            return;
        }

        match &self.esito {
            Some(Ok(percorso)) => {
                ui.heading("Firmato");
                ui.add_space(12.0);
                ui.monospace(percorso.display().to_string());
                ui.add_space(12.0);
                if ui.button("Apri cartella").clicked()
                    && let Some(dir) = percorso.parent()
                {
                    // ponytail: `open` di rfd non esiste; xdg-open e' su
                    // qualunque desktop e un fallimento qui non e' un problema.
                    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                }
                ui.add_space(20.0);
                ui.weak(
                    "Il certificato della CIE e' di autenticazione, non di firma qualificata: \
                     un validatore che controlla l'EKU puo' segnalarlo.",
                );
            }
            Some(Err(e)) => {
                ui.heading("Non firmato");
                ui.add_space(12.0);
                ui.colored_label(egui::Color32::from_rgb(200, 80, 60), e);
                ui.add_space(12.0);
                if ui.button("◀ Torna al PIN").clicked() {
                    self.fase = Fase::Pin;
                }
            }
            None => {
                ui.label("…");
            }
        }
    }
}
