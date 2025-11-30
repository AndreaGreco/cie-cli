//! Interfaccia principale per la Carta d'Identità Elettronica

use cryptoki::mechanism::Mechanism;
use cryptoki::object::{Attribute, AttributeType, ObjectClass, ObjectHandle};
use cryptoki::session::UserType;
use cryptoki::types::AuthPin;
use tracing::{debug, info, warn};

use crate::config::CieConfig;
use crate::error::{CieError, Result};
use crate::pkcs11::{Pkcs11Context, SlotInfo, TokenInfo};

/// Interfaccia principale per la CIE
pub struct Cie {
    ctx: Pkcs11Context,
    config: CieConfig,
}

impl Cie {
    /// Crea una nuova istanza CIE con configurazione di default
    pub fn new() -> Result<Self> {
        let config = CieConfig::auto_detect()
            .ok_or_else(|| CieError::Pkcs11LibraryNotFound {
                path: "Nessun percorso trovato automaticamente".into(),
            })?;
        Self::with_config(config)
    }

    /// Crea una nuova istanza CIE con configurazione personalizzata
    pub fn with_config(config: CieConfig) -> Result<Self> {
        let ctx = Pkcs11Context::new(&config.pkcs11_lib_path)?;
        Ok(Self { ctx, config })
    }

    /// Restituisce la configurazione corrente
    pub fn config(&self) -> &CieConfig {
        &self.config
    }

    /// Lista tutti i lettori di smart card disponibili
    pub fn list_readers(&self) -> Result<Vec<ReaderInfo>> {
        let slots = self.ctx.get_slots()?;
        let mut readers = Vec::new();

        for slot in slots {
            let slot_info = self.ctx.get_slot_info(slot)?;
            let token_info = if slot_info.has_token {
                Some(self.ctx.get_token_info(slot)?)
            } else {
                None
            };

            readers.push(ReaderInfo {
                slot,
                slot_info,
                token_info,
            });
        }

        Ok(readers)
    }

    /// Trova il primo slot con una CIE inserita
    pub fn find_cie(&self) -> Result<ReaderInfo> {
        let slots = self.ctx.get_slots_with_token()?;

        if slots.is_empty() {
            return Err(CieError::NoCiePresent);
        }

        let slot = slots[0];
        let slot_info = self.ctx.get_slot_info(slot)?;
        let token_info = Some(self.ctx.get_token_info(slot)?);

        Ok(ReaderInfo {
            slot,
            slot_info,
            token_info,
        })
    }

    /// Autentica l'utente con il PIN
    pub fn login(&self, reader: &ReaderInfo, pin: &str) -> Result<CieSession> {
        info!("Tentativo di login sulla CIE");

        let session = self.ctx.open_rw_session(reader.slot)?;
        let auth_pin = AuthPin::new(pin.into());

        session.login(UserType::User, Some(&auth_pin))
            .map_err(|e| {
                warn!("Errore durante il login: {:?}", e);
                // TODO: Parsare l'errore per capire se è PIN errato o bloccato
                CieError::Pkcs11Error(e)
            })?;

        info!("Login effettuato con successo");
        Ok(CieSession { session })
    }

    /// Legge i certificati pubblici dalla CIE (senza PIN)
    pub fn read_public_certificates(&self, reader: &ReaderInfo) -> Result<Vec<CieCertificate>> {
        debug!("Ricerca certificati pubblici sulla CIE");

        let session = self.ctx.open_session(reader.slot)?;

        let template = vec![
            Attribute::Class(ObjectClass::CERTIFICATE),
        ];

        let objects = session.find_objects(&template)?;
        debug!("Trovati {} certificati pubblici", objects.len());

        let mut certificates = Vec::new();
        for obj in objects {
            if let Ok(cert) = self.read_certificate_details(&session, obj) {
                certificates.push(cert);
            }
        }

        Ok(certificates)
    }

    /// Legge i certificati dalla CIE (con sessione autenticata)
    pub fn read_certificates(&self, cie_session: &CieSession) -> Result<Vec<CieCertificate>> {
        debug!("Ricerca certificati sulla CIE");

        let template = vec![
            Attribute::Class(ObjectClass::CERTIFICATE),
        ];

        let objects = cie_session.session.find_objects(&template)?;
        debug!("Trovati {} certificati", objects.len());

        let mut certificates = Vec::new();
        for obj in objects {
            if let Ok(cert) = self.read_certificate_details(&cie_session.session, obj) {
                certificates.push(cert);
            }
        }

        Ok(certificates)
    }

