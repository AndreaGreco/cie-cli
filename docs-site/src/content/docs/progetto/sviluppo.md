---
title: Compilare e testare
description: Compilare dai sorgenti, test, vettori crittografici e test con la carta reale.
---

## Compilare

Serve Rust stabile e gli header di `libpcsclite` (`libpcsclite-dev` su
Debian/Ubuntu, `pcsclite` su Arch).

```bash
git clone https://github.com/AttilioGreco/cie-cli
cd cie-cli
cargo build --release
```

Il binario esce in `target/release/cie`. Il secondo binario, `cie-gui`, si
compila con `cargo build --release -p cie-gui`.

### La dipendenza `underskrift`

PAdES e CAdES arrivano da [`underskrift`](https://github.com/AttilioGreco/underskrift),
e per ora serve un fork: il lavoro non è ancora terminato ed è stata fatta una
PR all'autore originale, per il momento dobbiamo usare il fork. Il `Cargo.toml`
del workspace è già configurato verso il fork.

```toml
[patch.crates-io]
underskrift = { git = "https://github.com/AttilioGreco/underskrift", branch = "main" }
```

Il fork sta su git e non su un path locale apposta: la CI di release deve poterlo
risolvere. Se ci lavori sopra in locale, copia `.cargo/config.toml.example` in
`.cargo/config.toml` per puntare al tuo path locale. In questo modo puoi fare
modifiche a `underskrift`.

:::danger[Non committare `Cargo.lock` con il patch locale attivo]
Finchè `.cargo/config.toml` è in uso, `Cargo.lock` punta al path locale. La CI
di release non riuscirebbe a risolverlo, fallendo la build.
:::

## Test

```bash
cargo test --workspace     # 67 test, nessuno richiede la carta
cargo clippy --workspace --all-targets
```

Il `justfile` ha le scorciatoie: `just test`, `just clippy`.

## Come sono fatti i test

I test crittografici usano **vettori calcolati indipendentemente**, non
l'output del codice stesso.

Il Retail MAC è ancorato al vettore pubblicato in **ICAO Doc 9303** (BAC), che
usa lo stesso ISO 9797-1 Algorithm 3 del Secure Messaging CIE: se quel test
passa, il MAC è conforme allo standard e non solo a sè stesso.

C'è anche un vettore end-to-end di una `VERIFY PIN` completa in Secure
Messaging, calcolato con un riferimento esterno. Protegge dall'errore più
insidioso del protocollo: paddare i TLV singolarmente invece di applicare un solo
ISO-padding finale.

## Firmare senza carta

`cie-firma` espone un firmatario software che imita il contratto della carta -
riceve un hash, costruisce il `DigestInfo`, applica PKCS#1 v1.5 - dietro la
feature `test-signer`:

```toml
[dev-dependencies]
cie-firma = { workspace = true, features = ["test-signer"] }
```

Il percorso di firma che i test attraversano è quindi quello vero, meno il
lettore. è cosi' che `cie-pgp` verifica la costruzione della certification, il
digest, la struttura della firma e l'interoperabilità con GnuPG senza una CIE.

## I test che richiedono la carta

Tre test sono `#[ignore]` perchè servono un lettore e una carta:

```bash
CIE_PIN=12345678 cargo test -p cie-pgp --test carta -- --ignored --nocapture
```

Quello di `cie-pgp` verifica anche che la chiave importata dall'adapter sia
davvero quella del certificato X.509: se modulo o esponente fossero letti male,
la certification verificherebbe comunque verso la chiave sbagliata.

## Interoperabilità con GnuPG

Il test che importa in GnuPG si salta da solo se `gpg` non è installato - un
ambiente senza GnuPG non è un fallimento, ma non deve nemmeno passare in
silenzio. Genera la chiave **con GnuPG**, la fa certificare, reimporta, controlla
che `gpg --check-sigs` mostri `sig:!`, e poi rimette la chiave a firmare per
verificare che sia rimasta utilizzabile.


## Riferimenti

- [cie-middleware-linux](https://github.com/italia/cie-middleware-linux) - il
  middleware ufficiale (BSD-3-Clause), da cui derivano le sequenze APDU e i
  parametri di protocollo, in particolare `cie-pkcs11/CSP/IAS.cpp`
- [ICAO Doc 9303](https://www.icao.int/publications/pages/publication.aspx?docnum=9303)
  - Machine Readable Travel Documents, per il Secure Messaging
- [Specifiche CIE 3.0](https://www.cartaidentita.interno.gov.it/)
- [RFC 9580](https://www.rfc-editor.org/rfc/rfc9580.html) - OpenPGP
