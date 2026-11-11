//! Ciclo de vida de partidas y sesiones (ARQUITECTURA.md §10).
//!
//! Todas las funciones reciben `now` (epoch ms) para poder probarse sin reloj.

use crate::store::{ChestType, Counts, EndReason, Scope, Store};
use rusqlite::Result;
use serde::Serialize;

/// Sin partidas durante este tiempo, la sesión se cierra sola.
pub const DEFAULT_INACTIVITY_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub in_match: bool,
    pub session_active: bool,
    pub session_started_at: Option<i64>,
    /// Partida en curso o, si no hay, la última de la sesión.
    pub match_counts: Counts,
    pub session_counts: Counts,
    pub total_counts: Counts,
    /// Partidas de la sesión activa.
    pub session_matches: u32,
    /// Partidas de siempre, incluidos los ajustes manuales.
    pub total_matches: u32,
}

pub struct Tracker {
    store: Store,
    inactivity_ms: i64,
    /// (id, inicio) de la sesión activa.
    session: Option<(i64, i64)>,
    active_match: Option<i64>,
    /// Partida cuyos cofres se muestran: la activa o la última terminada.
    shown_match: Option<i64>,
    last_activity: i64,
}

impl Tracker {
    /// Abre el estado sobre la base de datos y recupera lo que dejó un cierre
    /// inesperado: las partidas abiertas se cierran como interrumpidas y la
    /// sesión abierta se reanuda solo si su última actividad es reciente.
    pub fn new(store: Store, now: i64, inactivity_ms: i64) -> Result<Self> {
        store.close_interrupted_matches()?;
        let mut session = None;
        let mut last_activity = now;
        for (id, started_at) in store.open_sessions()? {
            let activity = store.last_activity(id)?;
            if session.is_none() && now - activity <= inactivity_ms {
                session = Some((id, started_at));
                last_activity = activity;
            } else {
                store.end_session(id, activity)?;
            }
        }
        Ok(Self {
            store,
            inactivity_ms,
            session,
            active_match: None,
            shown_match: None,
            last_activity,
        })
    }

    /// Inicia una sesión si no hay ninguna activa.
    pub fn start_session(&mut self, now: i64, auto: bool) -> Result<i64> {
        if let Some((id, _)) = self.session {
            return Ok(id);
        }
        let id = self.store.start_session(now, auto)?;
        self.session = Some((id, now));
        self.last_activity = now;
        Ok(id)
    }

    /// Cierra la sesión activa (y su partida en curso, como final manual).
    pub fn stop_session(&mut self, now: i64) -> Result<()> {
        self.match_ended(now, EndReason::Manual)?;
        if let Some((id, _)) = self.session.take() {
            self.store.end_session(id, now)?;
        }
        self.shown_match = None;
        Ok(())
    }

    /// Se detectó el HUD de partida. Inicia sesión automática si hace falta.
    pub fn match_started(&mut self, now: i64) -> Result<()> {
        if self.active_match.is_some() {
            return Ok(());
        }
        let session_id = self.start_session(now, true)?;
        let id = self.store.start_match(session_id, now)?;
        self.active_match = Some(id);
        self.shown_match = Some(id);
        self.last_activity = now;
        Ok(())
    }

    pub fn match_ended(&mut self, now: i64, reason: EndReason) -> Result<()> {
        if let Some(id) = self.active_match.take() {
            self.store.end_match(id, now, reason)?;
            self.last_activity = now;
        }
        Ok(())
    }

    /// Atajo "forzar nueva partida": cierra la actual y abre otra.
    pub fn force_new_match(&mut self, now: i64) -> Result<()> {
        self.match_ended(now, EndReason::Manual)?;
        self.match_started(now)
    }

    /// Registra un cofre abierto. Un cofre solo puede abrirse dentro de una
    /// partida, así que si no se había detectado ninguna se abre una ahora.
    pub fn chest_opened(
        &mut self,
        now: i64,
        chest: ChestType,
        confidence: f32,
        signals: u8,
    ) -> Result<()> {
        self.match_started(now)?;
        let (Some((session_id, _)), Some(match_id)) = (self.session, self.active_match) else {
            unreachable!("match_started deja sesión y partida activas");
        };
        self.store
            .add_chest(now, match_id, session_id, chest, confidence, signals)?;
        self.last_activity = now;
        Ok(())
    }

