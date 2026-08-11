---
title: Certificare una chiave OpenPGP
description: Legare la propria chiave OpenPGP al titolare della CIE con una certification standard, verificabile da GnuPG.
---

:::note[Questa pagina serve a poche persone]
Riguarda chi ha già una **chiave OpenPGP** e la usa per firmare email o
commit. Se sei arrivato qui per firmare un PDF, ti serve [Firmare un
documento](/cie-cli/guida/firmare/).
:::

`cie gpg certify` usa la carta per firmare una **certification OpenPGP
standard** sulla tua chiave: un modo di dire

> questa User ID e questa chiave pubblica sono state certificate dal titolare
> di quella CIE

con un artefatto che GnuPG capisce da solo. La chiave privata resta nella
carta, come per ogni altra firma.

## Il giro completo

Cinque comandi, in una directory vuota. La carta nel lettore.

```bash
mkdir -p ~/cie-pgp && cd ~/cie-pgp

# 1. la fingerprint della tua chiave: la riga di 40 caratteri sotto "pub"
gpg --list-keys

# 2. esportala, certificala con la CIE, rimettila nel portachiavi
gpg --armor --export FINGERPRINT > chiave.asc
cie gpg certify chiave.asc
gpg --import cie-certifier.asc chiave-certificata.asc

# 3. controlla: deve comparire un sig! con il tuo nome
gpg --check-sigs FINGERPRINT
```

Sostituisci `FINGERPRINT` con la tua, senza spazi. `cie gpg certify` chiede il
PIN e, se la chiave ha più User ID, quale certificare.

L'ultimo comando è la prova che è andata:

```
sig!  1A2B3C4D5E6F7890 2026-01-15  ROSSI MARIO (CIE 0000000000)
```

Il **punto esclamativo** è GnuPG che dice di aver verificato la firma, non solo
di averla vista. Se lo vedi, hai finito: il resto della pagina serve solo se
qualcosa non torna, o se vuoi sapere cosa è successo.

## Cosa hai ottenuto

`cie gpg certify` scrive tre file nella directory corrente.

| File | Cos'è | Dove va |
|---|---|---|
| `chiave-certificata.asc` | la tua chiave, piu' la certification | pubblicabile |
| `cie-certifier.asc` | la chiave della CIE come certificato OpenPGP | pubblicabile |
| `cie-certifier.cer` | il certificato X.509 della carta | **resta a te** |

I primi due vanno **entrambi** importati, ed è il motivo per cui il comando al
punto 2 ne elenca due: GnuPG non verifica una firma se non ha la chiave di chi
l'ha emessa. Senza `cie-certifier.asc` la certification resta lì, non
verificabile da nessuno.

:::danger[Il `.cer` contiene il tuo codice fiscale]
Ce lo mette il Ministero, nel subject del certificato: non è una scelta di
`cie-cli`. Serve solo a chi deve risalire dalla chiave alla PKI, e glielo
consegni tu, a chi decidi. Nel materiale pubblicabile il codice fiscale non
compare mai. Vedi [Cosa diventa pubblico](/cie-cli/riferimento/privacy/).
:::

## Se qualcosa non torna

| Cosa vedi | Cosa significa |
|---|---|
| `sig?` invece di `sig!` | manca `cie-certifier.asc`: importalo con `gpg --import cie-certifier.asc` |
| `sig-` | la verifica è fallita: la chiave è stata modificata dopo la certificazione |
| `Malformed Cert: Additional packets found, is this a keyring?` | hai esportato più di una chiave: vedi qui sotto |
| `[User ID not found]` accanto alla firma | come `sig?`: manca il certificatore |

### "Additional packets found"

Succede esportando per **email** invece che per fingerprint. Per `gpg --export`
un indirizzo è una stringa di ricerca, non un identificatore: se compare su due
chiavi - quella in uso e una vecchia o revocata - le esporta tutte e due, dentro
un unico blocco che sembra normale.

`cie gpg certify` ne vuole una sola. Riesporta usando la fingerprint, che di
chiavi ne individua sempre una:

```bash
gpg --armor --export FINGERPRINT > chiave.asc
```

