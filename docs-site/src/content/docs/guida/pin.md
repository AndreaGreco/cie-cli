---
title: Il PIN
description: Da dove viene il PIN, il portachiavi di sistema, e perche' qui si digitano tutte e otto le cifre.
---

## Perche' tutte e otto le cifre

Il middleware ufficiale, per funzionare, pretende una registrazione preventiva
della carta tramite un'applicazione Java. Quella registrazione salva **meta' del
PIN** in una cache su disco, cifrata con una chiave derivabile da chiunque abbia
la carta in mano, senza conoscerne il PIN. Da li' in poi chiede solo le altre
quattro cifre.

La cache non risolveva un problema tecnico, e nel farlo dimezzava lo spazio di
ricerca di una credenziale. Qui il PIN si digita per intero, quando serve.

Un PIN che non sia esattamente otto cifre numeriche viene rifiutato in locale,
prima di arrivare alla carta: non ha senso consumare un tentativo per un valore
che non puo' essere giusto.

## Da dove viene il PIN

Ogni comando che tocca la carta cerca il PIN in quest'ordine:

1. **L'argomento `--pin`**, se c'e'
2. **Il portachiavi di sistema**, se quella carta e' stata ricordata
3. **Il prompt**, che legge senza mostrare a schermo

:::danger[`--pin` finisce nella cronologia]
`cie sign --pin 12345678 file` lascia il PIN nella history della shell e lo
rende visibile in `ps` a chiunque sia sulla macchina. Esiste per gli script non
interattivi; per l'uso quotidiano il prompt e il portachiavi sono migliori sotto
ogni aspetto.
:::

## Il portachiavi di sistema

Digitare otto cifre a ogni firma e' scomodo, e la scomodita' e' nemica della
sicurezza tanto quanto una cache fatta male. La via di mezzo e' salvare il PIN
**per intero** nel gestore di password del sistema operativo.

Dopo una verifica andata a buon fine, e solo se il PIN e' stato digitato a mano,
`cie-cli` chiede se ricordarlo:

```
  Ricordare il PIN di questa carta nel portachiavi? [s/N]
```

Il portachiavi e' quello del sistema operativo — secret-service su Linux,
Credential Manager su Windows, Keychain su macOS — dietro la stessa API.
`cie-cli` non scrive nessun file di cache proprio: il segreto lo custodisce il
sistema, cifrato a riposo e sbloccato con le credenziali della sessione.

La differenza con la cache del middleware non e' «su disco» contro «non su
disco». E' che li' finiscono **quattro cifre**, in un file suo, cifrate con una
chiave che chiunque abbia la carta puo' derivare senza conoscere il PIN — quindi
lo spazio di ricerca si dimezza e la protezione non protegge. Qui ci finiscono
**tutte e otto**, nel deposito segreti del sistema, con la protezione che quel
deposito offre a tutte le altre password.

### Cosa protegge, e cosa no

| | |
|---|---|
| Disco rimosso, immagine, backup | protetto: il segreto e' cifrato a riposo |
| Un altro utente sulla stessa macchina | protetto |
| Chi legge `~/.config` o la home | protetto: non c'e' niente da leggere |
| Un processo che gira **come te**, a sessione sbloccata | **non protetto** |

L'ultima riga e' il modello di secret-service, non una particolarita' di
`cie-cli`: a portachiavi sbloccato, qualunque processo dell'utente puo'
chiederne il contenuto. Un portachiavi di sistema non e' una difesa contro
codice malevolo che gira gia' con i tuoi privilegi — per quello serve che il PIN
resti nella tua testa.

### Una chiave per carta

La voce nel portachiavi e' indicizzata sull'**ID_Servizi**, il numero letto dal
chip senza PIN. E' quello che permette a piu' CIE di convivere nello stesso
portachiavi senza pestarsi i piedi, e che permette di riconoscere quale carta e'
appoggiata sul lettore mentre il certificato e' ancora chiuso dietro il PIN.

### Cosa sta dentro il segreto, e cosa no

Insieme al PIN vengono ricordati nome, cognome e codice fiscale presi dal
certificato: sono cio' che permette di mostrare *di chi* e' la carta appena viene
appoggiata.

Stanno **dentro** il segreto cifrato, mai negli attributi di ricerca. Su
secret-service gli attributi sono metadati in chiaro, leggibili da chiunque stia
sul bus di sessione: metterci un codice fiscale significherebbe pubblicarlo a
ogni processo dell'utente.

### Non salvarlo mai

Salvare e' facoltativo, e la scelta prudente e' quella di default: la domanda si
risponde con Invio. Se non rispondi `s`, sul disco non finisce **niente** — ne'
il PIN ne' i dati del titolare — e il comando prosegue come se non l'avesse mai
chiesto.

Da li' in poi il PIN si digita ogni volta, che e' esattamente il comportamento
di chi non vuole nessuna traccia sulla macchina.

:::note[La domanda torna]
Non esiste un «non chiedermelo piu'»: finche' digiti il PIN a mano, dopo ogni
verifica riuscita la domanda ricompare. Se ti da' fastidio, aprine una issue —
un flag per zittirla e' un'aggiunta piccola, ma non l'ho messa senza che
servisse a qualcuno.
:::

### Dimenticare una carta

Il portachiavi si gestisce con gli strumenti del sistema. Su Linux, con
`secret-tool` o con l'applicazione portachiavi del desktop:

```bash
secret-tool search service cie
```

Se il portachiavi non e' disponibile o rifiuta di salvare, il comando lo dice e
prosegue: la firma e' gia' andata a buon fine, e un portachiavi che non
collabora non e' un buon motivo per fallire.
