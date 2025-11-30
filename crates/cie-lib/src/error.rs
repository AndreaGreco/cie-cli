//! Gestione degli errori per cie-lib

use thiserror::Error;

/// Tipo Result specializzato per cie-lib
pub type Result<T> = std::result::Result<T, CieError>;

/// Errori possibili durante l'uso della CIE
#[derive(Error, Debug)]
pub enum CieError {
    /// La libreria PKCS#11 non è stata trovata
    #[error("Libreria PKCS#11 non trovata: {path}")]
    Pkcs11LibraryNotFound { path: String },

    /// Errore durante l'inizializzazione PKCS#11
    #[error("Errore inizializzazione PKCS#11: {0}")]
    Pkcs11InitError(String),

    /// Errore caricamento libreria
    #[error("Libreria non trovata: {0}")]
    LibraryNotFound(String),

    /// Input non valido
    #[error("Input non valido: {0}")]
    InvalidInput(String),

    /// Nessun lettore di smart card trovato
    #[error("Nessun lettore di smart card trovato")]
    NoReaderFound,

    /// Nessuna CIE inserita nel lettore
    #[error("Nessuna CIE inserita nel lettore")]
    NoCiePresent,

    /// CIE non abilitata - necessario eseguire prima `cie enable`
    #[error("CIE non abilitata. Esegui prima: cie enable")]
    CieNotEnabled,

    /// PIN errato
    #[error("PIN errato. Tentativi rimasti: {attempts_left}")]
    WrongPin { attempts_left: u32 },

    /// PIN errato (formato alternativo per FFI)
    #[error("PIN errato. Tentativi rimasti: {0}")]
    PinIncorrect(u32),

    /// PIN bloccato
    #[error("PIN bloccato. Usa il PUK per sbloccare")]
    PinBlocked,

    /// PIN bloccato (alias per FFI)
    #[error("PIN bloccato permanentemente")]
    PinLocked,

    /// Errore durante la firma
    #[error("Errore durante la firma: {0}")]
    SigningError(String),

    /// Errore durante la lettura del certificato
    #[error("Errore lettura certificato: {0}")]
    CertificateError(String),

    /// Sessione non valida o scaduta
    #[error("Sessione non valida o scaduta")]
    InvalidSession,

    /// Operazione annullata dall'utente
    #[error("Operazione annullata")]
    Cancelled,

    /// Errore generico PKCS#11
    #[error("Errore PKCS#11: {0}")]
    Pkcs11Error(#[from] cryptoki::error::Error),

    /// Errore I/O
    #[error("Errore I/O: {0}")]
    IoError(#[from] std::io::Error),
}

impl CieError {
    /// Converte un errore PKCS#11 in stringa
    pub fn pkcs11_str(msg: impl Into<String>) -> Self {
        CieError::Pkcs11Error(cryptoki::error::Error::LibraryLoading(
            libloading::Error::DlOpen { desc: msg.into() }
        ))
    }
}
