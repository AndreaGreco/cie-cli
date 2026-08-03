# Punto della situazione — 3 agosto 2026

Documento di passaggio di consegne. Cancellabile quando il lavoro qui descritto
è chiuso.

Branch di lavoro: **`secure-messaging`** (`main` è ferma a `6e998e9`).

---

## Dove siamo arrivati

Il progetto parla alla CIE **direttamente in PC/SC**, senza il middleware
ufficiale, senza l'app Java di registrazione e senza scrivere niente su disco.

Funziona e **verificato su carta reale** (Bit4Id, ATR `3B 8F 80 01 80 31 80 65
49 54 4A 34 49 …`):

- selezione dell'applicazione in base al tipo di carta
- Secure Messaging completo: Diffie-Hellman, DAPP con autenticazione **mutua**
  (verifichiamo anche la firma del chip), 3DES-CBC, Retail MAC controllato in
  entrambe le direzioni
- verifica del PIN nel canale sicuro
- lettura dei file protetti (seriale, certificato, dati anagrafici)
- **firma on-card** SHA-256 / PKCS#1 v1.5, verificata in locale e in modo
  indipendente con OpenSSL (positiva sul file originale, negativa dopo
  l'alterazione di un byte)

- **PDF firmati in PAdES B-B** con riquadro visibile, verificati con `pdfsig`
- **interfaccia grafica** `cie-gui`: anteprima del PDF e riquadro trascinabile

42 test, nessuno richiede la carta. Zero warning clippy nel nostro codice.

```bash
cie info                 # dati leggibili senza PIN
cie auth                 # canale + PIN + dati protetti
cie sign <file>          # firma detached (.sig) + certificato (.cer)
cie sign-p7m <file>      # CAdES enveloping: contenuto + firma in <file>.p7m
cie sign-pdf <f.pdf>     # PAdES, firma dentro il PDF
cie -vv auth             # ogni APDU in TX/RX

cie-gui                  # le stesse firme, con il documento davanti
```

Il PIN si legge dal portachiavi: `secret-tool store --label='CIE PIN' service cie`.
Servono tutte e 8 le cifre. `cie-gui` scrive e cancella la stessa voce da solo.

## Il livello `cie-firma`

`CieSigner` e i tre contenitori vivevano dentro il binario `cie-cli`, dove
la GUI non poteva raggiungerli. Ora stanno in `crates/cie-firma`, fra `cie-lib`
e chi ha una faccia. La ragione per cui non sono finiti in `cie-lib` e' sempre
quella scritta piu' sotto: `underskrift` si porta dietro lopdf e un parser di
font, e il modulo PKCS#11 non deve ereditarli per esporre `C_Sign`.

Tre cambi di sostanza rispetto a prima:

- il riquadro PAdES non e' piu' cablato, arriva in `PadesOptions` (la CLI passa
  la posizione di prima, quindi `cie sign-pdf` si comporta come sempre)
- `open_and_authenticate` non stampa piu': riferisce l'avanzamento via callback, e i
  due chiamanti ne fanno quel che vogliono
- `sign.rs` ha perso l'handshake duplicato; la verifica locale della firma
  detached e' dentro `sign_detached`

## `cie-gui`

`eframe` + `rfd` + `hayro`, binario separato. Wizard di cinque passi, quattro
quando la firma non e' PAdES: il riquadro esiste solo li'.

Il pezzo che merita attenzione e' `preview.rs`. `SignatureRect::Absolute` di
underskrift finisce dritto nel `/Rect` dell'annotazione — **user space non
ruotato, origine in basso a sinistra** — mentre l'immagine che l'utente trascina
e' ruotata secondo `/Rotate` e ritagliata sul crop box. `to_pdf_rect()` annulla la
rotazione, esce dal crop box e ribalta la y. E' una funzione pura e i test la
esercitano sulle quattro rotazioni con un crop box traslato: sbagliarla
metterebbe la firma nel posto sbagliato **senza nessun errore visibile**.

Il portachiavi passa da `secret-service` (D-Bus) invece che dal binario
`secret-tool`, con attributo `service=cie` ed etichetta `CIE PIN`: sono gli
stessi che scriverebbe `secret-tool store`, ed e' l'unica cosa che tiene insieme
le due strade. **Da verificare a mano dopo la prima scrittura dalla GUI**:

```bash
secret-tool lookup service cie
```

La CLI continua a leggere con lo shell-out: non si tira dentro zbus.

Restano fuori, per scelta: drag & drop, zoom sull'anteprima, maniglie di
ridimensionamento del riquadro, memoria delle preferenze fra sessioni.

Il progetto completo sta in
`docs/superpowers/specs/2026-08-04-cie-gui-design.md`.

---

## PAdES: fatto

La domanda che bloccava tutto era: **le librerie Rust accettano un firmatario
esterno?** Sì. `cie sign-pdf` produce PDF firmati in **PAdES B-B** con riquadro
visibile, e la chiave non lascia mai il chip.

Libreria scelta: **`underskrift 0.1.4`** (BSD-2,
https://github.com/kushaldas/underskrift). Espone `CryptoSigner`, un tratto di
cinque metodi di cui uno è `sign_hash(&self, hash) -> Vec<u8>`: gli passiamo
`CieSigner`, lui ci consegna il digest e si riprende i 256 byte. Lo stesso
schema di HSM e firma remota.

Scartata `ades-rs 0.2.0`: ha lo stesso identico tratto (`Signer::sign_digest`),
ma il suo PAdES è un guscio — annotazione widget 0×0, nessun riquadro visibile,
niente `reason`/`location`, si ferma a B-T. 614 righe contro 26.000, e 64
download totali. Resta il candidato per il CAdES, se un giorno servirà.

Verificato con `pdfsig` di poppler su un PDF prodotto dal percorso di
produzione: `Signature is Valid`, `ETSI.CAdES.detached`, `Total document
signed`. Gli attributi firmati sono quelli che PAdES pretende — `contentType`,
`messageDigest`, `signingCertificateV2`.

### Il fork di underskrift

Sta in `../underskrift` (github.com/AttilioGreco/underskrift, branch
`fix/pades-conformance`), agganciato da `[patch.crates-io]` nel `Cargo.toml` di
workspace. **Senza quella cartella il progetto non compila.** Tutto il lavoro è
materiale da PR upstream. `patches/` conserva la vecchia forma a diff contro la
0.1.4, ormai superata dai commit del fork: si può buttare.

Le tre patch nate qui, ora commit nel fork:

1. **`tsp-ltv` senza `default-features = false`.** underskrift dichiara la
   dipendenza senza disattivarne i default, quindi reqwest + rustls + hyper +
   tokio entrano nel binario anche con `default-features = false` da parte
   nostra e anche se non si chiama mai una TSA. Aggiunto il flag e l'inoltro
   `tsp-ltv/tsp`. Misurato: **311 crate → 200**.

2. **`/M` mai scritto nel dizionario di firma.** PAdES vieta l'attributo CMS
   `signingTime` (ETSI EN 319 142-1), quindi la data va nel `/M` del dizionario
   — che underskrift legge nel suo inspector ma non scrive mai. Risultato: ogni
   lettore mostrava **1 gennaio 1970**. Otto righe in `src/signer.rs`.

3. **`certify` dichiarato e mai implementato.** Il campo esisteva in
   `SigningOptions` e non veniva mai letto; `doc_mdp.rs` erano 16 righe di enum
   inutilizzato. Chi lo impostava otteneva in silenzio una normale firma di
   approvazione — l'unico caso in cui l'API mentiva. Ora `--certifica [1|2|3]`
   produce `/Reference` con la trasformazione DocMDP nel dizionario di firma e
   `/Perms << /DocMDP … >>` nel catalogo, e rifiuta un documento già
   certificato. È la firma d'autore, quella col nastrino blu.

Nota sulla prima patch: al primo tentativo inoltrava solo `tsp-ltv/tsp` e non
`tsp-ltv/ltv`. Compilava con le nostre feature e falliva con quelle di default.
**Vanno provate entrambe le combinazioni**, sempre.

Il fork ha poi aggiunto il **CAdES standalone** (`cades::sign_attached`, firme
parallele, controfirme, `verify_enveloped`): è quello che alimenta
`cie sign-p7m`.

Due cose emerse integrandolo:

- `SigningOptions::doc_mdp_permissions` ora si chiama `certify_permissions`.
- **La feature `verify` non compila da sola**: `cms_verify.rs` usa
  `crate::tsp::token` per i timestamp, che esiste solo con la feature `tsp` — la
  quale riporta dentro reqwest e vanifica la patch 1. Stessa classe di bug della
  nota qui sopra. Per questo `cie` non usa il verificatore di underskrift e
  controlla le buste con OpenSSL nei test. Sistemarla vuol dire mettere
  `#[cfg(feature = "tsp")]` su `verify_timestamp_token`, `verify_doc_timestamp`
  e sui loro punti di chiamata in `verify/mod.rs`.

### Cosa è rimasto deliberatamente fuori

| Cosa | Perché |
|---|---|
| marca temporale (B-T) | è l'unica feature che riporta dentro uno stack HTTP; `cie` per ora parla solo con la carta |
| LTV, B-LT, B-LTA | servono le CA del Ministero, che la carta non espone |
| verifica/validazione | non è il nostro mestiere, per quello c'è `pdfsig` o il DSS |

Senza marca temporale la firma **invecchia**: quando il certificato CIE scade
non è più verificabile. È una scelta consapevole, non una svista.

### Confronto strutturale con il middleware

Fatto leggendo il loro sorgente, non a occhio. Il percorso è
`disigonsdk.cpp:sign_pdf` → `CSignatureGenerator::Generate(..., bDetached=true)`
→ `CSignerInfoGenerator::getSignedAttributes(attrs, false, !bDetached)`.

| | middleware | noi |
|---|---|---|
| SubFilter | `ETSI.CAdES.detached` (`CIESign.cpp:65`) | uguale |
| digest | SHA-256 | uguale |
| `signatureAlgorithm` | `sha256WithRSAEncryption` | uguale |
| identificatore del firmatario | `IssuerAndSerialNumber` | uguale |
| `contentType` | `id-data` | uguale |
| `messageDigest` | sì | uguale |
| `signingCertificateV2` | SHA-256 + `IssuerSerial` | uguale |
| `signingTime` nel CMS | **no**, perché detached | uguale |
| data della firma | `/M` nel dizionario (`SetSignatureDate`) | uguale, dopo la patch |
| certificazione DocMDP | no | uguale |
| byte riservati a `/Contents` | 10000 | 8192 (il nostro CMS ne usa 1361) |
| `CMSAlgorithmProtection` | **no** | **sì** |

L'unica differenza strutturale è l'ultima riga: underskrift aggiunge sempre
`id-aa-CMSAlgorithmProtection` (RFC 6211, lega insieme gli algoritmi per
impedire una sostituzione). È un attributo *firmato* e standard, e un
validatore che non lo conosce lo ignora — ma è l'unica cosa che ci distingue.
**Se l'app del Ministero rifiuta il PDF, è la prima cosa da togliere**:
underskrift lo scrive senza condizioni (`cms/builder.rs:516`), quindi
servirebbe una terza patch.

Nota: `/Contents` a 8192 non è stato allineato ai loro 10000 perché il margine
è già 6×. Se dovesse non bastare, underskrift fallisce in modo pulito
(`SignatureTooLarge`) — non tronca — ma lo fa *dopo* che la carta ha firmato,
quindi il tentativo di PIN è consumato.

### Il test che scioglie il dubbio, ora si può fare

Firmare un PDF con `cie sign-pdf` e darlo in pasto all'app del Ministero.
Vedi più sotto la questione del valore legale: è quello il criterio.

---

## L'altro fronte aperto: il modulo PKCS#11

Resta la strada più corta per far firmare a LibreOffice e autenticare Firefox
senza scrivere una riga di formato.

Nuovo crate `crates/cie-pkcs11/` con `crate-type = ["cdylib"]` → `libcie_pkcs11.so`.

È il motivo per cui **underskrift sta in `cie-firma` e non in `cie-lib`**: la
`cdylib` dipende da `cie-lib`, e non ha nessun bisogno di sapere cosa sia un PDF.
Se la libreria PAdES finisse nel livello carta, la `.so` si porterebbe dietro
lopdf e un parser di font per esporre `C_Sign`. `cie-lib` resta il livello carta
puro; il `CieSigner` che implementa `CryptoSigner` sono 40 righe in
`cie-firma`, e la `.so` non ha motivo di dipenderne.

Cose da sapere prima di iniziare:

1. **Serve `cryptoki-sys`, non `cryptoki`.** Il primo espone i tipi `CK_*` per
   *implementare* un modulo, il secondo è il client per consumarne uno. In Rust
   non esiste un crate maturo per implementare un provider: sono funzioni
   `extern "C"` scritte a mano più uno `static CK_FUNCTION_LIST`.
2. **Lo stato globale è facile**, contrariamente al solito: `SecureMessaging`
   non memorizza riferimenti alla carta (la riceve a ogni chiamata) e `PcscCard`
   è owned senza lifetime. Quindi basta
   `static STATO: Mutex<Option<Stato>>` con `{ ctx, card, sm }`. Niente
   self-referential struct.
3. **I panic non devono attraversare il confine FFI**: `catch_unwind` su ogni
   entry point, ritornando `CKR_GENERAL_ERROR`.
4. **L'handshake costa ~1 secondo**: farlo una volta e tenerlo nello stato.
5. Bastano ~15 funzioni; tutto il resto → `CKR_FUNCTION_NOT_SUPPORTED`.

Mappatura: `C_Login` → `verify_pin` (già fatto), `C_FindObjects` → certificato
0x1003 (già letto), `C_Sign` → `sign_sha256` (già fatto).

Firefox ha già la sua finestra per il PIN e passa gli 8 numeri via `C_Login`:
niente cache, niente registrazione, niente Java.

---

## Il nodo aperto sul valore legale

Il certificato sulla carta (file 0x1003) è **di autenticazione**:

```
Policy: 1.3.76.47.4 — "X.509 authentication certificate"
Extended Key Usage: TLS Web Client Authentication      ← e nient'altro
Key Usage: critical, Digital Signature                 ← manca nonRepudiation
nessuna estensione qcStatements
Issuer: Ministero dell'Interno — SUBCA003
```

Quindi le firme prodotte **non sono firme qualificate eIDAS**. Il middleware
ufficiale firma con questo stesso certificato, in locale, senza contattare
alcun server (verificato: nel percorso di firma non c'è nessuna chiamata remota).

**Decisione presa**: si procede comunque. Il criterio è empirico — se l'app
Android del Ministero valida i documenti che produciamo, la questione pratica è
chiusa, e vale più di qualunque ragionamento teorico. Il principio di non
discriminazione (eIDAS art. 25.1) è reale, e la firma qualificata serve davvero
solo per gli atti dell'art. 1350 nn. 1-12 del codice civile (immobiliare,
ipoteche): una fetta ristretta rispetto a "firmo il PDF e lo mando alla PA".

**Il test che scioglie il dubbio**: produrre un PDF firmato in PAdES e darlo in
pasto all'app del Ministero. Se lo valida, la strada è aperta. Se lo rifiuta, il
sospetto è che sia per l'EKU — che è un problema *indipendente* dal livello
giuridico: un certificato che dichiara `clientAuth` usato per firmare documenti
è fuori dallo scopo dichiarato, e un validatore che controlla l'EKU lo segnala
comunque. Ma è un sospetto, non una previsione.

Non verificato: l'architettura del servizio pubblico "Firma con CIE" (web/app
CieID), che potrebbe usare la CIE solo per autenticare e far apporre la firma
qualificata a un QTSP lato server. Plausibile ma non confermato.

---

## Cose da sapere sul codice

Il sorgente del middleware sta in `/home/agreco/Develop/cie-middleware-linux`
(BSD-3-Clause). Il file che conta è **`cie-pkcs11/CSP/IAS.cpp`**: praticamente
ogni costante e ogni sequenza APDU viene da lì.

Riferimenti utili in quel file:

| Cosa | Dove |
|---|---|
| `IAS::SM` — costruzione MAC del comando | riga 652 |
| `IAS::respSM` — verifica MAC della risposta | riga 753 |
| `IAS::DAPP` — le 7 APDU dell'autenticazione mutua | riga 426 |
| `IAS::DHKeyExchange` | riga 574 |
| `IAS::InitExtAuthKeyParam` — parametri CA dalla carta | riga 1086 |
| `IAS::SelectAID_IAS` — la regola delle due famiglie | riga 346 |
| `IAS::Sign` | riga 122 |
| `IAS::GetFirstPIN` — la cache su disco | riga ~1236 |
| enum `CIE_Type` e tabella ATR | `CSP/ATR.h:11`, `CSP/ATR.cpp` |

### Trappole già pagate (non ricascarci)

- **Le costanti si generano dal C++ con uno script, non si ricopiano.** Le prime
  chiavi DAPP nel repo erano completamente inventate.
- **Il MAC**: `ISOPad(SSC ‖ header)`, poi i TLV **grezzi**, poi **un solo**
  ISO-padding finale. Paddare i TLV singolarmente è l'errore classico.
- **L'SSC avanza esattamente una volta per comando e una per risposta.**
  `send_apdu` è l'unico punto che lo muove: non chiamare `encode_apdu` e
  `decode_response` separatamente.
- **Le risposte lunghe arrivano spezzate con `61xx`** e vanno ricomposte con
  GET RESPONSE *prima* di decodificare. Le GET RESPONSE non passano per l'SM e
  non toccano l'SSC.
- **APDU con campo dati vuoto non portano il byte Lc.**
- **Oltre 0xE7 byte serve il command chaining** (`CLA|0x10`).
- **I tag BER arrivano a 3 byte** (`BF A0 04`): non troncarli.
- **`SelectAID_IAS` sceglie in base alla famiglia**: NXP/Actalis/Bit4Id vogliono
  solo `SELECT MF`; Gemalto/STM l'AID IAS. E poi serve **comunque** un
  `SELECT` dell'AID CIE, altrimenti la VERIFY PIN risponde `6985`.
- **Il contatore PIN è a 3 tentativi.** Il trucco della VERIFY con campo dati
  vuoto per leggerlo senza consumarlo **non funziona su questa carta** (risponde
  `6988`) ed è stato tolto dal flusso. I passi 1-5 dell'handshake sono comunque
  PIN-free: si può iterare senza rischi.

### Come si testa senza carta

I test coprono la crittografia con vettori calcolati **indipendentemente**:

- il Retail MAC è ancorato al **vettore pubblicato ICAO Doc 9303 (BAC)**, che usa
  lo stesso ISO 9797-1 Algorithm 3: se passa, il MAC è conforme allo standard e
  non solo a sé stesso
- c'è un vettore end-to-end di una `VERIFY PIN` completa in SM, calcolato con un
  riferimento Python su pycryptodome
- il DigestInfo SHA-256 è confrontato con la codifica DER standard

Quando qualcosa non torna sulla carta, `-vv` stampa ogni APDU in TX/RX: è così
che sono stati trovati tutti i bug di oggi.
