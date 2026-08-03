# CIE-CLI

CLI e libreria Rust per la **Carta d'Identità Elettronica (CIE)** italiana, che
parlano alla carta **direttamente in PC/SC**.

Niente middleware ufficiale, niente PKCS#11 di terze parti, niente app Java di
registrazione, niente dati scritti su disco.

## Perché

Il middleware ufficiale (`cie-middleware-linux`) per funzionare pretende una
registrazione preventiva della carta tramite un'applicazione Java, che salva in
una cache su disco metà del PIN (cifrata con una chiave derivabile da chiunque
abbia la carta, senza conoscere il PIN). Da lì in poi ti chiede solo le altre 4
cifre.

Qui il PIN lo si digita per intero, quando serve: la cache non risolveva un
problema tecnico, e nel farlo dimezzava lo spazio di ricerca di una credenziale.

## Stato

Funzionante e verificato su carta reale (Bit4Id):

- [x] Rilevamento del tipo di carta dall'ATR e selezione dell'applicazione
- [x] Secure Messaging completo: Diffie-Hellman, DAPP (autenticazione **mutua**,
      con verifica della firma del chip), 3DES-CBC, Retail MAC verificato in
      entrambe le direzioni
- [x] Verifica del PIN nel canale sicuro
- [x] Lettura dei file protetti (seriale, certificato, dati del titolare)
- [x] Firma on-card SHA-256 / PKCS#1 v1.5, verificata in locale
- [x] PDF firmati in **PAdES B-B** con riquadro visibile, senza mai estrarre la chiave
- [x] CAdES B-B enveloping (`.p7m`) su qualunque file, verificato con OpenSSL
- [ ] Marca temporale (livello B-T): richiede una chiamata di rete a una TSA
- [ ] Modulo PKCS#11 (`cdylib`) per Firefox e client SPID/CieID

Il certificato sulla carta è **di autenticazione**, non di firma qualificata
(policy `1.3.76.47.4`, EKU `TLS Web Client Authentication`, Key Usage senza
`nonRepudiation`). Le firme prodotte con questa chiave non sono firme digitali
qualificate ai sensi eIDAS.

## Requisiti

- Linux con lettore di smart card PC/SC e una CIE 3.0
- `pcscd` in esecuzione

```bash
sudo pacman -S pcsclite ccid        # Arch
sudo apt install pcscd pcsc-tools   # Debian/Ubuntu
sudo systemctl enable --now pcscd
```

Il middleware CIE **non** serve.

Per compilare serve anche il fork di `underskrift` in `../underskrift`
(github.com/AttilioGreco/underskrift, branch `fix/pades-conformance`): il lavoro
non e' ancora a monte. Vedi `HANDOFF.md`.

## Uso

```bash
cargo build --release

cie info          # dati leggibili senza PIN
cie pcsc-test     # diagnostica APDU
cie auth          # canale sicuro + verifica PIN + dati protetti

cie sign <file>       # firma detached: <file>.sig + <file>.cer
cie sign-p7m <file>   # CAdES: contenuto e firma insieme in <file>.p7m
cie sign-pdf <f.pdf>  # PAdES: la firma finisce dentro il PDF

cie -vv auth      # con ogni APDU in TX/RX
```

`cie sign-p7m` vale per qualunque file, PDF compresi: produce una busta CAdES
enveloping, cioe' un DER che contiene sia il documento sia la firma. E' il
formato che la PA chiama `.p7m`. Si controlla con
`openssl cms -verify -in <file>.p7m -inform DER -noverify`.

`cie sign-pdf` disegna in basso a destra un riquadro con l'intestatario e la
data, presi dal certificato della carta. `--pagina N` lo sposta, `--motivo`
aggiunge una riga. Il risultato si controlla con `pdfsig file-firmato.pdf`.

`--certifica` produce una firma di certificazione (firma d'autore, quella che i
lettori marcano con il nastrino): `1` vieta ogni modifica successiva, `2` lascia
compilare i moduli e firmare — il default, quindi altri possono controfirmare —
e `3` permette anche le annotazioni. Ammessa solo sulla prima firma di un
documento.

### Il PIN

In ordine di precedenza: argomento `--pin`, portachiavi di sistema, prompt a
video. Il modo consigliato è il portachiavi, così il PIN non finisce né nella
history della shell né in `ps`:

```bash
secret-tool store --label='CIE PIN' service cie
```

Servono tutte e 8 le cifre.

## Architettura

```
crates/
├── cie-lib/                     libreria core
│   └── src/
│       ├── pcsc_direct.rs       trasporto PC/SC, APDU grezze
│       ├── apdu.rs              tipo carta da ATR, selezione AID, lettura file
│       ├── secure_messaging.rs  DH, DAPP, cifratura e MAC del canale
│       ├── crypto.rs            3DES-CBC, Retail MAC, derivazione chiavi
│       ├── asn1.rs              parser BER-TLV (tag fino a 4 byte)
│       ├── titolare.rs          estrazione dati dal certificato X.509
│       └── error.rs
└── cie-cli/                     binario `cie`
```

## Sviluppo

```bash
cargo test      # 22 test, nessuno richiede la carta
cargo clippy
```

I test coprono la parte crittografica con vettori calcolati indipendentemente.
In particolare il Retail MAC è ancorato al **vettore pubblicato ICAO Doc 9303**
(BAC), che usa lo stesso ISO 9797-1 Algorithm 3 del Secure Messaging CIE: se
quel test passa, il MAC è conforme allo standard e non solo a sé stesso.

C'è anche un vettore end-to-end di una `VERIFY PIN` completa in Secure
Messaging, calcolato con un riferimento esterno: protegge dall'errore più
insidioso del protocollo, cioè paddare i TLV singolarmente invece di applicare
un solo ISO-padding finale.

## Riferimenti

- [cie-middleware-linux](https://github.com/italia/cie-middleware-linux) —
  middleware ufficiale, BSD-3-Clause. Le sequenze APDU, i parametri di
  protocollo e le costanti della coppia di chiavi DAPP derivano dal suo
  sorgente, in particolare `cie-pkcs11/CSP/IAS.cpp`.
- [ICAO Doc 9303](https://www.icao.int/publications/pages/publication.aspx?docnum=9303) —
  Machine Readable Travel Documents, per il Secure Messaging
- [Specifiche CIE 3.0](https://www.cartaidentita.interno.gov.it/)

## Licenza

MIT.

Nota: il middleware da cui derivano le sequenze di protocollo è BSD-3-Clause,
che richiede il mantenimento della nota di copyright per le porzioni derivate.
