# Briefing per l'agent: immagini nelle apparenze custom di `underskrift`

Documento autoportante. Va letto da un agent che lavora **nella root del repo
`underskrift`**, non nel repo che lo ha prodotto.

Accanto a questo file c'è `underskrift-custom-images.diff`: la modifica già
scritta e verificata, presa dal fork `AttilioGreco/underskrift` a `cb24aee`.
Serve come riferimento di ciò che deve risultare, non come patch da applicare
alla cieca. **Riscrivi la modifica sul codice che trovi**, poi confronta.

Valgono le stesse regole del briefing accanto (`BRIEF-underskrift.md`): tutto
in inglese, nessun percorso assoluto, adeguarsi alle convenzioni del file che
si tocca, una patch un commit una PR, ogni patch porta il suo test.

## Come si compila e come si testa

```sh
./gen-test-fixtures.sh                                       # PRIMA di tutto
cargo test --lib                                             # feature di default
cargo test --lib --no-default-features --features visual,blocking
cargo test --doc
cargo clippy --all-targets
```

**`./gen-test-fixtures.sh` va lanciato per primo.** Senza, ~49 test falliscono
con `signer.p12: No such file or directory` e non è colpa tua: il fixture
PKCS#12 non è versionato, lo genera quello script. (Il briefing precedente
descriveva questo inciampo come una trappola del solo crate pubblicato; è anche
del repo git, appena clonato.)

Baseline misurata su `cb24aee`, a fixture generati: **375** test con le feature
di default, **147** con `visual,blocking`, clippy muto. Dopo la patch:
**378** e **150**.

---

## Il problema

`SignatureLayout::Custom` esiste per dare al chiamante il controllo completo
dell'apparenza: implementi `AppearanceRenderer`, restituisci i byte di un
content stream PDF, e disegni quello che vuoi dove vuoi.

Solo che non puoi disegnare un'immagine.

`CustomAppearanceResult` porta due campi, `content` e `font_resources`. Non c'è
modo di dichiarare un'immagine, e infatti `build_custom_appearance` chiude così:

```rust
Ok(AppearanceStream {
    content: stream,
    font_resources: custom_result.font_resources,
    #[cfg(feature = "visual")]
    embedded_font_resources: Vec::new(),
    #[cfg(feature = "visual")]
    image_resources: Vec::new(),   // <-- sempre vuoto
    bbox: [0.0, 0.0, width, height],
})
```

Un renderer può emettere `/Im1 Do` quanto vuole: nessun XObject viene creato e
l'operatore dipinge il nulla.

Il risultato è che chi vuole comporre una firma visibile a modo proprio — una
scansione autografa accanto a righe di testo che le quattro `Arrangement`
predefinite non dispongono come serve — non ha strada. `ImageAndText` divide il
riquadro 40/60 e non si può dire altrimenti; `Custom` sa posizionare solo testo.

## La correzione

Far arrivare le immagini del renderer fino all'`AppearanceStream`. La pipeline
del firmatario è già pronta: `signer.rs` cicla su `appearance.image_resources`,
costruisce un XObject per ciascuna e gestisce l'`SMask` per l'alpha, senza
sapere da quale layout arrivino.

**1. `src/visual/layout.rs` — `CustomAppearanceResult` dichiara le immagini**

```rust
#[derive(Debug, Clone, Default)]
pub struct CustomAppearanceResult {
    pub content: Vec<u8>,
    pub font_resources: Vec<(String, String)>,
    /// Images used in the content stream.
    /// Each entry is `(resource_name, image)` — e.g., `("Im1", config)`.
    #[cfg(feature = "visual")]
    pub image_resources: Vec<(String, ImageConfig)>,
}
```

La forma `(nome_risorsa, valore)` è quella che `font_resources` usa già nello
stesso struct: seguila invece di introdurre un tipo nuovo.

`Default` è parte della patch, non un extra: aggiungere un campo a uno struct
costruito per letterale è una rottura per chiunque stia a valle, e `Default`
dà la via d'uscita (`..Default::default()`) perché la prossima aggiunta non lo
sia. Aggiornare di conseguenza i tre punti di costruzione interni
(`SignatureTemplate::render`) e l'esempio nel doc comment di
`AppearanceRenderer`, che altrimenti non compila più.

**2. `src/visual/appearance.rs` — `build_custom_appearance` le prepara**

Al posto di `image_resources: Vec::new()`:

```rust
#[cfg(feature = "visual")]
let image_resources = custom_result
    .image_resources
    .iter()
    .map(|(resource_name, config)| {
        Ok(ImageResource {
            resource_name: resource_name.clone(),
            image: prepare_image(&config.data, config.format)?,
        })
    })
    .collect::<Result<Vec<_>, VisualError>>()?;
```

La decodifica sta qui e non nel renderer di proposito: così un'apparenza custom
consegna un `ImageConfig` come fa ogni altro layout, invece di dover sapere come
il PDF incorpora un'immagine.

Il gate `#[cfg(feature = "visual")]` serve: `Custom` non è dietro `visual`, ma
`ImageConfig`, `prepare_image` e lo stesso campo `image_resources` di
`AppearanceStream` sì.

## I test

Tre, in coda a `src/visual/appearance.rs`, dove stanno gli altri:

- un renderer che chiede un'immagine se la ritrova in `image_resources`, con il
  nome che ha dato e le dimensioni giuste, e `/Im1 Do` nel content stream;
- un renderer che non chiede niente non produce risorse — non un XObject vuoto
  che la pipeline dovrebbe saltare;
- byte che non sono il formato che dichiarano falliscono lì, dove al chiamante
  si può ancora dire quale risorsa era sbagliata, e non in mezzo alla scrittura
  del PDF.

Gli helper `test_jpeg_bytes()` e `test_png_rgba_bytes()` esistono già in quel
modulo di test: usali invece di generarne altri.
