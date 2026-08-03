//! Modulo crittografico per Secure Messaging CIE
//!
//! Implementa le primitive crittografiche necessarie per il protocollo
//! di Secure Messaging della CIE:
//! - 3DES CBC per cifratura
//! - Retail MAC (ISO 9797-1 Algorithm 3) per integrità
//! - SHA-256 per derivazione chiavi
//! - RSA per DAPP

use crate::error::{CieError, Result};
use des::cipher::{BlockEncrypt, BlockDecrypt, KeyInit, generic_array::GenericArray};
use sha2::{Sha256, Digest};

/// Dimensione blocco DES/3DES
const DES_BLOCK_SIZE: usize = 8;

/// Chiave 3DES (24 bytes = 3 chiavi DES da 8 bytes)
pub struct TripleDes {
    key: [u8; 24],
    iv: [u8; 8],
}

impl TripleDes {
    /// Crea una nuova istanza 3DES con chiave e IV
    pub fn new(key: &[u8], iv: &[u8]) -> Result<Self> {
        if key.len() < 16 {
            return Err(CieError::InvalidInput("Chiave 3DES troppo corta".into()));
        }

        // Se la chiave è 16 bytes, espandila a 24 (k1, k2, k1)
        let mut key24 = [0u8; 24];
        if key.len() == 16 {
            key24[..16].copy_from_slice(&key[..16]);
            key24[16..24].copy_from_slice(&key[..8]);
        } else if key.len() >= 24 {
            key24.copy_from_slice(&key[..24]);
        }

        let mut iv8 = [0u8; 8];
        if iv.len() >= 8 {
            iv8.copy_from_slice(&iv[..8]);
        }

        Ok(Self { key: key24, iv: iv8 })
    }

    /// Cifra dati con 3DES-CBC (con ISO padding)
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let padded = iso_pad(data);
        self.encrypt_raw(&padded)
    }

    /// Cifra dati già paddati
    pub fn encrypt_raw(&self, data: &[u8]) -> Result<Vec<u8>> {
        if !data.len().is_multiple_of(DES_BLOCK_SIZE) {
            return Err(CieError::InvalidInput("Dati non allineati a 8 bytes".into()));
        }

        let k1 = des::Des::new(GenericArray::from_slice(&self.key[0..8]));
        let k2 = des::Des::new(GenericArray::from_slice(&self.key[8..16]));
        let k3 = des::Des::new(GenericArray::from_slice(&self.key[16..24]));

        let mut result = Vec::with_capacity(data.len());
        let mut prev_block = self.iv;

        for chunk in data.chunks(DES_BLOCK_SIZE) {
            // XOR con blocco precedente (CBC)
            let mut block = [0u8; 8];
            for i in 0..8 {
                block[i] = chunk[i] ^ prev_block[i];
            }

            // 3DES: E(k1) -> D(k2) -> E(k3)
            let mut ga = GenericArray::from(block);
            k1.encrypt_block(&mut ga);
            k2.decrypt_block(&mut ga);
            k3.encrypt_block(&mut ga);

            prev_block.copy_from_slice(&ga);
            result.extend_from_slice(&ga);
        }

        Ok(result)
    }

    /// Decifra dati con 3DES-CBC
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if !data.len().is_multiple_of(DES_BLOCK_SIZE) {
            return Err(CieError::InvalidInput("Dati cifrati non allineati".into()));
        }

        let k1 = des::Des::new(GenericArray::from_slice(&self.key[0..8]));
        let k2 = des::Des::new(GenericArray::from_slice(&self.key[8..16]));
        let k3 = des::Des::new(GenericArray::from_slice(&self.key[16..24]));

        let mut result = Vec::with_capacity(data.len());
        let mut prev_block = self.iv;

        for chunk in data.chunks(DES_BLOCK_SIZE) {
            let mut block = [0u8; 8];
            block.copy_from_slice(chunk);

            // 3DES decrypt: D(k3) -> E(k2) -> D(k1)
            let mut ga = GenericArray::from(block);
            k3.decrypt_block(&mut ga);
            k2.encrypt_block(&mut ga);
            k1.decrypt_block(&mut ga);

            // XOR con blocco precedente (CBC)
            for i in 0..8 {
                ga[i] ^= prev_block[i];
            }

            prev_block.copy_from_slice(chunk);
            result.extend_from_slice(&ga);
        }

        // Rimuovi padding ISO
        Ok(remove_iso_pad(&result))
    }
}

