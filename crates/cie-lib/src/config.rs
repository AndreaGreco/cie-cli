//! Configurazione per cie-lib

use std::path::PathBuf;

/// Percorso di default della libreria PKCS#11 CIE su Linux
pub const DEFAULT_PKCS11_LIB_PATH: &str = "/usr/lib/libcie-pkcs11.so";

/// Percorsi alternativi dove cercare la libreria
pub const ALTERNATIVE_PKCS11_PATHS: &[&str] = &[
    "/usr/lib/pkcs11/libcie-pkcs11.so",
    "/usr/lib/libcie-pkcs11.so",
    "/usr/local/lib/libcie-pkcs11.so",
    "/usr/lib/x86_64-linux-gnu/libcie-pkcs11.so",
    "/usr/lib64/libcie-pkcs11.so",
];

/// Configurazione per la libreria CIE
#[derive(Debug, Clone)]
pub struct CieConfig {
    /// Percorso alla libreria PKCS#11
    pub pkcs11_lib_path: PathBuf,

    /// Timeout in secondi per le operazioni
    pub timeout_secs: u64,

    /// Abilita logging verboso
    pub verbose: bool,
}

impl Default for CieConfig {
    fn default() -> Self {
        Self {
            pkcs11_lib_path: PathBuf::from(DEFAULT_PKCS11_LIB_PATH),
            timeout_secs: 30,
            verbose: false,
        }
    }
}

impl CieConfig {
    /// Crea una nuova configurazione con il percorso specificato
    pub fn with_pkcs11_path(path: impl Into<PathBuf>) -> Self {
        Self {
            pkcs11_lib_path: path.into(),
            ..Default::default()
        }
    }

    /// Cerca automaticamente la libreria PKCS#11 nei percorsi noti
    pub fn auto_detect() -> Option<Self> {
        for path in ALTERNATIVE_PKCS11_PATHS {
            let path_buf = PathBuf::from(path);
            if path_buf.exists() {
                return Some(Self {
                    pkcs11_lib_path: path_buf,
                    ..Default::default()
                });
            }
        }
        None
    }

    /// Imposta il timeout
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Abilita il logging verboso
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }
}
