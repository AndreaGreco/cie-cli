//! Wrapper per l'interfaccia PKCS#11

use cryptoki::context::{CInitializeArgs, Pkcs11};
use cryptoki::session::Session;
use cryptoki::slot::Slot;
use std::path::Path;
use tracing::{debug, info};

use crate::error::{CieError, Result};

/// Wrapper per la gestione della libreria PKCS#11
pub struct Pkcs11Context {
    ctx: Pkcs11,
}

impl Pkcs11Context {
    /// Crea un nuovo contesto PKCS#11 caricando la libreria dal percorso specificato
    pub fn new(lib_path: &Path) -> Result<Self> {
        info!("Caricamento libreria PKCS#11 da: {:?}", lib_path);

        if !lib_path.exists() {
            return Err(CieError::Pkcs11LibraryNotFound {
                path: lib_path.display().to_string(),
            });
        }

        let ctx = Pkcs11::new(lib_path)?;

        debug!("Inizializzazione PKCS#11...");
        ctx.initialize(CInitializeArgs::OsThreads)?;

        info!("Libreria PKCS#11 inizializzata con successo");
        Ok(Self { ctx })
    }

    /// Restituisce la lista degli slot disponibili
    pub fn get_slots(&self) -> Result<Vec<Slot>> {
        let slots = self.ctx.get_all_slots()?;
        debug!("Trovati {} slot", slots.len());
        Ok(slots)
    }

    /// Restituisce gli slot con token (CIE) inserita
    pub fn get_slots_with_token(&self) -> Result<Vec<Slot>> {
        let slots = self.ctx.get_slots_with_token()?;
        debug!("Trovati {} slot con token", slots.len());
        Ok(slots)
    }

    /// Ottiene informazioni sullo slot
    pub fn get_slot_info(&self, slot: Slot) -> Result<SlotInfo> {
        let info = self.ctx.get_slot_info(slot)?;
        Ok(SlotInfo {
            description: info.slot_description().to_string(),
            manufacturer: info.manufacturer_id().to_string(),
            hardware_version: format!("{}.{}",
                info.hardware_version().major(),
                info.hardware_version().minor()
            ),
            firmware_version: format!("{}.{}",
                info.firmware_version().major(),
                info.firmware_version().minor()
            ),
            has_token: info.token_present(),
        })
    }

    /// Ottiene informazioni sul token (CIE)
    pub fn get_token_info(&self, slot: Slot) -> Result<TokenInfo> {
        let info = self.ctx.get_token_info(slot)?;
        Ok(TokenInfo {
            label: info.label().to_string(),
            manufacturer: info.manufacturer_id().to_string(),
            model: info.model().to_string(),
            serial_number: info.serial_number().to_string(),
        })
    }

    /// Apre una sessione di sola lettura
    pub fn open_session(&self, slot: Slot) -> Result<Session> {
        debug!("Apertura sessione per slot {:?}", slot);
        let session = self.ctx.open_ro_session(slot)?;
        Ok(session)
    }

    /// Apre una sessione di lettura/scrittura
    pub fn open_rw_session(&self, slot: Slot) -> Result<Session> {
        debug!("Apertura sessione RW per slot {:?}", slot);
        let session = self.ctx.open_rw_session(slot)?;
        Ok(session)
    }

    /// Accesso al contesto interno (per operazioni avanzate)
    pub fn inner(&self) -> &Pkcs11 {
        &self.ctx
    }
}

/// Informazioni su uno slot (lettore di smart card)
#[derive(Debug, Clone)]
pub struct SlotInfo {
    pub description: String,
    pub manufacturer: String,
    pub hardware_version: String,
    pub firmware_version: String,
    pub has_token: bool,
}

/// Informazioni sul token (CIE)
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub label: String,
    pub manufacturer: String,
    pub model: String,
    pub serial_number: String,
}

impl std::fmt::Display for SlotInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}) - Token: {}",
            self.description,
            self.manufacturer,
            if self.has_token { "presente" } else { "assente" }
        )
    }
}

impl std::fmt::Display for TokenInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - {} (S/N: {})",
            self.label, self.model, self.serial_number
        )
    }
}
