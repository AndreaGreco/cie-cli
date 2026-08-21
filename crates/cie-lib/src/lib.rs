//! # CIE-Lib
//!
//! Libreria Rust per interagire con la Carta d'Identità Elettronica (CIE) italiana
//! tramite comunicazione PC/SC diretta, senza dipendenze dal middleware ufficiale.
//!
//! ## Caratteristiche
//!
//! - Comunicazione diretta con la CIE tramite PC/SC
//! - Secure Messaging completo: Diffie-Hellman, autenticazione mutua DAPP,
//!   cifratura 3DES e Retail MAC verificato in entrambe le direzioni
//! - Nessun middleware, nessuna app Java, niente scritto su disco
//! - Lettura di certificati e dati anagrafici previa verifica del PIN
//!
//! ## Esempio
//!
//! I file protetti richiedono un canale sicuro attivo: il PIN da solo non basta,
//! e la CIE rifiuta la VERIFY finche' il canale non e' stabilito.
//!
//! ```rust,no_run
//! use cie_lib::{CieCard, CieType, PcscContext, Result, SecureMessaging};
//!
//! fn main() -> Result<()> {
//!     let ctx = PcscContext::new()?;
//!     let card = ctx.connect_to_cie()?;
//!
//!     // Seleziona l'applicazione giusta in base al tipo di carta.
//!     let cie_type = CieType::from_atr(&card.get_atr()?);
//!     CieCard::new(&card)?;
//!
//!     // Apre il canale: i primi due passi in chiaro, gli altri dentro l'SM.
//!     let mut sm = SecureMessaging::new(cie_type);
//!     sm.init_dh_params(&card)?;
//!     sm.init_ext_auth_key_param(&card)?;
//!     sm.dh_key_exchange(&card)?;
//!     sm.read_dapp_pub_key(&card)?;
//!     sm.dapp(&card)?;
//!
//!     sm.verify_pin(&card, "12345678")?;
//!     let certificato = sm.read_file(&card, 0x1003)?;
//!     println!("certificato: {} byte", certificato.len());
//!     Ok(())
//! }
//! ```

pub mod error;
pub mod titolare;
pub mod pcsc_direct;
pub mod presence;
pub mod apdu;
pub mod crypto;
pub mod asn1;
pub mod secure_messaging;

// Re-export principali
pub use error::{CieError, Result};
pub use titolare::DatiTitolare;
pub use pcsc_direct::{PcscContext, PcscCard};
pub use presence::{Presence, Watch};
pub use apdu::{CieCard, CieApdu, CieFileId, CieType};
pub use secure_messaging::SecureMessaging;
