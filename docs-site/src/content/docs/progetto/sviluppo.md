---
title: Sviluppo
description: Test, vettori crittografici, test con la carta reale, e come lavorare su questo sito.
---

```bash
cargo test --workspace     # 67 test, nessuno richiede la carta
cargo clippy --workspace --all-targets
```

Il `justfile` ha le scorciatoie: `just test`, `just clippy`.

## Come sono fatti i test

I test crittografici usano **vettori calcolati indipendentemente**, non
l'output del codice stesso.

Il Retail MAC e' ancorato al vettore pubblicato in **ICAO Doc 9303** (BAC), che
usa lo stesso ISO 9797-1 Algorithm 3 del Secure Messaging CIE: se quel test
passa, il MAC e' conforme allo standard e non solo a se' stesso.

C'e' anche un vettore end-to-end di una `VERIFY PIN` completa in Secure
Messaging, calcolato con un riferimento esterno. Protegge dall'errore piu'
insidioso del protocollo: paddare i TLV singolarmente invece di applicare un solo
ISO-padding finale.

## Firmare senza carta

`cie-firma` espone un firmatario software che imita il contratto della carta —
riceve un hash, costruisce il `DigestInfo`, applica PKCS#1 v1.5 — dietro la
feature `test-signer`:

```toml
[dev-dependencies]
cie-firma = { workspace = true, features = ["test-signer"] }
```

Il percorso di firma che i test attraversano e' quindi quello vero, meno il
lettore. E' cosi' che `cie-pgp` verifica la costruzione della certification, il
digest, la struttura della firma e l'interoperabilita' con GnuPG senza una CIE.

## I test che richiedono la carta

Tre test sono `#[ignore]` perche' servono un lettore e una carta:

```bash
CIE_PIN=12345678 cargo test -p cie-pgp --test carta -- --ignored --nocapture
```

Quello di `cie-pgp` verifica anche che la chiave importata dall'adapter sia
davvero quella del certificato X.509: se modulo o esponente fossero letti male,
la certification verificherebbe comunque contro la chiave sbagliata.

## Interoperabilita' con GnuPG

Il test che importa in GnuPG si salta da solo se `gpg` non e' installato — un
ambiente senza GnuPG non e' un fallimento, ma non deve nemmeno passare in
silenzio. Genera la chiave **con GnuPG**, la fa certificare, reimporta, controlla
che `gpg --check-sigs` mostri `sig:!`, e poi rimette la chiave a firmare per
verificare che sia rimasta utilizzabile.

## Questo sito

Astro con Starlight, in `docs-site/`.

```bash
cd docs-site
bun install
bun run dev      # anteprima locale
bun run build    # come lo costruisce la CI
```

Le pagine stanno in `docs-site/src/content/docs/`, la barra laterale in
`astro.config.mjs`. La pubblicazione sulle GitHub Pages e' automatica a ogni
push su `main` che tocchi `docs-site/`.

Il sito vive sotto `/cie-cli/`, quindi i link interni vanno scritti con quel
prefisso.

## I documenti di progettazione

Le decisioni di design stanno in `docs/superpowers/specs/` nel repository. Sono
documenti di progettazione — discutono alternative e registrano correzioni
emerse durante l'implementazione — non documentazione d'uso: la specifica per
chi deve verificare una certification e'
[qui](/cie-cli/riferimento/specifica-openpgp/).

- [Certificazione OpenPGP](https://github.com/AttilioGreco/cie-cli/blob/main/docs/superpowers/specs/2026-08-09-certificazione-openpgp-cie-design.md)
- [Interfaccia grafica](https://github.com/AttilioGreco/cie-cli/blob/main/docs/superpowers/specs/2026-08-08-cie-gui-iced-design.md)

## Riferimenti

- [cie-middleware-linux](https://github.com/italia/cie-middleware-linux) — il
  middleware ufficiale (BSD-3-Clause), da cui derivano le sequenze APDU e i
  parametri di protocollo, in particolare `cie-pkcs11/CSP/IAS.cpp`
- [ICAO Doc 9303](https://www.icao.int/publications/pages/publication.aspx?docnum=9303)
  — Machine Readable Travel Documents, per il Secure Messaging
- [Specifiche CIE 3.0](https://www.cartaidentita.interno.gov.it/)
- [RFC 9580](https://www.rfc-editor.org/rfc/rfc9580.html) — OpenPGP
