//! Dati del titolare della CIE
//!
//! Questo modulo gestisce l'estrazione e il parsing dei dati personali
//! contenuti nei certificati della CIE.

use der::Decode;
use serde::{Deserialize, Serialize};
use x509_cert::Certificate;

/// Dati anagrafici estratti dalla CIE
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatiTitolare {
    /// Nome del titolare
    pub nome: Option<String>,
    /// Cognome del titolare
    pub cognome: Option<String>,
    /// Nome completo (come appare nel certificato)
    pub nome_completo: Option<String>,
    /// Codice fiscale
    pub codice_fiscale: Option<String>,
    /// Numero seriale della CIE
    pub numero_cie: Option<String>,
    /// Data di nascita (se disponibile)
    pub data_nascita: Option<String>,
    /// Comune di nascita (se disponibile)
    pub luogo_nascita: Option<String>,
}

impl DatiTitolare {
    /// Estrae i dati del titolare da un certificato X.509 in formato DER
    pub fn from_certificate_der(der_data: &[u8]) -> Option<Self> {
        let cert = Certificate::from_der(der_data).ok()?;

        let mut dati = DatiTitolare::default();

        // Estrai il Subject del certificato
        let subject = &cert.tbs_certificate.subject;

        // Il Subject contiene RDNs (Relative Distinguished Names)
        // Cerchiamo Common Name (CN), Given Name (GN), Surname (SN), Serial Number
        for rdn in subject.0.iter() {
            for atv in rdn.0.iter() {
                let oid = &atv.oid;
                // Estrai il valore come stringa UTF-8
                let value = match String::from_utf8(atv.value.value().to_vec()) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                // OID comuni:
                // 2.5.4.3 = Common Name (CN)
                // 2.5.4.4 = Surname (SN)
                // 2.5.4.5 = Serial Number
                // 2.5.4.42 = Given Name (GN)
                // 2.5.4.6 = Country (C)

                let oid_str = oid.to_string();

                match oid_str.as_str() {
                    "2.5.4.3" => dati.nome_completo = Some(value), // CN
                    "2.5.4.4" => dati.cognome = Some(value),       // Surname
                    "2.5.4.42" => dati.nome = Some(value),         // Given Name
                    "2.5.4.5" => {
                        // Serial Number - potrebbe essere il CF o numero CIE
                        if value.len() == 16 && value.chars().all(|c| c.is_alphanumeric()) {
                            dati.codice_fiscale = Some(value);
                        } else {
                            dati.numero_cie = Some(value);
                        }
                    }
                    _ => {}
                }
            }
        }

        // Se non abbiamo nome/cognome separati, proviamo a estrarli dal CN
        if (dati.nome.is_none() || dati.cognome.is_none())
            && let Some(ref cn) = dati.nome_completo {
                // Il CN della CIE è spesso nel formato "COGNOME/NOME" o "COGNOME NOME"
                if let Some((cognome, nome)) = cn.split_once('/') {
                    if dati.cognome.is_none() {
                        dati.cognome = Some(cognome.trim().to_string());
                    }
                    if dati.nome.is_none() {
                        dati.nome = Some(nome.trim().to_string());
                    }
                }
            }

        // Cerca il codice fiscale anche nelle estensioni del certificato
        // (Subject Alternative Name può contenere info aggiuntive)
        if dati.codice_fiscale.is_none()
            && let Some(ref cn) = dati.nome_completo {
                // A volte il CF è nell'ultima parte del CN
                let parts: Vec<&str> = cn.split('/').collect();
                for part in parts {
                    let trimmed = part.trim();
                    if trimmed.len() == 16
                        && trimmed.chars().all(|c| c.is_alphanumeric())
                        && trimmed.chars().take(6).all(|c| c.is_alphabetic())
                    {
                        dati.codice_fiscale = Some(trimmed.to_uppercase());
                        break;
                    }
                }
            }

        Some(dati)
    }
}

impl std::fmt::Display for DatiTitolare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref nome) = self.nome {
            writeln!(f, "Nome:           {}", nome)?;
        }
        if let Some(ref cognome) = self.cognome {
            writeln!(f, "Cognome:        {}", cognome)?;
        }
        if let Some(ref cf) = self.codice_fiscale {
            writeln!(f, "Codice Fiscale: {}", cf)?;
        }
        if let Some(ref num) = self.numero_cie {
            writeln!(f, "Numero CIE:     {}", num)?;
        }
        if let Some(ref cn) = self.nome_completo {
            writeln!(f, "Subject CN:     {}", cn)?;
        }
        Ok(())
    }
}
