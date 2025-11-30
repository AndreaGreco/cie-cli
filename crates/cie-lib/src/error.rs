//! Gestione degli errori per cie-lib

use thiserror::Error;

/// Tipo Result specializzato per cie-lib
pub type Result<T> = std::result::Result<T, CieError>;

/// Errori possibili durante l'uso della CIE
#[derive(Error, Debug)]
pub enum CieError {
    #[error("Libreria PKCS#11 non trovata: {path}")]
    Pkcs11LibraryNotFound { path: String },

    #[error("Errore inizializzazione PKCS#11: {0}")]
    Pkcs11InitError(String),

    #[error("Libreria non trovata: {0}")]
    LibraryNotFound(String),

    #[error("Input non valido: {0}")]
    InvalidInput(String),

    #[error("Nessun lettore di smart card trovato")]
    NoReaderFound,

    #[error("Nessuna CIE inserita nel lettore")]
    NoCiePresent,

    #[error("CIE non abilitata. Esegui prima: cie enable")]
    CieNotEnabled,

    #[error("PIN errato. Tentativi rimasti: {attempts_left}")]
    WrongPin { attempts_left: u32 },

    #[error("PIN errato. Tentativi rimasti: {0}")]
    PinIncorrect(u32),

    #[error("PIN bloccato. Usa il PUK per sbloccare")]
    PinBlocked,

    #[error("PIN bloccato permanentemente")]
    PinLocked,

    #[error("Errore durante la firma: {0}")]
    SigningError(String),

    #[error("Errore lettura certificato: {0}")]
    CertificateError(String),

    #[error("Sessione non valida o scaduta")]
    InvalidSession,

    #[error("Operazione annullata")]
    Cancelled,

    #[error("Errore I/O: {0}")]
    IoError(#[from] std::io::Error),
}

impl CieError {
    pub fn pkcs11_msg(msg: impl Into<String>) -> Self {
        CieError::SigningError(msg.into())
    }
}
