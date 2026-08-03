//! Anteprima del PDF e conversione del riquadro in punti PDF.
//!
//! Qui vive l'unico calcolo non banale della GUI. Il riquadro lo si trascina
//! sull'immagine che si vede, ma `SignatureRect::Absolute` di underskrift
//! finisce dritto nel `/Rect` dell'annotazione: **user space non ruotato,
//! origine in basso a sinistra**. L'immagine invece e' ruotata secondo
//! `/Rotate`, ritagliata sul crop box, e ha l'origine in alto a sinistra.
//!
//! Ignorare rotazione e crop box mette la firma nel posto sbagliato senza
//! nessun errore visibile — e i PDF scansionati hanno spesso entrambi.

use anyhow::{anyhow, Result};
use eframe::egui;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::page::Rotation;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{RenderCache, RenderSettings};
use std::sync::Arc;

/// Un PDF aperto, pronto per l'anteprima.
///
/// I byte non li possiede: sono gli stessi che andranno alla carta, e li tiene
/// il wizard in un `Arc` che hayro condivide senza copiarli.
pub struct Documento {
    pdf: Pdf,
}

/// Quel che serve per riportare un punto dell'immagine in punti PDF.
#[derive(Debug, Clone, Copy)]
pub struct Geometria {
    /// Il crop box effettivo in punti PDF: `[x0, y0, x1, y1]`, y verso l'alto.
    pub crop: [f32; 4],
    /// La rotazione dichiarata dalla pagina, in senso orario.
    pub rotazione: Rotation,
}

impl Documento {
    pub fn apri(byte: &Arc<Vec<u8>>) -> Result<Self> {
        let pdf = Pdf::new(byte.clone()).map_err(|e| anyhow!("PDF illeggibile: {e:?}"))?;
        if pdf.pages().is_empty() {
            return Err(anyhow!("il PDF non ha pagine"));
        }
        Ok(Self { pdf })
    }

    pub fn n_pagine(&self) -> usize {
        self.pdf.pages().len()
    }

    pub fn geometria(&self, pagina: usize) -> Geometria {
        let page = &self.pdf.pages()[pagina];
        let c = page.intersected_crop_box();
        Geometria {
            crop: [c.x0 as f32, c.y0 as f32, c.x1 as f32, c.y1 as f32],
            rotazione: page.rotation(),
        }
    }

    /// Proporzioni della pagina come la si vede (rotazione inclusa).
    pub fn proporzioni(&self, pagina: usize) -> f32 {
        let (w, h) = self.pdf.pages()[pagina].render_dimensions();
        if h > 0.0 {
            w / h
        } else {
            1.0
        }
    }

    /// Rasterizza una pagina alla larghezza data, in pixel.
    pub fn rasterizza(&self, pagina: usize, larghezza_px: f32) -> egui::ColorImage {
        let page = &self.pdf.pages()[pagina];
        let (larghezza_pt, _) = page.render_dimensions();
        let scala = (larghezza_px / larghezza_pt.max(1.0)).clamp(0.05, 8.0);

        let pixmap = hayro::render(
            page,
            &RenderCache::new(),
            &InterpreterSettings::default(),
            &RenderSettings {
                x_scale: scala,
                y_scale: scala,
                // Senza sfondo la pagina arriva trasparente e sul tema scuro
                // il documento non si legge.
                bg_color: WHITE,
                ..Default::default()
            },
        );

        egui::ColorImage::from_rgba_premultiplied(
            [pixmap.width() as usize, pixmap.height() as usize],
            pixmap.data_as_u8_slice(),
        )
    }
}

/// Converte un riquadro espresso in frazioni dell'immagine (`0..1`, y **verso
/// il basso**) nel `[llx, lly, urx, ury]` in punti PDF che vuole l'annotazione.
pub fn punti_pdf(selezione: [f32; 4], g: &Geometria) -> [f32; 4] {
    let (ax, ay) = punto(selezione[0], selezione[1], g);
    let (bx, by) = punto(selezione[2], selezione[3], g);

    // Rotazione e ribaltamento della y possono scambiare gli angoli: il /Rect
    // vuole comunque prima l'angolo in basso a sinistra.
    [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)]
}

