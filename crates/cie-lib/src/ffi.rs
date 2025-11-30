//! FFI bindings for CIE middleware functions beyond PKCS#11
//!
//! Questo modulo contiene i bindings alle funzioni specifiche della libreria
//! cie-middleware-linux che non fanno parte dello standard PKCS#11.

use std::ffi::{c_char, c_int, c_void, CString};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::config::CieConfig;
use crate::error::{CieError, Result};

/// Codici di ritorno della libreria CIE
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CieReturnCode {
    Ok = 0,
    /// Errore generico
    GeneralError = 0x00000005,
    /// PIN non corretto
    PinIncorrect = 0x000000A0,
    /// PIN bloccato
    PinLocked = 0x000000A4,
    /// Carta già abilitata
    CardAlreadyEnabled = 0x000000F0,
    /// Carta non abilitata (per VerificaCIEAbilitata)
    CardNotEnabled = 0x000000F1,
}

impl From<u32> for CieReturnCode {
    fn from(value: u32) -> Self {
        match value {
            0 => CieReturnCode::Ok,
            0x000000A0 => CieReturnCode::PinIncorrect,
            0x000000A4 => CieReturnCode::PinLocked,
            0x000000F0 => CieReturnCode::CardAlreadyEnabled,
            0x000000F1 => CieReturnCode::CardNotEnabled,
            _ => CieReturnCode::GeneralError,
        }
    }
}

/// Tipo di firma per firmaConCIE
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum SignType {
    /// Firma PKCS#7 (.p7m)
    P7m = 0,
    /// Firma PDF
    Pdf = 1,
}

/// Callback per il progresso dell'operazione
pub type ProgressCallback = extern "C" fn(progress: c_int, message: *const c_char);

/// Callback per il completamento dell'operazione
pub type CompletedCallback = extern "C" fn(error: *const c_char);

/// Callback per il progresso dell'abilitazione CIE
pub type EnableProgressCallback = extern "C" fn(step: c_int, total: c_int);

// Funzioni FFI dalla libreria CIE
extern "C" {
    /// Verifica se una CIE è già abilitata
    /// Ritorna 0 se abilitata, CKR_TOKEN_NOT_PRESENT se non abilitata
    fn VerificaCIEAbilitata(pan: *const c_char) -> u32;

    /// Abilita una CIE per l'uso
    /// - pan: numero seriale della CIE (letto dalla carta)
    /// - pin: PIN completo a 8 cifre
    /// - attempts: puntatore per ricevere i tentativi rimanenti in caso di errore
    /// - progress_callback: callback per il progresso
    /// - completed_callback: callback per il completamento
    fn AbilitaCIE(
        pan: *const c_char,
        pin: *const c_char,
        attempts: *mut c_int,
        progress_callback: Option<ProgressCallback>,
        completed_callback: Option<CompletedCallback>,
    ) -> u32;

    /// Disabilita una CIE
    fn DisabilitaCIE(pan: *const c_char) -> u32;

    /// Firma un file con la CIE
    fn firmaConCIE(
        in_file_path: *const c_char,
        sign_type: c_int,
        pin: *const c_char,
        pan: *const c_char,
        page: c_int,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        image_path: *const c_char,
        out_file_path: *const c_char,
        progress_callback: Option<ProgressCallback>,
        completed_callback: Option<CompletedCallback>,
    ) -> u32;
}

// Variabili globali per memorizzare i callback results
static LAST_PROGRESS: AtomicU32 = AtomicU32::new(0);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

extern "C" fn default_progress_callback(progress: c_int, message: *const c_char) {
    LAST_PROGRESS.store(progress as u32, Ordering::SeqCst);
    if !message.is_null() {
        let msg = unsafe { std::ffi::CStr::from_ptr(message) };
        tracing::info!("Progresso: {}% - {:?}", progress, msg);
    } else {
        tracing::info!("Progresso: {}%", progress);
    }
}

