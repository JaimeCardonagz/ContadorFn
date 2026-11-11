//! Máquina de estados de apertura de contenedores (ARQUITECTURA.md §6).
//!
//! Es lógica pura: recibe una `Observation` por frame analizado y decide si se
//! abrió un cofre o una caja de munición. No sabe de dónde salen las señales.
//!
//! Así funciona en el juego (medido en `Debug/Cofres Prueba.mp4`, 2026-11-11):
//! al apuntar aparece el prompt "BUSCAR" junto a la mira; un **toque** de la
//! tecla arranca un anillo de progreso alrededor de la tecla que se completa
//! solo en ~0,7 s y **no se puede cancelar**; el prompt desaparece ~0,25 s
//! después y ~0,35 s más tarde salen los avisos de experiencia.

use crate::store::ChestType;

pub const S1_PROMPT: u8 = 1 << 0;
pub const S2_TYPE: u8 = 1 << 1;
pub const S3_INPUT: u8 = 1 << 2;
pub const S4_PROGRESS: u8 = 1 << 3;
pub const S5_AUDIO: u8 = 1 << 4;
pub const S5_VISUAL: u8 = 1 << 5;

/// Lo que se ve y se oye en un instante.
#[derive(Debug, Clone, Copy)]
pub struct Observation {
    pub t_ms: i64,
    /// S1: el prompt "BUSCAR" está visible junto a la mira.
    pub prompt: bool,
    /// S2: qué contenedor es; `None` si no es ninguno de los que se cuentan
    /// (puerta, arma en el suelo, nada reconocible).
    pub chest: Option<ChestType>,
    /// S3: la tecla de interactuar se pulsó desde la observación anterior
    /// (basta un toque); `None` si no hay escucha de entrada disponible.
    pub key_pressed: Option<bool>,
    /// S4: llenado del anillo de progreso, de 0.0 a 1.0.
    pub progress: f32,
    /// S5: se reconoció el sonido de apertura.
    pub audio_confirm: bool,
    /// S5: está en pantalla el aviso de apertura ("+PE COFRE",
    /// "CONTENEDOR ABIERTO").
    pub visual_confirm: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Llenado a partir del cual el anillo se da por completo.
    pub complete_at: f32,
    /// Plazo para que el anillo, una vez arrancado, se complete o desaparezca.
    pub opening_timeout_ms: i64,
    /// Tras completarse, plazo para que desaparezca el prompt y llegue S5.
    pub verify_window_ms: i64,
    /// Pausa tras contar, para que un parpadeo del prompt no cuente dos veces.
    pub cooldown_ms: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            complete_at: 0.9,
            opening_timeout_ms: 3_000,
            verify_window_ms: 1_500,
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
    /// La apertura terminó pero no llegó ninguna confirmación (S5).
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
    /// A la vista: el tipo queda fijado aquí. `pressed` recuerda el toque de
    /// tecla, que llega uno o dos frames antes de que arranque el anillo.
    InView {
        chest: ChestType,
        pressed: bool,
    },
    /// El anillo está avanzando.
    Opening {
        chest: ChestType,
        used_key: bool,
        deadline: i64,
    },
    /// La apertura terminó: se espera a que desaparezca el prompt y a S5.
    Verifying {
        chest: ChestType,
        signals: u8,
        prompt_gone: bool,
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
                State::Idle => match obs.chest {
                    Some(chest) if obs.prompt => {
                        self.state = State::InView {
                            chest,
                            pressed: false,
                        }
                    }
                    _ => return Outcome::Nothing,
                },
                State::InView { chest, pressed } => {
                    if !obs.prompt || obs.chest.is_none() {
                        self.state = State::Idle;
                        return Outcome::Nothing;
                    }
                    let pressed = pressed || obs.key_pressed == Some(true);
                    // Con escucha de entrada se exige haber visto el toque;
                    // sin ella basta con que el anillo arranque.
                    let allowed = pressed || obs.key_pressed.is_none();
                    if obs.progress <= 0.0 || !allowed {
                        self.state = State::InView { chest, pressed };
                        return Outcome::Nothing;
                    }
                    self.state = State::Opening {
                        chest,
                        used_key: pressed,
                        deadline: obs.t_ms + self.config.opening_timeout_ms,
                    };
                }
                State::Opening {
                    chest,
                    used_key,
                    deadline,
                } => {
                    let completed = obs.progress >= self.config.complete_at;
                    // La apertura no se puede cancelar: si el prompt deja de
                    // verse (el jugador giró la cámara), el cofre se abre igual.
                    if completed || !obs.prompt {
                        let mut signals = S1_PROMPT | S2_TYPE;
                        if used_key {
                            signals |= S3_INPUT;
                        }
                        if completed {
                            signals |= S4_PROGRESS;
                        }
                        self.state = State::Verifying {
                            chest,
                            signals,
                            prompt_gone: false,
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
                    return Outcome::Nothing;
                }
                State::Verifying {
                    chest,
                    mut signals,
                    mut prompt_gone,
                    deadline,
                } => {
                    prompt_gone |= !obs.prompt;
                    if obs.audio_confirm {
                        signals |= S5_AUDIO;
                    }
                    if obs.visual_confirm {
                        signals |= S5_VISUAL;
                    }
                    let confirmed = signals & (S5_AUDIO | S5_VISUAL) != 0;
                    if prompt_gone && confirmed {
                        self.state = State::Cooldown {
                            until: obs.t_ms + self.config.cooldown_ms,
                        };
                        return Outcome::Counted(opened(chest, signals));
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
                        signals,
                        prompt_gone,
                        deadline,
                    };
                    return Outcome::Nothing;
                }
            }
        }
    }
}

