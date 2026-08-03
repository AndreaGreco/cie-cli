//! Il PIN nel portachiavi di sistema, via D-Bus.
//!
//! Gli attributi sono **esattamente** quelli che scriverebbe
//! `secret-tool store --label='CIE PIN' service cie`: la CLI legge la stessa
//! voce con `secret-tool lookup service cie`, quindi i due strumenti devono
//! trovarsi d'accordo sul formato.

use anyhow::{Context, Result};
use secret_service::EncryptionType;
use std::collections::HashMap;

const SERVIZIO: &str = "cie";
const ETICHETTA: &str = "CIE PIN";

fn attributi() -> HashMap<&'static str, &'static str> {
    HashMap::from([("service", SERVIZIO)])
}

/// Esegue un'operazione sul portachiavi.
///
/// `secret-service` e' asincrono e non c'e' un'API bloccante; il runtime a
/// thread singolo basta perche' l'unico I/O e' il socket D-Bus.
fn con_portachiavi<T>(
    f: impl AsyncFnOnce(&secret_service::SecretService<'_>) -> Result<T>,
) -> Result<T> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("creazione del runtime")?;

    rt.block_on(async {
        let ss = secret_service::SecretService::connect(EncryptionType::Dh)
            .await
            .context("connessione al portachiavi di sistema")?;
        f(&ss).await
    })
}

/// Il PIN salvato, se c'e'.
///
/// Un portachiavi assente o bloccato non e' un errore da mostrare: vuol dire
/// soltanto che il PIN va chiesto a mano.
pub fn leggi() -> Option<String> {
    con_portachiavi(async |ss| {
        let trovati = ss.search_items(attributi()).await?;
        let Some(voce) = trovati.unlocked.first().or(trovati.locked.first()) else {
            return Ok(None);
        };
        voce.unlock().await?;
        let pin = String::from_utf8(voce.get_secret().await?)?;
        Ok(Some(pin.trim().to_string()).filter(|p| !p.is_empty()))
    })
    .ok()
    .flatten()
}

/// Scrive (o sovrascrive) il PIN nella collezione predefinita.
pub fn salva(pin: &str) -> Result<()> {
    con_portachiavi(async |ss| {
        let collezione = ss
            .get_default_collection()
            .await
            .context("collezione predefinita del portachiavi")?;
        collezione.unlock().await?;
        collezione
            .create_item(
                ETICHETTA,
                attributi(),
                pin.as_bytes(),
                true, // sostituisce la voce con gli stessi attributi
                "text/plain",
            )
            .await
            .context("scrittura della voce")?;
        Ok(())
    })
}

/// Cancella il PIN dal portachiavi. Se non c'era, non e' un errore.
pub fn dimentica() -> Result<()> {
    con_portachiavi(async |ss| {
        let trovati = ss.search_items(attributi()).await?;
        for voce in trovati.unlocked.iter().chain(trovati.locked.iter()) {
            voce.unlock().await?;
            voce.delete().await.context("cancellazione della voce")?;
        }
        Ok(())
    })
}
