# Certificazione OpenPGP con la chiave di firma della CIE

Data: 2026-08-09

## Obiettivo

Permettere al titolare di una CIE di produrre una **certification OpenPGP standard**
sulla propria chiave OpenPGP, firmata dalla chiave di firma custodita nella carta.

La proprietà che si vuole ottenere:

> questa User ID e questa chiave pubblica OpenPGP sono state certificate dal
> titolare di quella CIE.

La chiave privata non lascia mai la carta. Il risultato è una normale signature
OpenPGP, importabile e verificabile con GnuPG e Sequoia.

## Non obiettivi

- Nessun formato proprietario, nessun packet OpenPGP costruito a mano.
- Nessuna asserzione della CIE sull'indirizzo email della User ID.
- Nessuna prova a conoscenza zero sulla catena X.509.
- Nessun middleware CIE: si riusa l'implementazione nativa già presente.

## Contesto: cosa esiste già

`cie-firma` espone `CieSigner`, che implementa il trait `CryptoSigner` di
`underskrift`:

```rust
fn sign_hash(&self, hash: &[u8]) -> Result<Vec<u8>, CryptoError>;  // SHA-256 → 256 byte
fn certificate_der(&self) -> &[u8];
```

Sotto, `secure_messaging.rs:884` (`sign_sha256`) manda alla carta il
`DigestInfo(SHA-256)` e riceve indietro una firma RSA PKCS#1 v1.5.

**Questo è già esattamente il formato di una firma RSA OpenPGP.** L'adapter non
deve convertire niente: prende i byte e li reimpacchetta in `mpi::Signature::RSA`.

## Architettura

Nuovo crate fra `cie-firma` e `cie-cli`:

```
cie-lib      APDU, Secure Messaging, PIN, parsing X.509    invariato
cie-firma    CieSigner: digest SHA-256 → PKCS#1 v1.5       + feature `test-signer`
cie-pgp      adapter Sequoia ⟷ CryptoSigner                nuovo
cie-cli      cie gpg certify | verify                      nuovo comando
```

Crate separato e non modulo di `cie-firma` per una ragione concreta: `cie-gui`
dipende da `cie-firma`, e Sequoia lì dentro finirebbe nella build della GUI
senza servire a niente.

`cie-pgp` vede solo il trait `CryptoSigner`. Nessuna logica OpenPGP tocca APDU o
Secure Messaging, e nessuna logica di carta entra nel livello OpenPGP.

### `cie-pgp/src/signer.rs` — l'adapter

```rust
pub struct CieSequoiaSigner<'a> {
    inner: &'a dyn CryptoSigner,
    key: Key<PublicParts, UnspecifiedRole>,
}

impl sequoia_openpgp::crypto::Signer for CieSequoiaSigner<'_> {
    fn public(&self) -> &Key<PublicParts, UnspecifiedRole> { &self.key }

    fn acceptable_hashes(&self) -> &[HashAlgorithm] { &[HashAlgorithm::SHA256] }

    fn sign(&mut self, algo: HashAlgorithm, digest: &[u8]) -> Result<mpi::Signature> {
        // algo != SHA256 → errore; la carta non sa fare altro
        // digest → inner.sign_hash → mpi::Signature::RSA { s: MPI::new(&sig) }
    }
}
```

`acceptable_hashes` che restituisce solo SHA-256 fa sì che sia Sequoia a scegliere
l'hash giusto quando costruisce la firma, invece che noi a fallire dopo averla
costruita male.

`key` si costruisce dal SubjectPublicKeyInfo del certificato X.509 con
`Key4::import_public_rsa(e, n, Some(creation_time))`.

### `cie-pgp/src/lib.rs` — la semantica OpenPGP

```rust
pub fn certifier_cert(signer: &dyn CryptoSigner, card_id: &str) -> Result<Cert>;
pub fn certify(target: &Cert, uid: &UserID, signer: &dyn CryptoSigner) -> Result<Cert>;
pub fn holder_matches(cie: &DatiTitolare, uid: &UserID) -> bool;
```

