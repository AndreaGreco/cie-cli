# `cie-gui` — interfaccia grafica per la firma con CIE

Progetto del 4 agosto 2026. Binario separato, costruito sullo stesso livello
carta della CLI.

## Perche'

`cie sign`, `cie sign-p7m` e `cie sign-pdf` funzionano, ma pretendono un
terminale, un PIN gia' nel portachiavi e un riquadro di firma cablato nel
sorgente. L'app Java del Ministero fa peggio quasi in tutto, tranne una cosa:
mostra il documento e lascia scegliere dove mettere la firma. Questo progetto
copre quel divario.

Obiettivo: una finestra che apra un documento, lo mostri, faccia scegliere il
punto della firma, gestisca il PIN nel portachiavi e scriva il file firmato.
Niente di piu'.

## Struttura del workspace

```
crates/
  cie-lib/     invariato — livello carta puro (PC/SC, Secure Messaging, firma)
  cie-firma/   NUOVO   — firmatario on-card + contenitori PAdES/CAdES/detached
  cie-cli/     dimagrisce — clap, stampe a terminale, ottieni_pin
  cie-gui/     NUOVO   — binario `cie-gui` (eframe + rfd + hayro)
```

Il livello nuovo e' `cie-firma`. Nasce perche' `FirmatarioCie` e le tre
funzioni di firma vivono oggi dentro il binario `cie-cli` e la GUI non puo'
raggiungerle. Non finiscono in `cie-lib` per la ragione gia' scritta in
`HANDOFF.md`: `cie-lib` resta il livello carta puro, senza lopdf ne' parser di
font, altrimenti il futuro modulo PKCS#11 se li porterebbe dietro per esporre
`C_Sign`.

`cie-cli` continua a leggere il PIN con `secret-tool lookup` (shell-out): non si
tira dentro zbus.

### Superficie pubblica di `cie-firma`

```rust
pub struct FirmatarioCie;                    // impl underskrift::CryptoSigner

pub enum Passo { Connessione, CanaleSicuro, Pin, Certificato }
pub fn apri_e_autentica(
    ctx: &PcscContext,
    pin: &str,
    avanzamento: &dyn Fn(Passo),
) -> Result<FirmatarioCie>;

pub struct OpzioniPades {
    pub pagina: u32,
    pub riquadro: [f32; 4],          // llx, lly, urx, ury in punti PDF
    pub motivo: Option<String>,
    pub certifica: Option<u8>,
}
pub fn firma_pdf(pdf: &[u8], f: &dyn CryptoSigner, o: &OpzioniPades) -> Result<Vec<u8>>;
pub fn firma_p7m(contenuto: &[u8], f: &dyn CryptoSigner) -> Result<Vec<u8>>;
pub fn firma_detached(contenuto: &[u8], f: &dyn CryptoSigner) -> Result<Vec<u8>>;
```

Il trasloco e' quasi solo `git mv`. Tre differenze rispetto a oggi:

1. **Il riquadro non e' piu' cablato.** `firma_pdf` lo riceve in
   `OpzioniPades`. La CLI passa il valore attuale (`330, 40, 560, 110`), quindi
   `cie sign-pdf` non cambia comportamento.
2. **`apri_e_autentica` non stampa.** Riferisce via callback: la CLI stampa le
   sue `✓`, la GUI muove il wizard. Il PIN arriva gia' risolto dal chiamante —
   la scelta fra argomento, portachiavi e prompt resta della CLI.
3. **`sign.rs` perde l'handshake duplicato.** Oggi apre canale e verifica PIN
   per conto suo perche' gli serve `sm` per la verifica locale; quella verifica
   entra in `firma_detached`, che la fa con la pubblica del certificato.

Il firmatario finto dei test si sposta con loro e resta `#[cfg(test)]` dentro
`cie-firma`: la GUI non ne ha bisogno, quindi non serve esporlo dietro una
feature.

## Il wizard

Cinque passi. Il terzo esiste **solo per il PAdES**: per p7m e detached la
sequenza e' di quattro.

```
 ① File ─ ② Tipo ─ ③ Riquadro ─ ④ PIN ─ ⑤ Esito
```

| Passo | Contenuto |
|---|---|
| ① File | `rfd::FileDialog::pick_file()`, mostra nome e dimensione |
| ② Tipo | PAdES / p7m / detached. PAdES disabilitato se il file non inizia per `%PDF-` |
| ③ Riquadro | anteprima hayro, riquadro trascinabile, navigazione pagine, motivo, certifica |
| ④ PIN | campo password, ☑ salva nel portachiavi, \[Dimentica\], percorso di uscita con \[Cambia…\] |
| ⑤ Esito | percorso scritto e \[Apri cartella\], oppure errore e \[Riprova\] che riporta a ④ |

`Avanti` resta disabilitato finche' il passo non e' valido. Il PIN si valida a
④ — otto cifre numeriche — **prima** di toccare la carta: il contatore e' a tre
tentativi e non va speso su un errore di battitura.

Se il portachiavi contiene gia' un PIN, il campo di ④ arriva precompilato e
accanto compare \[Dimentica\], che cancella la voce.

### File di uscita

Il campo mostra un percorso precompilato con gli stessi suffissi della CLI, cosi'
i due strumenti producono gli stessi nomi:

| tipo | uscita |
|---|---|
| PAdES | `<nome>-firmato.pdf` |
| p7m | `<nome>.p7m` |
| detached | `<nome>.sig` piu' `<nome>.cer` |

