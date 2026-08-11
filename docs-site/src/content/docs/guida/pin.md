---
title: Il PIN
description: Le otto cifre della CIE, cosa succede se le sbagli, e come farsele ricordare dal computer.
---

Il PIN della CIE è di **otto cifre**, te lo hanno consegnato in due metà:
le prime quattro sulla ricevuta che ti hanno dato in Comune quando hai chiesto
la carta, le altre quattro nella busta arrivata a casa insieme alla carta.

Se lo hai perso dovrai rivolgerti al Comune.

:::danger[Attento a quante volte sbagli il PIN, dopo la seconda fermati e controlla in modo accurato]
La carta conta i tentativi sbagliati. Dopo tre di fila si blocca, e per
sbloccarla serve il **PUK** - l'altro codice arrivato con la carta. Se non hai
nemmeno quello, la carta va rifatta.
:::

Per ridurre i possibili errori di battitura, un PIN che non sia di otto cifre numeriche viene rifiutato
subito, senza nemmeno arrivare alla carta: cosi' una distrazione non ti costa
un tentativo.

## Salvare il PIN

Digitare continuamente 8 cifre è scomodo, e le cose scomode finiscono
per essere aggirate male. Per questo, dopo che il PIN è stato accettato, Firma
CIE ti propone di salvarlo nel **portachiavi** del tuo sistema operativo, cosi' non dovrai più digitarlo.

Da riga di comando la stessa domanda arriva cosi':

```bash
  Ricordare il PIN di questa carta nel portachiavi? [s/N]
```

Il **portachiavi** è il gestore di password del tuo sistema operativo:
Gestione credenziali su Windows, Portachiavi su macOS, il portachiavi di GNOME su Linux.

Firma CIE non scrive nessun file suo: tutti i secret vengono custoditi dal sistema operativo, che li tiene
cifrato, e lo sblocca quando entri nella tua sessione (Login utente).

## Oppure no

Rispondere di no è una scelta legittima, ed è quella predefinita: da riga di
comando basta premere Invio. In quel caso sul disco **non finisce niente** - né
il PIN né il tuo nome - con questa modalità il programma ti chiede il PIN ogni volta che serve.

:::note[La domanda ricompare]
Non c'è un "non chiedermelo piu'": finchè digiti il PIN a mano, dopo ogni
verifica riuscita la domanda torna. Se ti dà fastidio,
[scrivilo](https://github.com/AttilioGreco/cie-cli/issues).
:::

## Cosa protegge il portachiavi, e cosa no

| | |
|---|---|
| Qualcuno ti ruba il computer o il disco | protetto: il PIN è cifrato |
| Un'altra persona che usa lo stesso computer con il suo utente | protetto |
| Chi va a curiosare fra i tuoi file | protetto: non c'è nessun file da leggere |
| Un programma malevolo che gira **come te**, mentre la sessione è aperta | **non protetto** |

L'ultima riga vale per qualunque password nel portachiavi, non solo per questa:
a sessione sbloccata, i programmi che girano come te possono chiederne il contenuto.
Se è un rischio che ti riguarda, non salvare il PIN.
:::note[Attenzione alla sicurezza]
*NON* fidarsi dei programmi nella propria sessione è un problema di sicurezza generale, non specifico della CIE.
:::
## Più di una carta

Ogni CIE ha la sua voce nel portachiavi, riconosciuta dal numero della carta che è leggibile senza il PIN.  
È possibile salvare il PIN di più carte, anche se appartengono a persone diverse: il portachiavi le distingue, proprio come fa con le altre password.

:::caution[Attenzione se condividete l'utente del computer]
Per quanto sia una pratica comune, condividere un utente del computer con altre persone non è una buona idea.  
Anche se siete in famiglia, sarebbe bene avere un utente ciascuno, e ognuno ha le sue password.  
Una volta salvato nel porta chiavi, il PIN è disponibile a chiunque usi quell'utente, senza bisogno di digitarlo, gli bastera avere la carta in mano.  
:::

Insieme al PIN vengono ricordati nome, cognome e codice fiscale letti dalla  
carta - dentro il segreto cifrato, mai fra le etichette in chiaro che il  
portachiavi usa per cercare.

## Dimenticare una carta

In Firma CIE c'è il pulsante **Dimentica** accanto al PIN salvato.

Altrimenti vale il gestore di password del sistema: Gestione credenziali su
Windows, Accesso Portachiavi su macOS. Su Linux, se preferisci il terminale:

```bash
secret-tool search service cie
```

Se il portachiavi non è disponibile o si rifiuta di salvare, il programma lo notifica ma la firma è già andata a buon fine, e un portachiavi
che non collabora non è un buon motivo per buttarla via.

## Perché tutte e otto le cifre

Il middleware ufficiale ne chiede solo quattro. Le altre quattro le tiene
salvate lui, in un file, protette da una chiave che chiunque abbia la carta in
mano può ricavare senza conoscere il PIN - il che dimezza le combinazioni da
provare.

Il PIN si digita per intero, se decidi di salvarlo viene salvato per intero nel
portachiavi del sistema, con la stessa protezione di tutte le tue altre password.

## Da riga di comando

`cie` cerca il PIN in quest'ordine: l'opzione `--pin` se gliela dai, poi il
portachiavi, poi te lo chiede.

:::danger[`--pin` resta nella cronologia]
`cie sign --pin 12345678 file` lascia il PIN nella storia della shell e lo
rende visibile agli altri processi. Esiste per gli script automatici; per l'uso
normale il prompt e il portachiavi sono meglio sotto ogni aspetto.
:::
