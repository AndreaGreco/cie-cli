//! The PIN in the system keyring, over D-Bus.
//!
//! The attributes are **exactly** what
//! `secret-tool store --label='CIE PIN' service cie` would write: the CLI
//! reads the same entry with `secret-tool lookup service cie`, so the two
//! tools have to agree on the format.

use anyhow::{Context, Result};
use secret_service::EncryptionType;
use std::collections::HashMap;

const SERVICE: &str = "cie";
const LABEL: &str = "CIE PIN";

fn attributes() -> HashMap<&'static str, &'static str> {
    HashMap::from([("service", SERVICE)])
}

/// Runs one operation against the keyring.
///
/// `secret-service` is async with no blocking API; a single-threaded runtime
/// is enough because the only I/O is the D-Bus socket.
fn with_keyring<T>(f: impl AsyncFnOnce(&secret_service::SecretService<'_>) -> Result<T>) -> Result<T> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creating the runtime")?;

    rt.block_on(async {
        let ss = secret_service::SecretService::connect(EncryptionType::Dh)
            .await
            .context("connessione al portachiavi di sistema")?;
        f(&ss).await
    })
}

/// The stored PIN, if there is one.
///
/// A missing or locked keyring is not an error worth showing: it just means
/// the PIN has to be typed.
pub fn read() -> Option<String> {
    with_keyring(async |ss| {
        let found = ss.search_items(attributes()).await?;
        let Some(item) = found.unlocked.first().or(found.locked.first()) else {
            return Ok(None);
        };
        item.unlock().await?;
        let pin = String::from_utf8(item.get_secret().await?)?;
        Ok(Some(pin.trim().to_string()).filter(|p| !p.is_empty()))
    })
    .ok()
    .flatten()
}

/// Writes (or overwrites) the PIN in the default collection.
pub fn store(pin: &str) -> Result<()> {
    with_keyring(async |ss| {
        let collection = ss
            .get_default_collection()
            .await
            .context("collezione predefinita del portachiavi")?;
        collection.unlock().await?;
        collection
            .create_item(
                LABEL,
                attributes(),
                pin.as_bytes(),
                true, // replaces the entry carrying the same attributes
                "text/plain",
            )
            .await
            .context("scrittura della voce")?;
        Ok(())
    })
}

/// Deletes the PIN from the keyring. Not an error if there was none.
pub fn forget() -> Result<()> {
    with_keyring(async |ss| {
        let found = ss.search_items(attributes()).await?;
        for item in found.unlocked.iter().chain(found.locked.iter()) {
            item.unlock().await?;
            item.delete().await.context("cancellazione della voce")?;
        }
        Ok(())
    })
}
