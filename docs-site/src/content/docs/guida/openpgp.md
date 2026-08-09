---
title: Certificare una chiave OpenPGP
description: Legare la propria chiave OpenPGP al titolare della CIE con una certification standard, verificabile da GnuPG.
---

`cie gpg certify` firma con la chiave della carta una **certification OpenPGP
standard** sulla tua chiave. E' il modo di dire

> questa User ID e questa chiave pubblica sono state certificate dal titolare di
> quella CIE

con un artefatto che GnuPG capisce da solo, senza strumenti speciali e senza
nessun formato inventato per l'occasione.

La chiave privata resta nella carta, come per ogni altra firma.

## Cosa serve

Una chiave OpenPGP che hai gia'. Se non ne hai una:

```bash
gpg --quick-gen-key "Mario Rossi <mario@example.com>"
gpg --armor --export mario@example.com > chiave.asc
```

## Certificare

```bash
cie gpg certify chiave.asc
```

Il comando apre il canale sicuro, chiede il PIN, e produce tre file.

```
  Chiave certificata:  chiave-certificata.asc
  Certificatore CIE:   cie-certifier.asc
  Certificato X.509:   cie-certifier.cer
```

| File | Contenuto | Dove va |
|---|---|---|
| `chiave-certificata.asc` | la tua chiave, piu' la certification | pubblicabile |
| `cie-certifier.asc` | la chiave della CIE come certificato OpenPGP | pubblicabile |
| `cie-certifier.cer` | il certificato X.509 della carta | **resta a te** |

:::danger[Il `.cer` contiene il tuo codice fiscale]
Ce lo mette il Ministero, nel subject del certificato: non e' una scelta di
`cie-cli`. Serve solo a chi deve risalire dalla chiave alla PKI, e glielo
consegni tu, a chi decidi. Nel materiale pubblicabile il codice fiscale non
compare mai. Vedi [Cosa diventa pubblico](/cie-cli/riferimento/privacy/).
:::

### Perche' tre file e non uno

GnuPG non verifica una certification se non ha la chiave di chi l'ha emessa: la
mostrerebbe come `sig?  [no public key]` e basta. `cie-certifier.asc` e' la
chiave RSA della carta rappresentata come certificato OpenPGP autoportante,
auto-firmato dalla carta stessa. Senza, la certification non e' verificabile da
nessuno.

## Verificare

```bash
gpg --import cie-certifier.asc chiave-certificata.asc
gpg --check-sigs <fingerprint>
```

```
sig!  1A2B3C4D5E6F7890 2026-08-09  ROSSI MARIO (CIE 1234567890)
```

Il punto esclamativo dopo `sig` e' GnuPG che dice di aver **verificato** la
firma, non solo di averla vista. Un `sig?` significa che manca la chiave del
certificatore; un `sig-` che la verifica e' fallita.

Anche `cie-cli` sa verificare, e mostra in piu' le notation:

```bash
cie gpg verify chiave-certificata.asc --certifier cie-certifier.asc
```

## Piu' di una User ID

Se la chiave ne ha piu' d'una, il comando chiede quale certificare:

```
Quale User ID certificare?
  1. Mario Rossi <mario@example.com>
  2. Mario Rossi <m.rossi@lavoro.example>
  Scelta [1-2]:
```

Si sceglie in anticipo con `--uid N`, contando da 1. Per certificarne piu' di
una si rilancia il comando: ogni certification e' un atto a se'.

## Se il nome non corrisponde

`cie-cli` confronta il nome della User ID con quello sulla CIE, ignorando ordine,
maiuscole, accenti e punteggiatura. Se non combaciano:

```
  ⚠ Il nome della User ID non corrisponde a quello sulla CIE.
      CIE:      ROSSI MARIO
      User ID:  m.rossi

  Certificare comunque questa User ID? [s/N]
```

Non e' un blocco: certificare uno pseudonimo e' legittimo, e la certification
resta valida. Deve pero' essere una scelta, non una distrazione. `--yes` salta
la domanda, per gli script.

## L'email non e' certificata dalla CIE

La CIE conosce il nome del titolare. Di un indirizzo email non sa niente, e
questa implementazione non finge il contrario.

Una certification OpenPGP lega la User ID **intera** — nome ed email insieme — e
non e' divisibile. Per questo la firma e' di tipo `0x10` (generic) e non `0x13`
(positive): `0x13` dichiarerebbe che l'emittente ha verificato accuratamente la
User ID, email compresa. Cosa sia stato verificato davvero lo dice la notation
`cie-verified@cie-cli`, che contiene `holder-name` e nient'altro.

Se ti serve legare anche l'email, serve una verifica indipendente — un challenge
via email, tipicamente — che questo strumento non fa e non pretende di fare.

## La chiave continua a funzionare

Una certification e' un packet che si aggiunge; non tocca ne' la chiave ne' le
sue subkey. Dopo `cie gpg certify` la chiave firma i commit esattamente come
prima, e le firme verificano come prima. E' verificato da un test che genera la
chiave con GnuPG, la certifica e poi la rimette a firmare.

## Pubblicare, o no

La certification viaggia **solo se la esporti**:

```bash
gpg --export chiave                              # con la certification
gpg --export --export-options export-minimal     # senza
```

Quindi puoi tenere la chiave certificata in locale e caricare su GitHub quella
nuda, mostrando la certification solo a chi ti chiede di dimostrare l'identita'.

Se la esporti, diventano pubblici nome, cognome e numero della carta, che stanno
nella User ID del certificatore. Il codice fiscale no: quello e' solo nel `.cer`.

## Cosa resta fuori dalla portata

Chi ha solo il materiale OpenPGP puo' verificare che la chiave X abbia
certificato quella User ID. Ma la stringa `(CIE 1234567890)` dentro la User ID di
X e' un'auto-asserzione: senza il certificato X.509 nessuno puo' stabilire che X
sia davvero una CIE e non una chiave RSA qualunque.

E' una scelta deliberata, e reversibile in una sola direzione: il `.cer` si puo'
sempre consegnare dopo, mentre un codice fiscale finito su un keyserver non si
toglie piu'.

La [specifica](/cie-cli/riferimento/specifica-openpgp/) descrive la procedura
completa per chi riceve il `.cer` e vuole risalire fino alla PKI.
