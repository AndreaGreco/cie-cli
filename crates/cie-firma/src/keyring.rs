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

const SERVICE: &str = "cie";

/// What we keep for one card.
///
/// The holder's details are a cache, not a source of truth: they come out of
/// the certificate on the first successful signature and only serve to greet
/// the card by name when it is put on the reader, long before any PIN.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardSecret {
    pub pin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_code: Option<String>,
}

impl CardSecret {
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

fn entry(card_id: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, card_id).context("apertura del portachiavi di sistema")
}

/// What we have stored for this card, if anything.
///
/// A keyring that is missing, locked or empty is not an error worth showing:
/// it only means the PIN has to be typed and the card is not known yet.
pub fn read(card_id: &str) -> Option<CardSecret> {
    let raw = entry(card_id).ok()?.get_password().ok()?;
    serde_json::from_str(&raw).ok()
}

/// Writes (or replaces) what we know about this card.
pub fn store(card_id: &str, secret: &CardSecret) -> Result<()> {
    let raw = serde_json::to_string(secret).context("serializzazione della voce")?;
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
            pin: "12345678".to_string(),
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
            pin: "87654321".to_string(),
            ..Default::default()
        };

        let raw = serde_json::to_string(&secret).unwrap();
        let back: CardSecret = serde_json::from_str(&raw).unwrap();

        assert_eq!(back, secret);
        assert_eq!(back.holder(), None);
    }

    #[test]
    fn the_holder_reads_as_one_line() {
        let mut secret = CardSecret {
            pin: "12345678".to_string(),
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
