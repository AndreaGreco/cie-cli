---
title: Primi passi
description: Cosa la CIE espone senza PIN, cosa richiede il canale sicuro, e come leggere i dati protetti.
---

La CIE divide quello che ha in due: qualche file pubblico, leggibile appena la
carta e' sul lettore, e tutto il resto dietro un canale cifrato e la verifica del
PIN.

## Senza PIN

```bash
cie info
```

Legge il tipo di carta dall'ATR e i file pubblici, fra cui l'**ID_Servizi**: le
dodici cifre stampate sulla carta, che identificano quella CIE. Nessun PIN,
nessun canale sicuro, nessun contatore di tentativi toccato.

E' il comando da usare per capire se lettore e carta si parlano, prima di
mettere in mezzo il PIN.

## Diagnostica

```bash
cie pcsc-test
cie -vv info     # ogni APDU, in TX e RX
```

`-v` alza la verbosita' a passi: `-v` informativo, `-vv` ogni comando scambiato,
`-vvv` tutto. Quando qualcosa non torna, `-vv` e' il primo posto dove guardare:
si vedono le APDU e le status word che la carta restituisce.

## Con il PIN

```bash
cie auth
```

Fa il percorso completo, ed e' quello che ogni comando di firma ripete:

1. **Connessione** al lettore e identificazione della carta dall'ATR
2. **Canale sicuro** — Diffie-Hellman e autenticazione mutua DAPP, con verifica
   della firma del chip; da qui in poi tutto viaggia cifrato in 3DES-CBC con
   Retail MAC verificato in entrambe le direzioni
3. **VERIFY PIN** dentro il canale
4. **Lettura del certificato** dal file `0x1003`

I file protetti richiedono il canale attivo: il PIN da solo non basta, e la CIE
rifiuta la `VERIFY` finche' il canale non e' stabilito.

Con il certificato in mano, `cie auth` mostra i dati del titolare che vi sono
contenuti — nome, cognome, codice fiscale.

Per tenerne una copia:

```bash
cie auth --dump-cert certificato.cer
```

:::caution[Il certificato contiene il codice fiscale]
Il `.cer` che esce da qui e' lo stesso che serve a verificare le firme, e porta
il codice fiscale in chiaro nel subject. Non e' un file da pubblicare alla
leggera. Vedi [Cosa diventa pubblico](/cie-cli/riferimento/privacy/).
:::

## Il contatore dei tentativi

Il PIN della CIE ha un contatore di tentativi. Sbagliarlo ripetutamente blocca
la carta, e sbloccarla richiede il PUK. `cie-cli` non ha nessun modo di
aggirarlo, ed e' bene che sia cosi'.

Per questo il PIN viene risolto **prima** che il lettore venga toccato: se manca
o e' malformato, meglio scoprirlo senza aver aperto un canale sicuro per niente.
Un PIN che non sia otto cifre numeriche viene rifiutato in locale, senza
arrivare alla carta.

## E poi

- [Il PIN](/cie-cli/guida/pin/) — portachiavi di sistema, prompt, cosa non viene salvato
- [Firmare documenti](/cie-cli/guida/firma/) — PAdES, CAdES, firma nuda
- [Certificare una chiave OpenPGP](/cie-cli/guida/openpgp/)
