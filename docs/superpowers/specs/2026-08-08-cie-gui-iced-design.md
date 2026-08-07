# cie-gui su iced: riprogettazione della finestra e portachiavi multi-carta

Data: 2026-08-08
Stato: approvato, da implementare

## Perché

La finestra attuale (eframe/egui) fa il suo lavoro ma sembra un pannello di
debug: elenco piatto di sezioni, nessuna gerarchia, niente respiro. Il toolkit
non aiuta — egui non ha grassetto nei font predefiniti, non ha un sistema di
temi, e ogni spaziatura è cablata a mano.

Si passa a **iced 0.14**, che ha temi con tavolozza propria, font TTF
incorporabili, `stack` + `opaque` per i pannelli modali e le subscription per il
lavoro in background.

Nello stesso giro si risolve un limite vero: oggi il portachiavi tiene **un solo
PIN**, quindi chi ha più di una CIE in casa non è servito.

## Ambito

Dentro:

- riscrittura della vista di `cie-gui` su iced;
- portachiavi indicizzato per carta, con i dati del titolare in cache;
- sonda del lettore a eventi;
- adeguamento di `cie-cli` al nuovo portachiavi (il formato li lega).

Fuori:

- il porting a Windows. Si evita solo di sbarrarsene la strada: niente
  dipendenze legate a Linux dove esiste un'alternativa multipiattaforma che
  costa uguale o meno.
- migrazione delle voci vecchie del portachiavi. È tutto in sviluppo: il
  formato nuovo sostituisce il vecchio, chi ha una voce vecchia la rifà.

## Cosa si tiene

| File | Sorte |
|---|---|
| `cie-gui/src/app.rs` | buttato: 750 righe di egui |
| `cie-gui/src/job.rs` | intatto — thread + `mpsc`, non sa cosa sia un toolkit |
| `cie-gui/src/preview.rs` | tenuto; cambia una firma, `render()` restituisce `image::Handle` |
| `cie-gui/src/keyring.rs` | sostituito dal modulo condiviso in `cie-firma` |
| `cie-lib` | nessuna modifica: legge già tutto quello che serve |

La matematica di `preview.rs` — `to_pdf_rect`, `move_stamp`, `resize_stamp`, con
la gestione di `/Rotate` e crop box — è l'unica cosa delicata del crate e **non
si tocca**. Sbagliarla mette la firma nel posto sbagliato senza errori visibili.

## Struttura nuova

```
cie-firma/src/portachiavi.rs   condiviso da CLI e GUI (nuovo)
cie-gui/src/
  main.rs      iced::application(), font, tema, subscription
  state.rs     lo stato e update(): puro, testabile senza finestra
  theme.rs     tavolozze chiara e scura, stili dei widget
  lettore.rs   sonda PC/SC a eventi (nuovo)
  job.rs       invariato
  preview.rs   invariato salvo il tipo immagine
  view/
    mod.rs     impianto: barra, palco, colonna, barra d'azione
    palco.rs   documento + tela del riquadro
    colonna.rs le card
    firma.rs   il pannello modale: PIN → passi → esito
```

Un file per riquadro di responsabilità, nessuno oltre le ~250 righe. È anche una
questione pratica: file focalizzati si modificano con meno errori.

## Aspetto

Design system della PA italiana (Designers Italia / Bootstrap Italia).

|  | chiaro | scuro |
|---|---|---|
| sfondo tela | `#F2F7FC` | `#16222E` |
| superficie | `#FFFFFF` | `#22384E` |
| testo | `#17324D` | `#E8EEF4` |
| primario | `#0066CC` | `#4FA3FF` |
| successo | `#008758` | `#00CC85` |
| errore | `#D9364F` | `#FF5E73` |
| bordo | `#C5D2DE` | `#35506B` |

- Raggio 4 px: Bootstrap Italia è squadrato, non arrotondato.
- Scala di spazi 4 / 8 / 16 / 24.
- Titillium Web Regular + SemiBold incorporati con `include_bytes!` (licenza
  OFL, ~180 KB in due pesi). Non è installato di sistema e non si può darlo per
  scontato. Il grassetto vero manda in pensione il trucco attuale, dove il
  neretto del timbro è simulato scurendo il grigio.
- Tema: **segue il sistema** (crate `dark-light`), con scelta manuale nel menù
  della barra.
- Decorazioni della finestra lasciate al compositore. La barra "Firma con CIE" è
  interna all'app, non una CSD: su un window manager a piastrelle la finestra la
  piazza il compositore, come già dice il README.

## Impianto della finestra

