//! The PIN, and what we know about the holder, in the system keyring.
//!
//! One entry per card, so more than one CIE can live in the same keyring. The
//! key is the card's ID_Servizi (file `0x1001`): twelve ASCII digits, readable
//! without the PIN, and the very number printed on the card.
//!
//! The `keyring` crate picks the store per platform - secret-service here,
//! Credential Manager on Windows - so `cie` and `cie-gui` share one
//! implementation instead of agreeing on a convention.
//!
//! What goes where matters. On secret-service the *index* (service and user) is
//! plain metadata, readable by anything on the session bus; only the secret is
//! encrypted. So the index carries the card id and nothing else: the holder's
//! name and tax code live inside the secret.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const SERVICE: &str = "cie";

/// What we keep for one card.
///
/// The holder's details are a cache, not a source of truth: they come out of
/// the certificate on the first successful signature and only serve to greet
/// the card by name when it is put on the reader, long before any PIN.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardSecret {
    /// Wrapped so the bytes are overwritten when this entry is dropped, and
    /// not merely handed back to the allocator with the PIN still in them.
    pub pin: Zeroizing<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_code: Option<String>,
}

impl CardSecret {
    /// The same entry with the PIN dropped.
    ///
    /// What a window keeps between signatures: the holder's name has to stay
    /// on screen, the PIN has no business outliving the signature it was
    /// unlocked for.
    pub fn without_pin(self) -> Self {
        Self {
            pin: Zeroizing::default(),
            ..self
        }
    }

    /// The holder as one line, for a label. `None` until a signature has
    /// taught us who they are.
    pub fn holder(&self) -> Option<String> {
        match (&self.name, &self.surname) {
            (Some(n), Some(s)) => Some(format!("{n} {s}")),
            (Some(one), None) | (None, Some(one)) => Some(one.clone()),
            (None, None) => None,
        }
    }
}

/// Redacted on purpose: a `{:?}` on this struct anywhere - a log line, an
/// error chain, a panic message - would otherwise print the PIN in clear.
impl std::fmt::Debug for CardSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardSecret")
            .field("pin", &"<redacted>")
            .field("name", &self.name)
            .field("surname", &self.surname)
            .field("tax_code", &self.tax_code)
            .finish()
    }
}

fn entry(card_id: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, card_id).context("apertura del portachiavi di sistema")
}

/// What we have stored for this card, if anything.
///
/// A keyring that is missing, locked or empty is not an error worth showing:
/// it only means the PIN has to be typed and the card is not known yet.
pub fn read(card_id: &str) -> Option<CardSecret> {
    // The whole entry arrives as one JSON string, PIN included: that string is
    // a second copy of the secret and has to be wiped like the first.
    let raw = Zeroizing::new(entry(card_id).ok()?.get_password().ok()?);
    serde_json::from_str(&raw).ok()
}

/// The same entry, but only after the system has asked who is at the keyboard.
///
/// This is the read to use when the PIN is about to be *used*: [`read`] is for
/// the label on screen, this one is for signing.
pub fn read_with_prompt(card_id: &str) -> Option<CardSecret> {
    relock(card_id);
    read(card_id)
}

/// Puts the lock back on this card's entry, so the read right after it makes
/// the keyring ask for the user's password.
///
/// A secret-service collection unlocked at login hands the PIN over without a
/// word: whoever walks up to an unattended session signs as the holder. Taking
/// the lock back costs the person at the keyboard one password, and that is
/// exactly the proof we are after.
///
/// gnome-keyring only honours a lock aimed at a whole collection, so the
/// fallback takes the collection with it: same prompt, and everything else in
/// it comes back unlocked together with our entry.
///
/// Linux only. The Windows Credential Manager has no lock to take and macOS
/// wants an ACL set at creation time, so on those two the stored PIN stays as
/// reachable as it has always been.
#[cfg(target_os = "linux")]
fn relock(card_id: &str) {
    use secret_service::{blocking::SecretService, EncryptionType};

    // Plain: nothing secret crosses this connection, we only turn a key.
    let Ok(service) = SecretService::connect(EncryptionType::Plain) else {
        return;
    };
    let attributes = std::collections::HashMap::from([("service", SERVICE), ("username", card_id)]);
    let Ok(found) = service.search_items(attributes) else {
        return;
    };
    if found.unlocked.is_empty() {
        return; // already locked, or this card was never stored
    }

    let mut locked = false;
    for item in &found.unlocked {
        locked |= item.lock().is_ok() && item.is_locked().unwrap_or(false);
    }
    if !locked
        && let Ok(collection) = service.get_default_collection()
    {
        let _ = collection.lock();
    }
}

#[cfg(not(target_os = "linux"))]
fn relock(_card_id: &str) {}

/// Writes (or replaces) what we know about this card.
pub fn store(card_id: &str, secret: &CardSecret) -> Result<()> {
    let raw = Zeroizing::new(serde_json::to_string(secret).context("serializzazione della voce")?);
    entry(card_id)?
        .set_password(&raw)
        .context("scrittura nel portachiavi")
}

/// Drops this card from the keyring. Not an error if it was not there.
pub fn forget(card_id: &str) -> Result<()> {
    match entry(card_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::Error::new(e).context("cancellazione dal portachiavi")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stored form has to survive a round trip: it is the only thing
    /// standing between a saved PIN and a PIN typed again.
    #[test]
    fn a_secret_survives_the_round_trip() {
        let secret = CardSecret {
            pin: Zeroizing::new("12345678".to_string()),
            name: Some("MARIO".to_string()),
            surname: Some("ROSSI".to_string()),
            tax_code: Some("RSSMRA80A01H501U".to_string()),
        };

        let raw = serde_json::to_string(&secret).unwrap();
        assert_eq!(serde_json::from_str::<CardSecret>(&raw).unwrap(), secret);
    }

    /// A card seen but never signed with knows only its PIN, and must not
    /// invent a holder.
    #[test]
    fn a_card_without_a_holder_reads_back() {
        let secret = CardSecret {
            pin: Zeroizing::new("87654321".to_string()),
            ..Default::default()
        };

        let raw = serde_json::to_string(&secret).unwrap();
        let back: CardSecret = serde_json::from_str(&raw).unwrap();

        assert_eq!(back, secret);
        assert_eq!(back.holder(), None);
    }

    /// Dropping the PIN must leave the holder behind: the sidebar goes on
    /// showing who the card belongs to between signatures.
    #[test]
    fn without_pin_keeps_the_holder() {
        let secret = CardSecret {
            pin: Zeroizing::new("12345678".to_string()),
            name: Some("MARIO".to_string()),
            surname: Some("ROSSI".to_string()),
            tax_code: Some("RSSMRA80A01H501U".to_string()),
        };

        let bare = secret.clone().without_pin();
        assert!(bare.pin.is_empty());
        assert_eq!(bare.holder(), secret.holder());
        assert_eq!(bare.tax_code, secret.tax_code);
    }

    #[test]
    fn the_holder_reads_as_one_line() {
        let mut secret = CardSecret {
            pin: Zeroizing::new("12345678".to_string()),
            name: Some("MARIO".to_string()),
            surname: Some("ROSSI".to_string()),
            tax_code: None,
        };
        assert_eq!(secret.holder().as_deref(), Some("MARIO ROSSI"));

        // Some certificates carry only the common name.
        secret.name = None;
        assert_eq!(secret.holder().as_deref(), Some("ROSSI"));
    }
}
