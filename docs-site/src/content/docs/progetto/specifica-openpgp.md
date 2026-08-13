---
title: Specifica della certification
description: Formato esatto degli artefatti prodotti da cie gpg certify, e procedura di verifica per un terzo.
---

Questa pagina descrive cosa `cie gpg certify` produce e come verificarlo, con
abbastanza dettaglio da scrivere un verificatore indipendente. Profilo `v1`.

Per l'uso quotidiano basta la [guida](/riferimento/openpgp/).

## Modello di fiducia

```
identità CIE
      │  attestata dal Ministero nel certificato X.509
      ▼
chiave RSA della CIE                 ─── stessa chiave ───┐
      │  firma on-card, PKCS#1 v1.5                       │
      ▼                                                   ▼
certification OpenPGP                        certificato X.509 (.cer)
      │                                                   │
      ▼                                                   ▼
chiave pubblica OpenPGP + User ID              PKI della CIE
```

Il punto di giunzione è che la chiave RSA usata per la certification OpenPGP e
quella dentro il certificato X.509 sono **la stessa chiave**. Un verificatore lo
stabilisce confrontando modulo ed esponente; tutto il resto discende da li'.

## Artefatti

| File | Formato | Contiene dati personali |
|---|---|---|
| `<chiave>-certificata.asc` | OpenPGP armor ASCII | nome ed email già nella chiave |
| `cie-certifier.asc` | OpenPGP armor ASCII | nome, cognome, numero della carta |
| `cie-certifier.cer` | X.509 DER | **anche il codice fiscale** |

## Il certificatore CIE

Un certificato OpenPGP minimo: primary key, una User ID, una self-signature.

### Primary key

Packet chiave pubblica **v4**, algoritmo `1` (RSA Encrypt or Sign).

| Campo | Valore |
|---|---|
| MPI `n` | modulo, dal `SubjectPublicKeyInfo` del certificato X.509 |
| MPI `e` | esponente pubblico, dalla stessa fonte |
| creation time | `notBefore` del certificato X.509, in secondi dall'epoca |

Il creation time viene dal certificato e **non** dall'ora corrente. Poichè la
fingerprint OpenPGP v4 è `SHA-1(0x99 ‖ len ‖ 0x04 ‖ creation_time ‖ algo ‖
MPI)`, questo la rende una funzione deterministica del solo certificato X.509: a
partire dal `.cer` un verificatore può **ricalcolare** la fingerprint attesa e
confrontarla, senza fidarsi di quello che gli è stato consegnato.

Rieseguire il comando produce perciò sempre la stessa chiave, non una nuova.

### User ID

```
<COGNOME> <NOME> (CIE <ID_Servizi>)
```

Cognome e nome vengono dagli attributi `surname` (2.5.4.4) e `givenName`
(2.5.4.42) del subject; in mancanza, dal `commonName` (2.5.4.3). L'`ID_Servizi`
è il numero letto dal file `0x1001` della carta, che non richiede PIN. Se non è
leggibile la User ID diventa `<NOME> (CIE)`.

Non contiene **nessuna email**: la CIE non ne attesta nessuna, e una User ID
senza email non fa asserzioni implicite. Non contiene il codice fiscale.

### Self-signature

| Campo | Valore |
|---|---|
| tipo | `0x13` Positive certification |
| hash | SHA-256 |
| algoritmo | RSA, PKCS#1 v1.5 |
| creation time | uguale a quello della chiave |

Subpacket nell'area hashed:

| Subpacket | Valore |
|---|---|
| Key Flags | `0x01` - solo certification |
| Preferred Hash Algorithms | `[SHA256]` |
| Primary User ID | vero |
| Issuer / Issuer Fingerprint | della chiave stessa |

Le key flags dichiarano una chiave che **solo certifica**: non cifra e non firma
documenti, e dirlo evita che qualche strumento ci provi.

## La certification

Aggiunta alla chiave dell'utente come singolo packet di firma. Nessun altro
packet viene modificato, aggiunto o rimosso.

| Campo | Valore |
|---|---|
| tipo | `0x10` Generic certification |
| hash | SHA-256 |
| algoritmo | RSA, PKCS#1 v1.5 |
| creation time | l'istante della certificazione |

### Perchè 0x10 e non 0x13

`0x13` dichiara che l'emittente ha verificato accuratamente la User ID, e la
User ID comprende l'indirizzo email. La CIE attesta il nome del titolare;
dell'email non sa niente. `0x10` non fa asserzioni sul grado di verifica, quindi
non promette ciò che non si può mantenere.

