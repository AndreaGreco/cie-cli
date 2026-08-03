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

23 test, nessuno richiede la carta. Zero warning clippy.

```bash
cie info                 # dati leggibili senza PIN
cie auth                 # canale + PIN + dati protetti
cie sign <file>          # firma detached (.sig) + certificato (.cer)
cie -vv auth             # ogni APDU in TX/RX
```

Il PIN si legge dal portachiavi: `secret-tool store --label='CIE PIN' service cie`.
Servono tutte e 8 le cifre.

---

## Il prossimo passo

L'obiettivo dichiarato è **riprodurre ciò che fa il middleware ufficiale**, con
i suoi limiti. Quello che l'app del Ministero produce e noi ancora no:

| Cosa | Formato | Nel loro SDK |
|---|---|---|
| PDF firmato + riquadro grafico visibile | **PAdES** | `PdfSignatureGenerator` |
| `documento.pdf.p7m` | **CAdES** | `SignedDataGeneratorEx` |
| marca temporale | livello T / `.m7m` | `TSAClient`, `M7MParser` |

Oggi produciamo solo la firma RSA grezza: il mattone più interno di tutti e tre.
Manca lo strato contenitore.

### La domanda che decide la fattibilità

**Le librerie Rust accettano un firmatario esterno?**

La chiave privata è sul chip e non è estraibile: la firma la deve fare la carta.
Serve quindi una libreria che supporti la firma in due fasi — calcola il digest
da firmare, riceve la firma da fuori, la incorpora. È lo stesso schema usato per
HSM e firma remota, quindi una libreria seria dovrebbe prevederlo, ma **va
verificato prima di sceglierla**. È il primo lavoro della prossima sessione.

Candidati (tutti giovani: `esistono` ≠ `collaudati`):

- **`underskrift 0.1.4`** (BSD-2) — PAdES da B-B a B-LTA, LTV, TSP, firme
  visibili e invisibili, verifica. È il più vicino a ciò che serve.
  https://github.com/kushaldas/underskrift
- **`ades-rs 0.2.0`** (MIT/Apache) — CAdES, PAdES, XAdES, JAdES per eIDAS 2.0.
  https://github.com/lfern/eidas-rs
- **`cms 0.3.0-pre.2`** (RustCrypto) — CMS/PKCS#7 puro. Stessa famiglia di `der`
  e `x509-cert` che **già usiamo**: se serve costruire CAdES a mano, si parte da qui.
- `lopdf 0.44` — manipolazione PDF, se servisse scendere di livello.

Cosa cercare nelle loro API: un tratto tipo `Signer`, un callback di firma, o un
flusso `prepare() -> digest` / `embed(signature)`. Se una libreria vuole per
forza una `RsaPrivateKey`, non ci serve.

### Il pezzo che abbiamo già

`SecureMessaging::sign_sha256(&card, &hash) -> Vec<u8>` restituisce 256 byte di
firma PKCS#1 v1.5. È esattamente ciò che un firmatario esterno deve fornire.

Se una libreria chiede un digest diverso da SHA-256 o un padding PSS, va estesa
`sign_sha256`: la carta supporta anche SHA-1 (`CKM_SHA1_RSA_PKCS` nel C++), ma
non conviene usarlo.

---

## L'altro fronte aperto: il modulo PKCS#11

Resta la strada più corta per far firmare a LibreOffice e autenticare Firefox
senza scrivere una riga di formato.

Nuovo crate `crates/cie-pkcs11/` con `crate-type = ["cdylib"]` → `libcie_pkcs11.so`.

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
