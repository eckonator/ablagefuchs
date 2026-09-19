// SPDX-License-Identifier: MIT OR Apache-2.0
//! Mitte: gerenderte Seite mit Zoom und Seitenwechsel. Markierungen der Fundstellen folgen in M3.

use eframe::egui::{self, RichText};

use crate::app::{Action, Entry};
use crate::i18n::trf;

/// Zeichnet die Vorschau und gibt die verfügbare Breite zurück (für die Render-Auflösung).
pub fn show(ui: &mut egui::Ui, entry: Option<&Entry>, zoom: f32, actions: &mut Vec<Action>) -> f32 {
    let Some(e) = entry else { return ui.available_width() };
    egui::Frame::new()
        .fill(ui.visuals().panel_fill)
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(e.file_name()).monospace()).truncate());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let pages = e.pages();
                    if ui.add_enabled(e.page + 1 < pages, egui::Button::new("▶")).clicked() {
                        actions.push(Action::Page(e.page + 1));
                    }
                    ui.label(trf("Seite {} / {}", &[&(e.page + 1), &pages]));
                    if ui.add_enabled(e.page > 0, egui::Button::new("◀")).clicked() {
                        actions.push(Action::Page(e.page.saturating_sub(1)));
                    }
                    ui.add_space(12.0);
                    if ui.button("+").clicked() {
                        actions.push(Action::Zoom(zoom + 0.1));
                    }
                    ui.label(format!("{:.0} %", zoom * 100.0));
                    if ui.button("−").clicked() {
                        actions.push(Action::Zoom(zoom - 0.1));
                    }
                });
            });
        });
    let avail = ui.available_width() - 48.0;
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .id_salt(("vorschau", e.id, e.page))
        .show(ui, |ui| {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| match &e.preview {
                Some(p) => {
                    let size = p.texture.size_vec2();
                    let w = (avail * zoom).max(100.0);
                    let h = w * size.y / size.x.max(1.0);
                    let img = egui::Image::new((p.texture.id(), egui::vec2(w, h)));
                    egui::Frame::new()
                        .shadow(egui::Shadow {
                            offset: [0, 2],
                            blur: 10,
                            spread: 0,
                            color: egui::Color32::from_black_alpha(60),
                        })
                        .show(ui, |ui| {
                            ui.add(img);
                        });
                }
                None => {
                    ui.add_space(80.0);
                    ui.spinner();
                }
            });
            ui.add_space(24.0);
        });
    avail
}
