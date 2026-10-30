//! Réguas especiais, como as do Clip Studio: o traço "gruda" numa reta, em linhas paralelas, em
//! círculos concêntricos ou nas linhas de fuga de uma perspectiva. A régua só move os pontos do
//! traço; o pincel desenha normalmente.

#[derive(Clone, Debug, PartialEq)]
pub enum Ruler {
    /// Reta por `a` e `b`: o traço corre sobre ela.
    Line { a: [f64; 2], b: [f64; 2] },
    /// Linhas paralelas na direção de `angle` (radianos), passando por onde o traço começa.
    Parallel { angle: f64 },
    /// Círculos com centro em `center`: o traço mantém a distância do começo ao centro.
    Concentric { center: [f64; 2] },
    /// Linhas que saem de um ou mais pontos de fuga (perspectiva de 1, 2 ou 3 pontos). O traço
    /// escolhe o ponto pela direção dos primeiros pixels.
    Perspective { vanishing: Vec<[f64; 2]> },
}

impl Ruler {
    /// Os pontos que dá para arrastar na tela.
    pub fn handles(&self) -> Vec<[f64; 2]> {
        match self {
            Ruler::Line { a, b } => vec![*a, *b],
            Ruler::Parallel { .. } => vec![],
            Ruler::Concentric { center } => vec![*center],
            Ruler::Perspective { vanishing } => vanishing.clone(),
        }
    }

    pub fn set_handle(&mut self, i: usize, p: [f64; 2]) {
        match self {
            Ruler::Line { a, b } => {
                if i == 0 {
                    *a = p
                } else {
                    *b = p
                }
            }
            Ruler::Concentric { center } => *center = p,
            Ruler::Perspective { vanishing } => {
                if let Some(v) = vanishing.get_mut(i) {
                    *v = p;
                }
            }
            Ruler::Parallel { .. } => {}
        }
    }
}

/// O estado de um traço preso a uma régua.
#[derive(Clone, Debug)]
pub struct Snap {
    start: [f64; 2],
    /// Direção escolhida (vetor unitário) e o ponto por onde a reta passa.
    line: Option<([f64; 2], [f64; 2])>,
    /// Quanto o traço anda (px do documento) antes de a perspectiva escolher o ponto de fuga.
    decide: f64,
}

/// Distância padrão para a perspectiva decidir o ponto de fuga.
const DECIDE_PX: f64 = 6.0;

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn norm(v: [f64; 2]) -> Option<[f64; 2]> {
    let l = v[0].hypot(v[1]);
    (l > 1e-9).then(|| [v[0] / l, v[1] / l])
}

fn project(p: [f64; 2], origin: [f64; 2], dir: [f64; 2]) -> [f64; 2] {
    let d = sub(p, origin);
    let t = d[0] * dir[0] + d[1] * dir[1];
    [origin[0] + dir[0] * t, origin[1] + dir[1] * t]
}

impl Snap {
    pub fn new(ruler: &Ruler, start: [f64; 2]) -> Self {
        let line = match ruler {
            Ruler::Line { a, b } => norm(sub(*b, *a)).map(|d| (*a, d)),
            Ruler::Parallel { angle } => Some((start, [angle.cos(), angle.sin()])),
            _ => None,
        };
        Self {
            start,
            line,
            decide: DECIDE_PX,
        }
    }

    /// Muda a distância de decisão da perspectiva (use uns pixels de tela divididos pelo zoom).
    pub fn with_decide(mut self, px: f64) -> Self {
        self.decide = px.max(1.0);
        self
    }

    /// Onde o ponto `p` do traço vai parar.
    pub fn apply(&mut self, ruler: &Ruler, p: [f64; 2]) -> [f64; 2] {
        match ruler {
            Ruler::Line { .. } | Ruler::Parallel { .. } => match self.line {
                Some((o, d)) => project(p, o, d),
                None => p,
            },
            Ruler::Concentric { center } => {
                let r0 = sub(self.start, *center);
                let r = r0[0].hypot(r0[1]);
                match norm(sub(p, *center)) {
                    Some(d) => [center[0] + d[0] * r, center[1] + d[1] * r],
                    None => self.start,
                }
            }
            Ruler::Perspective { vanishing } => {
                if self.line.is_none() {
                    let moved = sub(p, self.start);
                    if moved[0].hypot(moved[1]) < self.decide {
                        return self.start;
                    }
                    let m = norm(moved).unwrap_or([1.0, 0.0]);
                    // O ponto de fuga cuja linha pelo começo do traço mais se alinha ao movimento.
                    self.line = vanishing
                        .iter()
                        .filter_map(|v| norm(sub(*v, self.start)))
                        .map(|d| (d, (d[0] * m[0] + d[1] * m[1]).abs()))
                        .max_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(d, _)| (self.start, d));
                }
                match self.line {
                    Some((o, d)) => project(p, o, d),
                    None => p,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
    }

    #[test]
    fn rulers_snap_points() {
        let line = Ruler::Line {
            a: [0.0, 0.0],
            b: [10.0, 0.0],
        };
        let mut s = Snap::new(&line, [3.0, 4.0]);
        assert!(close(s.apply(&line, [7.0, 5.0]), [7.0, 0.0]));

        let par = Ruler::Parallel { angle: 0.0 };
        let mut s = Snap::new(&par, [3.0, 4.0]);
        assert!(close(s.apply(&par, [9.0, 8.0]), [9.0, 4.0]));

        let conc = Ruler::Concentric { center: [0.0, 0.0] };
        let mut s = Snap::new(&conc, [5.0, 0.0]);
        assert!(close(s.apply(&conc, [0.0, 2.0]), [0.0, 5.0]));

        let persp = Ruler::Perspective {
            vanishing: vec![[100.0, 0.0], [0.0, 100.0]],
        };
        let mut s = Snap::new(&persp, [0.0, 0.0]);
        assert!(close(s.apply(&persp, [1.0, 1.0]), [0.0, 0.0]));
        // Andou para baixo: escolhe o ponto de fuga de baixo.
        let q = s.apply(&persp, [1.0, 20.0]);
        assert!(close(q, [0.0, 20.0]));
    }
}
