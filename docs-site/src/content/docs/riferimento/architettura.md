---
title: Architettura
description: I cinque crate del workspace, perche' i confini stanno dove stanno, e come si comunicano.
---

```
crates/
├── cie-lib/                     libreria core
│   └── src/
│       ├── pcsc_direct.rs       trasporto PC/SC, APDU grezze
│       ├── apdu.rs              tipo carta da ATR, selezione AID, lettura file
│       ├── secure_messaging.rs  DH, DAPP, cifratura e MAC del canale
│       ├── crypto.rs            3DES-CBC, Retail MAC, derivazione chiavi
│       ├── asn1.rs              parser BER-TLV (tag fino a 4 byte)
│       ├── titolare.rs          estrazione dati dal certificato X.509
│       └── error.rs
├── cie-firma/                   firmatario on-card e contenitori
│   └── src/
│       ├── signer.rs            CieSigner: la carta come CryptoSigner
│       ├── pades.rs             PAdES B-B, riquadro visibile
│       ├── cades.rs             CAdES enveloping (.p7m)
│       └── detached.rs          firma RSA grezza, con verifica locale
├── cie-pgp/                     certificazione OpenPGP
│   └── src/
│       ├── signer.rs            la carta come firmatario Sequoia
│       └── lib.rs               certificatore CIE e certification
├── cie-cli/                     binario `cie`
└── cie-gui/                     binario `cie-gui`
```

## Il cardine: `CryptoSigner`

Tutto il resto discende da un trait piccolo:

```rust
fn sign_hash(&self, hash: &[u8]) -> Result<Vec<u8>, CryptoError>;
fn certificate_der(&self) -> &[u8];
```

`CieSigner` lo implementa mandando il digest alla carta. Nessun livello sopra
vede mai una chiave: consegna 32 byte e ne riceve 256. E' lo stesso schema che
usano gli HSM e la firma remota, ed e' la ragione per cui librerie di firma
scritte per chiavi in memoria funzionano con una chiave non estraibile.

## Perche' i confini stanno dove stanno

**`cie-lib` resta il livello carta puro.** APDU, canale sicuro, PIN, lettura
file. Niente contenitori di firma, niente OpenPGP. E' la forma che serve al
futuro modulo PKCS#11, che deve esporre `C_Sign` senza trascinarsi dietro il
resto.

**`cie-firma` sta in mezzo apposta.** `underskrift` si porta dietro lopdf e un
parser di font: roba che non ha niente a che fare con una APDU, e che il modulo
PKCS#11 non deve ereditare.

**`cie-pgp` e' un crate a parte per la stessa ragione.** `cie-gui` dipende da
`cie-firma`; Sequoia dentro `cie-firma` finirebbe nella build della GUI senza
servire a niente.

## L'adapter OpenPGP

`cie-pgp` vede la carta **solo** attraverso `CryptoSigner`: nessun packet
OpenPGP arriva vicino a una APDU, e nessuna logica di carta entra nel livello
OpenPGP.

L'adapter e' sottile perche' i due formati coincidono gia': la carta riceve un
`DigestInfo(SHA-256)` e restituisce PKCS#1 v1.5, che e' esattamente com'e' fatta
una firma RSA OpenPGP. Non c'e' nessuna conversione, solo l'involucro da
cambiare.

```rust
impl sequoia_openpgp::crypto::Signer for CieSequoiaSigner<'_> {
    fn public(&self) -> &Key<PublicParts, UnspecifiedRole> { &self.key }
    fn sign(&mut self, algo: HashAlgorithm, digest: &[u8]) -> Result<mpi::Signature> {
        // algo != SHA256 → errore: la carta non sa fare altro
    }
}
```

Sequoia costruisce e verifica la semantica OpenPGP; `cie-cli` fornisce la
primitiva di firma hardware. Nessun packet, nessuna canonicalizzazione e nessun
hashing di certification scritti a mano.

:::note[Una trappola che vale la pena conoscere]
`Signer::acceptable_hashes` e' consultato solo dal firmatario di streaming, non
da `SignatureBuilder`: lasciato a se', il builder sceglie SHA-512 e la carta
rifiuta. L'hash va imposto con `set_hash_algo`.
:::

## Il backend crittografico di Sequoia

`sequoia-openpgp` usa di default nettle, una dipendenza C che complica la build
multipiattaforma. Qui si usa `crypto-rust`, in Rust puro, senza dipendenze di
sistema.

E' adeguato al caso d'uso: in questo livello non si fa crittografia segreta —
solo hash, verifica RSA e serializzazione di packet — perche' la chiave privata
e' nella carta.

## Lingua del codice

`cie-firma`, `cie-pgp` e `cie-gui` sono scritti in inglese; in italiano restano
solo le stringhe che l'utente legge. `cie-lib` e `cie-cli` sono ancora in
italiano.
