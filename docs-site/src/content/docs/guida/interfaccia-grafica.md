---
title: Interfaccia grafica
description: cie-gui — le stesse firme, con il documento davanti e il riquadro da posizionare a mano.
---

`cie-gui` e' un binario separato: chi usa la riga di comando non si porta dietro
un toolkit grafico.

```bash
cargo run --release -p cie-gui
```

## La schermata

Una sola: il documento al centro, le scelte in colonna a destra, in fondo dove
finira' il file e il pulsante per firmare.

La differenza con `cie sign-pdf` sta nel riquadro della firma. Nella CLI la
posizione e' cablata in basso a destra; qui il riquadro si trascina per
spostarlo e si prende per gli angoli per ridimensionarlo. Dentro c'e' un
facsimile di come verra', alla stessa scala che avra' nel PDF — quello che si
vede e' quello che finisce nel file.

Il nome del file firmato e' quello che proporrebbe la CLI, e si cambia con un
salva-con-nome.

## Comandi

| | |
|---|---|
| frecce, PagSu / PagGiu' | cambia pagina |
| rotella | cambia pagina a 100%, scorre quando sei zoomato |
| Ctrl `+` `−` `0` | zoom avanti, indietro, torna a 100% |
| trascinamento di un file | lo apre |

Il tema segue il desktop; l'interruttore in alto a destra forza chiaro o scuro.

## Il tipo di firma

Lo decide il file: un PDF va in PAdES con il riquadro visibile, qualunque altra
cosa in CAdES `.p7m`. Sono gli stessi contenitori della CLI, prodotti dallo
stesso codice — cambia l'involucro, non la firma.

## La carta

Il PIN viene dal portachiavi di sistema se quella carta e' stata ricordata,
altrimenti si digita nel campo apposito, che accetta otto cifre e non una di
piu'. Vale tutto quello che vale per la CLI: [Il PIN](/cie-cli/guida/pin/).

La carta lavora su un thread suo, cosi' l'interfaccia non si blocca durante il
secondo abbondante che serve ad aprire il canale sicuro.
