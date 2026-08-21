---
title: Architettura e protocollo
description: Il percorso fino a una firma, i cinque crate del workspace
---

## Il percorso fino a una firma

Ogni comando che tocca la carta ripete la stessa sequenza, ed è quella che
`cie auth` esegue da sola:

1. **Connessione** al lettore e identificazione della carta dall'ATR
2. **Canale sicuro** - Diffie-Hellman e autenticazione mutua DAPP, con verifica
   della firma del chip; da qui in poi tutto viaggia cifrato in 3DES-CBC con
   Retail MAC verificato in entrambe le direzioni
3. **VERIFY PIN** dentro il canale
4. **Lettura del certificato** dal file `0x1003`

I file protetti richiedono il canale attivo: il PIN da solo non basta, e la CIE
rifiuta la `VERIFY` finchè il canale non è stabilito.

Il PIN viene risolto **prima** che il lettore venga toccato. Se manca o non è
otto cifre numeriche, meglio scoprirlo senza aver aperto un canale sicuro per
niente e senza consumare un tentativo.

## I crate

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
│       ├── detached.rs          firma RSA grezza, con verifica locale
│       └── keyring.rs           il PIN nel portachiavi, e il lucchetto che
│                                gli rimette prima di ogni lettura
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
vede mai una chiave: consegna 32 byte e ne riceve 256. è lo stesso schema che
usano gli HSM e la firma remota, ed è la ragione per cui librerie di firma
scritte per chiavi in memoria funzionano con una chiave non estraibile.

## Perchè i confini stanno dove stanno

**`cie-lib` resta il livello carta puro.** APDU, canale sicuro, PIN, lettura
file. Niente contenitori di firma, niente OpenPGP. è la forma che serve al
futuro modulo PKCS#11, che deve esporre `C_Sign` senza trascinarsi dietro il
resto.

**`cie-firma` sta in mezzo apposta.** `underskrift` si porta dietro lopdf e un
parser di font: roba che non ha niente a che fare con una APDU, e che il modulo
PKCS#11 non deve ereditare.

**`cie-pgp` è un crate a parte per la stessa ragione.** `cie-gui` dipende da
`cie-firma`; Sequoia dentro `cie-firma` finirebbe nella build della GUI senza
servire a niente.

## L'adapter OpenPGP

`cie-pgp` vede la carta **solo** attraverso `CryptoSigner`: nessun packet
OpenPGP arriva vicino a una APDU, e nessuna logica di carta entra nel livello
OpenPGP.

L'adapter è due formati coincidono già: la carta riceve un
`DigestInfo(SHA-256)` e restituisce PKCS#1 v1.5, che com'è fatta
una firma RSA OpenPGP. Non c'è nessuna conversione, solo l'involucro da
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
`Signer::acceptable_hashes` è consultato solo dal firmatario di streaming, non
da `SignatureBuilder`: lasciato a sè, il builder sceglie SHA-512 e la carta
rifiuta. L'hash va imposto con `set_hash_algo`.
:::

## Il backend crittografico di Sequoia

`sequoia-openpgp` Per evitare di complicare la build con librerie di sistema,
uso `crypto-rust`.
