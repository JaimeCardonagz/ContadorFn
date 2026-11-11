//! Máquina de estados de apertura de cofres (ARQUITECTURA.md §6).
//!
//! Es lógica pura: recibe una `Observation` por frame analizado y decide si se
//! abrió un cofre. No sabe de dónde salen las señales (captura, audio, entrada).

use crate::store::ChestType;

pub const S1_PROMPT: u8 = 1 << 0;
pub const S2_COLOR: u8 = 1 << 1;
pub const S3_INPUT: u8 = 1 << 2;
pub const S4_PROGRESS: u8 = 1 << 3;
pub const S5_AUDIO: u8 = 1 << 4;
pub const S5_VISUAL: u8 = 1 << 5;

/// Lo que se ve y se oye en un instante.
#[derive(Debug, Clone, Copy)]
pub struct Observation {
    pub t_ms: i64,
    /// S1: el prompt de interacción está visible.
    pub prompt: bool,
    /// S2: tipo de cofre en la mira; `None` si no es un cofre (caja de
    /// munición, puerta, nada reconocible).
    pub chest: Option<ChestType>,
    /// S3: tecla/botón de interactuar mantenido; `None` si no hay escucha de
    /// entrada disponible.
    pub key_held: Option<bool>,
    /// S4: llenado del indicador de progreso, de 0.0 a 1.0.
    pub progress: f32,
    /// S5: se reconoció el sonido de apertura.
    pub audio_confirm: bool,
    /// S5: la mira muestra el cofre ya abierto / el botín.
    pub visual_confirm: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Llenado a partir del cual el indicador se da por completo.
    pub complete_at: f32,
    /// Tras completarse, plazo para que desaparezca el prompt y llegue S5.
    pub verify_window_ms: i64,
    /// Pausa tras contar, para que un parpadeo del prompt no cuente dos veces.
    pub cooldown_ms: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            complete_at: 0.95,
            verify_window_ms: 1_000,
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
    /// Se soltó la tecla antes de completar.
    KeyReleased,
    /// El prompt desapareció antes de completar.
    PromptLost,
    /// La mira dejó de apuntar a un cofre antes de completar.
    TargetChanged,
    /// El indicador se completó pero no llegó ninguna confirmación (S5).
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
    /// COFRE_A_LA_VISTA: el tipo queda fijado aquí.
    InView {
        chest: ChestType,
    },
    /// ABRIENDO, con el indicador aún sin completar.
    Opening {
        chest: ChestType,
        used_key: bool,
    },
    /// Indicador completo: se espera a que desaparezca el prompt y a S5.
    Verifying {
        chest: ChestType,
        used_key: bool,
        prompt_gone: bool,
        confirm: u8,
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
        // avanzar varios (p. ej. ver el cofre con la tecla ya pulsada).
        loop {
            match self.state {
                State::Cooldown { until } => {
                    if obs.t_ms < until {
                        return Outcome::Nothing;
                    }
                    self.state = State::Idle;
                }
                State::Idle => match obs.chest {
                    Some(chest) if obs.prompt => self.state = State::InView { chest },
                    _ => return Outcome::Nothing,
                },
                State::InView { chest } => {
                    if !obs.prompt || obs.chest.is_none() {
                        self.state = State::Idle;
                        return Outcome::Nothing;
                    }
                    // Sin escucha de entrada, la apertura se reconoce porque
                    // el indicador empieza a llenarse.
                    let started = obs.key_held.unwrap_or(obs.progress > 0.0);
                    if !started {
                        return Outcome::Nothing;
                    }
                    self.state = State::Opening {
                        chest,
                        used_key: obs.key_held == Some(true),
                    };
                }
                State::Opening { chest, used_key } => {
                    if obs.progress >= self.config.complete_at {
                        self.state = State::Verifying {
                            chest,
                            used_key,
                            prompt_gone: false,
                            confirm: 0,
                            deadline: obs.t_ms + self.config.verify_window_ms,
                        };
                        continue;
                    }
                    let rejected = if obs.key_held == Some(false) {
                        Rejected::KeyReleased
                    } else if !obs.prompt {
                        Rejected::PromptLost
                    } else if obs.chest.is_none() {
                        Rejected::TargetChanged
                    } else {
                        return Outcome::Nothing;
                    };
                    self.state = State::Idle;
                    return Outcome::Rejected(rejected);
                }
                State::Verifying {
                    chest,
                    used_key,
                    mut prompt_gone,
                    mut confirm,
                    deadline,
                } => {
                    prompt_gone |= !obs.prompt;
                    if obs.audio_confirm {
                        confirm |= S5_AUDIO;
                    }
                    if obs.visual_confirm {
                        confirm |= S5_VISUAL;
                    }
                    if prompt_gone && confirm != 0 {
                        self.state = State::Cooldown {
                            until: obs.t_ms + self.config.cooldown_ms,
                        };
                        return Outcome::Counted(opened(chest, used_key, confirm));
                    }
                    if obs.t_ms >= deadline {
                        // Se entra en pausa y no en reposo: si el prompt sigue
                        // en pantalla, es la misma aparición ya descartada.
                        self.state = State::Cooldown {
                            until: obs.t_ms + self.config.cooldown_ms,
                        };
                        return Outcome::Rejected(Rejected::NoConfirmation);
                    }
                    self.state = State::Verifying {
                        chest,
                        used_key,
                        prompt_gone,
                        confirm,
                        deadline,
                    };
                    return Outcome::Nothing;
                }
            }
        }
    }
}

