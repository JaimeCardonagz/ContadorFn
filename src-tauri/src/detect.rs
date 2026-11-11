//! Máquina de estados de apertura de contenedores (ARQUITECTURA.md §6).
//!
//! Es lógica pura: recibe una `Observation` por frame analizado y decide si se
//! abrió un cofre o una caja de munición. No sabe de dónde salen las señales.
//!
//! Así funciona en el juego (medido en los vídeos de `Debug/`, 2026-11-11):
//! al apuntar aparece el recuadro "BUSCAR" junto a la mira; un **toque** de la
//! tecla arranca un anillo de progreso alrededor de la tecla que se completa
//! solo en ~0,7 s y **no se puede cancelar**; el recuadro desaparece ~0,25 s
//! después y enseguida sale, en un sitio fijo, el aviso de experiencia cuya
//! última línea dice qué se abrió: "COFRE", "COFRE RARO", "CAJA DE MUNICIÓN".
//! Ese aviso confirma la apertura y a la vez da el tipo.

use crate::store::ChestType;

pub const S1_PROMPT: u8 = 1 << 0;
pub const S2_TYPE: u8 = 1 << 1;
pub const S3_INPUT: u8 = 1 << 2;
pub const S4_PROGRESS: u8 = 1 << 3;
pub const S5_NOTICE: u8 = 1 << 4;

