//! Firma con la chiave che vive sulla CIE.
//!
//! Sta sopra `cie-lib` (il livello carta puro) e sotto chi ha una faccia:
//! `cie-cli` e `cie-gui` firmano tutti e due passando di qui. La ragione per cui
//! questo livello esiste separato e' che `underskrift` si porta dietro lopdf e
//! un parser di font: roba che non ha niente a che fare con una APDU e che il
//! futuro modulo PKCS#11 non deve ereditare per esporre `C_Sign`.
//!
//! Tre contenitori, un solo firmatario:
//!
//! - [`firma_pdf`] — PAdES B-B, la firma finisce dentro il PDF
//! - [`firma_p7m`] — CAdES enveloping, contenuto e firma in un unico DER
//! - [`firma_detached`] — la firma RSA grezza, il mattone sotto gli altri due

mod cades;
mod detached;
mod firmatario;
mod pades;

pub use cades::firma_p7m;
pub use detached::firma_detached;
pub use firmatario::{apri_e_autentica, FirmatarioCie, Passo};
pub use pades::{firma_pdf, OpzioniPades};

// Chi firma ha bisogno del tratto per passare un `&dyn CryptoSigner`.
pub use underskrift::CryptoSigner;
