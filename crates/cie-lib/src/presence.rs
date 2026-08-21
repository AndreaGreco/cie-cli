//! Cosa c'e' sul lettore, senza connettersi alla carta.
//!
//! PC/SC sa dire da solo quando una carta viene appoggiata o tolta: la
//! `SCardGetStatusChange` blocca finche' lo stato di un lettore non cambia, e
//! il pseudo-lettore PnP fa lo stesso per i lettori attaccati e staccati. Non
//! serve interrogare la carta a ripetizione — e non si potrebbe nemmeno, dato
//! che ogni connessione durante una firma selezionerebbe l'applet sotto il
//! canale sicuro gia' aperto.
//!
//! Qui non si connette niente: si guarda e basta. Chi c'e' sul lettore lo dice
//! [`crate::CieCard`], che una connessione ce l'ha.

use std::time::Duration;

use pcsc::{Context, PNP_NOTIFICATION, ReaderState, Scope, State};
use tracing::debug;

use crate::error::{CieError, Result};

/// Cosa tengono i lettori, per quanto PC/SC dica senza connettersi.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Nessuno ha ancora guardato.
    #[default]
    Unknown,
    /// Nessun lettore: nessuno attaccato, oppure nessun demone PC/SC dietro.
    NoReader,
    /// Un lettore, senza niente sopra.
    Empty,
    /// Una carta su un lettore. Se sia una CIE lo dice solo una connessione.
    Card,
}

/// Una guardia sui lettori, da tenere aperta su un thread suo.
///
/// Lo stato vive qui dentro fra una chiamata e l'altra: e' quello che PC/SC
/// confronta per decidere se qualcosa e' cambiato, e ripartire ogni volta da
/// `UNAWARE` vorrebbe dire tornare subito con "cambiato" senza aspettare mai.
pub struct Watch {
    ctx: Context,
    states: Vec<ReaderState>,
}

impl Watch {
    pub fn new() -> Result<Self> {
        let ctx = Context::establish(Scope::User)
            .map_err(|e| CieError::Transport(format!("contesto PC/SC: {e:?}")))?;
        Ok(Self {
            // Il PnP c'e' sempre, anche senza lettori: e' cosi' che ci si
            // accorge del primo che viene attaccato.
            states: vec![ReaderState::new(PNP_NOTIFICATION(), State::UNAWARE)],
            ctx,
        })
    }

    /// Aspetta che cambi qualcosa, al massimo per `patience`.
    ///
    /// Torna quello che c'e' adesso, che sia cambiato o che il tempo sia
    /// scaduto: al chiamante interessa lo stato, non l'evento. Scaduto il
    /// tempo non e' un errore — e' la risposta "niente di nuovo", e serve a
    /// riaccorgersi di un demone morto sotto di noi.
    pub fn look(&mut self, patience: Duration) -> Result<Presence> {
        self.follow_the_readers();

        // Da qui in poi si aspetta: quello che sappiamo diventa quello con cui
        // PC/SC confronta.
        for state in &mut self.states {
            state.sync_current_state();
        }

        match self.ctx.get_status_change(patience, &mut self.states) {
            Ok(()) | Err(pcsc::Error::Timeout) => Ok(self.presence()),
            Err(e) => Err(CieError::Transport(format!("attesa sui lettori: {e:?}"))),
        }
    }

    /// Toglie i lettori spariti e prende quelli arrivati.
    ///
    /// Un lettore staccato resta nella lista come `UNKNOWN`, e lasciarcelo
    /// significa aspettare per sempre su qualcosa che non esiste piu'.
    fn follow_the_readers(&mut self) {
        self.states
            .retain(|s| !s.event_state().intersects(State::UNKNOWN | State::IGNORE));

        let mut buffer = [0; 2048];
        let Ok(names) = self.ctx.list_readers(&mut buffer) else {
            // Nessun lettore da elencare: rimane il PnP, che e' esattamente
            // quello che aspetta il prossimo.
            return;
        };
        for name in names {
            if !self.states.iter().any(|s| s.name() == name) {
                debug!("nuovo lettore: {:?}", name);
                self.states.push(ReaderState::new(name, State::UNAWARE));
            }
        }
    }

    fn presence(&self) -> Presence {
        let mut reader = false;
        for state in self.states.iter().filter(|s| s.name() != PNP_NOTIFICATION()) {
            reader = true;
            // `PRESENT` vale anche per una carta che non risponde: e' comunque
            // qualcosa appoggiato li' sopra, e dirlo assente sarebbe una bugia
            // piu' grande di dirlo presente.
            if state.event_state().contains(State::PRESENT) {
                return Presence::Card;
            }
        }
        if reader { Presence::Empty } else { Presence::NoReader }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Una guardia senza lettori dice "nessun lettore", non "vuoto": sono due
    /// cose diverse da fare — attaccare il lettore, o appoggiare la carta.
    #[test]
    fn senza_lettori_non_e_un_lettore_vuoto() {
        let watch = Watch {
            ctx: match Context::establish(Scope::User) {
                Ok(ctx) => ctx,
                // Senza demone PC/SC non c'e' niente da provare: la stessa
                // situazione la gestisce il chiamante, non questo test.
                Err(_) => return,
            },
            states: vec![ReaderState::new(PNP_NOTIFICATION(), State::UNAWARE)],
        };
        assert_eq!(watch.presence(), Presence::NoReader);
    }
}