/// Lo que se ve en un instante.
#[derive(Debug, Clone, Copy)]
pub struct Observation {
    pub t_ms: i64,
    /// S1: el recuadro "BUSCAR" está visible junto a la mira.
    pub prompt: bool,
    /// S3: la tecla de interactuar se pulsó desde la observación anterior
    /// (basta un toque); `None` si no hay escucha de entrada disponible.
    pub key_pressed: Option<bool>,
    /// S4: llenado del anillo de progreso, de 0.0 a 1.0.
    pub progress: f32,
    /// S2 + S5: tipo que nombra el aviso de experiencia en pantalla; `None`
    /// si no hay aviso o no es de un contenedor de los que se cuentan.
    pub notice: Option<ChestType>,
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Llenado a partir del cual el anillo se da por completo.
    pub complete_at: f32,
    /// Plazo para que el anillo, una vez arrancado, se complete o desaparezca.
    pub opening_timeout_ms: i64,
    /// Tras terminar el anillo, plazo para que llegue el aviso.
    pub verify_window_ms: i64,
    /// Si al abrir ya había en pantalla un aviso (del contenedor anterior), se
    /// espera este tiempo a que cambie antes de darlo por bueno.
    pub stale_wait_ms: i64,
    /// Pausa tras contar, para que un parpadeo del recuadro no cuente dos veces.
    pub cooldown_ms: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            complete_at: 0.9,
            opening_timeout_ms: 3_000,
            verify_window_ms: 1_500,
            stale_wait_ms: 800,
            cooldown_ms: 500,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChestOpened {
    pub chest: ChestType,
    /// Máscara de bits con las señales presentes (constantes `S*`).
    pub signals: u8,
    /// Fracción de las cinco señales S1..S5 que estuvieron presentes.
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    /// El anillo arrancó pero ni se completó ni desapareció a tiempo.
    TimedOut,
    /// La apertura terminó pero no llegó ningún aviso reconocido.
    NoConfirmation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    Nothing,
    Counted(ChestOpened),
    /// Apertura empezada que no cuenta; útil para el modo depuración.
    Rejected(Rejected),
}

#[derive(Debug, Clone, Copy)]
enum State {
    Idle,
    /// Recuadro a la vista. `pressed` recuerda el toque de tecla, que llega
    /// uno o dos frames antes de que arranque el anillo.
    InView {
        pressed: bool,
    },
    /// El anillo está avanzando. `stale` es el aviso que ya había en pantalla,
    /// que es de un contenedor anterior.
    Opening {
        used_key: bool,
        stale: Option<ChestType>,
        deadline: i64,
    },
    /// El anillo terminó: se espera a que desaparezca el recuadro y al aviso.
    Verifying {
        signals: u8,
        stale: Option<ChestType>,
        prompt_gone_at: Option<i64>,
        deadline: i64,
    },
    Cooldown {
        until: i64,
    },
}

pub struct OpeningDetector {
    config: Config,
    state: State,
}

impl OpeningDetector {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            state: State::Idle,
        }
    }

    pub fn step(&mut self, obs: &Observation) -> Outcome {
        // Cada vuelta avanza como mucho un estado; un mismo frame puede hacer
        // avanzar varios.
        loop {
            match self.state {
                State::Cooldown { until } => {
                    if obs.t_ms < until {
                        return Outcome::Nothing;
                    }
                    self.state = State::Idle;
                }
                State::Idle => {
                    if !obs.prompt {
                        return Outcome::Nothing;
                    }
                    self.state = State::InView { pressed: false };
                }
                State::InView { pressed } => {
                    if !obs.prompt {
                        self.state = State::Idle;
                        return Outcome::Nothing;
                    }
                    let pressed = pressed || obs.key_pressed == Some(true);
                    // Con escucha de entrada se exige haber visto el toque;
                    // sin ella basta con que el anillo arranque.
                    let allowed = pressed || obs.key_pressed.is_none();
                    if obs.progress <= 0.0 || !allowed {
                        self.state = State::InView { pressed };
                        return Outcome::Nothing;
                    }
                    self.state = State::Opening {
                        used_key: pressed,
                        stale: obs.notice,
                        deadline: obs.t_ms + self.config.opening_timeout_ms,
                    };
                }
                State::Opening {
                    used_key, deadline, ..
                } => {
                    let completed = obs.progress >= self.config.complete_at;
                    // La apertura no se puede cancelar: si el recuadro deja de
                    // verse (el jugador giró la cámara), se abre igual.
                    if completed || !obs.prompt {
                        let mut signals = S1_PROMPT;
                        if used_key {
                            signals |= S3_INPUT;
                        }
                        if completed {
                            signals |= S4_PROGRESS;
                        }
                        self.state = State::Verifying {
                            signals,
                            stale: obs.notice,
                            prompt_gone_at: None,
                            deadline: obs.t_ms + self.config.verify_window_ms,
                        };
                        continue;
                    }
                    if obs.t_ms >= deadline {
                        self.state = State::Cooldown {
                            until: obs.t_ms + self.config.cooldown_ms,
                        };
                        return Outcome::Rejected(Rejected::TimedOut);
                    }
                    self.state = State::Opening {
                        used_key,
                        stale: obs.notice,
                        deadline,
                    };
                    return Outcome::Nothing;
                }
                State::Verifying {
                    signals,
                    stale,
                    mut prompt_gone_at,
                    deadline,
                } => {
                    if !obs.prompt && prompt_gone_at.is_none() {
                        prompt_gone_at = Some(obs.t_ms);
                    }
                    // Un aviso distinto del que ya había es el de esta
                    // apertura. Si es igual puede ser el anterior aún visible:
                    // se le da un margen por si cambia y luego se acepta.
                    let confirmed = match (prompt_gone_at, obs.notice) {
                        (Some(gone_at), Some(kind)) => (Some(kind) != stale
                            || obs.t_ms - gone_at >= self.config.stale_wait_ms)
                            .then_some(kind),
                        _ => None,
                    };
                    if let Some(chest) = confirmed {
                        self.state = State::Cooldown {
                            until: obs.t_ms + self.config.cooldown_ms,
                        };
                        return Outcome::Counted(opened(chest, signals | S2_TYPE | S5_NOTICE));
                    }
                    if obs.t_ms >= deadline {
                        // Se entra en pausa y no en reposo: si el recuadro
                        // sigue en pantalla, es la misma aparición descartada.
                        self.state = State::Cooldown {
                            until: obs.t_ms + self.config.cooldown_ms,
                        };
                        return Outcome::Rejected(Rejected::NoConfirmation);
                    }
                    self.state = State::Verifying {
                        signals,
                        stale,
                        prompt_gone_at,
                        deadline,
                    };
                    return Outcome::Nothing;
                }
            }
        }
    }
}