```
┌──────────────────────────────────────────────────────────────┐
│ ▌Firma con CIE                 ● Mario Rossi · 1A2B…   ⚙  ─□×│
├───────────────────────────────────┬──────────────────────────┤
│                                   │  DOCUMENTO               │
│     ┌───────────────────────┐     │ ┌──────────────────────┐ │
│     │                       │     │ │ 📄 contratto.pdf     │ │
│     │     contratto.pdf     │     │ │  248 KB · 3 pagine   │ │
│     │       pagina 1        │     │ │             Cambia…  │ │
│     │           ┌─────────┐ │     │ └──────────────────────┘ │
│     │           │ ▓FIRMA▓ │ │     │  TIPO DI FIRMA           │
│     │           └─────────┘ │     │ ┌──────────────────────┐ │
│     └───────────────────────┘     │ │ ◉ PAdES   dentro     │ │
│                                   │ │ ○ CAdES   .p7m       │ │
│                                   │ │ ○ Separata  .sig     │ │
│          ◀    1 / 3    ▶          │ └──────────────────────┘ │
│                                   │  RIQUADRO       ⌄scroll  │
├───────────────────────────────────┴──────────────────────────┤
│  →  contratto-firmato.pdf   Cambia…         ┃   Firma   ┃    │
└──────────────────────────────────────────────────────────────┘
```

Colonna larga 340 px fissa, palco elastico, barra d'azione 64 px, finestra
1100×760 di partenza e 900×600 minimi.

### I cinque stati

1. **Nessun documento** — al centro una zona di rilascio: "Trascina qui un file
   o scegli". Il trascinamento arriva da `window::Event::FileDropped`.
2. **PDF aperto** — anteprima con riquadro trascinabile.
3. **File non-PDF** — niente anteprima, una scheda col nome e la dimensione; il
   tipo si porta da solo su `.p7m` e una riga spiega perché il PAdES non c'è.
4. **PDF illeggibile** — avviso nel palco, stessa ricaduta su `.p7m`.
5. **Firma** — pannello modale sopra tutto.

### Il riquadro della firma

Una `canvas` sopra l'`image` dentro uno `stack`. Comportamento identico a oggi:
trascini il corpo per spostare, gli angoli per ridimensionare, il cursore
cambia forma su ciascuna zona. Il facsimile del timbro dentro il riquadro
continua a venire da `cie_firma::preview_stamp_lines`, la stessa funzione che
compone il timbro vero: nessuna seconda copia del layout da tenere allineata.

### Il pannello di firma

Premi **Firma**:

- se il PIN è nel portachiavi → si va dritti ai passi;
- altrimenti → il pannello chiede il PIN, con la spunta "Ricordalo".

```
   ✓  Connessione alla carta
   ✓  Apertura del canale sicuro
   ◐  Verifica del PIN
   ·  Lettura del certificato
   ·  Firma del documento

   ⚠  Non togliere la carta dal lettore.
```

A fine corsa lo stesso pannello diventa l'esito: verde con **Apri il PDF** /
**Apri cartella** / **Chiudi**, oppure rosso col testo dell'errore e **Riprova**,
che torna al PIN senza rifare tutto.

Sugli errori di PIN il pannello dice **quanti tentativi restano**, letti con
`SecureMessaging::pin_retries_left()` — VERIFY con campo dati vuoto, la carta
risponde `63Cx` senza consumare un tentativo. Esiste già in
`cie-lib/src/secure_messaging.rs:836`.

## La sonda del lettore

Un thread bloccato su `pcsc::Context::get_status_change()`: nessun timer,
nessun polling. Il lettore fittizio `\\?PnP?\Notification` avvisa anche quando
il lettore stesso viene collegato o staccato, e vale sia su pcsclite sia su
Windows.

Quando compare una carta il thread legge i due file **pubblici**, senza PIN e
senza canale sicuro:

| valore | file | funzione | uso |
|---|---|---|---|
| ID_Servizi | `0x1001` | `read_id_servizi()` | chiave nel portachiavi |
| seriale | `0xD003` (PAN) | `get_serial_from_pan()` | etichetta leggibile, è il numero stampato sulla carta |

Da verificare con la carta alla mano prima di implementare: `cie info` deve
mostrare `IdServizi` e `PAN` valorizzati **senza** chiedere il PIN. Se uno dei
due risponde "(richiede PIN)", la chiave cambia e il resto del disegno regge
lo stesso.

Stati del chip nella barra:

```
○  Nessun lettore
○  Appoggia la carta
●  Mario Rossi · 1A2B3C4D5E6F      carta conosciuta
●  Carta nuova · 1A2B3C4D5E6F      mai vista prima
```

**L'identificazione si sospende mentre una firma è in corso.** Il thread della
firma tiene la carta; connettersi in parallelo per leggere il PAN farebbe
fallire l'una o l'altra. L'osservazione degli eventi può continuare — non
apre canali — ma la lettura dei file no.

## Portachiavi

Modulo condiviso in `cie-firma/src/portachiavi.rs`, sopra il crate `keyring`
4.1.6: secret-service su Linux, Credential Manager su Windows, stessa API.
Sostituisce sia il `secret-service` + `tokio` della GUI sia il sottoprocesso
`secret-tool` della CLI. Meno dipendenze di adesso, non di più.

