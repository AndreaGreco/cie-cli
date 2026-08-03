//! Parser ASN.1/BER minimale per le risposte CIE
//!
//! La CIE usa strutture ASN.1 per scambio chiavi DH e DAPP.
//! Questo parser gestisce i casi comuni senza dipendenze esterne pesanti.

use crate::error::{CieError, Result};

/// Nodo ASN.1 parsato
#[derive(Debug, Clone)]
pub struct Asn1Node {
    /// Tag completo, fino a 4 byte (es. 0x7F21, 0x5F37, 0xBFA004).
    pub tag: u32,
    pub content: Vec<u8>,
    pub children: Vec<Asn1Node>,
}

impl Asn1Node {
    /// Parsa una struttura ASN.1/BER
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(CieError::Protocol("Dati ASN.1 vuoti".into()));
        }

        let (node, _) = Self::parse_internal(data)?;
        Ok(node)
    }

    /// Parsa tutti i nodi in sequenza
    pub fn parse_all(data: &[u8]) -> Result<Vec<Self>> {
        let mut nodes = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            let (node, consumed) = Self::parse_internal(&data[offset..])?;
            nodes.push(node);
            offset += consumed;
        }

        Ok(nodes)
    }

    fn parse_internal(data: &[u8]) -> Result<(Self, usize)> {
        if data.is_empty() {
            return Err(CieError::Protocol("Dati ASN.1 insufficienti".into()));
        }

        // Tag multibyte se i 5 bit bassi del primo byte sono tutti 1.
        // Nei byte successivi il bit 8 acceso significa "ne segue un altro":
        // la CIE arriva a tre byte (es. BF A0 04 nei GET DATA della chiave CA).
        // Il tag va conservato intero, altrimenti 0x5F20 collide con 0x20 e il
        // parser diventa asimmetrico rispetto ad asn1_tag().
        let (tag, tag_bytes) = if (data[0] & 0x1F) == 0x1F {
            let mut idx = 1;
            while idx < data.len() && (data[idx] & 0x80) != 0 {
                idx += 1;
            }
            if idx >= data.len() {
                return Err(CieError::Protocol("Tag ASN.1 multibyte incompleto".into()));
            }
            idx += 1; // l'ultimo byte, quello senza bit di continuazione
            if idx > 4 {
                return Err(CieError::Protocol("Tag ASN.1 oltre 4 byte non supportato".into()));
            }
            let mut t: u32 = 0;
            for b in &data[..idx] {
                t = (t << 8) | *b as u32;
            }
            (t, idx)
        } else {
            (data[0] as u32, 1usize)
        };

        let (length, len_bytes) = Self::parse_length(&data[tag_bytes..])?;

        let content_start = tag_bytes + len_bytes;
        let content_end = content_start + length;

        if content_end > data.len() {
            return Err(CieError::Protocol(format!(
                "ASN.1 lunghezza {} supera dati {} (tag {:X})",
                length, data.len() - content_start, tag
            )));
        }

        let content = data[content_start..content_end].to_vec();

        // Se è una SEQUENCE (0x30) o un tag costruito (bit 5 set), parsa i figli
        let primo = (tag >> (8 * (tag_bytes as u32 - 1))) as u8;
        let children = if (primo & 0x20) != 0 {
            match Self::parse_all(&content) {
                Ok(nodes) => nodes,
                Err(e) => {
                    // Un tag costruito che non si parsa puo' essere contenuto
                    // primitivo, ma e' anche il sintomo di un tag non supportato:
                    // senza questa traccia il chiamante vede solo "tag assente".
                    tracing::debug!("tag {:X} costruito ma non parsabile: {}", tag, e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        Ok((Self { tag, content, children }, content_end))
    }

    fn parse_length(data: &[u8]) -> Result<(usize, usize)> {
        if data.is_empty() {
            return Err(CieError::Protocol("Lunghezza ASN.1 mancante".into()));
        }

        let first = data[0];

        if first < 0x80 {
            // Short form
            Ok((first as usize, 1))
        } else if first == 0x80 {
            // Indefinite length (non supportato)
            Err(CieError::Protocol("Lunghezza ASN.1 indefinita non supportata".into()))
        } else {
            // Long form
            let num_bytes = (first & 0x7F) as usize;
            if num_bytes > 4 || num_bytes + 1 > data.len() {
                return Err(CieError::Protocol("Lunghezza ASN.1 non valida".into()));
            }

            let mut length: usize = 0;
            for i in 0..num_bytes {
                length = (length << 8) | (data[1 + i] as usize);
            }

            Ok((length, 1 + num_bytes))
        }
    }

    /// Trova un figlio per tag
    pub fn find_tag(&self, tag: u32) -> Option<&Asn1Node> {
        self.children.iter().find(|c| c.tag == tag)
    }

    /// Cerca ricorsivamente un tag
    pub fn find_tag_recursive(&self, tag: u32) -> Option<&Asn1Node> {
        if self.tag == tag {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_tag_recursive(tag) {
                return Some(found);
            }
        }
        None
    }

    /// Ottiene il contenuto come intero (big-endian)
    pub fn as_integer(&self) -> Vec<u8> {
        // Rimuove eventuali zeri iniziali (padding ASN.1 per numeri positivi)
        let mut bytes = self.content.clone();
        while bytes.len() > 1 && bytes[0] == 0 {
            bytes.remove(0);
        }
        bytes
    }
}

/// Costruisce un tag ASN.1 con contenuto (supporta tag a 1 o 2 byte)
pub fn asn1_tag(tag: u32, content: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();

    // Emette solo i byte significativi, cosi' encode e decode restano simmetrici.
    let significativi = match tag {
        0..=0xFF => 1,
        0x100..=0xFFFF => 2,
        0x10000..=0xFFFFFF => 3,
        _ => 4,
    };
    for i in (0..significativi).rev() {
        result.push((tag >> (8 * i)) as u8);
    }

    // Encode length
    if content.len() < 0x80 {
        result.push(content.len() as u8);
    } else if content.len() < 0x100 {
        result.push(0x81);
        result.push(content.len() as u8);
    } else {
        result.push(0x82);
        result.push((content.len() >> 8) as u8);
        result.push((content.len() & 0xFF) as u8);
    }

    result.extend_from_slice(content);
    result
}

/// Costruisce una SEQUENCE ASN.1
pub fn asn1_sequence(items: &[&[u8]]) -> Vec<u8> {
    let mut content = Vec::new();
    for item in items {
        content.extend_from_slice(item);
    }
    asn1_tag(0x30, &content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        // INTEGER 5
        let data = vec![0x02, 0x01, 0x05];
        let node = Asn1Node::parse(&data).unwrap();
        assert_eq!(node.tag, 0x02);
        assert_eq!(node.content, vec![0x05]);
    }

    #[test]
    fn test_parse_sequence() {
        // SEQUENCE { INTEGER 1, INTEGER 2 }
        let data = vec![0x30, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x02];
        let node = Asn1Node::parse(&data).unwrap();
        assert_eq!(node.tag, 0x30);
        assert_eq!(node.children.len(), 2);
    }

    /// I tag multibyte devono sopravvivere al giro encode -> decode.
    #[test]
    fn test_tag_multibyte_roundtrip() {
        for tag in [0x5F20u32, 0x5F37, 0x5F38, 0x7F21, 0xBFA004] {
            let enc = asn1_tag(tag, &[0xAA, 0xBB]);
            let node = Asn1Node::parse(&enc).unwrap();
            assert_eq!(node.tag, tag, "tag {:04X} non conservato", tag);
        }
    }

    /// Il caso che rompeva i parametri CA: un tag a tre byte (BF A0 04) con
    /// figli dentro. Prima il parser lo rifiutava e l'errore veniva inghiottito,
    /// lasciando il nodo senza figli e la ricerca a mani vuote.
    #[test]
    fn test_tag_tre_byte_con_figli() {
        let mut interno = asn1_tag(0x81, &[0xAA; 4]);
        interno.extend_from_slice(&asn1_tag(0x82, &[0x01, 0x00, 0x01]));
        let data = asn1_tag(0xBFA004, &interno);

        let node = Asn1Node::parse(&data).unwrap();
        assert_eq!(node.tag, 0xBFA004);
        assert_eq!(node.children.len(), 2, "tag costruito a 3 byte deve avere figli");
        assert_eq!(node.find_tag_recursive(0x81).unwrap().content, vec![0xAA; 4]);
        assert_eq!(node.find_tag_recursive(0x82).unwrap().content, vec![0x01, 0x00, 0x01]);
    }

    #[test]
    fn test_build_tag() {
        let data = asn1_tag(0x80, &[0x01, 0x02, 0x03]);
        assert_eq!(data, vec![0x80, 0x03, 0x01, 0x02, 0x03]);
    }
}
