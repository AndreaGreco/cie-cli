//! Moduli dei comandi CLI

pub mod readers;
pub mod info;
pub mod data;
pub mod cert;
pub mod sign;
pub mod verify;

use cie_lib::{Cie, CieConfig};
use anyhow::Result;

/// Helper per creare un'istanza CIE con il percorso opzionale
pub fn create_cie(pkcs11_lib: Option<&str>) -> Result<Cie> {
    let cie = if let Some(path) = pkcs11_lib {
        Cie::with_config(CieConfig::with_pkcs11_path(path))?
    } else {
        Cie::new()?
    };
    Ok(cie)
}