fn opened(chest: ChestType, used_key: bool, confirm: u8) -> ChestOpened {
    let mut signals = S1_PROMPT | S2_COLOR | S4_PROGRESS | confirm;
    if used_key {
        signals |= S3_INPUT;
    }
    // S1, S2, S4 y S5 están siempre presentes al contar; S3 es la única opcional.
    let present = if used_key { 5.0 } else { 4.0 };
    ChestOpened {
        chest,
        signals,
        confidence: present / 5.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guion de frames a 30 fps sobre un detector con la configuración por defecto.
    struct Sim {
        det: OpeningDetector,
        t: i64,
        counted: Vec<ChestOpened>,
        rejected: Vec<Rejected>,
    }

    const NOTHING: Observation = Observation {
        t_ms: 0,
        prompt: false,
        chest: None,
        key_held: Some(false),
        progress: 0.0,
        audio_confirm: false,
        visual_confirm: false,
    };

    fn looking_at(chest: ChestType) -> Observation {
        Observation {
            prompt: true,
            chest: Some(chest),
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

        /// Mantener la tecla frente a `base` mientras el indicador sube hasta `to`.
        fn hold(&mut self, base: Observation, to: f32) {
            for i in 1..=10 {
                self.frame(Observation {
                    key_held: base.key_held.map(|_| true),
                    progress: to * i as f32 / 10.0,
                    ..base
                });
            }
        }
    }

    #[test]
    fn apertura_completa_cuenta_una_vez_con_todas_las_senales() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Rare);
        s.frames(5, chest);
        s.hold(chest, 1.0);
        s.frame(Observation {
            audio_confirm: true,
            ..NOTHING
        });
        s.frames(60, NOTHING);

        assert_eq!(s.rejected, []);
        assert_eq!(s.counted.len(), 1);
        let c = s.counted[0];
        assert_eq!(c.chest, ChestType::Rare);
        assert_eq!(
            c.signals,
            S1_PROMPT | S2_COLOR | S3_INPUT | S4_PROGRESS | S5_AUDIO
        );
        assert_eq!(c.confidence, 1.0);
    }

    #[test]
    fn la_confirmacion_visual_tambien_vale() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.hold(chest, 1.0);
        s.frames(3, NOTHING);
        s.frame(Observation {
            visual_confirm: true,
            ..NOTHING
        });
        assert_eq!(s.counted.len(), 1);
        assert_eq!(s.counted[0].signals & (S5_AUDIO | S5_VISUAL), S5_VISUAL);
    }

    #[test]
    fn mantener_la_tecla_sin_apuntar_a_nada_no_cuenta() {
        let mut s = Sim::new();
        s.frames(
            120,
            Observation {
                key_held: Some(true),
                audio_confirm: true,
                ..NOTHING
            },
        );
        assert!(s.counted.is_empty() && s.rejected.is_empty());
    }

    #[test]
    fn caja_de_municion_o_puerta_no_cuenta() {
        // Hay prompt y el indicador se llena, pero el color no es de cofre.
        let mut s = Sim::new();
        let ammo = Observation {
            prompt: true,
            ..NOTHING
        };
        s.hold(ammo, 1.0);
        s.frame(Observation {
            audio_confirm: true,
            visual_confirm: true,
            ..NOTHING
        });
        assert!(s.counted.is_empty() && s.rejected.is_empty());
    }

    #[test]
    fn soltar_antes_de_completar_no_cuenta() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.hold(chest, 0.6);
        s.frames(5, chest); // suelta, el prompt sigue ahí
        s.frames(
            30,
            Observation {
                audio_confirm: true,
                ..NOTHING
            },
        );
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::KeyReleased]);
    }

    #[test]
    fn alejarse_a_mitad_no_cuenta() {
        let mut s = Sim::new();
        s.hold(looking_at(ChestType::Normal), 0.6);
        s.frames(
            30,
            Observation {
                key_held: Some(true),
                audio_confirm: true,
                ..NOTHING
            },
        );
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::PromptLost]);
    }

    #[test]
    fn dejar_de_apuntar_al_cofre_a_mitad_no_cuenta() {
        let mut s = Sim::new();
        s.hold(looking_at(ChestType::Normal), 0.6);
        s.frame(Observation {
            prompt: true,
            key_held: Some(true),
            progress: 0.7,
            ..NOTHING
        });
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::TargetChanged]);
    }

    #[test]
    fn sin_confirmacion_posterior_no_cuenta() {
        let mut s = Sim::new();
        s.hold(looking_at(ChestType::Normal), 1.0);
        s.frames(60, NOTHING);
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::NoConfirmation]);
    }

    #[test]
    fn una_confirmacion_fuera_de_plazo_no_cuenta() {
        let mut s = Sim::new();
        s.hold(looking_at(ChestType::Normal), 1.0);
        s.frames(40, NOTHING); // 1,3 s > ventana de 1 s
        s.frames(
            5,
            Observation {
                audio_confirm: true,
                ..NOTHING
            },
        );
        assert!(s.counted.is_empty());
    }

    #[test]
    fn un_sonido_antes_de_completar_no_sirve_de_confirmacion() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.frames(
            5,
            Observation {
                audio_confirm: true,
                ..chest
            },
        );
        s.hold(
            Observation {
                audio_confirm: true,
                ..chest
            },
            0.9,
        );
        s.hold(chest, 1.0); // se completa ya sin sonido
        s.frames(60, NOTHING);
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::NoConfirmation]);
    }

    #[test]
    fn prompt_que_nunca_desaparece_no_cuenta_aunque_suene() {
        // Indicador "completo" pero el prompt sigue: no es una apertura real.
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.hold(chest, 1.0);
        s.frames(
            200,
            Observation {
                key_held: Some(true),
                progress: 1.0,
                audio_confirm: true,
                ..chest
            },
        );
        assert!(s.counted.is_empty());
    }

    #[test]
    fn el_parpadeo_del_prompt_tras_contar_no_cuenta_dos_veces() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.hold(chest, 1.0);
        s.frame(Observation {
            audio_confirm: true,
            ..NOTHING
        });
        // El prompt reaparece un instante con la tecla aún pulsada y el eco del sonido.
        s.frames(
            8,
            Observation {
                key_held: Some(true),
                progress: 1.0,
                audio_confirm: true,
                ..chest
            },
        );
        s.frames(
            30,
            Observation {
                audio_confirm: true,
                ..NOTHING
            },
        );
        assert_eq!(s.counted.len(), 1);
    }

    #[test]
    fn dos_cofres_seguidos_cuentan_dos_con_su_tipo() {
        let mut s = Sim::new();
        for kind in [ChestType::Normal, ChestType::Spirit] {
            let chest = looking_at(kind);
            s.frames(5, chest);
            s.hold(chest, 1.0);
            s.frame(Observation {
                audio_confirm: true,
                ..NOTHING
            });
            s.frames(60, NOTHING);
        }
        let kinds: Vec<_> = s.counted.iter().map(|c| c.chest).collect();
        assert_eq!(kinds, [ChestType::Normal, ChestType::Spirit]);
    }

    #[test]
    fn el_tipo_se_fija_al_ver_el_cofre_aunque_el_color_cambie_al_abrirse() {
        let mut s = Sim::new();
        s.frames(5, looking_at(ChestType::Boss));
        s.hold(looking_at(ChestType::Normal), 1.0);
        s.frame(Observation {
            visual_confirm: true,
            ..NOTHING
        });
        assert_eq!(s.counted[0].chest, ChestType::Boss);
    }

    #[test]
    fn sin_escucha_de_teclado_cuenta_con_las_otras_cuatro_senales() {
        let mut s = Sim::new();
        let chest = Observation {
            key_held: None,
            ..looking_at(ChestType::Normal)
        };
        s.frames(30, chest); // mirar el cofre sin abrirlo no hace nada
        assert!(s.counted.is_empty() && s.rejected.is_empty());

        s.hold(chest, 1.0);
        s.frame(Observation {
            key_held: None,
            audio_confirm: true,
            ..NOTHING
        });
        assert_eq!(s.counted.len(), 1);
        let c = s.counted[0];
        assert_eq!(c.signals & S3_INPUT, 0);
        assert_eq!(c.confidence, 0.8);
    }
}
