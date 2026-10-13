// SPDX-License-Identifier: MIT OR Apache-2.0
//! Mitte: gerenderte Seite mit Zoom und Seitenwechsel, farbigen Fundstellen, anklickbaren Datumsangaben
//! und Textauswahl per Ziehen (→ als Feld übernehmen).

use af_core::layout::Layout;
use af_core::{PageText, Suggestion};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};

use crate::app::{Action, AnalysisState, Entry, FieldFocus};
use crate::i18n::{tr, trf};
use crate::theme;

pub struct PreviewCtx {
    pub zoom: f32,
    pub show_highlights: bool,
    /// Feld, dessen Fundstelle hervorgehoben wird (Fokus oder Maus im Formular)
    pub active: Option<FieldFocus>,
}

/// Zeichnet die Vorschau und gibt die verfügbare Breite zurück (für die Render-Auflösung).
pub fn show(ui: &mut egui::Ui, entry: Option<&Entry>, c: &PreviewCtx, actions: &mut Vec<Action>) -> f32 {
    let Some(e) = entry else { return ui.available_width() };
    toolbar(ui, e, c, actions);
    let avail = ui.available_width() - 48.0;
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .id_salt(("vorschau", e.id, e.page))
        .show(ui, |ui| {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| match &e.preview {
                Some(p) if p.page == e.page => {
                    let size = p.texture.size_vec2();
                    let w = (avail * c.zoom).max(100.0);
                    let h = w * size.y / size.x.max(1.0);
                    let resp =
                        ui.add(egui::Image::new((p.texture.id(), egui::vec2(w, h))).sense(Sense::click_and_drag()));
                    ui.painter().rect_stroke(
                        resp.rect.expand(1.0),
                        0.0,
                        Stroke::new(1.0, Color32::from_black_alpha(50)),
                        StrokeKind::Outside,
                    );
                    let scale = resp.rect.width() / p.page_size_pt.x.max(1.0);
                    let page_no = u16::try_from(e.page + 1).unwrap_or(u16::MAX);
                    let page = PageView {
                        rect: resp.rect,
                        scale,
                        page: page_no,
                    };
                    if let AnalysisState::Ready(a) = &e.analysis {
                        overlays(ui, &page, &resp, &a.suggestion, c, actions);
                        selection(ui, &page, &resp, a.text.iter().find(|t| t.page == page_no), actions);
                    }
                }
                _ => {
                    ui.add_space(80.0);
                    ui.spinner();
                }
            });
            ui.add_space(24.0);
        });
    avail
}

fn toolbar(ui: &mut egui::Ui, e: &Entry, c: &PreviewCtx, actions: &mut Vec<Action>) {
    egui::Frame::new()
        .fill(ui.visuals().panel_fill)
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Von rechts: erst die Bedienelemente, der Dateiname bekommt den Rest und wird notfalls gekürzt
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
                    if ui.button("+").on_hover_text(zoom_hint("+")).clicked() {
                        actions.push(Action::Zoom(c.zoom + 0.1));
                    }
                    ui.label(format!("{:.0} %", c.zoom * 100.0));
                    if ui.button("−").on_hover_text(zoom_hint("−")).clicked() {
                        actions.push(Action::Zoom(c.zoom - 0.1));
                    }
                    ui.add_space(12.0);
                    if ui.selectable_label(c.show_highlights, tr("Markierungen")).clicked() {
                        actions.push(Action::ToggleHighlights);
                    }
                    if c.show_highlights {
                        legend(ui);
                    }
                    ui.add_space(12.0);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(e.file_name()).monospace()).truncate());
                    });
                });
            });
        });
}

fn zoom_hint(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{key}")
    } else {
        format!("Strg+{key}")
    }
}

/// Farblegende (von rechts nach links gezeichnet).
fn legend(ui: &mut egui::Ui) {
    for (color, label) in [
        (theme::THEMA, tr("Thema")),
        (theme::UNTERNEHMEN, tr("Unternehmen")),
        (theme::DATUM, tr("Datum")),
    ] {
        ui.label(RichText::new(label).small().weak());
        let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
        ui.painter().rect_filled(r, 2.0, color.gamma_multiply(0.45));
    }
}

/// Lage der Seite auf dem Bildschirm, zum Umrechnen von PDF-Punkten.
struct PageView {
    rect: Rect,
    scale: f32,
    page: u16,
}

