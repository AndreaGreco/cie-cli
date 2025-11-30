# CIE-CLI

CLI e libreria Rust per la **Carta d'Identità Elettronica (CIE)** italiana.

## Architettura

Il progetto è strutturato come workspace Rust con due crate:

```
cie-cli/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── cie-lib/            # Libreria core
│   │   └── src/
│   │       ├── lib.rs      # Entry point libreria
│   │       ├── cie.rs      # Interfaccia principale CIE
│   │       ├── pkcs11.rs   # Wrapper PKCS#11
│   │       ├── config.rs   # Configurazione
│   │       └── error.rs    # Gestione errori
│   └── cie-cli/            # CLI
│       └── src/
│           ├── main.rs     # Entry point CLI
│           └── commands/   # Comandi
```

### cie-lib

Libreria core che può essere usata da:
- **CLI** (questo progetto)
- **GUI** (futura implementazione)
- **Server API** (futura implementazione)

### cie-cli

Interfaccia a riga di comando per interagire con la CIE.

## Requisiti

### Sistema

- Linux (testato su Ubuntu/Debian)
- Lettore di smart card compatibile PC/SC
- CIE 3.0 (Carta d'Identità Elettronica)

### Software

1. **pcscd** - Daemon PC/SC per smart card
   ```bash
   sudo apt install pcscd pcsc-tools
   sudo systemctl enable pcscd
   sudo systemctl start pcscd
   ```

2. **CIE Middleware** - Libreria PKCS#11 per la CIE
   ```bash
   # Scarica e installa da:
   # https://github.com/italia/cie-middleware-linux
   ```

## Installazione

```bash
# Clona il repository
git clone https://github.com/agreco/cie-cli.git
cd cie-cli

# Compila
cargo build --release

# Installa (opzionale)
cargo install --path crates/cie-cli
```

## Utilizzo

### Comandi disponibili

```bash
# Lista i lettori di smart card
cie readers

# Mostra info sulla CIE inserita
cie info

# Lista i certificati
cie cert list

# Esporta un certificato
cie cert export -i "DS" -o certificato.der

# Firma un documento
cie sign -i documento.pdf -o documento.pdf.sig

# Verifica una firma (non ancora implementato)
cie verify -i documento.pdf -s documento.pdf.sig
```

### Opzioni globali

```bash
# Verbosità
cie -v info      # INFO
cie -vv info     # DEBUG  
cie -vvv info    # TRACE

# Percorso libreria PKCS#11 custom
cie --pkcs11-lib /path/to/libcie-pkcs11.so readers

# Oppure via variabile d'ambiente
export CIE_PKCS11_LIB=/path/to/libcie-pkcs11.so
cie readers
```

## Utilizzo come libreria

```rust
use cie_lib::{Cie, CieConfig, SigningMechanism};
use anyhow::Result;

fn main() -> Result<()> {
    // Inizializza con auto-detect
    let cie = Cie::new()?;
    
    // Oppure con percorso specifico
    let cie = Cie::with_config(
        CieConfig::with_pkcs11_path("/usr/lib/libcie-pkcs11.so")
    )?;
    
    // Lista lettori
    for reader in cie.list_readers()? {
        println!("{}", reader);
    }
    
    // Trova CIE e firma
    let reader = cie.find_cie()?;
    let session = cie.login(&reader, "12345678")?;
    
    let data = b"Dati da firmare";
    let signature = cie.sign(&session, data, SigningMechanism::Sha256RsaPkcs)?;
    
    session.logout()?;
    Ok(())
}
```

## Come funziona

### PKCS#11

La libreria utilizza lo standard **PKCS#11** per comunicare con la CIE. Il middleware CIE (`libcie-pkcs11.so`) espone un'interfaccia PKCS#11 standard che permette di:

- Enumerare i lettori (slot)
- Rilevare la presenza della carta (token)
- Autenticarsi con il PIN
- Leggere certificati
- Firmare dati con la chiave privata

### Binding Rust

Non è necessario un submodule o binding C/C++ diretto. Usiamo il crate [`cryptoki`](https://crates.io/crates/cryptoki) che:

1. Carica dinamicamente la libreria `.so` a runtime
2. Espone un'interfaccia Rust sicura e idiomatica
3. Gestisce automaticamente la memoria e gli errori

## Sviluppo

```bash
# Test
cargo test

# Clippy
cargo clippy

# Formato
cargo fmt

# Build release
cargo build --release
```

## Roadmap

- [x] Struttura workspace
- [x] Wrapper PKCS#11
- [x] CLI base
- [x] Lista lettori
- [x] Info CIE
- [x] Firma documenti
- [ ] Verifica firme
- [ ] Firma CAdES/PAdES
- [ ] Autenticazione
- [ ] GUI (Tauri/egui)

## Licenza

MIT

## Riferimenti

- [cie-middleware-linux](https://github.com/italia/cie-middleware-linux) - Middleware ufficiale CIE
- [cryptoki](https://crates.io/crates/cryptoki) - Binding PKCS#11 per Rust
- [Specifiche CIE 3.0](https://www.cartaidentita.interno.gov.it/)
