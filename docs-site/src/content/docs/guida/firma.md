---
title: Firmare documenti
description: PAdES dentro il PDF, CAdES in un .p7m, o la firma RSA nuda — e come verificarle.
---

Tre contenitori, un solo firmatario. La chiave privata non lascia mai la carta:
`cie-cli` calcola il digest, la CIE lo firma, il risultato viene impacchettato.

:::caution[Non sono firme qualificate]
Il certificato sulla CIE e' **di autenticazione**, non di firma qualificata
(policy `1.3.76.47.4`, EKU `TLS Web Client Authentication`, Key Usage senza
`nonRepudiation`). Le firme prodotte con questa chiave sono crittograficamente
valide e verificabili, ma **non** sono firme digitali qualificate ai sensi
eIDAS. Vedi [Limiti e stato](/cie-cli/progetto/limiti/).
:::

## PAdES — la firma dentro il PDF

```bash
cie sign-pdf documento.pdf
```

Produce `documento-firmato.pdf`, con la firma dentro il file e un riquadro
visibile in basso a destra che riporta intestatario e data, presi dal
certificato della carta.

| Opzione | Effetto |
|---|---|
| `--pagina N` | su quale pagina disegnare il riquadro (da 1) |
| `--motivo "..."` | una riga in piu' dentro il riquadro |
| `--output FILE` | dove scrivere (default: `<file>-firmato.pdf`) |
| `--certifica [1-3]` | firma di certificazione, vedi sotto |

Si controlla con:

```bash
pdfsig documento-firmato.pdf
```

### `--certifica`

Una firma di certificazione — la firma d'autore, quella che i lettori PDF
marcano con il nastrino — dichiara che il documento e' tuo e stabilisce cosa
resti lecito modificare:

| Livello | Cosa resta permesso |
|---|---|
| `1` | niente |
| `2` | compilare moduli e firmare — **il default**, quindi altri possono controfirmare |
| `3` | anche le annotazioni |

`--certifica` senza numero vale `2`. E' ammessa **solo sulla prima firma** di un
documento: un PDF gia' firmato non puo' essere certificato dopo.

## CAdES — contenuto e firma in un `.p7m`

```bash
cie sign-p7m contratto.odt
```

Produce `contratto.odt.p7m`: una busta CAdES enveloping, cioe' un DER che
contiene sia il documento sia la firma. E' il formato che la PA chiama `.p7m`, e
vale per qualunque file, PDF compresi.

```bash
openssl cms -verify -in contratto.odt.p7m -inform DER -noverify
```

`-noverify` salta la validazione della catena di certificati, che richiede le CA
del Ministero; la firma sul contenuto viene comunque verificata.

## La firma nuda

```bash
cie sign file.txt
```

Il mattone su cui gli altri due si costruiscono: una firma RSA PKCS#1 v1.5 di
256 byte sul digest SHA-256 del file, senza nessun contenitore.

Escono due file:

- `file.txt.sig` — la firma
- `file.txt.cer` — il certificato della carta, senza il quale la firma e' solo un blocco di byte

Il file originale non viene toccato: e' una firma separata, non un documento
firmato.

```bash
openssl x509 -inform DER -in file.txt.cer -pubkey -noout > pub.pem
openssl dgst -sha256 -verify pub.pem -signature file.txt.sig file.txt
```

`cie sign` verifica comunque il risultato in locale con la chiave pubblica del
certificato prima di scriverlo: se il comando termina bene, la firma e' gia'
stata controllata.

## Quale usare

| Situazione | Comando |
|---|---|
| Un PDF che deve restare leggibile e mostrare la firma | `cie sign-pdf` |
| Un file qualunque per la PA | `cie sign-p7m` |
| Serve solo provare che quel byte stream e' passato dalla carta | `cie sign` |

## Cosa non c'e'

La **marca temporale** (livello B-T) richiede una chiamata di rete a una TSA, e
il percorso di firma oggi non parla con nessuno all'infuori della carta: nel
build non c'e' nemmeno un client HTTP. Senza marca temporale, la validita' della
firma nel tempo dipende dalla validita' del certificato.