    /// Ajuste manual del histórico; no toca la partida ni la sesión.
    /// Devuelve `false` si dejaría el contador de ese tipo en negativo.
    pub fn adjust_total(&mut self, now: i64, chest: Option<ChestType>, delta: i64) -> Result<bool> {
        self.store.adjust_total(now, chest, delta)
    }

    /// Ajuste manual del total de partidas; no toca la sesión.
    /// Devuelve `false` si lo dejaría en negativo.
    pub fn adjust_matches(&mut self, now: i64, delta: i64) -> Result<bool> {
        self.store.adjust_matches(now, delta)
    }

    /// Cierra la sesión por inactividad. Devuelve `true` si cambió algo.
    pub fn tick(&mut self, now: i64) -> Result<bool> {
        let Some((id, _)) = self.session else {
            return Ok(false);
        };
        if self.active_match.is_some() || now - self.last_activity <= self.inactivity_ms {
            return Ok(false);
        }
        self.store.end_session(id, self.last_activity)?;
        self.session = None;
        self.shown_match = None;
        Ok(true)
    }

    pub fn stats(&self) -> Result<Stats> {
        let counts = |scope: Option<Scope>| match scope {
            Some(scope) => self.store.counts(scope),
            None => Ok(Counts::default()),
        };
        Ok(Stats {
            in_match: self.active_match.is_some(),
            session_active: self.session.is_some(),
            session_started_at: self.session.map(|(_, started_at)| started_at),
            match_counts: counts(self.shown_match.map(Scope::Match))?,
            session_counts: counts(self.session.map(|(id, _)| Scope::Session(id)))?,
            total_counts: counts(Some(Scope::All))?,
            session_matches: match self.session {
                Some((id, _)) => self.store.matches_in_session(id)?,
                None => 0,
            },
            total_matches: self.store.total_matches()?.max(0) as u32,
        })
    }

