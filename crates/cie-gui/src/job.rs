//! The card works off the window thread.
//!
//! The handshake costs about a second, signing a bit more: doing it inside
//! `update()` would freeze the window long enough for the compositor to
//! declare it dead.

use anyhow::{Context, Result};
use cie_firma::{sign_detached, sign_p7m, sign_pdf, PadesOptions, Step};
use cie_lib::{CardStatus, PcscContext, ReaderWatch};
use iced::futures::{channel::mpsc, Stream};
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
    pub pin: Pin,
    pub output: PathBuf,
    pub options: PadesOptions,
}

/// Where the PIN comes from.
pub enum Pin {
    /// Typed into the panel, and already in hand.
    Typed(String),
    /// In the keyring, under this card id.
    ///
    /// Reading it happens on the signing thread and not in `update()`: it puts
    /// the system's unlock prompt on screen, and the window thread cannot sit
    /// waiting for someone to type a password into another process.
    Stored(String),
}

pub enum Message {
    /// Where we are, to be shown as is.
    Progress(String),
    /// Done: the file written (for a detached signature, the signature file),
    /// and whoever the certificate turned out to belong to.
    ///
    /// The holder rides back with the result because this is the only moment
    /// it is knowable: the certificate lives behind the PIN, and the signer
    /// that held it is dropped as this thread ends.
    Done {
        path: PathBuf,
        holder: Option<cie_lib::DatiTitolare>,
    },
    /// The error chain, already formatted.
    Failed(String),
}

/// How long before trying PC/SC again after it went away: no pcscd running,
/// or the service restarting under us.
const RETRY: std::time::Duration = std::time::Duration::from_secs(2);

/// What the reader is doing, as a stream the event loop waits on.
///
/// PC/SC is blocking and has no async side: `SCardGetStatusChange` parks the
/// calling thread, and tokio would only park the same thread under a different
/// name. So the OS thread stays, and pushes into the channel the runtime is
/// already polling - which is what removes the timer, not an async runtime.
pub fn watch() -> impl Stream<Item = CardStatus> {
    let (tx, rx) = mpsc::unbounded();
    std::thread::spawn(move || {
        loop {
            if let Ok(mut watch) = ReaderWatch::new() {
                while let Ok(status) = watch.wait() {
                    if tx.unbounded_send(status).is_err() {
                        return; // window closed
                    }
                }
            }
            // No pcscd, or it just went away: there is no reader to speak of
            // until it comes back.
            if tx.unbounded_send(CardStatus::NoReader).is_err() {
                return;
            }
            std::thread::sleep(RETRY);
        }
    });
    rx
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
            Ok((path, holder)) => tx.send(Message::Done { path, holder }),
            Err(e) => tx.send(Message::Failed(format!("{e:#}"))),
        };
    });
    rx
}

fn run(r: &Request, tx: &Sender<Message>) -> Result<(PathBuf, Option<cie_lib::DatiTitolare>)> {
    let progress = |s: Step| {
        let text = match s {
            Step::Connecting => "Connessione alla carta…",
            Step::SecureChannel => "Apertura del canale sicuro…",
            Step::Pin => "Verifica del PIN…",
            Step::Certificate => "Lettura del certificato…",
        };
        let _ = tx.send(Message::Progress(text.to_string()));
    };

    // Before the reader is touched: if the keyring says no, there is no point
    // having opened a secure channel.
    let pin = match &r.pin {
        Pin::Typed(pin) => pin.clone(),
        Pin::Stored(card) => {
            let _ = tx.send(Message::Progress("Sblocco del portachiavi…".to_string()));
            cie_firma::keyring::read_with_prompt(card)
                .map(|entry| entry.pin)
                .context("il portachiavi non ha restituito il PIN")?
        }
    };

    let ctx = PcscContext::new()?;
    let signer = cie_firma::open_and_authenticate(&ctx, &pin, &progress)?;

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

    let holder = cie_lib::DatiTitolare::from_certificate_der(
        cie_firma::CryptoSigner::certificate_der(&signer),
    );
    Ok((r.output.clone(), holder))
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
