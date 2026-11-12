//! Versuch: ein Entwurfsfenster mit egui (Textfeld, Auswahl, Regler, Werte).

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

#[derive(Resource)]
pub struct Draft {
    pub name: String,
    pub engines: [usize; 2],
    pub tank: f32,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            name: "Frachter".into(),
            engines: [1, 0],
            tank: 30.0,
        }
    }
}

const ENGINES: [(&str, f32, f32, f32); 3] = [
    ("leer", 0.0, 0.0, 0.0),
    ("Standard (440 kN, 3200 m/s)", 440.0, 3200.0, 1.0),
    ("Sparsam (150 kN, 3900 m/s)", 150.0, 3900.0, 0.6),
];
const SPACE: f32 = 32.0;
const HULL: f32 = 1.5;

pub fn window(mut contexts: EguiContexts, mut draft: ResMut<Draft>) -> Result {
    egui::Window::new("Schiffsentwurf")
        .default_pos([300.0, 120.0])
        .show(contexts.ctx_mut()?, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut draft.name);
            });
            ui.separator();
            for k in 0..2 {
                let mut choice = draft.engines[k];
                egui::ComboBox::from_label(format!("Triebwerk {}", k + 1))
                    .selected_text(ENGINES[choice].0)
                    .show_ui(ui, |ui| {
                        for (i, e) in ENGINES.iter().enumerate() {
                            ui.selectable_value(&mut choice, i, e.0);
                        }
                    });
                draft.engines[k] = choice;
            }
            ui.add(
                egui::Slider::new(&mut draft.tank, 0.0..=SPACE)
                    .step_by(0.5)
                    .text("t Tank"),
            );
            let cargo = SPACE - draft.tank;
            ui.label(format!("Frachtraum: {cargo:.1} t (Innenraum {SPACE:.0} t)"));
            ui.separator();
            let thrust: f32 = draft.engines.iter().map(|&e| ENGINES[e].1).sum();
            let flow: f32 = draft
                .engines
                .iter()
                .filter(|&&e| e > 0)
                .map(|&e| ENGINES[e].1 / ENGINES[e].2)
                .sum();
            let dry = HULL + draft.engines.iter().map(|&e| ENGINES[e].3).sum::<f32>();
            let full = dry + draft.tank + cargo;
            if flow > 0.0 {
                let ve = thrust / flow;
                let dv = ve * (full / (dry + cargo)).ln();
                let twr = thrust / (full * 9.81);
                ui.label(format!("Masse leer {dry:.1} t, voll beladen {full:.1} t"));
                ui.label(format!("Δv beladen: {dv:.0} m/s"));
                let text = format!("Schub/Gewicht auf Zorb: {twr:.2}");
                if twr < 1.0 {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 120, 80),
                        format!("{text} (hebt nicht ab)"),
                    );
                } else {
                    ui.label(text);
                }
            } else {
                ui.colored_label(egui::Color32::from_rgb(255, 120, 80), "Kein Triebwerk");
            }
            ui.separator();
            ui.horizontal(|ui| {
                let _ = ui.button("Entwurf speichern");
                let _ = ui.button("Bestellen");
                let _ = ui.button("Abbrechen");
            });
        });
    Ok(())
}