Sta in `cie-firma` e non in un quarto crate perché sono ~80 righe e i due binari
già dipendono da lì; un crate a sé per così poco è impalcatura.

Una voce per carta:

```
service:  "cie"
user:     <ID_Servizi>
segreto:  {"pin":"12345678","nome":"MARIO","cognome":"ROSSI",
           "cf":"RSSMRA…","seriale":"1A2B3C4D5E6F"}
```

**Vincolo: nell'indice va solo l'ID_Servizi, mai nome o codice fiscale.** In
secret-service gli attributi sono metadati in chiaro, leggibili da chiunque stia
sul bus di sessione; il segreto no. I dati del titolare stanno solo dentro il
segreto.

I dati del titolare si imparano da soli: escono dal certificato alla prima firma
riuscita (`DatiTitolare::from_certificate_der`) e da lì in poi la carta viene
riconosciuta all'inserimento. Prima di allora è "Carta nuova".

Un portachiavi che rifiuta di scrivere **non blocca la firma**: si segnala e si
va avanti.

### Adeguamento della CLI

`cie-cli/src/commands/mod.rs`: `pin_da_portachiavi()` sparisce, sostituito da
`cie_firma::portachiavi`. Ordine di precedenza invariato — `--pin`, portachiavi,
prompt — ma la ricerca passa per l'ID_Servizi letto dalla carta, che la CLI ha
già in mano perché si collega comunque.

Il README va corretto: `secret-tool store --label='CIE PIN' service cie` non è
più il modo di popolare la voce, perché il segreto ora è JSON e la chiave è la
carta. Si popola firmando una volta dalla GUI, o dalla CLI col prompt.

## Comunicazione col thread della carta

`job.rs` continua a restituire un `Receiver`. Lo drena una subscription
`time::every(100 ms)` **attiva solo mentre un lavoro è in volo**. Cinque righe
invece di adattare l'`mpsc` a uno stream asincrono; per un'operazione da diversi
secondi, 100 ms non si vedono. Se un giorno servisse una barra di avanzamento
fluida, qui va `iced::stream::channel`.

## Errori

| dove | come |
|---|---|
| apertura del file | riga rossa dentro la card Documento, niente popup |
| PDF illeggibile | avviso nel palco, ricaduta su `.p7m` |
| firma fallita | pannello rosso, resta aperto, **Riprova** torna al PIN |
| PIN sbagliato | come sopra, più i tentativi rimasti |
| portachiavi | avviso, la firma prosegue |
| carta assente | lo dice il chip prima ancora di premere Firma |

## Test

`update()` resta puro e verificabile senza aprire una finestra:

- un PDF apre in PAdES con l'anteprima;
- un non-PDF ricade su `.p7m`;
- i nomi proposti sono quelli della CLI (test già esistente in `job.rs`);
- le transizioni del pannello di firma: inattivo → PIN → passi → esito, e
  **Riprova** torna al PIN senza ripartire da capo;
- il portachiavi: scrittura e rilettura per due carte diverse non si pestano i
  piedi (con un archivio finto, senza toccare quello di sistema).

Si perde `the_preview_builds_a_page_texture`, che sfruttava `run_ui` di egui —
iced non ha un equivalente. Lo sostituisce un test diretto su `Document::render`,
dove sta la sostanza; il resto era impalcatura del toolkit.

## Dipendenze

| | prima | dopo |
|---|---|---|
| toolkit | `eframe` 0.35 | `iced` 0.14 |
| portachiavi | `secret-service` 5 + `tokio` | `keyring` 4.1 (condiviso con la CLI) |
| tema di sistema | — | `dark-light` 2.0 |
| apri cartella | `xdg-open` a mano | `opener` 0.8 |
| dialoghi file | `rfd` 0.17 | invariato |
| PDF | `hayro` 0.7 | invariato |

Bilancio: due dipendenze entrano, due escono (`secret-service`, `tokio`), e
sparisce una chiamata a sottoprocesso. Sono tutte multipiattaforma, quindi la
strada per Windows resta aperta senza lavoro speculativo adesso.

## Ordine di realizzazione

1. `cie-firma/portachiavi.rs` e adeguamento della CLI — indipendente dalla GUI,
   testabile subito.
2. Identificazione della carta senza PIN, con la verifica di `cie info`.
3. Ossatura iced: tema, font, impianto della finestra, stati vuoti.
4. Palco: anteprima, tela del riquadro, trascinamento.
5. Pannello di firma: PIN, passi, esito, Riprova.
6. Sonda del lettore a eventi e chip nella barra.
7. Cancellazione di `app.rs` e delle dipendenze morte.

I passi 1 e 2 valgono da soli anche se la GUI non fosse mai finita.