/// Calcola Retail MAC (ISO 9797-1 Algorithm 3)
/// Usato per Secure Messaging CIE
pub struct RetailMac {
    key: [u8; 16], // k1 (8 bytes) + k2 (8 bytes)
    iv: [u8; 8],
}

impl RetailMac {
    pub fn new(key: &[u8], iv: &[u8]) -> Result<Self> {
        if key.len() < 16 {
            return Err(CieError::InvalidInput("Chiave MAC troppo corta".into()));
        }

        let mut key16 = [0u8; 16];
        key16.copy_from_slice(&key[..16]);

        let mut iv8 = [0u8; 8];
        if iv.len() >= 8 {
            iv8.copy_from_slice(&iv[..8]);
        }

        Ok(Self { key: key16, iv: iv8 })
    }

    /// Calcola MAC su dati (con ISO padding)
    pub fn compute(&self, data: &[u8]) -> Vec<u8> {
        let padded = iso_pad(data);
        self.compute_raw(&padded)
    }

    /// Calcola MAC su dati già paddati
    pub fn compute_raw(&self, data: &[u8]) -> Vec<u8> {
        let k1 = des::Des::new(GenericArray::from_slice(&self.key[0..8]));
        let k2 = des::Des::new(GenericArray::from_slice(&self.key[8..16]));

        // CBC-MAC con k1
        let mut mac = self.iv;
        for chunk in data.chunks(DES_BLOCK_SIZE) {
            let mut block = [0u8; 8];
            let len = chunk.len().min(8);
            block[..len].copy_from_slice(&chunk[..len]);

            // XOR
            for i in 0..8 {
                mac[i] ^= block[i];
            }

            // Encrypt con k1
            let mut ga = GenericArray::from(mac);
            k1.encrypt_block(&mut ga);
            mac.copy_from_slice(&ga);
        }

        // Retail MAC: decrypt con k2, poi encrypt con k1
        let mut ga = GenericArray::from(mac);
        k2.decrypt_block(&mut ga);
        k1.encrypt_block(&mut ga);

        ga.to_vec()
    }
}

/// ISO 9797-1 Padding Method 2
/// Aggiunge 0x80 seguito da 0x00 fino ad allineamento
pub fn iso_pad(data: &[u8]) -> Vec<u8> {
    let pad_len = DES_BLOCK_SIZE - (data.len() % DES_BLOCK_SIZE);
    let mut padded = Vec::with_capacity(data.len() + pad_len);
    padded.extend_from_slice(data);
    padded.push(0x80);
    padded.resize(data.len() + pad_len, 0x00);
    padded
}

/// Rimuove ISO padding
pub fn remove_iso_pad(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    // Cerca 0x80 dalla fine
    for i in (0..data.len()).rev() {
        if data[i] == 0x80 {
            return data[..i].to_vec();
        } else if data[i] != 0x00 {
            // Nessun padding valido, ritorna tutto
            return data.to_vec();
        }
    }

    data.to_vec()
}

/// SHA-256 hash
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Deriva chiavi di sessione da segreto condiviso (come fa il middleware C++)
/// sessENC = SHA256(secret || 0x00000001)[0..16]
/// sessMAC = SHA256(secret || 0x00000002)[0..16]
pub fn derive_session_keys(secret: &[u8]) -> ([u8; 16], [u8; 16]) {
    let mut data_enc = Vec::with_capacity(secret.len() + 4);
    data_enc.extend_from_slice(secret);
    data_enc.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
    let hash_enc = sha256(&data_enc);

    let mut data_mac = Vec::with_capacity(secret.len() + 4);
    data_mac.extend_from_slice(secret);
    data_mac.extend_from_slice(&[0x00, 0x00, 0x00, 0x02]);
    let hash_mac = sha256(&data_mac);

    let mut sess_enc = [0u8; 16];
    let mut sess_mac = [0u8; 16];
    sess_enc.copy_from_slice(&hash_enc[..16]);
    sess_mac.copy_from_slice(&hash_mac[..16]);

    (sess_enc, sess_mac)
}