## Il certificato del certificatore CIE

GnuPG non verifica una certification se non ha la chiave di chi l'ha emessa.
Serve quindi rappresentare la chiave RSA della CIE come certificato OpenPGP
autoportante.

```
pub  rsa2048/<keyid>  <notBefore del certificato X.509>
uid                   ROSSI MARIO (CIE 1234567890)
     key flags        certification only
     preferred hash   SHA256
```

- **Primary key**: modulo ed esponente presi dal SubjectPublicKeyInfo del
  certificato X.509 letto dal file `0x1003`.
- **Creation time**: `notBefore` del certificato X.509, **non** l'ora corrente.
  La fingerprint OpenPGP dipende dal creation time: usare l'ora corrente
  produrrebbe una chiave diversa a ogni esecuzione. Con `notBefore` la
  fingerprint è deterministica e riproducibile.
- **User ID**: `<COGNOME> <NOME> (CIE <id_servizi>)`, da `DatiTitolare` e da
  `read_id_servizi`. Nessuna email: la CIE non ne attesta nessuna, e una UID
  senza email non fa asserzioni implicite. Nessun codice fiscale.
- **Self-signature**: prodotta dalla carta. Un solo PIN copre sia questa sia la
  certification: `CieSigner` tiene aperto il canale sicuro e `sign_sha256` è
  richiamabile più volte.
- **Key flags**: solo certification, così nessuno tenta di cifrare verso questa
  chiave.

## La certification

Tipo **0x10 Generic certification**, non 0x13 Positive.

0x13 significa «l'emittente ha verificato accuratamente la User ID», e la User ID
comprende l'email. La CIE verifica il nome del titolare, non l'email. 0x10 non fa
asserzioni sul grado di verifica, quindi non promette ciò che non si può
mantenere. Cosa sia stato effettivamente verificato lo dicono le notation.

Una certification OpenPGP lega la User ID intera — nome ed email insieme — e non
è divisibile: le notation sono l'unico meccanismo standard per dichiarare quale
parte sia stata verificata.

Notation, tutte nell'area hashed:

| Notation | Valore | Significato |
|---|---|---|
| `cie-verified@cie-cli` | `holder-name` | l'unico dato verificato dalla CIE. Ciò che non è elencato non è attestato: in particolare **non** l'email |
| `cie-x509-sha256@cie-cli` | hex | SHA-256 del certificato X.509 della carta |
| `cie-profile@cie-cli` | `v1` | versione del profilo di certificazione |

Il DER del certificato X.509 **non** viene incorporato. Contiene il codice fiscale
in chiaro, e la chiave OpenPGP è materiale pubblico e irrevocabile una volta
diffuso. Viene scritto come file separato, che il titolare consegna a chi decide.

La notation `cie-x509-sha256` è un impegno vincolante firmato dalla carta: chi
riceve il `.cer` verifica che sia esattamente quello legato alla certification, e
nessuno può sostituirlo dopo.

### Cosa si perde non incorporando il DER

Niente sul piano crittografico: la certification è valida e verificabile
identica. Si perde l'autoportanza della catena verso la PKI. Chi ha solo il
materiale OpenPGP verifica che la chiave X abbia certificato quella UID, ma la
stringa `(CIE 1234567890)` nella UID di X resta un'auto-asserzione: senza il
`.cer` non può stabilire che X sia davvero una CIE.

La decisione è reversibile in una sola direzione — il `.cer` si può sempre
consegnare dopo senza rifirmare niente, mentre un DER finito su un keyserver non
si toglie più. Per questo il default è non incorporarlo.

## Coerenza anagrafica

Confronto normalizzato (maiuscole, accenti, spazi, ordine nome/cognome) fra i dati
letti dalla CIE e la parte nome della User ID da certificare.

Se non combaciano: stampa entrambe le identità e chiede conferma esplicita.
`--yes` la salta. Non è un blocco — certificare uno pseudonimo è legittimo — ma
deve essere una scelta deliberata.

## CLI