\[Cambia…\] apre `rfd::FileDialog::save_file()` gia' posizionato su quel nome.
Il dialogo di sistema si occupa da solo della conferma di sovrascrittura.

## Anteprima e mappatura del riquadro

L'unico pezzo di logica non banale.

La pagina si rasterizza con `hayro::render` a una scala che la fa entrare in
larghezza nel pannello. La texture si ricalcola solo quando cambia la pagina o
la larghezza del pannello, non a ogni frame. Il riquadro si disegna trascinando
il mouse sull'anteprima e resta dov'e'.

Le coordinate schermo vanno convertite in punti PDF. Il vincolo e' che
`SignatureRect::Absolute` di underskrift finisce dritto nel `/Rect`
dell'annotazione, cioe' **user space non ruotato, origine in basso a sinistra**,
mentre l'immagine che l'utente vede e' ruotata e ritagliata:

```
rect schermo → frazioni 0..1 dell'immagine
             → si annulla /Rotate                   (page.rotation())
             → × dimensioni del crop box + origine  (page.crop_box())
             → si ribalta y                         (schermo top-down, PDF bottom-up)
```

Il crop box diverso dal media box e' comune nei documenti scansionati, e la
rotazione a 90° o 270° lo e' altrettanto: ignorarli metterebbe la firma nel
posto sbagliato senza nessun errore visibile.

`punti_da_schermo()` e' una funzione pura. Il test la esercita sulle quattro
rotazioni con un crop box traslato, senza carta e senza finestra.

### API di hayro verificate

- `hayro::render(&Page, &RenderCache, &InterpreterSettings, &RenderSettings) -> Pixmap`
- `RenderSettings { x_scale, y_scale, width, height, bg_color }`
- `Pixmap::data_as_u8_slice()` — RGBA **premoltiplicato** →
  `egui::ColorImage::from_rgba_premultiplied`
- `Page::render_dimensions()`, `Page::crop_box()`, `Page::rotation()`
- `hayro` richiede rustc ≥ 1.92 (in locale: 1.93) e la feature `embed-fonts`
  copre i 14 font standard

## Carta, portachiavi, errori

**Thread.** La carta lavora fuori dal thread della UI:

```rust
pub enum Messaggio { Passo(String), Fatto(PathBuf), Errore(String) }
pub fn avvia(richiesta: Richiesta) -> Receiver<Messaggio>;
```

`Richiesta` possiede tutto (percorso, tipo, PIN, opzioni) e viene spostata nel
thread. `update()` svuota il canale e chiama `request_repaint()` finche' il
lavoro e' in corso. L'handshake costa circa un secondo, piu' la firma: senza
thread la finestra risulterebbe bloccata.

**Portachiavi.** `secret-service 5.1` con feature `rt-tokio-crypto-rust`.
Attributo **esattamente `service=cie`**, label `CIE PIN`: sono quelli che
scriverebbe `secret-tool store`, quindi le voci gia' salvate si leggono e quelle
scritte dalla GUI restano leggibili da `cie`. Questa compatibilita' va verificata
a mano con `secret-tool lookup service cie` dopo la prima scrittura dalla GUI:
e' un'ipotesi sul formato degli attributi, non un fatto misurato.

L'API di `secret-service` e' asincrona; si usa `Runtime::block_on` come gia' fa
`firma_pdf`. Le feature di tokio necessarie (almeno `net` per il socket D-Bus)
si accertano in fase di implementazione.

**Errori.** `anyhow` risale fino a ⑤ e si stampa con `{:#}`, catena intera. Tre
casi meritano un messaggio scritto in italiano invece del wrap tecnico:

- nessun lettore collegato
- nessuna carta nel lettore
- PIN errato — con il numero di tentativi rimasti, se la carta lo riporta

## Test

- **`cie-firma`**: i test esistenti si spostano tali e quali (PAdES con
  ByteRange e Widget, DocMDP presente e assente, busta p7m verificata con
  OpenSSL, busta alterata rifiutata, righe del riquadro dal certificato).
- **`cie-gui`**: un test su `punti_da_schermo()` — quattro rotazioni, crop box
  traslato. Nessun test di interfaccia.

Nessun test richiede la carta, come oggi.

## Build

```bash
cargo build -p cie-cli      # binario `cie`      — nessuna traccia di eframe
cargo build -p cie-gui      # binario `cie-gui`
```

Restano validi i vincoli del workspace: serve `../underskrift` (branch
`fix/pades-conformance`) per via di `[patch.crates-io]`, e le feature di
underskrift vanno provate in entrambe le combinazioni — con le nostre e con le
sue di default — perche' e' li' che si sono nascosti gia' due bug.

## Fuori scopo, deliberatamente

| Cosa | Perche' |
|---|---|
| drag & drop del file | il dialogo copre il caso |
| zoom sull'anteprima | la pagina a larghezza pannello basta per posizionare un riquadro |
| maniglie di ridimensionamento | il riquadro si disegna trascinando; ridisegnarlo costa un gesto |
| memoria delle preferenze fra sessioni | nessuno stato su disco, coerente col resto del progetto |
| marca temporale, LTV | riporterebbero dentro uno stack HTTP; vale la scelta gia' fatta per la CLI |
| anteprima del riquadro renderizzato | il contenuto del riquadro lo compone underskrift; qui si sceglie solo dove va |

Resta valido anche il limite di fondo: il certificato della CIE e' di
autenticazione, non di firma qualificata. La GUI deve dirlo, come gia' fa la
CLI, nella schermata di esito.