impl PageView {
    fn to_screen(&self, r: &af_core::Rect) -> Rect {
        Rect::from_min_size(
            self.rect.min + egui::vec2(r.x * self.scale, r.y * self.scale),
            egui::vec2(r.w * self.scale, r.h * self.scale),
        )
    }

    fn to_page(&self, p: Pos2) -> Pos2 {
        let v = (p - self.rect.min) / self.scale;
        Pos2::new(v.x, v.y)
    }
}

/// Fundstellen einfärben, Datumskandidaten unterstreichen; Klicks darauf auswerten.
fn overlays(
    ui: &mut egui::Ui,
    page: &PageView,
    resp: &egui::Response,
    s: &Suggestion,
    c: &PreviewCtx,
    actions: &mut Vec<Action>,
) {
    let painter = ui.painter_at(page.rect);
    let hover = resp.hover_pos();
    let clicked = resp.clicked();
    let mut handled = false;

    // Datumskandidaten: gestrichelt unterstrichen, anklickbar
    for d in s.dates.iter().filter(|d| d.page == page.page) {
        let r = page.to_screen(&d.rect).expand(1.5);
        let hovered = hover.is_some_and(|p| r.contains(p));
        if c.show_highlights || hovered {
            painter.add(egui::Shape::dashed_line(
                &[r.left_bottom(), r.right_bottom()],
                Stroke::new(1.2, theme::DATUM),
                3.0,
                2.0,
            ));
        }
        if hovered {
            painter.rect_filled(r, 2.0, theme::DATUM.gamma_multiply(0.15));
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if clicked && !handled {
                actions.push(Action::SetDate(d.date));
                handled = true;
            }
        }
    }

    for (field, color, highlights) in [
        (FieldFocus::Datum, theme::DATUM, &s.datum.highlights),
        (FieldFocus::Unternehmen, theme::UNTERNEHMEN, &s.unternehmen.highlights),
        (FieldFocus::Thema, theme::THEMA, &s.thema.highlights),
    ] {
        let active = c.active == Some(field);
        for h in highlights.iter().filter(|h| h.page == page.page) {
            let r = page.to_screen(&h.rect).expand(2.0);
            if c.show_highlights || active {
                painter.rect_filled(r, 2.0, color.gamma_multiply(0.22));
            }
            if active {
                painter.rect_stroke(r, 2.0, Stroke::new(2.0, color), StrokeKind::Outside);
            }
            // Klick auf eine Fundstelle springt ins Feld
            if !handled && clicked && hover.is_some_and(|p| r.contains(p)) {
                actions.push(Action::Focus(field));
                handled = true;
            }
        }
    }
}

/// Text durch Ziehen markieren. Beim Loslassen bietet die App an, ihn als Feld zu übernehmen.
fn selection(
    ui: &mut egui::Ui,
    page: &PageView,
    resp: &egui::Response,
    text: Option<&PageText>,
    actions: &mut Vec<Action>,
) {
    let Some(text) = text else { return };
    let key = egui::Id::new("vorschau-auswahl-start");
    if resp.drag_started()
        && let Some(p) = resp.interact_pointer_pos()
    {
        ui.data_mut(|d| d.insert_temp(key, page.to_page(p)));
    }
    let Some(start) = ui.data(|d| d.get_temp::<Pos2>(key)) else {
        return;
    };
    let Some(now) = resp.interact_pointer_pos().or_else(|| ui.ctx().pointer_latest_pos()) else {
        return;
    };
    let sel = Rect::from_two_pos(start, page.to_page(now));
    let words: Vec<&af_core::Word> = text
        .words
        .iter()
        .filter(|w| Rect::from_min_size(Pos2::new(w.bbox.x, w.bbox.y), egui::vec2(w.bbox.w, w.bbox.h)).intersects(sel))
        .collect();
    let painter = ui.painter_at(page.rect);
    let accent = theme::accent(ui.ctx());
    painter.rect_stroke(
        Rect::from_two_pos(page.rect.min + start.to_vec2() * page.scale, now),
        0.0,
        Stroke::new(1.0, accent),
        StrokeKind::Middle,
    );
    for w in &words {
        painter.rect_filled(page.to_screen(&w.bbox), 1.0, accent.gamma_multiply(0.3));
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<Pos2>(key));
        // Wörter in Lesereihenfolge zusammensetzen
        let picked = PageText {
            words: words.into_iter().cloned().collect(),
            ..text.clone()
        };
        let joined = Layout::new(&[picked])
            .lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if !joined.is_empty() {
            actions.push(Action::Selection { text: joined, pos: now });
        }
    }
}
