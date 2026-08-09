//! Signing with the key that lives on the CIE.
//!
//! Sits above `cie-lib` (the bare card layer) and below anything with a face:
//! `cie-cli` and `cie-gui` both sign through here. This layer exists on its own
//! because `underskrift` drags in lopdf and a font parser — things that have
//! nothing to do with an APDU, and that the future PKCS#11 module must not
//! inherit just to expose `C_Sign`.
//!
//! Three containers, one signer:
//!
//! - [`sign_pdf`] — PAdES B-B, the signature goes inside the PDF
//! - [`sign_p7m`] — CAdES enveloping, content and signature in a single DER
//! - [`sign_detached`] — the raw RSA signature, the brick under the other two

mod cades;
mod detached;
pub mod keyring;
mod pades;
mod signer;

pub use cades::sign_p7m;
pub use detached::sign_detached;
pub use pades::{preview_stamp_lines, sign_pdf, stamp_lines, PadesOptions};
pub use signer::{card_id, identify, open_and_authenticate, CieSigner, Step};

#[cfg(any(test, feature = "test-signer"))]
pub use signer::fake;

// Callers need the trait in scope to hand out a `&dyn CryptoSigner`.
pub use underskrift::CryptoSigner;
// The preview draws the stamp itself, so it needs the line type.
pub use underskrift::TextLine;
