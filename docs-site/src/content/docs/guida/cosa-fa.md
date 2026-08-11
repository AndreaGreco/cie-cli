---
title: Cos'è e cosa puoi farci
description: A cosa serve Firma CIE, che cosa ti serve per usarlo, e che valore ha la firma che produce.
---

Firma CIE mette la tua firma su un documento usando la **Carta d'Identità
Elettronica**. La carta fa da penna: il documento le passa davanti, lei lo
firma, e la firma resta legata a quel file per sempre.

Serve quando qualcuno ti chiede un documento firmato digitalmente e tu hai in
tasca la CIE.

## Cosa ti serve

- La tua **CIE 3.0** (quella con la fotografia e il chip, dal 2016 in poi)
- Un **lettore di smart card** collegato al computer - quelli USB da una
  ventina di euro vanno bene, cosi' come la maggior parte dei portatili con il
  lettore incorporato
- Il **PIN** della carta, tutte e otto le cifre

Il PIN è quello che ti hanno consegnato in Comune, spezzato in due metà: le
prime quattro cifre sulla ricevuta della richiesta, le altre quattro nella
busta arrivata con la carta. Se le hai perse entrambe, il PIN si recupera solo
in Comune.

## Cosa non ti serve

Non devi installare il **middleware CIE**, non devi registrare la carta con
nessuna applicazione, non devi creare un account. Firma CIE parla al chip
direttamente e non manda niente su internet: il documento resta sul tuo
computer, e nemmeno il programma lo vede passare da nessuna parte.

## Che cosa esce

A seconda del file che gli dai:

| Se firmi | Ottieni | Quando serve |
|---|---|---|
| un PDF | un PDF firmato, con il riquadro della firma visibile sulla pagina | quando il documento deve restare leggibile e mostrare che è firmato |
| qualunque altro file | un file `.p7m` | quando un ufficio ti chiede "il p7m" |

Nel primo caso l'originale resta com'è e ti ritrovi accanto un secondo file,
`documento-firmato.pdf`.

## Quanto vale questa firma

Il certificato dentro la CIE nasce per **farti entrare nei siti della pubblica
amministrazione**, non per firmare contratti. Tecnicamente non è un
certificato di firma qualificata, e non sostituisce la firma digitale che ti
vende un gestore accreditato (Aruba, InfoCert, Namirial e simili).

:::note[Con la pubblica amministrazione, però, vale]
Il DM 17 giugno 2014, art. 3 comma 1, dice che "la firma digitale apposta con
la Carta d'Identità Elettronica, ai sensi del decreto del Ministro
dell'Interno 24 ottobre 2007, n. 156, è equivalente alla firma digitale
qualificata".
:::

In pratica:

- **Un ufficio pubblico** che accetta la CIE accetta questa firma.
- **Un privato** che ti chiede una firma digitale qualificata - una banca, un
  notaio - potrebbe non accettarla.
- **Un programma di verifica** (Acrobat, Dike, i validatori online) può
  segnalare la firma con un avviso, perchè controlla a cosa serve il
  certificato e vede che è di autenticazione. La firma resta valida e
  verificabile: è l'etichetta a non combaciare, non la matematica.

Se hai un dubbio, chiedilo a chi ti sta chiedendo il documento **prima** di
firmare.

# Come proseguire ?

- [Installare](/cie-cli/guida/installazione/) - Windows, macOS, Linux
- [Firmare un documento](/cie-cli/guida/firmare/) - la procedura, passo per passo
- [Se qualcosa non va](/cie-cli/guida/problemi/)

C'è anche un programma da **riga di comando**, `cie`, che fa le stesse cose
senza finestre aggiuntive: vedi [Primi passi](/cie-cli/riferimento/primi-passi/).