    /// Legge i dettagli di un certificato
    fn read_certificate_details(
        &self,
        session: &cryptoki::session::Session,
        handle: ObjectHandle,
    ) -> Result<CieCertificate> {
        let attrs = session.get_attributes(
            handle,
            &[
                AttributeType::Label,
                AttributeType::Id,
                AttributeType::Value,
            ],
        )?;

        let label = attrs.iter()
            .find_map(|a| match a {
                Attribute::Label(l) => Some(String::from_utf8_lossy(l).to_string()),
                _ => None,
            })
            .unwrap_or_default();

        let id = attrs.iter()
            .find_map(|a| match a {
                Attribute::Id(id) => Some(id.clone()),
                _ => None,
            })
            .unwrap_or_default();

        let value = attrs.iter()
            .find_map(|a| match a {
                Attribute::Value(v) => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_default();

        Ok(CieCertificate {
            label,
            id,
            der_value: value,
        })
    }

    /// Firma dati con la chiave privata della CIE
    pub fn sign(
        &self,
        cie_session: &CieSession,
        data: &[u8],
        mechanism: SigningMechanism,
    ) -> Result<Vec<u8>> {
        info!("Firma dati ({} bytes)", data.len());

        // Trova la chiave privata
        let template = vec![
            Attribute::Class(ObjectClass::PRIVATE_KEY),
        ];

        let keys = cie_session.session.find_objects(&template)?;
        if keys.is_empty() {
            return Err(CieError::SigningError("Nessuna chiave privata trovata".into()));
        }

        let private_key = keys[0];
        let mechanism = mechanism.to_pkcs11();

        let signature = cie_session.session.sign(&mechanism, private_key, data)?;
        info!("Firma completata ({} bytes)", signature.len());

        Ok(signature)
    }
}

/// Sessione autenticata con la CIE
pub struct CieSession {
    session: cryptoki::session::Session,
}

impl CieSession {
    /// Effettua il logout dalla sessione
    pub fn logout(self) -> Result<()> {
        self.session.logout()?;
        info!("Logout effettuato");
        Ok(())
    }
}

/// Informazioni su un lettore e la sua CIE (se presente)
#[derive(Debug)]
pub struct ReaderInfo {
    pub slot: cryptoki::slot::Slot,
    pub slot_info: SlotInfo,
    pub token_info: Option<TokenInfo>,
}

impl std::fmt::Display for ReaderInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.slot_info)?;
        if let Some(ref token) = self.token_info {
            write!(f, "\n  └─ {}", token)?;
        }
        Ok(())
    }
}

/// Certificato estratto dalla CIE
#[derive(Debug, Clone)]
pub struct CieCertificate {
    pub label: String,
    pub id: Vec<u8>,
    pub der_value: Vec<u8>,
}

impl CieCertificate {
    /// Restituisce l'ID come stringa esadecimale
    pub fn id_hex(&self) -> String {
        hex::encode(&self.id)
    }

    /// Estrae i dati del titolare dal certificato
    pub fn dati_titolare(&self) -> Option<crate::titolare::DatiTitolare> {
        crate::titolare::DatiTitolare::from_certificate_der(&self.der_value)
    }
}

/// Meccanismi di firma supportati
#[derive(Debug, Clone, Copy)]
pub enum SigningMechanism {
    /// RSA PKCS#1 v1.5
    RsaPkcs,
    /// RSA PKCS#1 v1.5 con SHA-256
    Sha256RsaPkcs,
    /// RSA PKCS#1 v1.5 con SHA-1
    Sha1RsaPkcs,
}

impl SigningMechanism {
    fn to_pkcs11(self) -> Mechanism<'static> {
        match self {
            SigningMechanism::RsaPkcs => Mechanism::RsaPkcs,
            SigningMechanism::Sha256RsaPkcs => Mechanism::Sha256RsaPkcs,
            SigningMechanism::Sha1RsaPkcs => Mechanism::Sha1RsaPkcs,
        }
    }
}
