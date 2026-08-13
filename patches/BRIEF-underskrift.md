# Briefing per l'agent: patch a `underskrift`

Documento autoportante. Va letto da un agent che lavora **nella root del repo
`underskrift`** (https://github.com/kushaldas/underskrift), non nel repo che lo
ha prodotto.

Accanto a questo file c'è `underskrift-0.1.4.diff`: sono le stesse modifiche già
scritte e verificate, applicate però al **crate pubblicato 0.1.4**. Serve come
riferimento di ciò che deve risultare, non come patch da applicare alla cieca -
il git di upstream è più avanti del pacchetto su crates.io e i contesti possono
non combaciare. **Riscrivi le modifiche sul codice che trovi**, poi confronta.

---

## Regole, prima di tutto

1. **Tutto ciò che finisce nel repo è in inglese**: codice, commenti, doc
   comment, messaggi di commit, titolo e corpo della PR. Questo briefing è in
   italiano perché è per te, non per upstream.
2. **Nessun percorso assoluto** in codice, test o documentazione. Né `/home/...`
   né percorsi di macchine specifiche. Per i file temporanei nei test usa
   `tempfile` (è già una dev-dependency) o `env!("CARGO_MANIFEST_DIR")`.
3. **Adegúati alle convenzioni del file che tocchi**: densità dei commenti,
   stile dei nomi, forma dei doc comment. Non riformattare codice che non stai
   cambiando, non correggere refusi altrui, non riordinare gli `use`. Il diff
   deve contenere solo ciò che serve.
4. **Una patch, un commit, una PR.** Sono indipendenti e vanno valutate
   separatamente. Non impacchettarle insieme.
5. Ogni patch porta il suo test. Il progetto usa test unitari in coda al file
   (`#[cfg(test)] mod tests`): segui quello schema.

## Come si compila e come si testa

```sh
cargo test --lib                                             # feature di default
cargo test --lib --no-default-features --features visual,blocking
cargo clippy --all-targets
```

**Entrambe le combinazioni**, sempre. La patch 1 riguarda proprio le feature, e
un errore lì non si vede compilando in un modo solo - è successo davvero,
vedi sotto.

Trappola nota: nel crate **pubblicato** 25 test falliscono con
`signer.p12: No such file or directory`. Il fixture PKCS#12 non è incluso nel
pacchetto. **Nel repo git dovrebbero passare.** Se falliscono anche lì, fermati
e segnalalo: significa che il problema è un altro e non è tuo. In ogni caso
prendi il conteggio dei fallimenti *prima* di toccare qualsiasi cosa e usalo
come riferimento - l'unico numero che conta è che non aumenti.

---

## Patch 1 - `tsp-ltv` entra sempre, anche a feature spente

### Il problema

`underskrift` dichiara la dipendenza senza disattivarne i default:

```toml
[dependencies.tsp-ltv]
version = "0.2"
```

`tsp-ltv` ha `default = ["tsp", "ltv", "blocking"]` e la sua feature `tsp` tira
`reqwest`. Risultato: chi compila `underskrift` con
`default-features = false, features = ["visual"]` - cioè chi non vuole né TSA né
LTV - si ritrova comunque **reqwest, rustls, hyper e tokio** nel binario, e non
ha alcun modo di levarli.

Misurato su un progetto reale: **311 crate contro 200**.

### La correzione

Disattivare i default e inoltrare esplicitamente dalle feature omonime:

```toml
[features]
blocking = ["dep:tokio", "tsp-ltv/blocking"]
ltv      = ["tsp", "tsp-ltv/ltv"]
tsp      = ["dep:reqwest", "dep:tokio", "tsp-ltv/tsp"]

[dependencies.tsp-ltv]
version = "0.2"
default-features = false
```

### L'errore da non ripetere

Al primo tentativo era stato aggiunto solo `tsp-ltv/tsp`, dimenticando
`tsp-ltv/ltv`. Con `--no-default-features --features visual,blocking` compilava
benissimo; con le feature di **default** falliva, perché `src/ltv/mod.rs`
importa `tsp_ltv::ltv`, che è dietro `tsp-ltv/ltv`.

Prima di dichiarare fatta questa patch, verifica che **ogni** re-export da
`tsp_ltv` sia coperto dalla feature che lo abilita. Almeno questi:

| dove | cosa importa | feature necessaria |
|---|---|---|
| `src/lib.rs` | `tsp_ltv::tsp` | `tsp-ltv/tsp` |
| `src/ltv/mod.rs` | `tsp_ltv::ltv` | `tsp-ltv/ltv` |
| `src/crypto/mod.rs` | `tsp_ltv::crypto::{algorithm, verify}` | nessuna, sempre disponibile - **conferma** |
| `src/lib.rs` | `tsp_ltv::{der_utils, trust}` | nessuna, sempre disponibile - **conferma** |

Non fidarti di questa tabella: rifai il `grep` sul codice che hai davanti.

### Verifica

```sh
cargo tree -e normal -i reqwest --no-default-features --features visual,blocking
# atteso: "did not match any packages"
cargo test --lib     # con le feature di default: deve compilare
```

---

## Patch 2 - `/M` non viene mai scritto, i lettori mostrano il 1970

### Il problema

`SigningOptions` ha `cms_signing_time`, ma nel profilo PAdES l'attributo CMS
`signingTime` è vietato (ETSI EN 319 142-1) e infatti il builder giustamente lo
omette. La data dovrebbe quindi finire in `/M` nel dizionario di firma - e lì
non ci arriva mai. `build_sig_dict` non lo scrive, e il resto del percorso
nemmeno.

Il codice della libreria `/M` lo sa **leggere**: `src/inspect/signatures.rs` e
`src/verify/extractor.rs` lo estraggono. Nessuno lo scrive.

Conseguenza pratica: `pdfsig` su un PDF firmato con `cms_signing_time`
impostato riporta `Signing Time: Jan 01 1970 01:00:00`.

### La correzione

In `src/signer.rs`, accanto a dove vengono aggiunti `Reason`, `Location` e
`ContactInfo` al dizionario di firma:

```rust
// /M - signing time. PAdES forbids the CMS signingTime attribute
// (ETSI EN 319 142-1), so the date lives here or nowhere. Without it
// every reader shows the epoch.
if let Some(t) = &self.options.cms_signing_time {
    let m = format!("D:{}", t.format("%Y%m%d%H%M%S"));
    sig_dict.set("M", Object::String(m.into_bytes(), lopdf::StringFormat::Literal));
}
```

### Un punto da discutere con il manutentore

`cms_signing_time` è una `NaiveDateTime`: non porta fuso orario. La sintassi PDF
per `/M` prevede un offset opzionale (`D:YYYYMMDDHHmmSSOHH'mm`), qui omesso.
Poppler lo accetta. Due alternative, da proporre nella PR senza deciderle da
solo:

- aggiungere `Z00'00` e documentare che il valore è inteso UTC;
- cambiare il tipo in `DateTime<FixedOffset>` - più corretto ma **rompe l'API**.

Se non c'è risposta, lascia com'è: è il cambiamento più piccolo e non peggiora
niente rispetto a oggi.

### Verifica

Firma un PDF con `cms_signing_time` valorizzato e controlla che `pdfsig`
riporti la data giusta invece del 1970. Aggiungi un test unitario che asserisce
la presenza di `/M` nell'output.

---

## Patch 3 - `certify` è dichiarato ma non fa niente

### Il problema

`SigningOptions::certify: bool` esiste, è documentato come «Whether this is a
certification signature (first signature with DocMDP)», e **non viene mai
letto**. Nel crate pubblicato le uniche due occorrenze sono la dichiarazione del
campo e il valore in `Default`.

Stessa storia per `src/core/doc_mdp.rs`: un file di 16 righe che dichiara
`DocMdpPermissions { NoChanges = 1, FormFillingAndSigning = 2,
FormFillingSigningAndAnnotation = 3 }` e non la usa da nessuna parte. Il modulo
è un segnaposto.

Chi imposta `certify: true` oggi ottiene silenziosamente una normale firma di
approvazione. È l'unico dei tre problemi in cui l'API **mente**.

### Che cosa serve produrre

Una firma di certificazione richiede due cose, e servono entrambe: il
`/Reference` da solo è inerte se il catalogo non lo indica.

Nel dizionario di firma:

```
/Reference [ << /Type /SigRef
                /TransformMethod /DocMDP
                /TransformParams << /Type /TransformParams /V /1.2 /P n >> >> ]
```

Nel catalogo del documento:

```
/Perms << /DocMDP <riferimento indiretto al dizionario di firma> >>
```

`/V` è la versione dei parametri di trasformazione DocMDP ed è sempre `1.2`
(ISO 32000-1 §12.8.2.2). `/P` vale 1, 2 o 3 secondo `DocMdpPermissions`.

**Vincolo da far rispettare**: una sola firma di certificazione per documento, e
dev'essere la prima. Se il catalogo ha già `/Perms` con `/DocMDP`, la firma deve
**fallire con un errore chiaro**, non produrre un file che i lettori segnalano
come manomesso.

### Come agganciarla

- `src/core/doc_mdp.rs`: due funzioni pubbliche, una che costruisce l'oggetto
  `/Reference` a partire da `DocMdpPermissions`, una che scrive `/Perms` nel
  catalogo e rifiuta il documento già certificato.
- `src/signer.rs`: se `options.certify`, aggiungi `/Reference` al dizionario di
  firma **prima** di `doc.add_object(...)`, e chiama la seconda funzione
  **dopo**, perché le serve l'`ObjectId` del dizionario.
- `src/lib.rs`: esporta `DocMdpPermissions` fra i re-export pubblici.

Il catalogo è già modificabile in questo punto del flusso - `src/core/acroform.rs`
ci scrive `/AcroForm` e l'update incrementale lo riscrive: prendi quel codice a
modello per recuperare l'ID del catalogo dal trailer.

### Sulla forma dell'API

Aggiungi un campo `doc_mdp_permissions: DocMdpPermissions` a `SigningOptions`,
**senza toccare** `certify: bool`. `DocMdpPermissions` ha già un `Default`
(`FormFillingAndSigning`, cioè `/P 2`), quindi il campo nuovo non rompe nessuno
e il comportamento predefinito resta quello che consente le controfirme.

La tentazione di sostituire i due campi con un unico
`certify: Option<DocMdpPermissions>` è più elegante ma **rompe l'API**.
Proponila nella descrizione della PR, non farla.

### Verifica

Nel diff allegato ci sono tre test unitari già scritti: il `/Reference` porta il
livello richiesto, i valori dell'enum corrispondono ai numeri PDF, e un
documento già certificato viene rifiutato. Aggiungi un test di integrazione che
firma con `certify: true` e controlla che nel PDF compaiano sia
`/TransformMethod /DocMDP` sia `/Perms`, e che senza `certify` **non** compaia
nessuno dei due.

---

## Proposta 4 - CAdES (`.p7m`): apri una discussione, non una PR

Questa non è una correzione: è un ampliamento dello scopo del progetto, e la
decisione non è tua né mia.

**Apri prima una issue.** `underskrift` si descrive come *«PDF digital signing
library»*. Un `.p7m` non è un PDF: è un CMS enveloping che avvolge un file
qualsiasi. Chiedere prima di scrivere il codice è la differenza fra una PR
accolta e una PR che fa perdere tempo a tutti.

Materiale per argomentarla, se il manutentore è interessato:

- **La parte difficile è già lì.** `src/cms/builder.rs` costruisce già
  `SignedData` con gli attributi firmati che CAdES-B-B richiede: `contentType`,
  `messageDigest`, `signingCertificateV2` con `IssuerSerial`, più
  `CMSAlgorithmProtection`. È la stessa struttura, verificata contro il
  middleware CIE ufficiale.
- **Manca poco.** `PdfCmsBuilder` produce CMS *detached* - `econtent: None`,
  e riceve un hash già calcolato invece dei dati. Un `.p7m` vuole l'opposto:
  `econtent: Some(dati)` e il digest calcolato internamente. In pratica una
  variante del builder più un modo di serializzare il risultato.
- **Il costo vero è di scopo, non di righe.** Significa che il crate esce dal
  dominio PDF: nome, descrizione, README e keyword non lo dicono, e il
  manutentore si ritroverebbe a mantenere un'area nuova.

Se la risposta è no, va benissimo: la stessa cosa si costruisce fuori sopra il
crate `cms` di RustCrypto. Se è sì, tratta il CMS builder come punto di
partenza e non riscriverlo.

---

## Come verificare qualunque cosa tu produca

Non fidarti dei soli test unitari: sono strumenti che verificano sé stessi. Usa
almeno uno strumento terzo.

```sh
pdfsig documento-firmato.pdf     # poppler: validità, tipo, ByteRange, data
```

Deve dire `Signature Validation: Signature is Valid.`

Per guardare dentro il CMS: estrai il contenuto esadecimale di `/Contents` dal
PDF, decodificalo in binario e passalo a

```sh
openssl asn1parse -inform DER -in cms.der -i
```

Da lì si leggono gli attributi firmati, gli OID degli algoritmi e la struttura
di `signingCertificateV2`.

## Ordine consigliato

1. Patch 1 (feature). È indipendente e sblocca le build snelle.
2. Patch 2 (`/M`). Piccola, evidente, difficile da rifiutare.
3. Patch 3 (DocMDP). La più sostanziosa: completa una funzione dichiarata e mai
   finita, quindi presentala così, non come funzione nuova.
4. Issue per il CAdES. Solo dopo che le prime tre hanno avuto una risposta: ti
   dice anche quanto è reattivo il manutentore prima di investire su una cosa
   grossa.
