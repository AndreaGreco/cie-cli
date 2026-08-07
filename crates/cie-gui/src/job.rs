//! The card works off the window thread.
//!
//! The handshake costs about a second, signing a bit more: doing it inside
//! `update()` would freeze the window long enough for the compositor to
//! declare it dead.

use anyhow::{Context, Result};
use cie_firma::{sign_detached, sign_p7m, sign_pdf, PadesOptions, Step};
use cie_lib::PcscContext;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// PAdES: the signature goes inside the PDF.
    Pades,
    /// CAdES enveloping: document and signature in one `.p7m`.
    P7m,
    /// Raw RSA signature beside the file, plus the certificate.
    Detached,
}

/// Everything signing needs, already owned by the thread.
pub struct Request {
    pub bytes: Arc<Vec<u8>>,
    pub kind: Kind,
    pub pin: String,
    pub output: PathBuf,
    pub options: PadesOptions,
}

pub enum Message {
    /// Where we are, to be shown as is.
    Progress(String),
    /// Done: the file written (for a detached signature, the signature file).
    Done(PathBuf),
    /// The error chain, already formatted.
    Failed(String),
}

/// Asks the reader which card is on it, off the window thread.
///
/// Public files only, so no PIN is involved. It costs about half a second, and
/// the answer is what tells us whether this card's PIN is already known.
pub fn identify() -> Receiver<Option<String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let id = PcscContext::new()
            .ok()
            .and_then(|ctx| cie_firma::identify(&ctx).ok());
        let _ = tx.send(id);
    });
    rx
}

/// Starts signing on a thread and returns the channel to drain in `update`.
pub fn spawn(request: Request) -> Receiver<Message> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let outcome = run(&request, &tx);
        let _ = match outcome {
            Ok(path) => tx.send(Message::Done(path)),
            Err(e) => tx.send(Message::Failed(format!("{e:#}"))),
        };
    });
    rx
}

fn run(r: &Request, tx: &Sender<Message>) -> Result<PathBuf> {
    let progress = |s: Step| {
        let text = match s {
            Step::Connecting => "Connessione alla carta…",
            Step::SecureChannel => "Apertura del canale sicuro…",
            Step::Pin => "Verifica del PIN…",
            Step::Certificate => "Lettura del certificato…",
        };
        let _ = tx.send(Message::Progress(text.to_string()));
    };

    let ctx = PcscContext::new()?;
    let signer = cie_firma::open_and_authenticate(&ctx, &r.pin, &progress)?;

    let _ = tx.send(Message::Progress("Firma del documento…".to_string()));

    let produced = match r.kind {
        Kind::Pades => sign_pdf(&r.bytes, &signer, &r.options)?,
        Kind::P7m => sign_p7m(&r.bytes, &signer)?,
        Kind::Detached => {
            let signature = sign_detached(&r.bytes, &signer)?;
            // Without the certificate a detached signature is just bytes:
            // whoever has to check it has nothing to check against.
            let cert = r.output.with_extension("cer");
            std::fs::write(&cert, cie_firma::CryptoSigner::certificate_der(&signer))
                .with_context(|| format!("scrittura di {}", cert.display()))?;
            signature
        }
    };

    std::fs::write(&r.output, &produced)
        .with_context(|| format!("scrittura di {}", r.output.display()))?;

    Ok(r.output.clone())
}

/// The path proposed for the signed file, by signature kind.
///
/// Same suffixes as the CLI: the two tools must produce the same names, or you
/// end up with two copies of the same document.
pub fn default_output(source: &std::path::Path, kind: Kind) -> PathBuf {
    match kind {
        Kind::Pades => {
            let stem = source
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "documento".to_string());
            source.with_file_name(format!("{stem}-firmato.pdf"))
        }
        // `.p7m` and `.sig` append to the whole name, extension included:
        // `contratto.pdf.p7m` still says there is a PDF inside.
        Kind::P7m => append_extension(source, "p7m"),
        Kind::Detached => append_extension(source, "sig"),
    }
}

fn append_extension(path: &std::path::Path, ext: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(ext);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_proposed_names_are_the_cli_ones() {
        let f = Path::new("/tmp/contratto.pdf");
        assert_eq!(
            default_output(f, Kind::Pades),
            Path::new("/tmp/contratto-firmato.pdf")
        );
        assert_eq!(default_output(f, Kind::P7m), Path::new("/tmp/contratto.pdf.p7m"));
        assert_eq!(
            default_output(f, Kind::Detached),
            Path::new("/tmp/contratto.pdf.sig")
        );
    }

    /// A file with no extension must not lose its name along the way.
    #[test]
    fn it_copes_with_a_file_without_an_extension() {
        let f = Path::new("/tmp/lettera");
        assert_eq!(default_output(f, Kind::P7m), Path::new("/tmp/lettera.p7m"));
        assert_eq!(
            default_output(f, Kind::Pades),
            Path::new("/tmp/lettera-firmato.pdf")
        );
    }
}