fn opened(chest: ChestType, signals: u8) -> ChestOpened {
    // S5 cuenta como una sola señal aunque lleguen sonido e imagen.
    let present = [
        S1_PROMPT,
        S2_TYPE,
        S3_INPUT,
        S4_PROGRESS,
        S5_AUDIO | S5_VISUAL,
    ]
    .iter()
    .filter(|&&bit| signals & bit != 0)
    .count();
    ChestOpened {
        chest,
        signals,
        confidence: present as f32 / 5.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        chest: None,
        key_pressed: Some(false),
        progress: 0.0,
        audio_confirm: false,
        visual_confirm: false,
    };
    const POPUP: Observation = Observation {
        visual_confirm: true,
        ..NOTHING
    };
    const TAP: Option<bool> = Some(true);

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

        /// El anillo sube hasta `to` en ~0,7 s (22 frames) mirando a `base`.
        fn ring(&mut self, base: Observation, to: f32) {
            for i in 1..=22 {
                self.frame(Observation {
                    progress: to * i as f32 / 22.0,
                    ..base
                });
            }
        }

        /// Apertura real: toque, anillo completo, el prompt se queda ~0,25 s,
        /// desaparece y ~0,35 s después sale el aviso.
        fn open(&mut self, base: Observation) {
            self.frame(Observation {
                key_pressed: base.key_pressed.map(|_| true),
                ..base
            });
            self.ring(base, 1.0);
            self.frames(7, base);
            self.frames(
                10,
                Observation {
                    key_pressed: base.key_pressed,
                    ..NOTHING
                },
            );
            self.frames(
                60,
                Observation {
                    key_pressed: base.key_pressed,
                    ..POPUP
                },
            );
        }
    }

    #[test]
    fn un_toque_y_el_anillo_completo_cuentan_una_vez_con_todas_las_senales() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Rare);
        s.frames(15, chest);
        s.open(chest);
        s.frames(60, NOTHING);

        assert_eq!(s.rejected, []);
        assert_eq!(s.counted.len(), 1);
        let c = s.counted[0];
        assert_eq!(c.chest, ChestType::Rare);
        assert_eq!(
            c.signals,
            S1_PROMPT | S2_TYPE | S3_INPUT | S4_PROGRESS | S5_VISUAL
        );
        assert_eq!(c.confidence, 1.0);
    }

    #[test]
    fn la_tecla_no_hace_falta_mantenerla() {
        // Tras el toque la tecla está suelta durante todo el anillo.
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.frame(Observation {
            key_pressed: TAP,
            ..chest
        });
        s.ring(chest, 1.0);
        s.frames(10, NOTHING);
        s.frame(POPUP);
        assert_eq!(s.counted.len(), 1);
        assert_ne!(s.counted[0].signals & S3_INPUT, 0);
    }

    #[test]
    fn la_confirmacion_por_sonido_tambien_vale() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.frame(Observation {
            key_pressed: TAP,
            ..chest
        });
        s.ring(chest, 1.0);
        s.frames(3, NOTHING);
        s.frame(Observation {
            audio_confirm: true,
            ..NOTHING
        });
        assert_eq!(s.counted.len(), 1);
        assert_eq!(s.counted[0].signals & (S5_AUDIO | S5_VISUAL), S5_AUDIO);
    }

    #[test]
    fn la_caja_de_municion_cuenta_como_municion() {
        let mut s = Sim::new();
        s.open(looking_at(ChestType::Ammo));
        assert_eq!(s.counted.len(), 1);
        assert_eq!(s.counted[0].chest, ChestType::Ammo);
    }

    #[test]
    fn pulsar_la_tecla_sin_apuntar_a_nada_no_cuenta() {
        let mut s = Sim::new();
        s.frames(
            120,
            Observation {
                key_pressed: TAP,
                ..POPUP
            },
        );
        assert!(s.counted.is_empty() && s.rejected.is_empty());
    }

    #[test]
    fn una_puerta_u_otro_objeto_no_cuenta() {
        // Hay prompt, anillo y aviso, pero no es un contenedor reconocido.
        let mut s = Sim::new();
        s.open(Observation {
            prompt: true,
            ..NOTHING
        });
        assert!(s.counted.is_empty() && s.rejected.is_empty());
    }

    #[test]
    fn pulsar_frente_al_cofre_sin_que_arranque_el_anillo_no_cuenta() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.frames(
            30,
            Observation {
                key_pressed: TAP,
                ..chest
            },
        );
        s.frames(10, NOTHING);
        s.frames(30, POPUP);
        assert!(s.counted.is_empty() && s.rejected.is_empty());
    }

    #[test]
    fn un_anillo_sin_haber_visto_la_tecla_no_cuenta() {
        // Hay escucha de teclado y no vio ningún toque: no se da por bueno.
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.ring(chest, 1.0);
        s.frames(10, NOTHING);
        s.frames(30, POPUP);
        assert!(s.counted.is_empty() && s.rejected.is_empty());
    }

    #[test]
    fn mirar_un_cofre_con_el_aviso_del_anterior_en_pantalla_no_cuenta() {
        let mut s = Sim::new();
        s.frames(
            90,
            Observation {
                visual_confirm: true,
                ..looking_at(ChestType::Normal)
            },
        );
        assert!(s.counted.is_empty() && s.rejected.is_empty());
    }

    #[test]
    fn sin_confirmacion_posterior_no_cuenta() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.frame(Observation {
            key_pressed: TAP,
            ..chest
        });
        s.ring(chest, 1.0);
        s.frames(90, NOTHING);
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::NoConfirmation]);
    }

    #[test]
    fn una_confirmacion_fuera_de_plazo_no_cuenta() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.frame(Observation {
            key_pressed: TAP,
            ..chest
        });
        s.ring(chest, 1.0);
        s.frames(50, NOTHING); // 1,65 s > ventana de 1,5 s
        s.frames(5, POPUP);
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::NoConfirmation]);
    }

    #[test]
    fn un_aviso_antes_de_que_termine_el_anillo_no_sirve_de_confirmacion() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.frame(Observation {
            key_pressed: TAP,
            visual_confirm: true,
            ..chest
        });
        s.ring(
            Observation {
                visual_confirm: true,
                ..chest
            },
            0.8,
        );
        s.ring(chest, 1.0);
        s.frames(90, NOTHING);
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::NoConfirmation]);
    }

    #[test]
    fn girar_la_camara_durante_el_anillo_no_cancela_la_apertura() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Spirit);
        s.frame(Observation {
            key_pressed: TAP,
            ..chest
        });
        s.ring(chest, 0.5);
        s.frames(25, NOTHING); // deja de verse el prompt; el cofre se abre igual
        s.frames(30, POPUP);

        assert_eq!(s.counted.len(), 1);
        let c = s.counted[0];
        assert_eq!(c.chest, ChestType::Spirit);
        assert_eq!(c.signals & S4_PROGRESS, 0, "no se vio el anillo completo");
        assert_eq!(c.confidence, 0.8);
    }

    #[test]
    fn un_anillo_que_no_termina_nunca_caduca_sin_contar() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.frame(Observation {
            key_pressed: TAP,
            ..chest
        });
        s.frames(
            150,
            Observation {
                progress: 0.4,
                visual_confirm: true,
                ..chest
            },
        );
        assert!(s.counted.is_empty());
        assert_eq!(s.rejected, [Rejected::TimedOut]);
    }

    #[test]
    fn prompt_que_nunca_desaparece_no_cuenta_aunque_salga_el_aviso() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.frame(Observation {
            key_pressed: TAP,
            ..chest
        });
        s.ring(chest, 1.0);
        s.frames(
            200,
            Observation {
                progress: 1.0,
                visual_confirm: true,
                ..chest
            },
        );
        assert!(s.counted.is_empty());
    }

    #[test]
    fn el_parpadeo_del_prompt_tras_contar_no_cuenta_dos_veces() {
        let mut s = Sim::new();
        let chest = looking_at(ChestType::Normal);
        s.frame(Observation {
            key_pressed: TAP,
            ..chest
        });
        s.ring(chest, 1.0);
        s.frames(3, NOTHING);
        s.frame(POPUP);
        // El prompt reaparece un instante con el aviso todavía en pantalla.
        s.frames(
            8,
            Observation {
                key_pressed: TAP,
                progress: 1.0,
                visual_confirm: true,
                ..chest
            },
        );
        s.frames(30, POPUP);
        assert_eq!(s.counted.len(), 1);
    }

    #[test]
    fn dos_cofres_seguidos_cuentan_dos_con_su_tipo() {
        let mut s = Sim::new();
        for kind in [ChestType::Normal, ChestType::Spirit] {
            s.frames(5, looking_at(kind));
            s.open(looking_at(kind));
        }
        let kinds: Vec<_> = s.counted.iter().map(|c| c.chest).collect();
        assert_eq!(kinds, [ChestType::Normal, ChestType::Spirit]);
        assert_eq!(s.rejected, []);
    }

    #[test]
    fn el_tipo_se_fija_al_ver_el_cofre_aunque_cambie_al_abrirse() {
        let mut s = Sim::new();
        s.frames(5, looking_at(ChestType::Spirit));
        s.open(looking_at(ChestType::Normal));
        assert_eq!(s.counted[0].chest, ChestType::Spirit);
    }

    #[test]
    fn sin_escucha_de_teclado_cuenta_con_las_otras_cuatro_senales() {
        let mut s = Sim::new();
        let chest = Observation {
            key_pressed: None,
            ..looking_at(ChestType::Normal)
        };
        s.frames(30, chest); // mirar el cofre sin abrirlo no hace nada
        assert!(s.counted.is_empty() && s.rejected.is_empty());

        s.open(chest);
        assert_eq!(s.counted.len(), 1);
        let c = s.counted[0];
        assert_eq!(c.signals & S3_INPUT, 0);
        assert_eq!(c.confidence, 0.8);
    }
}
