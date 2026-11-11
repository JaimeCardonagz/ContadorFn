//! Persistencia en SQLite (ARQUITECTURA.md §8). Se guardan eventos, no contadores.

use rusqlite::{Connection, Result, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChestType {
    Normal,
    Rare,
    Spirit,
    /// Caja de munición: se cuenta aparte, no suma al total de cofres.
    Ammo,
}

impl ChestType {
    pub fn as_str(self) -> &'static str {
        match self {
            ChestType::Normal => "normal",
            ChestType::Rare => "rare",
            ChestType::Spirit => "spirit",
            ChestType::Ammo => "ammo",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Eliminated,
    Victory,
    Lobby,
    Manual,
}

impl EndReason {
    fn as_str(self) -> &'static str {
        match self {
            EndReason::Eliminated => "eliminated",
            EndReason::Victory => "victory",
            EndReason::Lobby => "lobby",
            EndReason::Manual => "manual",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub normal: u32,
    pub rare: u32,
    pub spirit: u32,
    /// Cajas de munición: se cuentan aparte.
    pub ammo: u32,
    /// Total de cofres (tesoro + raro + espíritu); no incluye la munición.
    pub total: u32,
}

/// Qué eventos sumar.
#[derive(Debug, Clone, Copy)]
pub enum Scope {
    Match(i64),
    Session(i64),
    All,
}

// `chest_type` no lleva CHECK: los tipos cambian con las temporadas y así no
// hace falta migrar. 'interrupted' marca partidas que quedaron abiertas porque
// la app se cerró sin terminarlas.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
  id          INTEGER PRIMARY KEY,
  started_at  INTEGER NOT NULL,
  ended_at    INTEGER,
  auto_start  INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS matches (
  id          INTEGER PRIMARY KEY,
  session_id  INTEGER REFERENCES sessions(id),
  started_at  INTEGER NOT NULL,
  ended_at    INTEGER,
  end_reason  TEXT CHECK (end_reason IN ('eliminated','victory','lobby','manual','interrupted'))
);
CREATE TABLE IF NOT EXISTS chest_events (
  id          INTEGER PRIMARY KEY,
  ts          INTEGER NOT NULL,
  match_id    INTEGER REFERENCES matches(id),
  session_id  INTEGER REFERENCES sessions(id),
  chest_type  TEXT NOT NULL,
  confidence  REAL NOT NULL,
  signals     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_events_session ON chest_events(session_id);
CREATE INDEX IF NOT EXISTS idx_events_match   ON chest_events(match_id);
CREATE INDEX IF NOT EXISTS idx_events_ts      ON chest_events(ts);
CREATE TABLE IF NOT EXISTS adjustments (
  id          INTEGER PRIMARY KEY,
  ts          INTEGER NOT NULL,
  chest_type  TEXT,
  delta       INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS match_adjustments (
  id          INTEGER PRIMARY KEY,
  ts          INTEGER NOT NULL,
  delta       INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT);
INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', '1');
";

// Último instante con actividad en una sesión: su inicio, su última partida o
// su último cofre.
const LAST_ACTIVITY: &str = "
SELECT MAX(
  s.started_at,
  COALESCE((SELECT MAX(COALESCE(ended_at, started_at)) FROM matches WHERE session_id = s.id), 0),
  COALESCE((SELECT MAX(ts) FROM chest_events WHERE session_id = s.id), 0)
) FROM sessions s WHERE s.id = ?1";

const COUNT_BY_MATCH: &str =
    "SELECT chest_type, COUNT(*) FROM chest_events WHERE match_id = ?1 GROUP BY chest_type";
const COUNT_BY_SESSION: &str =
    "SELECT chest_type, COUNT(*) FROM chest_events WHERE session_id = ?1 GROUP BY chest_type";
// El histórico suma los eventos detectados y los ajustes manuales.
const COUNT_ALL: &str = "
SELECT chest_type, SUM(n) FROM (
  SELECT chest_type, COUNT(*) AS n FROM chest_events GROUP BY chest_type
  UNION ALL
  SELECT chest_type, SUM(delta) FROM adjustments GROUP BY chest_type
) GROUP BY chest_type";

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn start_session(&self, now: i64, auto: bool) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions (started_at, auto_start) VALUES (?1, ?2)",
            params![now, auto],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn end_session(&self, id: i64, ended_at: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET ended_at = ?2 WHERE id = ?1",
            params![id, ended_at],
        )?;
        Ok(())
    }

    /// Sesiones sin cerrar, de la más reciente a la más antigua: (id, inicio).
    pub fn open_sessions(&self) -> Result<Vec<(i64, i64)>> {
        self.conn
            .prepare("SELECT id, started_at FROM sessions WHERE ended_at IS NULL ORDER BY id DESC")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    }

    pub fn last_activity(&self, session_id: i64) -> Result<i64> {
        self.conn
            .query_row(LAST_ACTIVITY, params![session_id], |r| r.get(0))
    }

    pub fn start_match(&self, session_id: i64, now: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO matches (session_id, started_at) VALUES (?1, ?2)",
            params![session_id, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn end_match(&self, id: i64, now: i64, reason: EndReason) -> Result<()> {
        self.conn.execute(
            "UPDATE matches SET ended_at = ?2, end_reason = ?3 WHERE id = ?1",
            params![id, now, reason.as_str()],
        )?;
        Ok(())
    }

    /// Cierra las partidas que quedaron abiertas tras un cierre inesperado,
    /// usando como final su último cofre (o su inicio si no hubo ninguno).
    pub fn close_interrupted_matches(&self) -> Result<()> {
        self.conn.execute(
            "UPDATE matches SET end_reason = 'interrupted', ended_at = MAX(
               started_at,
               COALESCE((SELECT MAX(ts) FROM chest_events WHERE match_id = matches.id), 0))
             WHERE ended_at IS NULL",
            [],
        )?;
        Ok(())
    }

    pub fn add_chest(
        &self,
        ts: i64,
        match_id: i64,
        session_id: i64,
        chest: ChestType,
        confidence: f32,
        signals: u8,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO chest_events (ts, match_id, session_id, chest_type, confidence, signals)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                ts,
                match_id,
                session_id,
                chest.as_str(),
                confidence,
                signals
            ],
        )?;
        Ok(())
    }

    /// Cofres por tipo en el ámbito pedido. El tipo `None` son ajustes sin
    /// clasificar de versiones anteriores; siguen sumando al total.
    fn buckets(&self, scope: Scope) -> Result<Vec<(Option<String>, i64)>> {
        let (sql, id) = match scope {
            Scope::Match(id) => (COUNT_BY_MATCH, Some(id)),
            Scope::Session(id) => (COUNT_BY_SESSION, Some(id)),
            Scope::All => (COUNT_ALL, None),
        };
        let mut stmt = self.conn.prepare_cached(sql)?;
        let row = |r: &rusqlite::Row| Ok((r.get(0)?, r.get(1)?));
        match id {
            Some(id) => stmt.query_map(params![id], row)?.collect(),
            None => stmt.query_map([], row)?.collect(),
        }
    }

    pub fn counts(&self, scope: Scope) -> Result<Counts> {
        let mut counts = Counts::default();
        for (kind, n) in self.buckets(scope)? {
            let n = n.max(0) as u32;
            match kind.as_deref() {
                Some("normal") => counts.normal = n,
                Some("rare") => counts.rare = n,
                Some("spirit") => counts.spirit = n,
                Some("ammo") => {
                    counts.ammo = n;
                    continue; // la munición no es un cofre: no suma al total
                }
                _ => {} // sin clasificar o tipo de otra versión: solo suma al total
            }
            counts.total += n;
        }
        Ok(counts)
    }

    pub fn matches_in_session(&self, session_id: i64) -> Result<u32> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM matches WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
    }

    /// Partidas jugadas en total: las registradas más los ajustes manuales.
    pub fn total_matches(&self) -> Result<i64> {
        self.conn.query_row(
            "SELECT (SELECT COUNT(*) FROM matches)
                  + (SELECT COALESCE(SUM(delta), 0) FROM match_adjustments)",
            [],
            |r| r.get(0),
        )
    }

    /// Ajuste manual del total de partidas. Devuelve `false`, sin guardar
    /// nada, si lo dejaría por debajo de cero.
    pub fn adjust_matches(&self, now: i64, delta: i64) -> Result<bool> {
        if self.total_matches()? + delta < 0 {
            return Ok(false);
        }
        self.conn.execute(
            "INSERT INTO match_adjustments (ts, delta) VALUES (?1, ?2)",
            params![now, delta],
        )?;
        Ok(true)
    }

    /// Ajuste manual del histórico (p. ej. cofres abiertos antes de instalar
    /// la app). Devuelve `false`, sin guardar nada, si el ajuste dejaría ese
    /// tipo por debajo de cero.
    pub fn adjust_total(&self, now: i64, chest: ChestType, delta: i64) -> Result<bool> {
        let kind = Some(chest.as_str());
        let current = self
            .buckets(Scope::All)?
            .into_iter()
            .find(|(k, _)| k.as_deref() == kind)
            .map_or(0, |(_, n)| n);
        if current + delta < 0 {
            return Ok(false);
        }
        self.conn.execute(
            "INSERT INTO adjustments (ts, chest_type, delta) VALUES (?1, ?2, ?3)",
            params![now, kind, delta],
        )?;
        Ok(true)
    }
}