/// RSA operazione pura (senza padding) per DAPP
/// result = base^exp mod modulus
pub fn rsa_raw(base: &[u8], exp: &[u8], modulus: &[u8]) -> Vec<u8> {
    use num_bigint::BigUint;

    let b = BigUint::from_bytes_be(base);
    let e = BigUint::from_bytes_be(exp);
    let m = BigUint::from_bytes_be(modulus);

    let result = b.modpow(&e, &m);
    let mut bytes = result.to_bytes_be();

    // Padding a sinistra per avere la stessa lunghezza del modulo
    while bytes.len() < modulus.len() {
        bytes.insert(0, 0);
    }

    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iso_pad() {
        let data = vec![0x01, 0x02, 0x03];
        let padded = iso_pad(&data);
        assert_eq!(padded.len(), 8);
        assert_eq!(padded[3], 0x80);
        assert_eq!(padded[4], 0x00);

        let unpadded = remove_iso_pad(&padded);
        assert_eq!(unpadded, data);
    }

    #[test]
    fn test_sha256() {
        let hash = sha256(b"test");
        assert_eq!(hash.len(), 32);
    }

    /// Vettore pubblicato ICAO Doc 9303 (BAC), che usa lo stesso Retail MAC
    /// (ISO 9797-1 Algorithm 3, padding method 2) del Secure Messaging CIE.
    /// Se questo test passa, il MAC e' conforme allo standard e non solo a se stesso.
    #[test]
    fn test_retail_mac_icao9303() {
        let key = hex::decode("7962D9ECE03D1ACD4C76089DCE131543").unwrap();
        let data = hex::decode(
            "72C29C2371CC9BDB65B779B8E8D37B29ECC154AA56A8799FAE2F498F76ED92F2",
        ).unwrap();

        let mac = RetailMac::new(&key, &[0u8; 8]).unwrap();
        assert_eq!(hex::encode_upper(mac.compute(&data)), "5F1448EEA8AD90A7");
    }

    /// Il MAC si calcola su dati gia' paddati: `compute` = `compute_raw(iso_pad(..))`.
    #[test]
    fn test_retail_mac_vettori() {
        let key = hex::decode("000102030405060708090A0B0C0D0E0F").unwrap();
        let mac = RetailMac::new(&key, &[0u8; 8]).unwrap();

        for (data, atteso) in [
            ("", "C333A918D43B4A16"),
            ("434945", "1FEA2956C5FEA5E0"),
            ("3132333435363738", "2419541B9E18284B"),
        ] {
            let d = hex::decode(data).unwrap();
            assert_eq!(hex::encode_upper(mac.compute(&d)), atteso, "data={}", data);
            assert_eq!(hex::encode_upper(mac.compute_raw(&iso_pad(&d))), atteso);
        }
    }

    /// 3DES-CBC a due chiavi (K1,K2,K1) con IV zero, come usa il SM.
    #[test]
    fn test_3des_cbc_vettore() {
        let key = hex::decode("00112233445566778899AABBCCDDEEFF").unwrap();
        let des3 = TripleDes::new(&key, &[0u8; 8]).unwrap();
        let ct = des3.encrypt(b"Hello CIE World!").unwrap();
        assert_eq!(
            hex::encode_upper(&ct),
            "0A463594CA85DFC600ECF3421EFE3E992C338EC32B8E1FB9"
        );
        assert_eq!(des3.decrypt(&ct).unwrap(), b"Hello CIE World!");
    }

    #[test]
    fn test_3des_encrypt_decrypt() {
        let key = [0u8; 24];
        let iv = [0u8; 8];
        let des3 = TripleDes::new(&key, &iv).unwrap();

        let plaintext = b"Hello CIE World!";
        let ciphertext = des3.encrypt(plaintext).unwrap();
        let decrypted = des3.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }
}
