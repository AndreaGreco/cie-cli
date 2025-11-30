//! # CIE-Lib
//!
//! Libreria Rust per interagire con la Carta d'Identità Elettronica (CIE) italiana.
//!
//! Questa libreria fornisce un'interfaccia di alto livello per:
//! - Connessione e gestione della CIE tramite PKCS#11
//! - Firma digitale di documenti
//! - Autenticazione
//! - Lettura dei dati dalla carta
//!
//! ## Architettura
//!
//! La libreria è progettata per essere utilizzata da diversi frontend:
//! - CLI (Command Line Interface)
//! - GUI (Graphical User Interface) - future
//! - API Server - future
//!
//! ## Esempio
//!
//! ```rust,no_run
//! use cie_lib::Cie;
//!
//! fn main() -> anyhow::Result<()> {
//!     let cie = Cie::new()?;
//!     let slots = cie.list_readers()?;
//!     println!("Lettori trovati: {:?}", slots);
//!     Ok(())
//! }
//! ```

pub mod error;
pub mod cie;
pub mod pkcs11;
pub mod config;
pub mod titolare;

// Re-export principali
pub use cie::{Cie, CieSession, ReaderInfo, CieCertificate, SigningMechanism};
pub use error::{CieError, Result};
pub use config::CieConfig;
pub use titolare::DatiTitolare;
