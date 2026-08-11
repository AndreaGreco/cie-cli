---
title: Installare
description: Scaricare e installare Firma CIE su Windows, macOS e Linux, e collegare il lettore di smart card.
---

Tutto quello che serve sta sulla [pagina dei
download](https://github.com/AttilioGreco/cie-cli/releases/latest). Scegli il
file per il tuo sistema, dalla lista qui sotto.

Il programma con le finestre si chiama **Firma CIE**. Se preferisci la riga di
comando c'è anche `cie`, piu' in fondo.

## Windows

1. Dalla [pagina dei
   download](https://github.com/AttilioGreco/cie-cli/releases/latest) scarica
   il file che finisce per **`-setup.exe`**.
2. Aprilo e vai avanti. Non chiede la password di amministratore: si installa
   solo per il tuo utente.
3. Trovi **Firma CIE** nel menu Start.

:::caution[Windows dirà che il programma non è sicuro]
Compare una finestra blu, **"Windows ha protetto il PC"**. Succede a tutti i
programmi non firmati con un certificato commerciale, e questo non lo è.
Visto che un certificato di quel tipo costa circa 300 euro l'anno, e io sono
uno sviluppatore indipendente, se non ti fidi del programma è legittimo, non installarlo.
Se pensi di essere in grado, analizza il codice: è tutto disponibile.

Se hai scaricato il file dalla pagina dei download qui sopra, clicca
**"Ulteriori informazioni"** e poi **"Esegui comunque"**.
:::

Il lettore di smart card non ha bisogno di niente: Windows ha il supporto già
dentro e il driver del lettore se lo scarica da solo la prima volta che lo
colleghi. Aspetta che finisca prima di aprire Firma CIE.

## macOS

1. Scarica il file **`.dmg`** - quello con `aarch64` se il Mac ha un chip
   Apple (M1, M2, M3...), quello con `x86_64` se è un Mac Intel.
2. Aprilo e trascina **Firma CIE** nella cartella Applicazioni.

:::caution[La prima apertura va fatta col tasto destro]
macOS non apre le applicazioni scaricate da fuori dall'App Store: dice che
proviene da **"uno sviluppatore non identificato"**.

Fai clic destro sull'icona di Firma CIE, scegli **Apri**, e conferma **Apri**
anche nella finestra che compare. Solo la prima volta.
:::

## Linux

Su Debian, Ubuntu e derivate scarica il pacchetto **`.deb`** e aprilo con il
gestore di pacchetti del desktop, oppure da terminale nella cartella dove l'hai
scaricato:

```bash
sudo apt install ./*.deb
```

Altrove c'è l'**`.AppImage`**, che non si installa: si rende eseguibile e si
apre con un doppio clic.

```bash
chmod +x *.AppImage
```

Su Linux il servizio che parla con il lettore va acceso a mano, una volta sola:

```bash
sudo apt install pcscd pcsc-tools    # Debian/Ubuntu
sudo pacman -S pcsclite ccid         # Arch
sudo systemctl enable --now pcscd
```

Il `.deb` si tira dietro `libpcsclite1` da solo; con l'AppImage il servizio
deve esserci comunque.

## Controlla che il lettore veda la carta

Collega il lettore, appoggia la CIE con il chip verso il basso e aspetta un
paio di secondi. Poi apri Firma CIE: se scegli un documento e clicchi
**Firma**, il programma ti chiede il PIN. Se invece dice che non trova la
carta, vedi [Se qualcosa non va](/cie-cli/guida/problemi/).

## La riga di comando

`cie` fa le stesse firme senza finestre, e in piu' legge i dati della carta e
certifica chiavi OpenPGP. Su Linux e macOS:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/AttilioGreco/cie-cli/releases/latest/download/cie-cli-installer.sh | sh
```

Su Windows, in PowerShell:

```powershell
irm https://github.com/AttilioGreco/cie-cli/releases/latest/download/cie-cli-installer.ps1 | iex
```

Sulla stessa pagina ci sono anche il `.msi` per Windows, il `.pkg` per macOS e
gli archivi `.tar.xz` / `.zip` da scompattare a mano, uno per piattaforma.
Accanto a ogni file c'è il suo `.sha256`.

Per aggiornare piu' avanti basta `cie-cli-update`.

Da qui in poi: [Primi passi](/cie-cli/riferimento/primi-passi/).

### Completamento automatico dei comandi

```bash
cie completions --install
```

Indovina la shell da `$SHELL` e mette lo script dove bash o fish lo trovano da
soli; per zsh stampa la riga da aggiungere a `~/.zshrc`. Vale la pena
rilanciarlo dopo un aggiornamento.
