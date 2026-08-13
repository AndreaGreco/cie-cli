# CIE-CLI

**Firma CIE**, la finestra per firmare i PDF con la **Carta d'Identità
Elettronica** italiana, e `cie`, la stessa cosa da riga di comando. Parlano alla
carta **direttamente in PC/SC**.

Niente middleware ufficiale, niente PKCS#11 di terze parti, niente app Java di
registrazione, nessun file di cache scritto dallo strumento.

## Installazione

Tutti i pacchetti stanno nell'ultima
[release](https://github.com/AttilioGreco/cie-cli/releases/latest). **Un solo
download**: dentro ci sono sia la finestra sia il comando `cie`.

| Sistema | File | Cosa fa |
|---|---|---|
| Windows | `….msi` | chiede l'amministratore, mette l'icona sul desktop |
| Windows senza amministratore | `…-setup.exe` | installa per il tuo utente, il collegamento sul desktop lo chiede |
| macOS | `….dmg` | trascini «Firma CIE» in Applicazioni |
| Debian, Ubuntu | `….deb` | `sudo apt install ./file.deb`, voce nel menu |
| altre Linux | `….AppImage` | `chmod +x`, si lancia com'è |

Nella stessa release ci sono anche `.pkg`, gli archivi e gli installer
`curl`/`irm`: quelli sono **solo `cie`**, senza finestra e senza icona.

## Perché

Il middleware ufficiale (`cie-middleware-linux`) per funzionare pretende una
registrazione preventiva della carta tramite un'applicazione Java, che salva in
una cache su disco metà del PIN (cifrata con una chiave derivabile da chiunque
abbia la carta, senza conoscere il PIN). Da lì in poi ti chiede solo le altre 4
cifre.

Qui il PIN si digita per intero. L'obiezione non è che quella cache stia su
disco: è che tiene **metà** credenziale sotto una protezione che non protegge, e
così dimezza lo spazio di ricerca di un PIN che ha un contatore di tentativi.

Se lo vuoi comodo, il PIN va **tutto** nel gestore di password del sistema —
secret-service, Credential Manager, Keychain — che è il posto costruito apposta
per custodire un segreto a riposo. `cie` non tiene un formato suo. Ed è
facoltativo: la domanda ha il no come default, e chi risponde Invio non lascia
niente sulla macchina.

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

Un lettore di smart card PC/SC e una CIE 3.0. Su Windows e macOS lo stack PC/SC
è già nel sistema; su Linux va acceso `pcscd`:

```bash
sudo pacman -S pcsclite ccid        # Arch
sudo apt install pcscd pcsc-tools   # Debian/Ubuntu
sudo systemctl enable --now pcscd
```

Il middleware CIE **non** serve.

PAdES e CAdES arrivano da `underskrift`, e per ora serve un fork: il lavoro non
e' ancora a monte. Non devi fare niente — il `Cargo.toml` del workspace punta al
fork su git e Cargo lo risolve da solo. Per lavorarci sopra in locale, copia
`.cargo/config.toml.example` in `.cargo/config.toml`.

## Firma CIE

Una schermata sola: il documento al centro, le scelte in colonna a destra, in
fondo dove finirà il file e il pulsante per firmare. Il riquadro della firma si
trascina per spostarlo e si prende per gli angoli per ridimensionarlo, al posto
della posizione cablata di `cie sign-pdf`; dentro il riquadro c'è un facsimile
di come verrà, alla stessa scala che avrà nel PDF. Il nome del file firmato è
quello proposto dalla CLI, e si può cambiare con un salva-con-nome.

Comodità:

| | |
|---|---|
| frecce, PagSu / PagGiù | cambia pagina |
| rotella | cambia pagina a 100%, scorre quando sei zoomato |
| Ctrl `+` `−` `0` | zoom avanti, indietro, torna a 100% |
| trascinamento di un file | lo apre |

Il tema segue il desktop; l'interruttore in alto a destra forza chiaro o scuro.
La lingua arriva da `LANG`, con l'italiano come ripiego.

Il PIN si legge, si salva e si cancella dal portachiavi direttamente dalla
finestra. La voce è la stessa che usa `cie`, perché i due strumenti condividono
il codice invece di accordarsi su una convenzione.

Su un window manager a piastrelle la finestra la piazza il compositore, non il
programma: l'unica cosa che `cie-gui` può fare è dire chi è. Su Sway:

```
for_window [app_id="cie-gui"] floating enable, resize set 1280 880
```

## Da riga di comando

`cie` è un binario a parte da `cie-gui`: chi lavora in terminale non si porta
dietro un toolkit grafico. I pacchetti qui sopra li installano entrambi.

```bash
cie info          # dati leggibili senza PIN
cie pcsc-test     # diagnostica APDU
cie auth          # canale sicuro + verifica PIN + dati protetti

cie sign <file>       # firma detached: <file>.sig + <file>.cer
cie sign-p7m <file>   # CAdES: contenuto e firma insieme in <file>.p7m
cie sign-pdf <f.pdf>  # PAdES: la firma finisce dentro il PDF

cie gpg certify <chiave.asc>  # certifica una chiave OpenPGP con la CIE
cie gpg verify <chiave.asc>   # mostra e verifica le certification presenti

cie completions --install  # completamento per bash, zsh o fish

cie -vv auth      # con ogni APDU in TX/RX

cie-gui           # le stesse firme, con il documento davanti
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

### La certificazione OpenPGP

`cie gpg certify` firma con la chiave della carta una certification OpenPGP
standard sulla tua chiave: il modo di dire «questa chiave e' del titolare di
quella CIE» con un artefatto che GnuPG capisce da solo. La chiave privata resta
nella carta, come per ogni altra firma.

Escono tre file:

| File | Contenuto | Dove va |
|---|---|---|
| `<chiave>-certificata.asc` | la tua chiave, piu' la certification | pubblicabile |
| `cie-certifier.asc` | la chiave della CIE come certificato OpenPGP | pubblicabile |
| `cie-certifier.cer` | il certificato X.509 della carta | **resta a te** |

Il `.cer` contiene il codice fiscale in chiaro — ce lo mette il Ministero, non
noi — e serve solo a chi deve risalire dalla chiave alla PKI. Lo consegni tu, a
chi decidi. Nel materiale pubblico il codice fiscale non compare: la User ID del
certificatore porta nome, cognome e numero della carta, e la firma porta solo lo
SHA-256 del `.cer`, che e' un impegno vincolante su quale certificato sia.

```bash
gpg --import cie-certifier.asc chiave-certificata.asc
gpg --check-sigs <fingerprint>
```

La certification e' di tipo 0x10 (generic), non 0x13 (positive): 0x13
dichiarerebbe che l'emittente ha verificato accuratamente la User ID, email
compresa, e la CIE dell'email non sa niente. Cosa sia stato verificato davvero
lo dice la notation `cie-verified@cie-cli`, che contiene `holder-name` e basta.

La chiave certificata resta una normale chiave OpenPGP: continua a firmare i
commit come prima. La certification viaggia solo se la esporti — `gpg
--export-options export-minimal` la toglie — quindi puoi tenerla per chi ti
chiede di dimostrare l'identita' senza pubblicarla.

### Completamento automatico

```bash
cie completions --install     # indovina la shell da $SHELL
cie completions zsh --install # oppure la scegli tu
cie completions fish          # solo stampa, se preferisci piazzarlo a mano
```

Lo script finisce dove bash e fish lo trovano da soli
(`~/.local/share/bash-completion/completions/`, `~/.config/fish/completions/`);
per zsh il comando stampa la riga di `fpath` da aggiungere a `~/.zshrc`. Vale
la pena rilanciarlo dopo un aggiornamento: le opzioni nuove arrivano da lì.

## Il PIN

In ordine di precedenza: argomento `--pin`, portachiavi di sistema, prompt a
video. Il modo consigliato è il portachiavi, così il PIN non finisce né nella
history della shell né in `ps`.

Il portachiavi tiene **una voce per carta**, così più di una CIE può convivere
nello stesso profilo. La chiave è l'identificativo del chip (file `0x1001`,
dodici cifre — lo stesso numero stampato sulla carta), che si legge senza PIN:
basta appoggiare la tessera perché lo strumento sappia di quale PIN ha bisogno.

Per popolarlo basta firmare o autenticarsi una volta digitando il PIN: a
verifica riuscita `cie` chiede se ricordarlo. Insieme al PIN salva nome,
cognome e codice fiscale letti dal certificato — è cache, serve a riconoscere
la carta appena appoggiata, quando il certificato è ancora chiuso dietro il PIN.

Nel portachiavi il PIN finisce **per intero**, e a custodirlo è il deposito
segreti del sistema: cifrato a riposo, sbloccato con le credenziali della
sessione. `cie` non scrive nessun file di cache suo. È l'opposto della cache del
middleware, che tiene quattro cifre in un file proprio, cifrate con una chiave
derivabile da chiunque abbia la carta.

Resta il limite del modello: a portachiavi sbloccato, qualunque processo che
giri come te può chiederne il contenuto. Un portachiavi di sistema non difende
da codice che ha già i tuoi privilegi.

Salvare è facoltativo e la domanda ha come default il no: rispondendo Invio non
finisce niente sul disco, né il PIN né i dati del titolare, e il PIN si digita
ogni volta.

Nome e codice fiscale stanno **dentro** il segreto cifrato, mai nell'indice: su
secret-service gli attributi di ricerca sono metadati in chiaro, leggibili da
chiunque stia sul bus di sessione.

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
├── cie-firma/                   firmatario on-card e contenitori
│   └── src/
│       ├── signer.rs            CieSigner: la carta come CryptoSigner
│       ├── pades.rs             PAdES B-B, riquadro visibile
│       ├── cades.rs             CAdES enveloping (.p7m)
│       └── detached.rs          firma RSA grezza, con verifica locale
├── cie-pgp/                     certificazione OpenPGP
│   └── src/
│       ├── signer.rs            la carta come firmatario Sequoia
│       └── lib.rs               certificatore CIE e certification
├── cie-cli/                     binario `cie`
└── cie-gui/                     binario `cie-gui`
    └── src/
        ├── app.rs               la schermata unica
        ├── preview.rs           hayro + riquadro → punti PDF
        ├── job.rs               la carta su un thread suo
        └── keyring.rs           il PIN via D-Bus
```

`cie-firma` e `cie-gui` sono scritti in inglese; in italiano restano solo le
stringhe che l'utente legge. `cie-lib` e `cie-cli` sono ancora in italiano.

`cie-firma` sta in mezzo apposta: `underskrift` si porta dietro lopdf e un
parser di font, roba che non ha niente a che fare con una APDU. `cie-lib` resta
il livello carta puro, come serve al futuro modulo PKCS#11.

`cie-pgp` e' un crate a parte per la stessa ragione: Sequoia dentro `cie-firma`
finirebbe nella build della GUI senza servire a niente. Vede la carta solo
attraverso `CryptoSigner`, quindi nessun packet OpenPGP arriva vicino a una
APDU. L'adapter e' sottile perche' i formati coincidono gia': la carta riceve un
`DigestInfo(SHA-256)` e restituisce PKCS#1 v1.5, che e' esattamente com'e' fatta
una firma RSA OpenPGP.

## Sviluppo

```bash
cargo test --workspace   # 67 test, nessuno richiede la carta
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

## Documentazione

Completa, su <https://attiliogreco.github.io/cie-cli/>: guida, riferimento dei
comandi, specifica della certification OpenPGP e cosa diventa pubblico. Il
sorgente sta in `docs-site/`.

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
