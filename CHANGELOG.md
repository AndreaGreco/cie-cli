# Changelog

Tutte le modifiche degne di nota finiscono qui.

Il formato segue [Keep a Changelog](https://keepachangelog.com/it/1.1.0/) e il
versionamento e' [SemVer](https://semver.org/lang/it/).

## [Unreleased]

## [0.1.7] - 2026-08-13

### Fixed

- Il sito della documentazione era senza CSS: si e' spostato sul dominio
  `cie-cli.crazylinux.it`, che le Pages servono dalla radice, ma restava
  costruito con `base: '/cie-cli'` e cercava fogli di stile e link interni
  sotto un prefisso che li' non esiste.

## [0.1.6] - 2026-08-13

### Fixed

- Il workflow che costruisce i pacchetti della GUI non e' mai partito, in
  nessuna release: scattava su `release: published`, ma la release la crea dist
  con il `GITHUB_TOKEN` e Actions non fa partire workflow dagli eventi di quel
  token. Ora parte sul tag, in parallelo a dist, e aspetta che la release
  compaia prima di caricare. Fino alla 0.1.5 compresa sulle release c'era solo
  la CLI.

## [0.1.5] - 2026-08-13

### Added

- MSI di «Firma CIE» per Windows: installa per tutte le utenze, piazza l'icona
  sul desktop e nel menu Start e mette `cie` nel `PATH` di sistema. Il
  `-setup.exe` resta per chi non ha l'amministratore.

### Changed

- I bundle della GUI (`.msi`, `-setup.exe`, `.dmg`, `.deb`, `.AppImage`)
  portano dentro anche il binario `cie`: un solo download da' finestra e riga
  di comando.
- README e pagina di installazione aprono con «Firma CIE» e i suoi pacchetti;
  `.pkg` e gli installer `curl`/`irm` sono marcati come soli-CLI.

### Removed

- L'MSI della sola CLI, che non aveva icone perche' installava un programma da
  terminale: su Windows era il file sbagliato da scaricare. Lo rimpiazza l'MSI
  della GUI, che contiene anche `cie`.

### Fixed

- La finestra della GUI ora ha la sua icona anche in barra applicazioni, non
  solo nel lanciatore.

## [0.1.4] - 2026-08-12

## [0.1.3] - 2026-08-08

## [0.1.1] - 2026-08-08

### Added

- Pacchetti multipiattaforma: archivi e installer `.pkg`/`.msi` per la CLI
  (`dist`), bundle `.dmg`/`.exe`/`.deb`/`.AppImage` per la GUI
  (`cargo-packager`).