Una certification OpenPGP lega la User ID intera e non è divisibile: le notation
sono l'unico meccanismo standard per dichiarare quale parte sia stata verificata.

### Notation

Tutte nell'area **hashed** - quindi coperte dalla firma - non critiche e marcate
human-readable.

| Nome | Valore | Significato |
|---|---|---|
| `cie-verified@cie-cli` | `holder-name` | l'unico dato verificato dalla CIE. **Ciò che non è elencato non è attestato**, in particolare non l'email |
| `cie-x509-sha256@cie-cli` | 64 cifre esadecimali minuscole | SHA-256 del certificato X.509 in DER |
| `cie-profile@cie-cli` | `v1` | versione di questo profilo |

Il DER del certificato X.509 **non** viene incorporato: contiene il codice
fiscale in chiaro, e il materiale OpenPGP è pubblico e irrevocabile una volta
diffuso. La notation `cie-x509-sha256` è un impegno vincolante firmato dalla
carta: chi riceverà il `.cer` potrà verificare che sia esattamente quello
legato a questa firma, e nessuno può sostituirlo a posteriori.

Nella firma compare anche `salt@notations.sequoia-pgp.org`, 32 byte casuali che
Sequoia inserisce in ogni firma v4 per renderla imprevedibile. Non ha significato
per questo profilo e va ignorata.

### Firma

Il digest SHA-256 calcolato da Sequoia viene passato alla carta, che costruisce
il `DigestInfo`, applica il padding PKCS#1 v1.5 ed esegue l'operazione RSA. Il
risultato è esattamente il formato di una firma RSA OpenPGP e viene inserito
tale e quale come MPI.

La chiave privata non lascia la carta in nessun momento.

## Verifica

### 1. La certification

Con GnuPG:

```bash
gpg --import cie-certifier.asc chiave-certificata.asc
gpg --check-sigs <fingerprint>    # cerca `sig!`
```

Con `cie-cli`:

```bash
cie gpg verify chiave-certificata.asc --certifier cie-certifier.asc
```

Con Sequoia, `Signature::verify_userid_binding(chiave_cie, chiave_utente, uid)`.

Superato questo passo si sa che **quella chiave RSA** ha certificato quella User
ID. Non ancora che sia una CIE.

### 2. Il certificato X.509

Serve il `.cer`, che il titolare consegna a chi decide.

```bash
# Deve corrispondere alla notation cie-x509-sha256@cie-cli
sha256sum cie-certifier.cer
```

### 3. Il legame fra i due mondi

è il passo che rende la catena una catena. Confronta la chiave pubblica RSA del
certificato X.509 con quella del certificatore OpenPGP:

```bash
openssl x509 -inform DER -in cie-certifier.cer -noout -modulus
gpg --export <keyid-certificatore> | gpg --list-packets | grep -A2 'pkey\[0\]'
```

Il modulo deve essere lo stesso. In alternativa, e in modo piu' robusto,
ricalcola la fingerprint OpenPGP attesa a partire dal `.cer`, come descritto
sopra: `notBefore` piu' modulo ed esponente determinano interamente la chiave, e
quindi la sua fingerprint.

### 4. La PKI

```bash
openssl verify -CAfile ministero.pem -inform DER cie-certifier.cer
```

La carta espone solo il proprio certificato: le CA del Ministero non sono sulla
carta e vanno procurate a parte. `cie gpg verify` non valida la catena e non
pretende di farlo - date di validità, name chaining, estensioni e revoca sono un
validatore X.509 intero, e `openssl` esiste già.

## Cosa la certification non dice

- **Non dice che l'email sia del titolare.** Vedi `cie-verified@cie-cli`.
- **Non è una firma qualificata eIDAS.** Il certificato sulla CIE è di
  autenticazione (policy `1.3.76.47.4`, EKU `TLS Web Client Authentication`, Key
  Usage senza `nonRepudiation`).
- **Non attesta un istante.** Il creation time della firma non è marcato
  temporalmente da nessuno; è un'affermazione di chi firma.
- **Non è revocabile dalla CIE dopo il fatto**, se non emettendo una revocation
  signature con la stessa carta.

## Compatibilità verificata

Con GnuPG 2.4.9, in un test che gira nella suite: import di entrambi i
certificati, `gpg --check-sigs` che mostra `sig:!`, e la chiave certificata che
continua a firmare e a verificare. Con Sequoia 2.4, che è la libreria che li
costruisce.