extern "C" fn default_completed_callback(error: *const c_char) {
    if !error.is_null() {
        let err = unsafe { std::ffi::CStr::from_ptr(error) };
        let err_str = err.to_string_lossy().to_string();
        tracing::error!("Errore completamento: {}", err_str);
        if let Ok(mut guard) = LAST_ERROR.lock() {
            *guard = Some(err_str);
        }
    } else {
        tracing::info!("Operazione completata con successo");
    }
}

/// Interfaccia Rust per le funzioni CIE FFI
pub struct CieNative {
    _lib: libloading::Library,
}

impl CieNative {
    /// Carica la libreria CIE
    pub fn new() -> Result<Self> {
        let config = CieConfig::default();
        let lib_path = config.find_library()?;
        
        let lib = unsafe {
            libloading::Library::new(&lib_path)
                .map_err(|e| CieError::LibraryNotFound(format!("{}: {}", lib_path, e)))?
        };
        
        Ok(Self { _lib: lib })
    }

    /// Verifica se una CIE è già abilitata
    pub fn is_cie_enabled(&self, pan: &str) -> Result<bool> {
        let pan_c = CString::new(pan)
            .map_err(|_| CieError::InvalidInput("PAN contiene caratteri non validi".into()))?;
        
        let result = unsafe { VerificaCIEAbilitata(pan_c.as_ptr()) };
        
        match CieReturnCode::from(result) {
            CieReturnCode::Ok => Ok(true),
            CieReturnCode::CardNotEnabled => Ok(false),
            _ => Err(CieError::Pkcs11Error(format!("Errore verifica CIE: 0x{:08X}", result))),
        }
    }

    /// Abilita una CIE
    /// 
    /// # Arguments
    /// * `pan` - Numero seriale della CIE
    /// * `pin` - PIN completo a 8 cifre
    /// 
    /// # Returns
    /// * `Ok(())` se l'abilitazione ha successo
    /// * `Err(CieError::PinIncorrect(attempts))` se il PIN è sbagliato
    /// * `Err(CieError::PinLocked)` se il PIN è bloccato
    pub fn enable_cie(&self, pan: &str, pin: &str) -> Result<()> {
        if pin.len() != 8 || !pin.chars().all(|c| c.is_ascii_digit()) {
            return Err(CieError::InvalidInput(
                "Il PIN deve essere composto da 8 cifre".into()
            ));
        }

        let pan_c = CString::new(pan)
            .map_err(|_| CieError::InvalidInput("PAN contiene caratteri non validi".into()))?;
        let pin_c = CString::new(pin)
            .map_err(|_| CieError::InvalidInput("PIN contiene caratteri non validi".into()))?;
        
        let mut attempts: c_int = 0;
        
        // Reset error state
        if let Ok(mut guard) = LAST_ERROR.lock() {
            *guard = None;
        }
        LAST_PROGRESS.store(0, Ordering::SeqCst);
        
        let result = unsafe {
            AbilitaCIE(
                pan_c.as_ptr(),
                pin_c.as_ptr(),
                &mut attempts,
                Some(default_progress_callback),
                Some(default_completed_callback),
            )
        };
        
        match CieReturnCode::from(result) {
            CieReturnCode::Ok => Ok(()),
            CieReturnCode::CardAlreadyEnabled => {
                tracing::info!("La CIE è già abilitata");
                Ok(())
            }
            CieReturnCode::PinIncorrect => {
                Err(CieError::PinIncorrect(attempts as u32))
            }
            CieReturnCode::PinLocked => {
                Err(CieError::PinLocked)
            }
            _ => {
                // Check if there's an error message from callback
                let error_msg = LAST_ERROR.lock()
                    .ok()
                    .and_then(|guard| guard.clone())
                    .unwrap_or_else(|| format!("Errore abilitazione: 0x{:08X}", result));
                Err(CieError::Pkcs11Error(error_msg))
            }
        }
    }

    /// Disabilita una CIE
    pub fn disable_cie(&self, pan: &str) -> Result<()> {
        let pan_c = CString::new(pan)
            .map_err(|_| CieError::InvalidInput("PAN contiene caratteri non validi".into()))?;
        
        let result = unsafe { DisabilitaCIE(pan_c.as_ptr()) };
        
        if result == 0 {
            Ok(())
        } else {
            Err(CieError::Pkcs11Error(format!("Errore disabilitazione: 0x{:08X}", result)))
        }
    }

