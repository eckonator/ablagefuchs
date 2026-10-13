// SPDX-License-Identifier: MIT OR Apache-2.0
//! Linke Spalte: Filter und Dateien mit Status.

use eframe::egui::{self, RichText, Sense};

use crate::app::{Action, AnalysisState, Entry, Filter};
use crate::i18n::tr;
use crate::theme;

pub fn show(ui: &mut egui::Ui, entries: &[Entry], current: Option<usize>, filter: Filter, actions: &mut Vec<Action>) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        for (f, label) in [
            (Filter::All, "Alle"),
            (Filter::Open, "Offen"),
            (Filter::Check, "Prüfen"),
            (Filter::Done, "Erledigt"),
        ] {
            let count = entries.iter().filter(|e| f.matches(e)).count();
            if ui
                .selectable_label(filter == f, format!("{} {count}", tr(label)))
                .clicked()
            {
                actions.push(Action::SetFilter(f));
            }
        }
    });
    ui.separator();
    let ctx = ui.ctx().clone();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (i, e) in entries.iter().enumerate().filter(|(_, e)| filter.matches(e)) {
            let selected = current == Some(i);
            let (icon, color) = if !e.is_open() {
                ("✔", theme::ok(&ctx))
            } else if e.needs_check() {
                ("⚠", theme::warn(&ctx))
            } else {
                ("○", theme::muted(&ctx))
            };
            let sub = if !e.is_open() {
                e.file_name()
            } else {
                match &e.analysis {
                    AnalysisState::Ready(a) if a.suggestion.is_complete() => {
                        format!("{} · {}", a.suggestion.unternehmen.value, a.suggestion.thema.value)
                    }
                    AnalysisState::Ready(_) => tr("nichts erkannt").to_string(),
                    AnalysisState::Failed(_) => tr("nicht lesbar").to_string(),
                    _ => tr("wird gelesen …").to_string(),
                }
            };
            let frame = egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(8, 6))
                .corner_radius(6)
                .fill(if selected {
                    ui.visuals().selection.bg_fill.gamma_multiply(0.35)
                } else {
                    egui::Color32::TRANSPARENT
                });
            let resp = frame
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon).color(color).size(15.0));
                        ui.vertical(|ui| {
                            let name = RichText::new(&e.original).monospace().size(12.0);
                            let name = if e.is_open() { name } else { name.strikethrough().weak() };
                            ui.add(egui::Label::new(name).truncate());
                            let sub = RichText::new(sub).size(11.5);
                            let sub = if e.is_open() {
                                sub.weak()
                            } else {
                                sub.color(theme::ok(&ctx))
                            };
                            ui.add(egui::Label::new(sub).truncate());
                        });
                    });
                })
                .response
                .interact(Sense::click());
            if resp.clicked() {
                actions.push(Action::Select(i));
            }
            // Nach einem Wechsel (auch per Tastatur) die gewählte Zeile einmal sichtbar machen
            let key = egui::Id::new("liste-zuletzt-sichtbar");
            if selected && ui.data(|d| d.get_temp::<u64>(key)) != Some(e.id) {
                resp.scroll_to_me(None);
                ui.data_mut(|d| d.insert_temp(key, e.id));
            }
        }
    });
}