/// Un singolo punto: si annulla la rotazione, poi si esce dal crop box.
fn punto(fx: f32, fy: f32, g: &Geometria) -> (f32, f32) {
    // (u, v) sono le frazioni sulla pagina *non* ruotata, v ancora dall'alto.
    let (u, v) = match g.rotazione {
        Rotation::None => (fx, fy),
        Rotation::Horizontal => (fy, 1.0 - fx),
        Rotation::Flipped => (1.0 - fx, 1.0 - fy),
        Rotation::FlippedHorizontal => (1.0 - fy, fx),
    };

    let [x0, y0, x1, y1] = g.crop;
    (x0 + u * (x1 - x0), y1 - v * (y1 - y0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un crop box traslato: se qualcuno dimentica l'origine, qui si vede.
    fn geometria(rotazione: Rotation) -> Geometria {
        Geometria {
            crop: [10.0, 20.0, 605.0, 862.0], // 595 × 842, spostato di (10, 20)
            rotazione,
        }
    }

    fn quasi(a: [f32; 4], atteso: [f32; 4]) {
        for (i, (x, y)) in a.iter().zip(atteso.iter()).enumerate() {
            assert!(
                (x - y).abs() < 0.01,
                "componente {i}: {a:?} invece di {atteso:?}"
            );
        }
    }

    #[test]
    fn tutta_la_pagina_e_tutto_il_crop_box_in_ogni_rotazione() {
        for rot in [
            Rotation::None,
            Rotation::Horizontal,
            Rotation::Flipped,
            Rotation::FlippedHorizontal,
        ] {
            quasi(
                punti_pdf([0.0, 0.0, 1.0, 1.0], &geometria(rot)),
                [10.0, 20.0, 605.0, 862.0],
            );
        }
    }

    /// Il quarto in alto a sinistra dell'immagine, seguito a mano per ognuna
    /// delle quattro rotazioni orarie.
    #[test]
    fn il_quarto_in_alto_a_sinistra_finisce_dove_deve() {
        let sel = [0.0, 0.0, 0.5, 0.5];

        // Nessuna rotazione: e' il quarto in alto a sinistra della pagina, e in
        // coordinate PDF "in alto" vuol dire y grande.
        quasi(
            punti_pdf(sel, &geometria(Rotation::None)),
            [10.0, 441.0, 307.5, 862.0],
        );

        // 90° orari: quel che si vede in alto a sinistra e' l'angolo in basso a
        // sinistra della pagina originale.
        quasi(
            punti_pdf(sel, &geometria(Rotation::Horizontal)),
            [10.0, 20.0, 307.5, 441.0],
        );

        // 180°: l'angolo opposto.
        quasi(
            punti_pdf(sel, &geometria(Rotation::Flipped)),
            [307.5, 20.0, 605.0, 441.0],
        );

        // 270° orari: l'angolo in alto a destra.
        quasi(
            punti_pdf(sel, &geometria(Rotation::FlippedHorizontal)),
            [307.5, 441.0, 605.0, 862.0],
        );
    }

    /// Un riquadro trascinato dal basso verso l'alto arriva con gli angoli
    /// invertiti: il `/Rect` deve uscirne comunque normalizzato.
    #[test]
    fn gli_angoli_escono_sempre_ordinati() {
        let dritto = punti_pdf([0.2, 0.3, 0.6, 0.8], &geometria(Rotation::None));
        let rovescio = punti_pdf([0.6, 0.8, 0.2, 0.3], &geometria(Rotation::None));
        assert_eq!(dritto, rovescio);
        assert!(dritto[0] < dritto[2] && dritto[1] < dritto[3]);
    }
}
