---
title: Firmare un documento
description: La procedura completa con Firma CIE, dalla scelta del file al documento firmato.
---

Prima di cominciare: lettore collegato, CIE appoggiata sopra con il chip verso
il basso, PIN a portata di mano.

## 1. Scegli il documento

Apri **Firma CIE** e trascina il file nella finestra, oppure clicca **Scegli un
documento…**.

Se è un PDF lo vedi comparire al centro, e puoi sfogliarlo:

| | |
|---|---|
| frecce, PagSu, PagGiu | cambia pagina |
| rotella del mouse | cambia pagina; quando sei ingrandito, scorre |
| Ctrl `+` `−` `0` | ingrandisci, riduci, torna al 100% |

## 2. Controlla il tipo di firma

Lo sceglie il programma da solo in base al file, e nella quasi totalità dei
casi va bene com'è:

- **PAdES** per i PDF - la firma finisce dentro il documento, che resta un PDF
  normale e apribile da chiunque.
- **CAdES (.p7m)** per tutto il resto - documento e firma finiscono insieme in
  un unico file. è il "p7m" che chiedono gli uffici.

C'è anche una **firma separata (`.sig`)**, che serve raramente: produce un
file di firma staccato dal documento. Se non sai di averne bisogno, non ti
serve.

## 3. Sistema il riquadro della firma

Solo per i PDF. Il riquadro è quel rettangolo che comparirà sulla pagina a
dire che il documento è firmato.

Trascinalo dove vuoi e prendilo per gli angoli per cambiargli dimensione:
quello che vedi è esattamente quello che finirà nel file. **Ripristina** lo
rimette dov'era.

Il testo dentro al riquadro è un facsimile: nome e codice fiscale il
programma li legge dalla carta al momento della firma, non prima.

Due campi facoltativi:

- **Motivo** aggiunge una riga dentro il riquadro (`Per approvazione`, `Visto`,
  quello che ti serve).
- **Certifica il documento** è la firma d'autore, quella che i lettori PDF
  marcano con il nastrino azzurro. Dichiara che il documento è tuo e decide
  cosa altri possano ancora modificarci: *nessuna modifica*, *moduli e altre
  firme*, oppure *moduli, firme e annotazioni*. Si può usare **solo sul primo
  documento firmato**: un PDF che ha già una firma non si certifica piu'.

  Se qualcun altro dovrà controfirmare, lascia *moduli e altre firme* o non
  certificare affatto.

In fondo, sotto **Verrà scritto**, c'è il nome del file che stai per creare.
L'originale non viene toccato.

## 4. Firma

Clicca **Firma**. Il programma chiede il PIN della carta: otto cifre, non ne
accetta di piu'.

La casella **Ricordalo nel portachiavi** lo fa salvare nel gestore di password
del tuo computer, cosi' la volta dopo non lo ridigiti. Puoi lasciarla vuota e
digitarlo ogni volta - vedi [Il PIN](/guida/pin/).

Poi parte la firma vera e propria, e ci mette qualche secondo:

```
  Connessione alla carta
  Apertura del canale sicuro
  Verifica del PIN
  Lettura del certificato
  Firma del documento
```

:::danger[Non togliere la carta dal lettore]
Finchè la barra non è arrivata in fondo, la carta deve restare dov'è.
:::

## 5. Fatto

Compare **Documento firmato**, con due pulsanti per aprire il file o la
cartella che lo contiene.

Il file firmato sta accanto all'originale, con `-firmato` aggiunto al nome
(`contratto.pdf` → `contratto-firmato.pdf`), oppure con `.p7m` in coda.

Nella stessa schermata può comparire un avviso: il certificato della CIE è di
autenticazione e non di firma qualificata, quindi certi programmi di verifica
lo segnalano. Non è un errore ed è spiegato in [Cos'è e cosa puoi
farci](/guida/cosa-fa/).

## Controllare una firma

Su un PDF firmato: aprilo con Acrobat Reader e guarda il pannello delle firme,
oppure usa uno dei validatori online della pubblica amministrazione.

Su un `.p7m`: gli stessi validatori, oppure `cie` da riga di comando.

## Il tema

L'interruttore in alto a destra sceglie fra chiaro, scuro e *segui il sistema*.

## Le stesse firme da riga di comando

`cie sign-pdf documento.pdf` e `cie sign-p7m file.odt` producono esattamente
gli stessi file: cambia l'involucro, non la firma. L'unica differenza è il
riquadro, che da riga di comando finisce sempre in basso a destra invece che
dove lo trascini tu.

Tutte le opzioni stanno in [Tutti i comandi](/riferimento/comandi/).
