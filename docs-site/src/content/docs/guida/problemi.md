---
title: Se qualcosa non va
description: Gli errori che capitano davvero - lettore, carta, PIN, PDF - e cosa fare.
---

Cerca qui il messaggio che ti compare.

## "Nessun lettore di smart card trovato"

Il computer non vede il lettore, e la carta non c'entra ancora niente.

1. Scollega e ricollega il lettore, possibilmente in un'altra porta USB e non
   in un hub.
2. **Su Windows**, la prima volta il driver se lo scarica Windows Update:
   aspetta un minuto e riprova. Se il lettore compare in Gestione dispositivi
   con un punto esclamativo, il driver non è arrivato - cercalo sul sito di
   chi ha fatto il lettore.
3. **Su Linux**, il servizio va acceso a mano:

   ```bash
   sudo systemctl enable --now pcscd
   pcsc_scan
   ```

   Se `pcsc_scan` non elenca nessun lettore, il problema è prima di Firma CIE
   e va risolto li'. Sui lettori USB il driver che serve quasi sempre si chiama
   `ccid`.
4. **Su macOS**, il supporto è incluso: se il lettore non compare, prova un
   altro cavo o un'altra porta.

## "Nessuna CIE inserita nel lettore"

Il lettore c'è, la carta no.

- Il **chip va verso il basso** nella maggior parte dei lettori da tavolo, e va
  verso l'alto in quelli a fessura dei portatili. Prova a girarla.
- Spingi la carta fino in fondo e aspetta due o tre secondi: la spia del
  lettore di solito cambia quando aggancia.
- Pulisci i contatti dorati se sono opachi.
- Deve essere una **CIE 3.0**, quella con la fotografia e il chip emessa dal
  2016. Le vecchie carte d'identità cartacee e le CIE piu' vecchie non hanno
  il chip che serve.

## "Errore di comunicazione con il lettore"

Quasi sempre la carta è stata tolta a metà lavoro, oppure c'è un altro
programma che se l'è presa.

:::caution[Il middleware CIE litiga con Firma CIE]
Se sul computer hai installato il **middleware CIE ufficiale** o l'app CieID
per il desktop, quei programmi possono tenere occupata la carta e riconfigurare
il servizio del lettore.

Chiudili - su Windows anche dall'area di notifica, e se serve ferma i loro
servizi - prima di usare Firma CIE. Non devono girare insieme.
:::

Chiudi anche il browser se lo stavi usando per entrare con la CIE su un sito.

## "PIN errato. Tentativi rimasti: 2"

Sono le otto cifre: le prime quattro dalla ricevuta del Comune, le altre
quattro dalla busta arrivata con la carta. Vedi [Il PIN](/guida/pin/).

**Fermati dopo il secondo errore.** Al terzo la carta si blocca. Meglio cercare
i fogli con calma che restare senza carta.

Se il PIN era salvato nel portachiavi ed è cambiato, cancella la voce salvata
(**Dimentica**, nella schermata del PIN) e ridigitalo.

## Mi chiede la password del portachiavi ogni volta

Non è un errore, è voluto. Il PIN salvato viene richiuso a chiave prima di
ogni uso: il portachiavi ti risparmia le otto cifre della carta, non ti
risparmia di dimostrare che al computer ci sei tu. Vedi
[Il PIN](/guida/pin/#salvato-non-vuol-dire-aperto).

Se non vuoi digitare nulla in una procedura automatica, da riga di comando c'è
`--pin`, che salta il portachiavi - con i rischi scritti in
[Comandi](/riferimento/comandi/).

## Dopo aver firmato, un altro programma mi chiede la password del portachiavi

Succede **su Linux**, e non è un guasto. GNOME sa chiudere a chiave solo un
portachiavi intero, mai la singola voce: chiudendo quella della CIE si chiude
per un attimo tutto quello che sta nello stesso portachiavi. Se in quell'istante
un altro programma va a cercarsi una password, se la vede chiedere.

La prima password che digiti riapre tutto insieme, quindi la cosa si risolve da
sé.

## "PIN bloccato. Serve il PUK per sbloccarlo"

La carta ha esaurito i tentativi. Il **PUK** è l'altro codice arrivato insieme
alla carta; si usa con l'app **CieID** o con il middleware ufficiale, che sono
gli strumenti che sanno sbloccarla. Firma CIE non lo fa.

Se hai perso anche il PUK, la carta va rifatta in Comune.

## "Autenticazione del chip fallita"

Il chip non ha superato il controllo di autenticità. Può voler dire che la
carta non è una CIE, che il lettore sta corrompendo i dati, o che c'è di
mezzo qualcosa che non dovrebbe esserci.

Prova con un altro lettore. Se succede solo con quella carta e su lettori
diversi, non firmare e chiedi in Comune.

## "PDF illeggibile, resta la firma in busta"

Firma CIE non riesce ad aprire quel PDF per mostrartelo - di solito perchè è
protetto da password o costruito in modo strano.

Puoi comunque firmarlo come **`.p7m`**: la firma vale lo stesso, ma non avrai
l'anteprima nè il riquadro visibile sulla pagina. Se ti serve il riquadro,
riapri il PDF con un altro programma e risalvalo.

## "Non è un PDF, quindi niente PAdES nè anteprima"

Non è un errore: stai firmando un file che non è un PDF (un `.odt`, un `.zip`,
un'immagine) e per quelli esiste solo il `.p7m`. Vai avanti tranquillamente.

## Il programma non parte

**Windows** mostra "Windows ha protetto il PC": clicca *Ulteriori informazioni*
→ *Esegui comunque*. **macOS** dice "sviluppatore non identificato": clic
destro sull'icona → *Apri*. Sono spiegati in
[Installare](/guida/installazione/).

## Il documento firmato viene segnalato dal validatore

Se Acrobat o un validatore online mette un avviso sulla firma pur dicendo che
è valida, è probabilmente il certificato della CIE che risulta di
autenticazione e non di firma qualificata. è normale ed è spiegato in
[Cos'è e cosa puoi farci](/guida/cosa-fa/).

## Guardare cosa succede davvero

Da riga di comando ci sono due comandi che non toccano il PIN e non consumano
tentativi:

```bash
cie pcsc-test    # prova la comunicazione con il lettore
cie info         # legge i dati che la carta espone senza PIN
cie -vv info     # come sopra, mostrando ogni comando scambiato col chip
```

Se `cie info` risponde, lettore e carta si parlano e il problema sta piu'
avanti.

## Non ti si è risolto

[Apri una segnalazione su
GitHub](https://github.com/AttilioGreco/cie-cli/issues) scrivendo il sistema
operativo, il modello del lettore e il messaggio esatto. Se puoi, allega
l'output di `cie -vv info`: **non contiene il PIN**, ma contiene il numero
della carta, quindi valuta se ti va bene renderlo pubblico.
