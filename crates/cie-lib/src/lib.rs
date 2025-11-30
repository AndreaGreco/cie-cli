//! # CIE-Lib
//!
//! Libreria Rust per interagire con la Carta d'Identità Elettronica (CIE) italiana
//! tramite comunicazione PC/SC diretta, senza dipendenze dal middleware ufficiale.
//!
//! ## Caratteristiche
//!
//! - Comunicazione diretta con la CIE tramite PC/SC
//! - Nessuna dipendenza da middleware Java o librerie proprietarie
//! - Lettura certificati e dati anagrafici
//! - Firma digitale (in sviluppo)
//!
//! ## Esempio
//!
//! ```rust,no_run
//! use cie_lib::{PcscContext, CieCard};
//!
//! fn main() -> anyhow::Result<()> {
//!     let ctx = PcscContext::new()?;
//!     let card = ctx.connect_to_cie()?;
//!     let cie = CieCard::new(&card)?;
//!
//!     let atr = card.get_atr()?;
//!     println!("ATR: {}", hex::encode(&atr));
//!     Ok(())
//! }
//! ```

pub mod error;
pub mod titolare;
pub mod pcsc_direct;
pub mod apdu;

// Re-export principali
pub use error::{CieError, Result};
pub use titolare::DatiTitolare;
pub use pcsc_direct::{PcscContext, PcscCard};
pub use apdu::{CieCard, CieApdu, CieFileId, CieType};