### Più di una User ID

Il comando chiede quale certificare:

```
Quale User ID certificare?
  1. Nome Cognome <nome@example.com>
  2. Nome Cognome <n.cognome@lavoro.example>
  Scelta [1-2]:
```

Si sceglie in anticipo con `--uid N`, contando da 1. Per certificarne più di
una si rilancia il comando: ogni certification è un atto a sè.

### Il nome non corrisponde alla CIE

`cie-cli` confronta il nome della User ID con quello sulla carta, ignorando
ordine, maiuscole, accenti e punteggiatura. Se non combaciano chiede conferma.

Non è un blocco: certificare uno pseudonimo è legittimo, e la certification
resta valida. Deve però essere una scelta, non una distrazione. `--yes` salta
la domanda.

## Verificare più a fondo

`gpg --check-sigs` basta a sapere che è andata. Se invece stai verificando la
chiave di **qualcun altro**, i livelli sono tre.

**1. GnuPG**, come sopra. È la verifica che conta di più, perché GnuPG è
un'implementazione indipendente da quella che ha prodotto la firma.

**2. `cie gpg verify`**, stesso controllo crittografico piu' le notation in
chiaro:

```bash
cie gpg verify chiave-certificata.asc --certifier cie-certifier.asc
```

Se su quella chiave non trova nessuna certification di quel certificatore esce
con errore, quindi anche il solo successo è un esito.

**3. Che il certificatore sia davvero una CIE.** I primi due dimostrano che
*una certa chiave* ha firmato la User ID; che quella chiave stia dentro una CIE
lo dice solo il certificato X.509, ed è la sezione qui sotto.

## Il `.cer`, per chi deve risalire alla PKI

Serve solo a chi ha ricevuto il `.cer` e vuole arrivare fino al Ministero. Se
stai certificando la tua chiave, puoi saltare tutto.

### Aprirlo

È un certificato X.509 in DER, cioè binario:

```bash
openssl x509 -in cie-certifier.cer -inform DER -noout -issuer -dates
openssl x509 -in cie-certifier.cer -inform DER -out cie-certifier.pem
```

Il secondo comando lo converte in PEM, la forma che molti strumenti si
aspettano. Nessuno dei due tocca l'originale.

:::caution
`-subject` e `-text` stampano il **codice fiscale**, che sta nel subject. Occhio
a dove finisce l'output: log, incolla in chat, screenshot.
:::

### `gpg` non lo importa, `gpgsm` sì

X.509 e OpenPGP sono due mondi, e i file prodotti stanno uno per parte:

| File | Comando |
|---|---|
| `cie-certifier.asc` (OpenPGP) | `gpg --import cie-certifier.asc` |
| `cie-certifier.cer` (X.509) | `gpgsm --import cie-certifier.cer` |

`gpgsm` è la parte X.509 di GnuPG e si installa insieme a GnuPG. Finché non ha
anche la catena del Ministero elencherà il certificato come non attendibile: è
corretto, non ha modo di risalire a una radice. Per la verifica della
certification - i livelli 1 e 2 - `gpgsm` non serve.

### Legare il `.cer` a questa firma

La certification contiene una notation `cie-x509-sha256@cie-cli` con l'SHA-256
del certificato. I due valori devono coincidere, altrimenti quel `.cer` è di
un'altra carta:

```bash
sha256sum cie-certifier.cer
```

### Trovare la CA del Ministero

Comincia dal certificato, che dice da chi è stato emesso e dove stanno revoca e
policy:

```bash
openssl x509 -in cie-certifier.cer -inform DER -noout \
  -issuer -ext authorityInfoAccess,crlDistributionPoints,certificatePolicies
```

```
issuer=CN=Issuing sub CA for the Italian Electronic Identity Card - SUBCA00N,
  OU=Direz. Centr. per i Servizi Demografici - CNSD, O=Ministero dell'Interno, C=IT
Authority Information Access:
    OCSP - URI:https://ocsp.cie.interno.gov.it/
X509v3 CRL Distribution Points:
    URI:http://ldap.cie.interno.gov.it/ciesubca00N.crl
X509v3 Certificate Policies:
    Policy: 1.3.76.47.4
      CPS: http://www.cartaidentita.interno.gov.it/policy/cittadini_cps.pdf
```

