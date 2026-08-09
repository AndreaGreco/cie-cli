---
title: Installazione
description: Requisiti, lettore PC/SC, pcscd e compilazione di cie-cli.
---

## Cosa serve

- Un lettore di smart card PC/SC e una **CIE 3.0**
- `pcscd` in esecuzione
- Rust stabile, per compilare

Il middleware CIE **non** serve, e non va installato: questo progetto parla alla
carta direttamente in PC/SC.

## Il demone PC/SC

```bash
sudo pacman -S pcsclite ccid          # Arch
sudo apt install pcscd pcsc-tools    # Debian/Ubuntu
sudo systemctl enable --now pcscd
```

Verifica che il lettore ci sia e che la carta venga vista:

```bash
pcsc_scan
```

Se `pcsc_scan` non mostra nessun lettore, il problema e' a monte di `cie-cli` e
va risolto li'. Il driver piu' comune per i lettori USB e' `ccid`.

:::caution[Il middleware ufficiale confligge]
Se sul sistema c'e' il middleware CIE, puo' tenere occupata la carta o
riconfigurare `pcscd`. Su una macchina dove entrambi sono presenti, conviene
fermare i servizi del middleware prima di usare `cie-cli`.
:::

## Compilare

```bash
git clone https://github.com/AttilioGreco/cie-cli
cd cie-cli
cargo build --release
```

Il binario esce in `target/release/cie`. Il secondo binario, `cie-gui`, si
compila con `cargo build --release -p cie-gui`.

### La dipendenza `underskrift`

PAdES e CAdES arrivano da [`underskrift`](https://github.com/AttilioGreco/underskrift),
e per ora serve un fork: il lavoro non e' ancora a monte. La cosa e' gia'
sistemata nel `Cargo.toml` del workspace, che punta al fork su git — non devi
fare niente, Cargo lo risolve da solo al primo build.

```toml
[patch.crates-io]
underskrift = { git = "https://github.com/AttilioGreco/underskrift", branch = "main" }
```

Il fork sta su git e non su un path locale apposta: la CI di release deve poterlo
risolvere. Se ci lavori sopra in locale, copia `.cargo/config.toml.example` in
`.cargo/config.toml` per puntare a `../underskrift`.

:::danger[Non committare `Cargo.lock` con il patch locale attivo]
Finche' `.cargo/config.toml` e' in uso, `Cargo.lock` punta al path locale. La CI
di release non lo sa risolvere e la build fallisce.
:::

## Il primo comando

Con la carta nel lettore:

```bash
cie info
```

Legge quello che la CIE espone senza PIN. Se risponde, il percorso fino alla
carta funziona ed e' tutto pronto.

## Completamento automatico

```bash
cie completions --install      # indovina la shell da $SHELL
cie completions zsh --install  # oppure la scegli tu
cie completions fish           # solo stampa, se preferisci piazzarlo a mano
```

Lo script finisce dove bash e fish lo trovano da soli
(`~/.local/share/bash-completion/completions/`, `~/.config/fish/completions/`);
per zsh il comando stampa la riga di `fpath` da aggiungere a `~/.zshrc`.

Vale la pena rilanciarlo dopo un aggiornamento: le opzioni nuove arrivano da li'.