    #[cfg(test)]
    fn into_store(self) -> Store {
        self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;
    const HOUR: i64 = 60 * MIN;

    fn tracker() -> Tracker {
        Tracker::new(Store::open_in_memory().unwrap(), 0, HOUR).unwrap()
    }

    fn chest(t: &mut Tracker, now: i64, kind: ChestType) {
        t.chest_opened(now, kind, 1.0, 0b11_1111).unwrap();
    }

    #[test]
    fn empieza_vacio() {
        let s = tracker().stats().unwrap();
        assert!(!s.in_match && !s.session_active);
        assert_eq!(s.total_counts.total, 0);
    }

    #[test]
    fn la_partida_inicia_sesion_automatica_y_cuenta_en_los_tres_niveles() {
        let mut t = tracker();
        t.match_started(1_000).unwrap();
        chest(&mut t, 2_000, ChestType::Normal);
        chest(&mut t, 3_000, ChestType::Rare);
        chest(&mut t, 4_000, ChestType::Normal);

        let s = t.stats().unwrap();
        assert!(s.in_match && s.session_active);
        assert_eq!(s.session_started_at, Some(1_000));
        for c in [s.match_counts, s.session_counts, s.total_counts] {
            assert_eq!(
                (c.normal, c.rare, c.spirit, c.boss, c.total),
                (2, 1, 0, 0, 3)
            );
        }
    }

    #[test]
    fn partida_nueva_reinicia_el_contador_de_partida_pero_no_el_de_sesion() {
        let mut t = tracker();
        t.match_started(1_000).unwrap();
        chest(&mut t, 2_000, ChestType::Normal);
        t.match_ended(3_000, EndReason::Eliminated).unwrap();

        // Terminada la partida se sigue mostrando su total.
        let s = t.stats().unwrap();
        assert!(!s.in_match);
        assert_eq!(s.match_counts.total, 1);

        t.match_started(4_000).unwrap();
        chest(&mut t, 5_000, ChestType::Spirit);
        chest(&mut t, 6_000, ChestType::Boss);
        let s = t.stats().unwrap();
        assert_eq!(s.match_counts.total, 2);
        assert_eq!(s.session_counts.total, 3);
        assert_eq!(s.total_counts.total, 3);
    }

    #[test]
    fn forzar_nueva_partida_separa_los_cofres() {
        let mut t = tracker();
        t.match_started(1_000).unwrap();
        chest(&mut t, 2_000, ChestType::Normal);
        t.force_new_match(3_000).unwrap();
        let s = t.stats().unwrap();
        assert!(s.in_match);
        assert_eq!(s.match_counts.total, 0);
        assert_eq!(s.session_counts.total, 1);
    }

    #[test]
    fn un_cofre_sin_partida_detectada_abre_partida_y_sesion() {
        let mut t = tracker();
        chest(&mut t, 1_000, ChestType::Normal);
        let s = t.stats().unwrap();
        assert!(s.in_match && s.session_active);
        assert_eq!(s.match_counts.total, 1);
    }

    #[test]
    fn detener_cierra_sesion_y_partida_y_conserva_el_historico() {
        let mut t = tracker();
        chest(&mut t, 1_000, ChestType::Normal);
        t.stop_session(2_000).unwrap();
        let s = t.stats().unwrap();
        assert!(!s.in_match && !s.session_active);
        assert_eq!(s.match_counts.total, 0);
        assert_eq!(s.session_counts.total, 0);
        assert_eq!(s.total_counts.total, 1);

        // Una sesión nueva empieza en cero.
        t.start_session(3_000, false).unwrap();
        assert_eq!(t.stats().unwrap().session_counts.total, 0);
    }

    #[test]
    fn la_sesion_se_cierra_tras_la_inactividad_pero_no_durante_una_partida() {
        let mut t = tracker();
        t.match_started(0).unwrap();
        assert!(!t.tick(5 * HOUR).unwrap(), "en partida no caduca");

        t.match_ended(5 * HOUR, EndReason::Victory).unwrap();
        assert!(!t.tick(5 * HOUR + 59 * MIN).unwrap());
        assert!(t.tick(6 * HOUR + MIN).unwrap());
        assert!(!t.stats().unwrap().session_active);
    }

    #[test]
    fn una_sesion_que_cruza_la_medianoche_sigue_siendo_una_sola() {
        let mut t = tracker();
        // 23:00 → 02:00 con partidas seguidas: nunca hay 60 min de inactividad.
        let mut now = 23 * HOUR;
        while now < 26 * HOUR {
            t.match_started(now).unwrap();
            chest(&mut t, now + MIN, ChestType::Normal);
            t.match_ended(now + 20 * MIN, EndReason::Eliminated)
                .unwrap();
            assert!(!t.tick(now + 25 * MIN).unwrap());
            now += 25 * MIN;
        }
        let s = t.stats().unwrap();
        assert_eq!(s.session_started_at, Some(23 * HOUR));
        assert_eq!(s.session_counts.total, s.total_counts.total);
    }

    #[test]
    fn tras_un_cierre_inesperado_reciente_se_reanuda_la_sesion() {
        let mut t = tracker();
        t.match_started(1_000).unwrap();
        chest(&mut t, 2_000, ChestType::Rare);
        let store = t.into_store(); // la app muere con sesión y partida abiertas

        let t = Tracker::new(store, 10 * MIN, HOUR).unwrap();
        let s = t.stats().unwrap();
        assert!(s.session_active, "la sesión reciente se reanuda");
        assert!(
            !s.in_match,
            "la partida abierta se cierra como interrumpida"
        );
        assert_eq!(s.session_started_at, Some(1_000));
        assert_eq!(s.session_counts.total, 1);
    }

    #[test]
    fn tras_un_cierre_inesperado_antiguo_la_sesion_se_cierra() {
        let mut t = tracker();
        chest(&mut t, 1_000, ChestType::Rare);
        let store = t.into_store();

        let mut t = Tracker::new(store, 3 * HOUR, HOUR).unwrap();
        let s = t.stats().unwrap();
        assert!(!s.session_active);
        assert_eq!(s.total_counts.total, 1);

        // Y no quedan sesiones abiertas que reaparezcan en el siguiente arranque.
        t.start_session(3 * HOUR, false).unwrap();
        t.stop_session(3 * HOUR + MIN).unwrap();
        let t = Tracker::new(t.into_store(), 3 * HOUR + 2 * MIN, HOUR).unwrap();
        assert!(!t.stats().unwrap().session_active);
    }

    #[test]
    fn el_ajuste_manual_solo_afecta_al_historico() {
        let mut t = tracker();
        chest(&mut t, 1_000, ChestType::Normal);
        assert!(t.adjust_total(2_000, Some(ChestType::Rare), 40).unwrap());
        assert!(t.adjust_total(2_000, None, 300).unwrap());

        let s = t.stats().unwrap();
        assert_eq!(s.match_counts.total, 1);
        assert_eq!(s.session_counts.total, 1);
        let c = s.total_counts;
        assert_eq!((c.normal, c.rare, c.total), (1, 40, 341));
    }

    #[test]
    fn el_ajuste_manual_puede_restar_pero_no_bajar_de_cero() {
        let mut t = tracker();
        chest(&mut t, 1_000, ChestType::Normal);
        t.adjust_total(2_000, Some(ChestType::Normal), 9).unwrap();

        assert!(t.adjust_total(3_000, Some(ChestType::Normal), -4).unwrap());
        assert_eq!(t.stats().unwrap().total_counts.normal, 6);

        assert!(!t.adjust_total(4_000, Some(ChestType::Normal), -7).unwrap());
        assert!(!t.adjust_total(4_000, Some(ChestType::Boss), -1).unwrap());
        assert!(!t.adjust_total(4_000, None, -1).unwrap());
        let c = t.stats().unwrap().total_counts;
        assert_eq!(
            (c.normal, c.total),
            (6, 6),
            "un ajuste rechazado no guarda nada"
        );
    }

    #[test]
    fn el_ajuste_manual_sobrevive_a_un_reinicio() {
        let mut t = tracker();
        t.adjust_total(1_000, None, 250).unwrap();
        let t = Tracker::new(t.into_store(), 2_000, HOUR).unwrap();
        assert_eq!(t.stats().unwrap().total_counts.total, 250);
    }

    #[test]
    fn cuenta_las_partidas_de_la_sesion_y_del_historico() {
        let mut t = tracker();
        let s = t.stats().unwrap();
        assert_eq!((s.session_matches, s.total_matches), (0, 0));

        t.match_started(1_000).unwrap();
        t.match_started(1_500).unwrap(); // el HUD sigue ahí: misma partida
        t.match_ended(2_000, EndReason::Eliminated).unwrap();
        t.match_started(3_000).unwrap();
        t.force_new_match(4_000).unwrap();
        let s = t.stats().unwrap();
        assert_eq!((s.session_matches, s.total_matches), (3, 3));

        // Otra sesión: su cuenta empieza de cero, el histórico sigue.
        t.stop_session(5_000).unwrap();
        assert_eq!(t.stats().unwrap().session_matches, 0);
        chest(&mut t, 6_000, ChestType::Normal); // cofre sin partida detectada
        let s = t.stats().unwrap();
        assert_eq!((s.session_matches, s.total_matches), (1, 4));
    }

    #[test]
    fn el_total_de_partidas_se_puede_ajustar_a_mano() {
        let mut t = tracker();
        t.match_started(1_000).unwrap();
        assert!(t.adjust_matches(2_000, 57).unwrap());
        let s = t.stats().unwrap();
        assert_eq!((s.session_matches, s.total_matches), (1, 58));
        assert_eq!(s.total_counts.total, 0, "no toca los cofres");

        assert!(t.adjust_matches(3_000, -8).unwrap());
        assert!(!t.adjust_matches(4_000, -51).unwrap());
        assert_eq!(t.stats().unwrap().total_matches, 50);

        let t = Tracker::new(t.into_store(), 5_000, HOUR).unwrap();
        assert_eq!(t.stats().unwrap().total_matches, 50);
    }
}
