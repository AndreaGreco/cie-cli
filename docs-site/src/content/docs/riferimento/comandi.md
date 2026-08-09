---
title: Comandi
description: Riferimento completo del binario cie — opzioni, default e file prodotti.
---

```
cie [-v...] <comando>
```

`-v` alza la verbosita': `-v` informativo, `-vv` ogni APDU in TX e RX, `-vvv`
tutto. E' la prima cosa da provare quando qualcosa non torna.

L'opzione `--pin` esiste su tutti i comandi che toccano la carta. Senza, il PIN
viene dal portachiavi di sistema o da un prompt che non mostra a schermo — vedi
[Il PIN](/cie-cli/guida/pin/).

:::danger
`--pin` lascia il PIN nella cronologia della shell e visibile in `ps`. Esiste per
gli script non interattivi.
:::

## `cie info`

Dati leggibili senza PIN: tipo di carta dall'ATR e file pubblici, fra cui
l'`ID_Servizi`. Non tocca il contatore dei tentativi.

## `cie pcsc-test`

Diagnostica della comunicazione PC/SC, con il dettaglio delle APDU.

## `cie auth`

Canale sicuro, verifica del PIN, lettura dei dati protetti.

| Opzione | Default | Effetto |
|---|---|---|
| `-p, --pin <PIN>` | portachiavi o prompt | il PIN, 8 cifre |
| `--dump-cert <FILE>` | — | salva su file il certificato della carta, in DER |

## `cie sign <file>`

Firma RSA nuda (PKCS#1 v1.5, 256 byte) sul digest SHA-256 del file.

| Opzione | Default | Effetto |
|---|---|---|
| `-p, --pin <PIN>` | portachiavi o prompt | |
| `-o, --output <FILE>` | `<file>.sig` | dove scrivere la firma |

Scrive anche `<file>.cer`, il certificato della carta. Il file originale non
viene modificato. La firma viene verificata in locale prima di essere scritta.

## `cie sign-p7m <file>`

Busta CAdES enveloping: contenuto e firma in un solo DER. Vale per qualunque
file.

| Opzione | Default |
|---|---|
| `-p, --pin <PIN>` | portachiavi o prompt |
| `-o, --output <FILE>` | `<file>.p7m` |

## `cie sign-pdf <file.pdf>`

PAdES B-B: la firma finisce dentro il PDF, con un riquadro visibile.

| Opzione | Default | Effetto |
|---|---|---|
| `-p, --pin <PIN>` | portachiavi o prompt | |
| `-o, --output <FILE>` | `<file>-firmato.pdf` | |
| `--pagina <N>` | `1` | pagina del riquadro, contata da 1 |
| `--motivo <TESTO>` | — | una riga in piu' nel riquadro |
| `--certifica [1-3]` | — (`2` se dato senza valore) | firma di certificazione |

`--certifica`: `1` vieta ogni modifica, `2` lascia compilare moduli e firmare,
`3` permette anche le annotazioni. Ammessa solo sulla prima firma di un
documento.

## `cie gpg certify <chiave>`

Certifica con la CIE una User ID di una chiave OpenPGP.

| Opzione | Default | Effetto |
|---|---|---|
| `--uid <N>` | chiede a video | quale User ID certificare, da 1 |
| `-p, --pin <PIN>` | portachiavi o prompt | |
| `-o, --output <FILE>` | `<chiave>-certificata.asc` | |
| `--yes` | — | non chiede conferma se il nome non corrisponde alla CIE |

Scrive tre file: la chiave certificata, `cie-certifier.asc` e
`cie-certifier.cer`. Gli ultimi due finiscono nella stessa directory
dell'output.

Accetta chiavi in armor ASCII o binarie.

## `cie gpg verify <chiave>`

Mostra le certification presenti su una chiave.

| Opzione | Effetto |
|---|---|
| `--certifier <FILE>` | verifica davvero, con `cie-certifier.asc` |

Senza `--certifier` si limita a elencarle, dichiarando che non sono state
verificate.

## `cie completions [shell]`

| Opzione | Effetto |
|---|---|
| `[shell]` | `bash`, `zsh`, `fish`... default: quella indicata da `$SHELL` |
| `--install` | scrive lo script dove la shell lo trova, invece di stamparlo |

## `cie-gui`

Binario separato, con le stesse firme e il documento davanti. Vedi
[Interfaccia grafica](/cie-cli/guida/interfaccia-grafica/).
