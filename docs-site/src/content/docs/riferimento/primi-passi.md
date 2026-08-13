---
title: Primi passi
description: I primi comandi di cie - leggere la carta, verificare il PIN, firmare - e come controllare quello che esce.
---

`cie` fa le stesse firme di [Firma CIE](/guida/firmare/) senza finestre,
e in piu' legge i dati della carta, certifica chiavi OpenPGP e si lascia
infilare in uno script.

Se non lo hai ancora installato, vedi
[Installare](/guida/installazione/#la-riga-di-comando).

## Guardare la carta senza toccare il PIN

```bash
cie info
```

Legge il tipo di carta e i file che la CIE espone in chiaro, fra cui l'
**ID_Servizi**: le dodici cifre stampate sulla carta, quelle che identificano
proprio quella. Nessun PIN, nessun tentativo consumato.

è il comando da provare per primo: se risponde, lettore e carta si parlano.

```bash
cie pcsc-test    # diagnostica della comunicazione col lettore
cie -vv info     # ogni comando scambiato col chip, in andata e ritorno
```

`-v` alza il dettaglio a scatti: `-v` informativo, `-vv` ogni comando, `-vvv`
tutto. Quando qualcosa non torna, `-vv` è il primo posto dove guardare.

## Con il PIN

```bash
cie auth
```

Apre il canale cifrato verso la carta, verifica il PIN e legge il certificato,
che è quello che contiene i tuoi dati: nome, cognome, codice fiscale. è il
percorso che ogni firma ripete.

Il PIN viene dal portachiavi se quella carta è stata ricordata, altrimenti te
lo chiede senza mostrarlo a schermo. Vedi [Il PIN](/guida/pin/).

Per tenerti una copia del certificato:

```bash
cie auth --dump-cert certificato.cer
```

:::caution[Il certificato contiene il tuo codice fiscale]
Ce lo mette il Ministero, in chiaro. Il `.cer` serve a chi deve verificare le
tue firme, ma non è un file da pubblicare alla leggera. Vedi [Cosa diventa
pubblico](/riferimento/privacy/).
:::

## Firmare

```bash
cie sign-pdf documento.pdf     # PAdES: firma dentro il PDF -> documento-firmato.pdf
cie sign-p7m contratto.odt     # CAdES: documento e firma insieme -> contratto.odt.p7m
cie sign file.txt              # firma separata -> file.txt.sig + file.txt.cer
```

Il primo mette anche un riquadro visibile in basso a destra, con nome e data
presi dal certificato. Da riga di comando la posizione è fissa: per
trascinarlo dove vuoi serve [Firma CIE](/guida/firmare/).

Opzioni e default stanno in [Tutti i comandi](/riferimento/comandi/).

## Controllare quello che è uscito

```bash
pdfsig documento-firmato.pdf
```

```bash
openssl cms -verify -in contratto.odt.p7m -inform DER -noverify
```

`-noverify` salta il controllo della catena di certificati, che richiederebbe
le CA del Ministero; la firma sul contenuto viene comunque verificata.

Per la firma separata servono il `.sig` e il `.cer` che escono insieme:

```bash
openssl x509 -inform DER -in file.txt.cer -pubkey -noout > pub.pem
openssl dgst -sha256 -verify pub.pem -signature file.txt.sig file.txt
```

`cie sign` in realtà ha già fatto questo controllo per conto suo prima di
scrivere il file: se il comando è andato a buon fine, la firma è già stata
verificata.

## Quale delle tre

| Situazione | Comando |
|---|---|
| Un PDF che deve restare leggibile e mostrare la firma | `cie sign-pdf` |
| Un file qualunque da mandare a un ufficio pubblico | `cie sign-p7m` |
| Devi solo dimostrare che quei byte sono passati dalla carta | `cie sign` |

## Cosa non c'è

La **marca temporale** (il livello B-T) richiede di chiamare un servizio
esterno, e il percorso di firma non parla con nessuno all'infuori della carta:
nel programma non c'è nemmeno un client HTTP. Senza marca temporale, la
validità della firma nel tempo dipende da quella del certificato della carta.

## E poi

- [Tutti i comandi](/riferimento/comandi/)
- [Certificare una chiave OpenPGP](/riferimento/openpgp/)
- [Cosa diventa pubblico](/riferimento/privacy/)
- [Se qualcosa non va](/guida/problemi/)
