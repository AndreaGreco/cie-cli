---
title: Cosa diventa pubblico
description: Dove finisce ogni dato personale, cosa vede chi verifica una firma, e cosa non si può piu' ritirare.
---

Il dato piu' delicato che la CIE porta con sè è il **codice fiscale**, che il
Ministero scrive nel subject del certificato X.509. Questa pagina dice dove
finisce, e dove no.

## Regola breve

Il codice fiscale sta **solo** nel certificato X.509 (`.cer`). Non entra mai nel
materiale OpenPGP pubblicabile.

## Dove sta ogni cosa

| Dato | Certificato X.509 | User ID del certificatore | Certification |
|---|:---:|:---:|:---:|
| Nome e cognome | sì | sì | - |
| Codice fiscale | **sì** | no | no |
| Numero della carta (ID_Servizi) | no | sì | - |
| Numero di serie del certificato | sì | no | no |
| Emittente e validità | sì | no | no |
| SHA-256 del certificato | - | - | sì |

Della certification, quello che diventa pubblico è la User ID del certificatore
che vi è associata: nome, cognome e numero della carta.

## I tre file

| File | Pubblicabile |
|---|---|
| `<chiave>-certificata.asc` | sì |
| `cie-certifier.asc` | sì |
| `cie-certifier.cer` | **no** - lo consegni tu, a chi decidi |

## Uno scenario concreto

Firmi i commit Git con la tua chiave OpenPGP e la carichi su GitHub. Chi verifica
un tuo commit ottiene il tuo codice fiscale?

**No.** Circolano tre cose diverse, e solo una porta il codice fiscale:

- Il commit lo firmi con la **tua** chiave, non con la CIE. Chi verifica la firma
  del commit ha bisogno soltanto della tua chiave pubblica.
- La certification non entra nella verifica del commit.
- Il `.cer` a GitHub non arriva mai.

Se pubblichi anche la chiave certificata, diventano pubblici nome, cognome e
numero della carta - dalla User ID del certificatore. Il codice fiscale no.

## La certification viaggia solo se la esporti

```bash
gpg --export chiave                              # con la certification
gpg --export --export-options export-minimal     # senza
```

Quindi la scelta resta tua a ogni singola consegna: puoi tenere la chiave
certificata in locale e pubblicare quella nuda, mostrando la certification solo
a chi ti chiede di dimostrare l'identità.

## Il numero della carta

L'`ID_Servizi` è stampato sulla carta e si legge dal chip senza PIN. Non è un
segreto, ma è un **identificatore stabile**: correla fra loro tutte le chiavi
che certifichi con quella CIE, e una volta su un keyserver non si toglie piu'.

Sta nella User ID perchè distingue due CIE dello stesso titolare. Se quel caso
non ti riguarda, è un dato che stai pubblicando senza guadagnarci niente.

## Cosa non si può ritirare

Il materiale OpenPGP diffuso è irrevocabile in pratica: i keyserver non
dimenticano e le copie restano. Per questo il default è il piu' prudente
possibile, ed è reversibile in una sola direzione.

- Il `.cer` si può **sempre** consegnare dopo, e la catena verso la PKI si
  ricompone senza rifirmare niente.
- Un codice fiscale finito su un keyserver non si toglie piu'.

## Il portachiavi

Nome, cognome e codice fiscale ricordati insieme al PIN stanno **dentro** il
segreto cifrato, mai negli attributi di ricerca. Su secret-service gli attributi
sono metadati in chiaro, leggibili da qualunque processo sul bus di sessione.

Vedi [Il PIN](/guida/pin/).

## Cosa `cie-cli` non fa

- Non scrive nessun file di cache suo, e in particolare nessun pezzo di PIN. Il
  solo segreto che può essere persistito è il PIN intero, se scegli di
  affidarlo al gestore di password del sistema - vedi [Il PIN](/guida/pin/).
- Non parla con la rete durante la firma: senza la feature di marca temporale nel
  build non c'è nemmeno un client HTTP.
- Non manda niente a nessun servizio, di nessun tipo.
