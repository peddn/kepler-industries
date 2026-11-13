//! Δv-Balken unten: der Tank des gewählten Schiffs als Balken. Die volle
//! Breite ist der volle Tank; gefüllt ist, was an Bord ist. Darin liegen die
//! geplanten Brennvorgänge der Kette (Start gelb, Nodes violett, Bremsen beim
//! Landen türkis, Andocken rosa), dahinter der Rest (grün). Verlangt die Kette
//! mehr, als an Bord ist, steht der Fehlbetrag rot dahinter.
//!
//! Die Zahlen kommen aus `World::delta_v_budget`; hier wird nur gezeichnet.

use crate::rendezvous::TARGET_COLOR;
use crate::ui::color32;
use crate::zahl::De;
use bevy_egui::egui::{self, Color32};
use sim::{BudgetItem, World};

const BAR_HEIGHT: f32 = 36.0;
/// Abstand der Teilstriche in m/s.
const TICK_STEP: f64 = 500.0;

const EMPTY_COLOR: Color32 = Color32::from_rgb(13, 15, 23);
const LAUNCH_COLOR: Color32 = Color32::from_rgb(242, 204, 64);
const NODE_COLORS: [Color32; 2] = [
    Color32::from_rgb(153, 102, 242),
    Color32::from_rgb(110, 71, 191),
];
const LANDING_COLOR: Color32 = Color32::from_rgb(51, 179, 204);
const REST_COLOR: Color32 = Color32::from_rgb(56, 158, 84);
const MISSING_COLOR: Color32 = Color32::from_rgb(217, 56, 46);
const SEPARATOR_COLOR: Color32 = Color32::from_rgb(8, 8, 13);
const BORDER_COLOR: Color32 = Color32::from_rgb(64, 77, 102);

/// Ein Abschnitt des Balkens.
struct Piece {
    delta_v: f64,
    color: Color32,
    label: String,
    selected: bool,
}

/// Die Abschnitte: geplante Brennvorgänge, dann Rest und Fehlbetrag.
/// `chosen`: der gewählte Node.
fn pieces(budget: &sim::DeltaVBudget, chosen: Option<usize>) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = budget
        .burns
        .iter()
        .map(|&(item, delta_v)| match item {
            BudgetItem::Launch => Piece {
                delta_v,
                color: LAUNCH_COLOR,
                label: format!("Start {delta_v:.0}", delta_v = De(delta_v)),
                selected: false,
            },
            BudgetItem::Node(k) => Piece {
                delta_v,
                color: NODE_COLORS[k % 2],
                label: format!("{}: {delta_v:.0}", k + 1, delta_v = De(delta_v)),
                selected: chosen == Some(k),
            },
            BudgetItem::Landing => Piece {
                delta_v,
                color: LANDING_COLOR,
                label: format!("Landen {delta_v:.0}", delta_v = De(delta_v)),
                selected: false,
            },
            BudgetItem::Docking => Piece {
                delta_v,
                color: color32(TARGET_COLOR),
                label: format!("Andocken {delta_v:.0}", delta_v = De(delta_v)),
                selected: false,
            },
        })
        .collect();
    for (delta_v, color, name) in [
        (budget.rest, REST_COLOR, "Rest"),
        (budget.missing, MISSING_COLOR, "fehlt"),
    ] {
        if delta_v > 0.5 {
            pieces.push(Piece {
                delta_v,
                color,
                label: format!("{name} {delta_v:.0}", delta_v = De(delta_v)),
                selected: false,
            });
        }
    }
    pieces
}

/// Zeichnet den Balken für das Schiff `vessel` an den unteren Rand von
/// `root` (nichts für Schiffe ohne Triebwerk oder Tank).
pub fn show(root: &mut egui::Ui, world: &World, vessel: usize, chosen: Option<usize>) {
    let params = world.params(vessel);
    if params.thrust <= 0.0 || params.fuel_capacity <= 0.0 {
        return;
    }
    let budget = world.delta_v_budget(vessel);
    // Volle Breite = voller Tank; fehlt etwas, wird der Balken so lang, dass
    // auch das hineinpasst.
    let scale = budget.full.max(budget.available + budget.missing).max(1e-9);
    egui::Panel::bottom("dv_bar").show(root, |ui| {
        ui.label(format!(
            "Δv {:.0} von {:.0} m/s (voller Tank)",
            De(budget.available),
            De(budget.full)
        ));
        let size = egui::vec2(ui.available_width(), BAR_HEIGHT);
        let (bar, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter_at(bar);
        painter.rect_filled(bar, 0.0, EMPTY_COLOR);
        let x = |delta_v: f64| bar.left() + bar.width() * (delta_v / scale) as f32;
        let font = egui::TextStyle::Body.resolve(ui.style());
        let mut start = 0.0;
        for piece in pieces(&budget, chosen) {
            let rect =
                egui::Rect::from_x_y_ranges(x(start)..=x(start + piece.delta_v), bar.y_range());
            start += piece.delta_v;
            painter.rect_filled(rect, 0.0, piece.color);
            let outline = if piece.selected {
                egui::Stroke::new(2.0, Color32::WHITE)
            } else {
                egui::Stroke::new(1.0, SEPARATOR_COLOR)
            };
            painter.rect_stroke(rect, 0.0, outline, egui::StrokeKind::Inside);
            // Beschriftung nur, wenn sie hineinpasst; auf Gelb schwarz.
            let color = if piece.color == LAUNCH_COLOR {
                Color32::BLACK
            } else {
                Color32::WHITE
            };
            let text = painter.layout_no_wrap(piece.label, font.clone(), color);
            if text.size().x + 10.0 <= rect.width() {
                painter.galley(rect.center() - text.size() / 2.0, text, color);
            }
        }
        let ticks = (1..)
            .map(|k| k as f64 * TICK_STEP)
            .take_while(|&at| at < scale);
        for at in ticks {
            let top = egui::pos2(x(at), bar.bottom() - 10.0);
            painter.line_segment(
                [top, egui::pos2(x(at), bar.bottom())],
                egui::Stroke::new(1.0, Color32::from_white_alpha(115)),
            );
        }
        painter.rect_stroke(
            bar,
            0.0,
            egui::Stroke::new(1.0, BORDER_COLOR),
            egui::StrokeKind::Inside,
        );
    });
}
