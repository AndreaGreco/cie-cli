//! Graphical front end for signing documents with the CIE.
//!
//! Same card and same signatures as `cie`, but with a PDF preview and a stamp
//! you place by dragging. The binary is separate on purpose: command-line users
//! do not drag a GUI toolkit along.

#![windows_subsystem = "windows"]

mod job;
mod keyring;
mod preview;
mod wizard;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 760.0])
            .with_min_inner_size([560.0, 420.0])
            .with_title("Firma con CIE"),
        ..Default::default()
    };

    eframe::run_native(
        "Firma con CIE",
        options,
        Box::new(|cc| Ok(Box::new(wizard::App::new(cc)))),
    )
}
