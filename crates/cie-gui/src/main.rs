//! Interfaccia grafica per firmare documenti con la CIE.
//!
//! Stessa carta e stesse firme di `cie`, ma con l'anteprima del PDF e il
//! riquadro che si sceglie trascinandolo. Il binario e' separato apposta: chi
//! usa la riga di comando non si porta dietro un toolkit grafico.

#![windows_subsystem = "windows"]

mod anteprima;
mod lavoro;
mod portachiavi;
mod wizard;

fn main() -> eframe::Result {
    let opzioni = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 760.0])
            .with_min_inner_size([560.0, 420.0])
            .with_title("Firma con CIE"),
        ..Default::default()
    };

    eframe::run_native(
        "Firma con CIE",
        opzioni,
        Box::new(|cc| Ok(Box::new(wizard::App::new(cc)))),
    )
}