fn opened(chest: ChestType, signals: u8) -> ChestOpened {
    ChestOpened {
        chest,
        signals,
        confidence: (signals & 0b1_1111).count_ones() as f32 / 5.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ChestType::{Ammo, Normal, Rare, Spirit};

    /// Guion de frames a 30 fps con los tiempos medidos en el juego.
    struct Sim {
        det: OpeningDetector,
        t: i64,
        counted: Vec<ChestOpened>,
        rejected: Vec<Rejected>,
    }

    const NOTHING: Observation = Observation {
        t_ms: 0,
        prompt: false,
        key_pressed: Some(false),
        progress: 0.0,
        notice: None,
    };
    /// Apuntando a algo que se puede buscar.
    const PROMPT: Observation = Observation {
        prompt: true,
        ..NOTHING
    };
    const TAP: Observation = Observation {
        key_pressed: Some(true),
        ..PROMPT
    };

    fn notice(kind: ChestType) -> Observation {
        Observation {
            notice: Some(kind),
            ..NOTHING
        }
    }

    impl Sim {
        fn new() -> Self {
            Self {
                det: OpeningDetector::new(Config::default()),
                t: 0,
                counted: vec![],
                rejected: vec![],
            }
        }

        fn frame(&mut self, obs: Observation) {
            self.t += 33;
            match self.det.step(&Observation {
                t_ms: self.t,
                ..obs
            }) {
                Outcome::Counted(c) => self.counted.push(c),
                Outcome::Rejected(r) => self.rejected.push(r),
                Outcome::Nothing => {}
            }
        }

        fn frames(&mut self, n: usize, obs: Observation) {
            for _ in 0..n {
                self.frame(obs);
            }
        }

        /// El anillo sube hasta `to` en ~0,7 s (22 frames) sobre `base`.
        fn ring(&mut self, base: Observation, to: f32) {
            for i in 1..=22 {
                self.frame(Observation {
                    progress: to * i as f32 / 22.0,
                    ..base
                });
            }
        }

        /// Toque, anillo completo, el recuadro se queda ~0,25 s y desaparece.
        /// `showing` es el aviso que haya en pantalla mientras tanto.
        fn tap_and_ring(&mut self, showing: Option<ChestType>) {
            let prompt = Observation {
                notice: showing,
                ..PROMPT
            };
            self.frame(Observation {
                notice: showing,
                ..TAP
            });
            self.ring(prompt, 1.0);
            self.frames(7, prompt);
        }

        /// Apertura real completa de un contenedor de tipo `kind`.
        fn open(&mut self, kind: ChestType) {
            self.tap_and_ring(None);
            self.frames(10, NOTHING);
            self.frames(60, notice(kind));
        }

        fn kinds(&self) -> Vec<ChestType> {
            self.counted.iter().map(|c| c.chest).collect()
        }
    }

    #[test]
    fn un_toque_y_el_anillo_completo_cuentan_una_vez_con_todas_las_senales() {
        let mut s = Sim::new();
        s.frames(15, PROMPT);
        s.open(Rare);
        s.frames(60, NOTHING);

        assert_eq!(s.rejected, []);
        assert_eq!(s.counted.len(), 1);
        let c = s.counted[0];
        assert_eq!(c.chest, Rare);
        assert_eq!(
            c.signals,
            S1_PROMPT | S2_TYPE | S3_INPUT | S4_PROGRESS | S5_NOTICE
        );
        assert_eq!(c.confidence, 1.0);
    }

    #[test]
    fn el_tipo_lo_da_el_texto_del_aviso() {
        let mut s = Sim::new();
        for kind in [Normal, Rare, Spirit, Ammo] {
            s.open(kind);
        }
        assert_eq!(s.kinds(), [Normal, Rare, Spirit, Ammo]);
        assert_eq!(s.rejected, []);
    }

    #[test]
    fn pulsar_la_tecla_sin_apuntar_a_nada_no_cuenta() {
        let mut s = Sim::new();
        s.frames(
            120,
            Observation {
                key_pressed: Some(true),
                ..notice(Normal)
            },
        );
        assert!(s.counted.is_empty() && s.rejected.is_empty());
    }

    #[test]
    fn un_contenedor_sin_aviso_reconocido_no_cuenta() {
        // Otro objeto con "BUSCAR" (nevera, barril): hay anillo pero el aviso
        // no nombra ningún tipo de los que se cuentan.
        let mut s = Sim::new();
        s.tap_and_ring(None);
        s.frames(90, NOTHING);
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::NoConfirmation]);
    }

    #[test]
    fn pulsar_frente_al_cofre_sin_que_arranque_el_anillo_no_cuenta() {
        let mut s = Sim::new();
        s.frames(30, TAP);
        s.frames(10, NOTHING);
        s.frames(30, notice(Normal));
        assert!(s.counted.is_empty() && s.rejected.is_empty());
    }

    #[test]
    fn un_anillo_sin_haber_visto_la_tecla_no_cuenta() {
        // Hay escucha de teclado y no vio ningún toque: no se da por bueno.
        let mut s = Sim::new();
        s.ring(PROMPT, 1.0);
        s.frames(10, NOTHING);
        s.frames(30, notice(Normal));
        assert!(s.counted.is_empty() && s.rejected.is_empty());
    }

    #[test]
    fn mirar_un_cofre_con_el_aviso_del_anterior_en_pantalla_no_cuenta() {
        let mut s = Sim::new();
        s.frames(
            90,
            Observation {
                notice: Some(Normal),
                ..PROMPT
            },
        );
        assert!(s.counted.is_empty() && s.rejected.is_empty());
    }

    #[test]
    fn un_aviso_fuera_de_plazo_no_cuenta() {
        let mut s = Sim::new();
        s.tap_and_ring(None);
        s.frames(50, NOTHING); // 1,65 s desde que terminó el anillo
        s.frames(5, notice(Normal));
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::NoConfirmation]);
    }

    #[test]
    fn girar_la_camara_durante_el_anillo_no_cancela_la_apertura() {
        let mut s = Sim::new();
        s.frame(TAP);
        s.ring(PROMPT, 0.5);
        s.frames(25, NOTHING); // deja de verse el recuadro; se abre igual
        s.frames(30, notice(Spirit));

        assert_eq!(s.counted.len(), 1);
        let c = s.counted[0];
        assert_eq!(c.chest, Spirit);
        assert_eq!(c.signals & S4_PROGRESS, 0, "no se vio el anillo completo");
        assert_eq!(c.confidence, 0.8);
    }

    #[test]
    fn un_anillo_que_no_termina_nunca_caduca_sin_contar() {
        let mut s = Sim::new();
        s.frame(TAP);
        s.frames(
            150,
            Observation {
                progress: 0.4,
                notice: Some(Normal),
                ..PROMPT
            },
        );
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::TimedOut]);
    }

    #[test]
    fn recuadro_que_nunca_desaparece_no_cuenta_aunque_salga_el_aviso() {
        let mut s = Sim::new();
        s.frame(TAP);
        s.ring(PROMPT, 1.0);
        s.frames(
            200,
            Observation {
                progress: 1.0,
                notice: Some(Normal),
                ..PROMPT
            },
        );
        assert!(s.counted.is_empty());
    }

    #[test]
    fn el_parpadeo_del_recuadro_tras_contar_no_cuenta_dos_veces() {
        let mut s = Sim::new();
        s.tap_and_ring(None);
        s.frames(3, NOTHING);
        s.frame(notice(Normal));
        // El recuadro reaparece un instante con el aviso todavía en pantalla.
        s.frames(
            8,
            Observation {
                progress: 1.0,
                notice: Some(Normal),
                ..TAP
            },
        );
        s.frames(30, notice(Normal));
        assert_eq!(s.counted.len(), 1);
    }

    #[test]
    fn con_el_aviso_anterior_aun_visible_se_espera_al_nuevo_para_el_tipo() {
        // Caja de munición y, enseguida, un cofre raro: mientras se abre el
        // cofre sigue en pantalla "CAJA DE MUNICIÓN".
        let mut s = Sim::new();
        s.tap_and_ring(None);
        s.frames(10, NOTHING);
        s.frames(20, notice(Ammo));

        s.frames(
            5,
            Observation {
                notice: Some(Ammo),
                ..PROMPT
            },
        );
        s.tap_and_ring(Some(Ammo));
        s.frames(8, notice(Ammo)); // el aviso viejo todavía no ha cambiado
        s.frames(30, notice(Rare));

        assert_eq!(s.kinds(), [Ammo, Rare]);
        assert_eq!(s.rejected, []);
    }

    #[test]
    fn dos_seguidos_del_mismo_tipo_cuentan_dos_aunque_el_aviso_no_cambie() {
        let mut s = Sim::new();
        s.tap_and_ring(None);
        s.frames(10, NOTHING);
        s.frames(20, notice(Normal));

        s.frames(
            5,
            Observation {
                notice: Some(Normal),
                ..PROMPT
            },
        );
        s.tap_and_ring(Some(Normal));
        s.frames(40, notice(Normal));

        assert_eq!(s.kinds(), [Normal, Normal]);
        assert_eq!(s.rejected, []);
    }

    #[test]
    fn sin_escucha_de_teclado_cuenta_con_las_otras_cuatro_senales() {
        let mut s = Sim::new();
        let prompt = Observation {
            key_pressed: None,
            ..PROMPT
        };
        let nothing = Observation {
            key_pressed: None,
            ..NOTHING
        };
        s.frames(30, prompt); // mirar el cofre sin abrirlo no hace nada
        assert!(s.counted.is_empty() && s.rejected.is_empty());

        s.ring(prompt, 1.0);
        s.frames(10, nothing);
        s.frames(
            30,
            Observation {
                notice: Some(Normal),
                ..nothing
            },
        );
        assert_eq!(s.counted.len(), 1);
        let c = s.counted[0];
        assert_eq!(c.signals & S3_INPUT, 0);
        assert_eq!(c.confidence, 0.8);
    }
}
