---
title: Limiti e stato
description: Cosa funziona, cosa manca, e cosa questo strumento non è - in particolare riguardo a eIDAS.
---

## Non sono firme qualificate

Il certificato che la CIE custodisce è **di autenticazione**, non di firma
qualificata:

| | |
|---|---|
| Policy | `1.3.76.47.4` |
| Extended Key Usage | `TLS Web Client Authentication` |
| Key Usage | senza `nonRepudiation` |

Le firme prodotte con questa chiave sono crittograficamente valide e
verificabili, ma **non sono firme digitali qualificate ai sensi eIDAS** e non
sostituiscono un dispositivo di firma qualificata. Dimostrano il controllo della
carta, che è un'altra cosa.

Questo vale per PAdES, CAdES, la firma nuda e la certification OpenPGP allo
stesso modo.

## Stato

Funzionante e verificato su carta reale (Bit4Id):

- Rilevamento del tipo di carta dall'ATR e selezione dell'applicazione
- Secure Messaging completo: Diffie-Hellman, DAPP (autenticazione **mutua**, con
  verifica della firma del chip), 3DES-CBC, Retail MAC verificato in entrambe le
  direzioni
- Verifica del PIN nel canale sicuro
- Lettura dei file protetti: seriale, certificato, dati del titolare
- Firma on-card SHA-256 / PKCS#1 v1.5, verificata in locale
- PDF firmati in **PAdES B-B** con riquadro visibile, senza mai estrarre la chiave
- CAdES B-B enveloping (`.p7m`) su qualunque file, verificato con OpenSSL
- Certification OpenPGP, verificata con GnuPG e con Sequoia

## Cosa manca

**Marca temporale (livello B-T).** Richiede una chiamata di rete a una TSA. Oggi
il percorso di firma non parla con nessuno all'infuori della carta: senza la
feature `tsp` nel build non c'è nemmeno un client HTTP. Senza marca temporale la
validità nel tempo dipende da quella del certificato.

**Modulo PKCS#11.** Un `cdylib` per Firefox e per i client SPID/CieID. è la
ragione per cui `cie-lib` resta un livello carta puro.

**LTV.** La carta espone solo il proprio certificato: le CA del Ministero non
sono sul chip, e senza quelle non si costruisce una catena completa da
incorporare.

## Limiti della certificazione OpenPGP

- **L'email non è attestata dalla CIE.** Vedi la
  [specifica](/cie-cli/progetto/specifica-openpgp/).
- **Senza il certificato X.509, un terzo non può stabilire che la chiave
  certificatrice sia davvero una CIE.** Scelta deliberata: il `.cer` contiene il
  codice fiscale.
- **Nessuna prova a conoscenza zero.** Dimostrare «questa chiave è stata emessa
  dalla PKI CIE» senza rivelare *chi* richiederebbe una prova ZK sulla catena
  X.509. Esiste come ricerca, non come qualcosa da mettere in questo progetto. Il
  motivo è strutturale: la CA firma il `TBSCertificate`, che contiene chiave
  pubblica e dati anagrafici insieme, e cambiare un byte invalida la firma.

## Piattaforme

Sviluppato e verificato su Linux. Il codice non ha dipendenze di sistema oltre a
PC/SC, e il portachiavi passa dalla stessa API su tutti e tre i sistemi, ma
Windows e macOS non sono stati esercitati con una carta reale.

## Note di licenza

MIT. Le sequenze APDU, i parametri di protocollo e le costanti della coppia di
chiavi DAPP derivano dal sorgente di
[cie-middleware-linux](https://github.com/italia/cie-middleware-linux)
(BSD-3-Clause), che richiede il mantenimento della nota di copyright per le
porzioni derivate.