    /// Firma un file direttamente con la CIE
    /// 
    /// # Arguments
    /// * `input_file` - Percorso del file da firmare
    /// * `output_file` - Percorso del file firmato
    /// * `pin` - Ultime 4 cifre del PIN (la CIE deve essere già abilitata)
    /// * `pan` - Numero seriale della CIE
    /// * `sign_type` - Tipo di firma (P7M o PDF)
    /// * `pdf_options` - Opzioni per la firma PDF visibile (page, x, y, w, h, image_path)
    pub fn sign_file(
        &self,
        input_file: &Path,
        output_file: &Path,
        pin: &str,
        pan: &str,
        sign_type: SignType,
        pdf_options: Option<PdfSignOptions>,
    ) -> Result<()> {
        if pin.len() != 4 || !pin.chars().all(|c| c.is_ascii_digit()) {
            return Err(CieError::InvalidInput(
                "Il PIN deve essere composto dalle ultime 4 cifre".into()
            ));
        }

        let input_c = CString::new(input_file.to_string_lossy().as_bytes())
            .map_err(|_| CieError::InvalidInput("Percorso input non valido".into()))?;
        let output_c = CString::new(output_file.to_string_lossy().as_bytes())
            .map_err(|_| CieError::InvalidInput("Percorso output non valido".into()))?;
        let pin_c = CString::new(pin)
            .map_err(|_| CieError::InvalidInput("PIN non valido".into()))?;
        let pan_c = CString::new(pan)
            .map_err(|_| CieError::InvalidInput("PAN non valido".into()))?;
        
        let (page, x, y, w, h, image_c) = match pdf_options {
            Some(opts) => {
                let img = opts.image_path
                    .map(|p| CString::new(p.to_string_lossy().as_bytes()).ok())
                    .flatten();
                (opts.page, opts.x, opts.y, opts.width, opts.height, img)
            }
            None => (0, 0.0, 0.0, 0.0, 0.0, None),
        };
        
        let image_ptr = image_c.as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(std::ptr::null());
        
        // Reset error state
        if let Ok(mut guard) = LAST_ERROR.lock() {
            *guard = None;
        }
        
        let result = unsafe {
            firmaConCIE(
                input_c.as_ptr(),
                sign_type as c_int,
                pin_c.as_ptr(),
                pan_c.as_ptr(),
                page,
                x,
                y,
                w,
                h,
                image_ptr,
                output_c.as_ptr(),
                Some(default_progress_callback),
                Some(default_completed_callback),
            )
        };
        
        match CieReturnCode::from(result) {
            CieReturnCode::Ok => Ok(()),
            CieReturnCode::PinIncorrect => {
                Err(CieError::PinIncorrect(0))
            }
            CieReturnCode::PinLocked => {
                Err(CieError::PinLocked)
            }
            CieReturnCode::CardNotEnabled => {
                Err(CieError::CieNotEnabled)
            }
            _ => {
                let error_msg = LAST_ERROR.lock()
                    .ok()
                    .and_then(|guard| guard.clone())
                    .unwrap_or_else(|| format!("Errore firma: 0x{:08X}", result));
                Err(CieError::SigningError(error_msg))
            }
        }
    }
}

/// Opzioni per la firma PDF visibile
#[derive(Debug, Clone)]
pub struct PdfSignOptions {
    /// Pagina su cui apporre la firma (0 = ultima pagina)
    pub page: c_int,
    /// Coordinata X della firma
    pub x: f32,
    /// Coordinata Y della firma
    pub y: f32,
    /// Larghezza della firma
    pub width: f32,
    /// Altezza della firma
    pub height: f32,
    /// Percorso dell'immagine della firma (opzionale)
    pub image_path: Option<std::path::PathBuf>,
}

impl Default for PdfSignOptions {
    fn default() -> Self {
        Self {
            page: 0,
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
            image_path: None,
        }
    }
}