Il numero della sub-CA cambia da carta a carta, quindi la CA giusta è quella
che *quel* certificato nomina, non una scelta a priori.

Nota cosa **non** c'è nell'`Authority Information Access`: nessun `caIssuers`.
Il certificato dice dove chiedere se è stato revocato, ma non da dove scaricare
chi lo ha emesso. La catena non è auto-scaricabile e va procurata dal sito CIE
del Ministero, lo stesso dominio da cui è pubblicato il CPS. Quando la ottieni,
non fidarti del canale: confrontane l'impronta.

```bash
openssl x509 -in ministero.pem -noout -fingerprint -sha256
openssl verify -CAfile ministero.pem -inform DER cie-certifier.cer
```

## Per gli script

`-` al posto del file legge da stdin e scrive su stdout, quindi il giro sta in
una riga:

```bash
gpg --export FINGERPRINT | cie gpg certify - --uid 1 --outdir ~/cie-pgp | gpg --import
```

Avanzamento, PIN e riepilogo passano su stderr: nella pipe viaggia solo la
chiave. Quello che non si può chiedere a video va deciso prima:

| Se | Serve |
|---|---|
| la chiave ha piu' di una User ID | `--uid N` |
| il nome non corrisponde alla CIE | `--yes` |
| il PIN non è nel portachiavi | `-p <PIN>` |

`--outdir` non è un dettaglio: `cie-certifier.asc` e `.cer` su stdout non ci
stanno - sono tre artefatti, non uno - e finiscono comunque su disco. E quel
`gpg --import` in fondo importa **solo la tua chiave**: il certificatore va
importato a parte, o resta un `sig?`.

```bash
gpg --import ~/cie-pgp/cie-certifier.asc
```

## L'email non è certificata dalla CIE

La CIE conosce il nome del titolare. Di un indirizzo email non sa niente, e
questa implementazione non finge il contrario.

Una certification OpenPGP lega la User ID **intera** - nome ed email insieme - e
non è divisibile. Per questo la firma è di tipo `0x10` (generic) e non `0x13`
(positive): `0x13` dichiarerebbe che l'emittente ha verificato accuratamente la
User ID, email compresa. Cosa sia stato verificato davvero lo dice la notation
`cie-verified@cie-cli`, che contiene `holder-name` e nient'altro.

Se ti serve legare anche l'email, serve una verifica indipendente - un challenge
via email, tipicamente - che questo strumento non fa e non pretende di fare.

## La chiave continua a funzionare

Una certification è un packet che si aggiunge; non tocca nè la chiave nè le
sue subkey. Dopo `cie gpg certify` la chiave firma i commit esattamente come
prima, e le firme verificano come prima. È verificato da un test che genera la
chiave con GnuPG, la certifica e poi la rimette a firmare.

## Pubblicare, o no

La certification viaggia **solo se la esporti**:

```bash
gpg --export FINGERPRINT                          # con la certification
gpg --export --export-options export-minimal      # senza
```

Quindi puoi tenere la chiave certificata in locale e caricare su GitHub quella
nuda, mostrando la certification solo a chi ti chiede di dimostrare l'identità.

Se la esporti, diventano pubblici nome, cognome e numero della carta, che stanno
nella User ID del certificatore. Il codice fiscale no: quello è solo nel `.cer`.

## Cosa resta fuori dalla portata

Chi ha solo il materiale OpenPGP può verificare che la chiave X abbia
certificato quella User ID. Ma la stringa `(CIE 0000000000)` dentro la User ID
di X è un'auto-asserzione: senza il certificato X.509 nessuno può stabilire che
X sia davvero una CIE e non una chiave RSA qualunque.

è una scelta deliberata, e reversibile in una sola direzione: il `.cer` si può
sempre consegnare dopo, mentre un codice fiscale finito su un keyserver non si
toglie piu'.

La [specifica](/cie-cli/progetto/specifica-openpgp/) descrive la procedura
completa per chi riceve il `.cer` e vuole risalire fino alla PKI.
