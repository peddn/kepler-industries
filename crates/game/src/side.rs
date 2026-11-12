//! Seitenleiste am linken oder rechten Rand: durchscheinend (dahinter läuft
//! die Karte weiter), mit eigenem Regler für die Deckkraft am unteren Ende,
//! und über eine kleine Schaltfläche ein- und ausklappbar. Eingeklappt bleibt
//! ein schmaler Streifen mit der Schaltfläche zum Ausklappen.

use bevy_egui::egui;

/// An welchem Rand die Leiste liegt.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
}

/// Zustand einer Seitenleiste (Bedienung, nicht im Spielstand).
pub struct Side {
    edge: Edge,
    open: bool,
    /// Deckkraft von 0 (durchsichtig) bis 1 (deckend).
    opacity: f32,
}

/// Breite des Streifens, der eingeklappt bleibt (Platz für die
/// Schaltfläche).
const CLOSED_WIDTH: f32 = 36.0;
/// Kleinste einstellbare Deckkraft: Die Schrift muss lesbar bleiben.
const MIN_OPACITY: f32 = 0.2;

impl Side {
    pub fn new(edge: Edge) -> Self {
        Self {
            edge,
            open: true,
            opacity: 0.85,
        }
    }

    fn panel(&self, id: &str) -> egui::Panel {
        let id = egui::Id::new(id);
        match self.edge {
            Edge::Left => egui::Panel::left(id),
            Edge::Right => egui::Panel::right(id),
        }
    }

    fn frame(&self, style: &egui::Style) -> egui::Frame {
        let frame = egui::Frame::side_top_panel(style);
        frame.fill(frame.fill.gamma_multiply(self.opacity))
    }

    /// Pfeil der Schaltfläche: zeigt, wohin die Leiste dabei geht.
    fn arrow(&self) -> &'static str {
        match (self.edge, self.open) {
            (Edge::Left, true) | (Edge::Right, false) => "◀",
            (Edge::Left, false) | (Edge::Right, true) => "▶",
        }
    }

    /// Zeichnet die Leiste in der Breite `width`: oben eine Zeile mit der
    /// Schaltfläche zum Einklappen (außen) und `header`, darunter `contents`
    /// (zum Scrollen), unten der Regler für die Deckkraft.
    pub fn show(
        &mut self,
        root: &mut egui::Ui,
        id: &str,
        width: f32,
        header: impl FnOnce(&mut egui::Ui),
        contents: impl FnOnce(&mut egui::Ui),
    ) {
        let frame = self.frame(root.style());
        if !self.open {
            self.panel(&format!("{id}_closed"))
                .resizable(false)
                .exact_size(CLOSED_WIDTH)
                .frame(frame)
                .show(root, |ui| {
                    if ui.button(self.arrow()).clicked() {
                        self.open = true;
                    }
                });
            return;
        }
        self.panel(id)
            .resizable(false)
            .exact_size(width)
            .frame(frame)
            .show(root, |ui| {
                egui::Panel::bottom(egui::Id::new((id, "footer")))
                    .frame(egui::Frame::NONE)
                    .resizable(false)
                    .show(ui, |ui| {
                        ui.spacing_mut().slider_width = ui.available_width();
                        ui.add(
                            egui::Slider::new(&mut self.opacity, MIN_OPACITY..=1.0)
                                .show_value(false),
                        )
                        .on_hover_text("Deckkraft dieser Leiste");
                    });
                let layout = match self.edge {
                    Edge::Left => egui::Layout::left_to_right(egui::Align::Center),
                    Edge::Right => egui::Layout::right_to_left(egui::Align::Center),
                };
                // Eine Zeile hoch (ein Layout ohne feste Höhe nähme sich alles).
                let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
                ui.allocate_ui_with_layout(row, layout, |ui| {
                    if ui.button(self.arrow()).clicked() {
                        self.open = false;
                    }
                    header(ui);
                });
                egui::ScrollArea::vertical().show(ui, contents);
            });
    }
}
