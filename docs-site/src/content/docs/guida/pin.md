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

Dopo una verifica del PIN andata a buon fine, e solo se il PIN e' stato digitato
a mano, `cie-cli` chiede se ricordarlo:

```
  Ricordare il PIN di questa carta nel portachiavi? [s/N]
```

Il portachiavi e' quello del sistema operativo — secret-service su Linux,
Credential Manager su Windows, Keychain su macOS — dietro la stessa API. Non c'e'
nessun file di cache scritto da `cie-cli`.

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

### Dimenticare una carta

Il portachiavi si gestisce con gli strumenti del sistema. Su Linux, con
`secret-tool` o con l'applicazione portachiavi del desktop:

```bash
secret-tool search service cie
```

Se il portachiavi non e' disponibile o rifiuta di salvare, il comando lo dice e
prosegue: la firma e' gia' andata a buon fine, e un portachiavi che non
collabora non e' un buon motivo per fallire.
