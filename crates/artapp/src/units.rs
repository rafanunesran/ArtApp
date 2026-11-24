//! Tamanho da tela em pixels, centímetros, milímetros ou polegadas, com a resolução (dpi), como
//! na caixa de nova tela do Clip Studio.

use eframe::egui::{self, RichText};

use crate::theme::color;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Unit {
    #[default]
    Px,
    Cm,
    Mm,
    In,
}

impl Unit {
    pub const ALL: [Unit; 4] = [Unit::Px, Unit::Cm, Unit::Mm, Unit::In];

    pub fn label(self) -> &'static str {
        match self {
            Unit::Px => "px",
            Unit::Cm => "cm",
            Unit::Mm => "mm",
            Unit::In => "pol",
        }
    }

    /// Quantas unidades cabem numa polegada.
    fn per_inch(self) -> f64 {
        match self {
            Unit::Px => 1.0,
            Unit::Cm => 2.54,
            Unit::Mm => 25.4,
            Unit::In => 1.0,
        }
    }

    pub fn of_px(self, px: u32, dpi: f32) -> f64 {
        match self {
            Unit::Px => f64::from(px),
            _ => f64::from(px) / f64::from(dpi.max(1.0)) * self.per_inch(),
        }
    }

    pub fn to_px(self, v: f64, dpi: f32) -> u32 {
        let px = match self {
            Unit::Px => v,
            _ => v / self.per_inch() * f64::from(dpi.max(1.0)),
        };
        px.round().clamp(1.0, f64::from(u32::MAX)) as u32
    }
}

/// Largura, altura e dpi editáveis na unidade escolhida. `w` e `h` ficam sempre em pixels (até
/// `max`). Devolve se algo mudou.
pub fn size_editor(
    ui: &mut egui::Ui,
    w: &mut u32,
    h: &mut u32,
    dpi: &mut f32,
    unit: &mut Unit,
    max: u32,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for u in Unit::ALL {
            changed |= ui.selectable_value(unit, u, u.label()).changed();
        }
    });
    let (speed, decimals) = match unit {
        Unit::Px => (1.0, 0),
        Unit::Mm => (0.5, 1),
        Unit::Cm | Unit::In => (0.05, 2),
    };
    let max_v = unit.of_px(max, *dpi);
    for (label, side) in [("Largura", &mut *w), ("Altura", &mut *h)] {
        let mut v = unit.of_px(*side, *dpi);
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::DragValue::new(&mut v)
                    .speed(speed)
                    .fixed_decimals(decimals)
                    .range(unit.of_px(1, *dpi)..=max_v)
                    .suffix(format!(" {}", unit.label())),
            );
            ui.label(label);
            if r.changed() {
                *side = unit.to_px(v, *dpi).min(max);
                changed = true;
            }
        });
    }
    // Em cm, mm ou pol, mudar o dpi mantém o tamanho no papel (e muda os pixels).
    let old_dpi = *dpi;
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::DragValue::new(dpi)
                .speed(1.0)
                .range(36.0..=1200.0)
                .fixed_decimals(0)
                .suffix(" dpi"),
        );
        ui.label("Resolução");
        changed |= r.changed();
    });
    ui.horizontal_wrapped(|ui| {
        for d in [72.0f32, 150.0, 300.0, 350.0, 600.0] {
            if ui
                .selectable_label((*dpi - d).abs() < 0.5, format!("{d}"))
                .clicked()
            {
                *dpi = d;
                changed = true;
            }
        }
    });
    if *dpi != old_dpi && *unit != Unit::Px {
        for side in [&mut *w, &mut *h] {
            let v = unit.of_px(*side, old_dpi);
            *side = unit.to_px(v, *dpi).min(max);
        }
    }
    ui.label(
        RichText::new(format!(
            "{} × {} px  ·  {:.1} × {:.1} cm a {:.0} dpi",
            w,
            h,
            Unit::Cm.of_px(*w, *dpi),
            Unit::Cm.of_px(*h, *dpi),
            dpi
        ))
        .size(12.0)
        .color(color::TEXT_MUTED),
    );
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_at_300_dpi() {
        assert_eq!(Unit::Mm.to_px(210.0, 300.0), 2480);
        assert_eq!(Unit::Cm.to_px(29.7, 300.0), 3508);
        assert!((Unit::In.of_px(600, 300.0) - 2.0).abs() < 1e-9);
        assert_eq!(Unit::Px.to_px(12.4, 72.0), 12);
    }
}