```
cie gpg certify <chiave.asc> [--uid N] [--pin ...] [-o out.asc] [--yes]
cie gpg verify  <chiave.asc> [--certifier cie-certifier.asc] [--ca ministero.pem]
```

`certify` produce:

| File | Contenuto | Destinazione |
|---|---|---|
| `<out>.asc` | la chiave originale più la certification CIE | pubblicabile |
| `cie-certifier.asc` | chiave OpenPGP della CIE, auto-firmata dalla carta | pubblicabile |
| `cie-certifier.cer` | certificato X.509, **contiene il codice fiscale** | resta al titolare |

Più di una User ID e nessun `--uid` → scelta interattiva. Il PIN passa per
`commands::open_card`, come ogni altro comando.

`verify` valida la certification con Sequoia e, se viene fornita `--ca`, valida
anche la catena X.509 fino al Ministero. La carta espone solo il proprio leaf —
le CA non ci sono, vedi `signer.rs:66` — quindi senza `--ca` la catena non è
verificabile e il comando lo dice esplicitamente invece di tacere.

## Privacy: cosa diventa pubblico

Scenario di riferimento: firma dei commit Git con la propria chiave OpenPGP
caricata su GitHub.

- Il commit si firma con la **chiave dell'utente**, non con la CIE. Chi verifica
  il commit non ha bisogno della certification.
- La certification viaggia solo se la si esporta: `gpg --export-options
  export-minimal` la rimuove. Si può tenere la chiave certificata in locale e
  pubblicare quella nuda.
- Se la si esporta, diventano pubblici nome, cognome e numero della carta, dalla
  UID del certificatore. Il numero della carta è l'ID_Servizi, stampato sulla
  carta e leggibile senza PIN.
- Il **codice fiscale non è mai nel materiale pubblico**. Sta solo nel `.cer`.

## Test

`cie-firma` ha già `fake::FakeSigner`, che imita il contratto della carta
(riceve un hash, costruisce il DigestInfo, applica PKCS#1 v1.5). Va esposto
dietro la feature `test-signer` invece di duplicarlo, spostando il `#[cfg(test)]`
in `#[cfg(any(test, feature = "test-signer"))]`.

Con il mock:

1. La self-signature del certificatore verifica, e il `Cert` fa round-trip
   attraverso il parser di Sequoia.
2. La certification verifica con `sig.verify_userid_binding(...)`.
3. La fingerprint del certificatore è identica su due esecuzioni.
4. User ID e subkey originali della chiave certificata restano intatte: i packet
   della chiave in ingresso ricompaiono identici in uscita, con in più la sola
   signature.
5. Le notation attese sono presenti nell'area hashed.
6. Un digest che non sia SHA-256 viene rifiutato dall'adapter.

Interoperabilità reale, in un test che si salta da solo se `gpg` non è
installato:

7. `gpg --import` di entrambi i file, poi `gpg --check-sigs` mostra la
   certification come valida (`sig!`).

Con la carta vera, `#[ignore]`:

8. Certificazione completa end-to-end e verifica del risultato con Sequoia.

## Rischi

**Backend crittografico di Sequoia — verificato, rischio chiuso.** Il default è
nettle, dipendenza C che complica la build multipiattaforma di `cargo-dist`
(Windows in particolare). Si usa invece `crypto-rust`, che richiede anche
`allow-experimental-crypto` e `allow-variable-time-crypto`. È adeguato al caso
d'uso: qui non si fa crypto segreta, solo hash, verifica RSA e serializzazione di
packet — la chiave privata è nella carta.

Provato prima di scrivere lo spec: `sequoia-openpgp` 2.4.1 con
`--no-default-features --features crypto-rust,allow-experimental-crypto,allow-variable-time-crypto`
compila in 29 s senza alcuna dipendenza di sistema.

**Formato del CN sulla CIE.** `DatiTitolare` gestisce sia `COGNOME/NOME` sia
givenName/surname separati. Se un certificato reale usa un formato diverso, il
confronto anagrafico degrada a richiesta di conferma, non a un errore.
