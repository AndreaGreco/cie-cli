//! Gestione degli errori per cie-lib

use thiserror::Error;

/// Tipo Result specializzato per cie-lib
pub type Result<T> = std::result::Result<T, CieError>;

/// Errori possibili durante l'uso della CIE.
///
/// Le varianti distinguono *di chi* e' la colpa, perche' la risposta e' diversa:
/// la carta ha rifiutato ([`CieError::Card`]), il lettore non risponde
/// ([`CieError::Transport`]), oppure abbiamo costruito male noi un comando
/// ([`CieError::Protocol`]).
#[derive(Error, Debug)]
pub enum CieError {
    #[error("Nessun lettore di smart card trovato")]
    NoReaderFound,

    #[error("Nessuna CIE inserita nel lettore")]
    NoCiePresent,

    /// Errore del livello PC/SC: lettore scollegato, daemon assente, carta estratta.
    #[error("Errore di comunicazione con il lettore: {0}")]
    Transport(String),

    /// La carta c'e' ma non risponde ancora: appena resettata, o un lettore
    /// NFC che la sta riattivando. Passa da sola in qualche centinaio di
    /// millisecondi, quindi chi apre la connessione riprova invece di arrendersi.
    #[error("La carta non e' ancora pronta: {0}")]
    NotReady(String),

    /// La carta ha risposto con uno status word diverso da 9000.
    #[error("La carta ha risposto {sw:04X}")]
    Card { sw: u16 },

    /// Struttura APDU/ASN.1 inattesa: di norma un bug nostro, non della carta.
    #[error("Errore di protocollo: {0}")]
    Protocol(String),

    #[error("Input non valido: {0}")]
    InvalidInput(String),

    #[error("PIN errato. Tentativi rimasti: {0}")]
    PinIncorrect(u32),

    #[error("PIN bloccato. Serve il PUK per sbloccarlo")]
    PinBlocked,

    #[error("Secure Messaging non attivo")]
    SmNotActive,

    /// Il MAC calcolato non coincide con quello della carta: canale compromesso
    /// oppure SSC/chiavi di sessione disallineati.
    #[error("MAC della risposta non valido: canale SM non integro")]
    MacMismatch,

    /// La carta non ha superato l'autenticazione mutua: non e' una CIE genuina,
    /// oppure il canale e' stato manomesso.
    #[error("Autenticazione del chip fallita")]
    ChipAuthFailed,

    #[error("Errore I/O: {0}")]
    IoError(#[from] std::io::Error),
}

impl CieError {
    /// Vale la pena riprovare da capo, con una connessione nuova.
    ///
    /// Solo prima che il canale sicuro sia aperto: a meta' sessione una carta
    /// resettata ha perso le chiavi, e riprovare il comando non la riporta indietro.
    pub fn is_transient(&self) -> bool {
        matches!(self, CieError::NotReady(_) | CieError::NoCiePresent)
    }
}
