// SPDX-License-Identifier: MIT OR Apache-2.0
//! Rechte Spalte: erkannte Felder mit Sicherheit und Herkunft, Live-Dateiname und Aktionen.

use std::path::Path;

use af_core::dates::DateKind;
use af_core::naming::{NamingOptions, Template};
use af_core::topic::TopicSource;
use af_core::{Source, Suggestion, TextSource};
use eframe::egui::{self, Color32, RichText};

use crate::app::{Action, AnalysisState, Entry, FieldFocus, parse_date};
use crate::i18n::{tr, trf};
use crate::theme;

pub struct FormCtx<'a> {
    pub template: Option<&'a Template>,
    pub naming: &'a NamingOptions,
    pub threshold: u8,
    /// Zielordner, falls ins Archiv verschoben wird
    pub target_dir: Option<&'a Path>,
    /// Namen der übrigen erledigten Dateien (Hinweis auf Namensgleichheit)
    pub taken: &'a [String],
    pub can_undo: bool,
    /// Vorschläge für die Autovervollständigung
    pub companies: &'a [String],
    pub topics: &'a [String],
    /// Name einer anderen Datei mit gleichem Inhalt/Text
    pub duplicate: Option<&'a str>,
    pub meta: &'a crate::settings::MetaSettings,
    /// (Titel, Stichwörter, Finder-Tags), wie sie geschrieben würden
    pub meta_preview: &'a (String, String, Vec<String>),
}

/// Höchstens so viele Einträge in der Vorschlagsliste.
const MAX_COMPLETIONS: usize = 6;
/// Merkt sich, ob in einer Vorschlagsliste gerade ein Eintrag gewählt ist (dann gehört ↵ der Liste).
const COMPLETION_ACTIVE: &str = "vervollstaendigung-aktiv";

pub fn completion_active(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new(COMPLETION_ACTIVE)))
        .unwrap_or(false)
}

/// Zeichnet das Formular. Gibt das Feld zurück, über dem die Maus gerade steht (für die Markierung in der Vorschau).
pub fn show(
    ui: &mut egui::Ui,
    e: &mut Entry,
    c: &FormCtx<'_>,
    focus: Option<FieldFocus>,
    actions: &mut Vec<Action>,
) -> Option<FieldFocus> {
    let mut hovered = None;
    ui.data_mut(|d| d.insert_temp(egui::Id::new(COMPLETION_ACTIVE), false));
    egui::Panel::bottom("formular-aktionen").show(ui, |ui| {
        ui.add_space(8.0);
        let ready =
            e.is_open() && e.fields.datum.is_some() && !e.fields.unternehmen.trim().is_empty() && c.template.is_some();
        let accept = egui::Button::new(
            RichText::new(format!("{}   ↵", tr("Umbenennen & weiter")))
                .strong()
                .color(Color32::WHITE),
        )
        .fill(theme::accent(ui.ctx()))
        .min_size(egui::vec2(ui.available_width(), 34.0));
        if ui.add_enabled(ready, accept).clicked() {
            actions.push(Action::Accept);
        }
        ui.columns(2, |cols| {
            let w = cols[0].available_width();
            if cols[0]
                .add_sized([w, 28.0], egui::Button::new(tr("Überspringen")).truncate())
                .clicked()
            {
                actions.push(Action::Skip);
            }
            let w = cols[1].available_width();
            if cols[1]
                .add_enabled_ui(e.suggestion().is_some(), |ui| {
                    ui.add_sized([w, 28.0], egui::Button::new(tr("Zurücksetzen")).truncate())
                })
                .inner
                .on_hover_text(tr("Felder wieder auf den erkannten Vorschlag setzen"))
                .clicked()
            {
                actions.push(Action::ResetFields);
            }
        });
        if !e.is_open() && c.can_undo && ui.button(tr("Umbenennung rückgängig machen")).clicked() {
            actions.push(Action::Undo);
        }
        ui.add_space(6.0);
    });

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // Bereich eines Felds (von `top` bis zur aktuellen Position) unter der Maus?
        let track = |ui: &egui::Ui, top: f32, field: FieldFocus, hovered: &mut Option<FieldFocus>| {
            let area = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
            if ui.rect_contains_pointer(area) {
                *hovered = Some(field);
            }
        };
        ui.add_space(10.0);
        let ctx = ui.ctx().clone();
        let suggestion =
            match &e.analysis {
                AnalysisState::Ready(a) => {
                    ui.horizontal_wrapped(|ui| match &a.source {
                    _ if a.no_text() => {
                        badge(ui, tr("Kein Text"), theme::bad(&ctx));
                        let hint = if cfg!(target_os = "linux") {
                            tr("Nichts erkannt. Für Scans wird Tesseract benötigt (tesseract-ocr, tesseract-ocr-deu).")
                        } else {
                            tr("Nichts erkannt (Foto, Handschrift oder leere Seite?)")
                        };
                        ui.label(RichText::new(hint).weak());
                    }
                    TextSource::Embedded => {
                        badge(ui, tr("Text"), theme::ok(&ctx));
                        ui.label(RichText::new(tr("Textebene vorhanden")).weak());
                    }
                    TextSource::Ocr { engine, mean_confidence } => {
                        badge(ui, "OCR", theme::accent(&ctx));
                        let pct = (mean_confidence * 100.0).round() as u32;
                        ui.label(RichText::new(trf("Texterkennung: {} · Sicherheit {} %", &[engine, &pct])).weak());
                    }
                });
                    Some(a.suggestion.clone())
                }
                AnalysisState::Failed(err) => {
                    ui.colored_label(theme::bad(&ctx), trf("Datei nicht lesbar: {}", &[err]));
                    None
                }
                _ => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr("wird gelesen …"));
                    });
                    None
                }
            };
        let s = suggestion.as_ref();
        if let Some(other) = c.duplicate
            && e.is_open()
        {
            ui.add_space(6.0);
            egui::Frame::new()
                .fill(theme::warn(&ctx).gamma_multiply(0.15))
                .corner_radius(8)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.colored_label(theme::warn(&ctx), RichText::new(tr("Doppelt gescannt?")).strong());
                    ui.label(RichText::new(trf("Gleicher Inhalt wie {}", &[&other])).small());
                    if ui.button(tr("In den Papierkorb")).clicked() {
                        actions.push(Action::TrashCurrent);
                    }
                });
        }
        if !e.is_open() {
            ui.add_space(4.0);
            ui.colored_label(theme::ok(&ctx), trf("Erledigt: {}", &[&e.file_name()]));
        }
        ui.add_space(8.0);

        // Datum
        let top = ui.cursor().top();
        let conf = s.map(|s| (s.datum.value == e.fields.datum).then_some(s.datum.confidence));
        field_header(ui, theme::DATUM, tr("Datum"), conf, c.threshold, "1");
        let show_invalid = parse_date(&e.date_text).is_none() && !e.date_text.is_empty();
        let edit = egui::TextEdit::singleline(&mut e.date_text)
            .id(FieldFocus::Datum.id())
            .hint_text(tr("JJJJ-MM-TT"));
        let edit = if show_invalid {
            edit.text_color(theme::bad(&ctx))
        } else {
            edit
        };
        let resp = ui.add_sized([ui.available_width(), 26.0], edit);
        if resp.changed() {
            e.fields.datum = parse_date(&e.date_text);
        }
        take_focus(&resp, focus, FieldFocus::Datum);
        if let Some(s) = s {
            date_chips(ui, s, e);
            source_line(ui, &s.datum.source);
        }
        track(ui, top, FieldFocus::Datum, &mut hovered);
        ui.add_space(10.0);

        // Unternehmen
        let top = ui.cursor().top();
        let conf = s.map(|s| (s.unternehmen.value == e.fields.unternehmen).then_some(s.unternehmen.confidence));
        field_header(ui, theme::UNTERNEHMEN, tr("Unternehmen"), conf, c.threshold, "2");
        let resp = ui.add_sized(
            [ui.available_width(), 26.0],
            egui::TextEdit::singleline(&mut e.fields.unternehmen).id(FieldFocus::Unternehmen.id()),
        );
        take_focus(&resp, focus, FieldFocus::Unternehmen);
        autocomplete(
            ui,
            &resp,
            &mut e.fields.unternehmen,
            c.companies,
            FieldFocus::Unternehmen.id(),
        );
        if let Some(s) = s {
            source_line(ui, &s.unternehmen.source);
        }
        track(ui, top, FieldFocus::Unternehmen, &mut hovered);
        ui.add_space(10.0);

        // Thema
        let top = ui.cursor().top();
        let conf = s.map(|s| (s.thema.value == e.fields.thema).then_some(s.thema.confidence));
        field_header(ui, theme::THEMA, tr("Thema"), conf, c.threshold, "3");
        let resp = ui.add_sized(
            [ui.available_width(), 26.0],
            egui::TextEdit::singleline(&mut e.fields.thema).id(FieldFocus::Thema.id()),
        );
        take_focus(&resp, focus, FieldFocus::Thema);
        autocomplete(ui, &resp, &mut e.fields.thema, c.topics, FieldFocus::Thema.id());
        if let Some(s) = s
            && !s.known_topics.is_empty()
        {
            ui.horizontal_wrapped(|ui| {
                for t in s.known_topics.iter().take(5) {
                    if ui
                        .selectable_label(&e.fields.thema == t, RichText::new(t).small())
                        .clicked()
                    {
                        actions.push(Action::SetTopic(t.clone()));
                    }
                }
            });
        }
        if let Some(s) = s {
            source_line(ui, &s.thema.source);
        }
        track(ui, top, FieldFocus::Thema, &mut hovered);
        ui.add_space(10.0);

        // Rechnung
        ui.horizontal(|ui| {
            swatch(ui, theme::RECHNUNG);
            ui.checkbox(&mut e.fields.rechnung, RichText::new(tr("Rechnung")).strong());
            if let Some(s) = s {
                confidence(
                    ui,
                    (s.rechnung.value == e.fields.rechnung).then_some(s.rechnung.confidence),
                    c.threshold,
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(key_hint("R")).monospace().small().weak());
            });
        });
        if let Some(s) = s {
            source_line(ui, &s.rechnung.source);
        }
        ui.add_space(14.0);

        result_box(ui, e, c);
        ui.add_space(10.0);
        metadata_section(ui, e, c);
    });
    hovered
}

/// Vorschlagsliste unter einem Textfeld: passende Einträge (Anfang zuerst), ↓/↑ wählen, ↵ oder Klick übernimmt.
fn autocomplete(ui: &mut egui::Ui, resp: &egui::Response, value: &mut String, options: &[String], id: egui::Id) {
    let index_key = id.with("index");
    let rect_key = id.with("liste");
    let popup_rect: Option<egui::Rect> = ui.data(|d| d.get_temp(rect_key));
    // Offen, solange das Feld den Fokus hat oder die Maus über der Liste steht (sonst ginge der Klick verloren)
    let over_popup = popup_rect.is_some_and(|r| ui.rect_contains_pointer(r));
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    let index: Option<usize> = ui.data(|d| d.get_temp(index_key)).flatten();
    let needle = value.trim().to_lowercase();
    let mut matches: Vec<&String> = options
        .iter()
        .filter(|o| !needle.is_empty() && o.to_lowercase().contains(&needle) && o.trim() != value.trim())
        .collect();
    matches.sort_by_key(|o| (!o.to_lowercase().starts_with(&needle), o.len()));
    matches.truncate(MAX_COMPLETIONS);

    // ↵ im Feld übernimmt den gewählten Eintrag (das Feld verliert dabei den Fokus)
    if resp.lost_focus() && enter {
        if let Some(choice) = index.and_then(|i| matches.get(i)) {
            *value = (*choice).clone();
        }
        ui.data_mut(|d| {
            d.remove::<Option<usize>>(index_key);
            d.remove::<egui::Rect>(rect_key);
        });
        return;
    }
    if matches.is_empty() || !(resp.has_focus() || over_popup) {
        ui.data_mut(|d| {
            d.remove::<Option<usize>>(index_key);
            d.remove::<egui::Rect>(rect_key);
        });
        return;
    }
    let mut index = index.filter(|&i| i < matches.len());
    if resp.has_focus() {
        ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                index = Some(index.map_or(0, |x| (x + 1).min(matches.len() - 1)));
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                index = index.and_then(|x| x.checked_sub(1));
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                index = None;
            }
        });
    }
    let mut picked = None;
    let area = egui::Area::new(id.with("popup"))
        .fixed_pos(resp.rect.left_bottom() + egui::vec2(0.0, 2.0))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(resp.rect.width() - 12.0);
                for (i, m) in matches.iter().enumerate() {
                    let r = ui.add(
                        egui::Button::selectable(index == Some(i), m.as_str())
                            .min_size(egui::vec2(ui.available_width(), 0.0)),
                    );
                    if r.clicked() {
                        picked = Some((*m).clone());
                    }
                }
            });
        });
    if let Some(p) = picked {
        *value = p;
        ui.data_mut(|d| {
            d.remove::<Option<usize>>(index_key);
            d.remove::<egui::Rect>(rect_key);
        });
        resp.request_focus();
        return;
    }
    ui.data_mut(|d| {
        d.insert_temp(index_key, index);
        d.insert_temp(rect_key, area.response.rect);
        if index.is_some() {
            d.insert_temp(egui::Id::new(COMPLETION_ACTIVE), true);
        }
    });
}

fn take_focus(resp: &egui::Response, focus: Option<FieldFocus>, me: FieldFocus) {
    if focus == Some(me) {
        resp.request_focus();
    }
}

fn key_hint(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌥{key}")
    } else {
        format!("Alt+{key}")
    }
}

fn swatch(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
}

fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(7, 1))
        .show(ui, |ui| ui.label(RichText::new(text).small().strong().color(color)));
}

/// `None` = noch kein Vorschlag, `Some(None)` = vom Nutzer geändert, `Some(Some(c))` = Vorschlag mit Sicherheit.
fn confidence(ui: &mut egui::Ui, conf: Option<u8>, threshold: u8) {
    let ctx = ui.ctx().clone();
    match conf {
        Some(c) => badge(ui, &format!("{c} %"), theme::confidence(&ctx, c, threshold)),
        None => badge(ui, tr("manuell"), theme::accent(&ctx)),
    }
}

fn field_header(ui: &mut egui::Ui, color: Color32, label: &str, conf: Option<Option<u8>>, threshold: u8, key: &str) {
    ui.horizontal(|ui| {
        swatch(ui, color);
        ui.label(RichText::new(label).strong());
        if let Some(conf) = conf {
            confidence(ui, conf, threshold);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(key_hint(key)).monospace().small().weak());
        });
    });
}

fn date_chips(ui: &mut egui::Ui, s: &Suggestion, e: &mut Entry) {
    let mut seen = Vec::new();
    ui.horizontal_wrapped(|ui| {
        for c in &s.dates {
            if seen.contains(&c.date) {
                continue;
            }
            seen.push(c.date);
            let selected = e.fields.datum == Some(c.date);
            let text = format!("{}  {}", c.date.format("%d.%m.%Y"), date_kind(c.kind));
            if ui.selectable_label(selected, RichText::new(text).small()).clicked() {
                e.fields.datum = Some(c.date);
                e.date_text = c.date.format("%Y-%m-%d").to_string();
            }
        }
    });
}

pub fn date_kind(kind: DateKind) -> &'static str {
    match kind {
        DateKind::Document => tr("Dokumentdatum"),
        DateKind::Due => tr("Fälligkeit"),
        DateKind::PeriodStart => tr("Zeitraum von"),
        DateKind::PeriodEnd => tr("Zeitraum bis"),
        DateKind::Service => tr("Leistungsdatum"),
        DateKind::Other => tr("sonstiges Datum"),
        DateKind::Unknown => tr("Datum"),
    }
}

/// Herkunft eines Vorschlags als kurzer Satz.
pub fn describe(source: &Source) -> String {
    match source {
        Source::Nothing => tr("Nichts gefunden – bitte von Hand eintragen").into(),
        Source::Date(DateKind::Document) => tr("Im Dokument als Datum ausgewiesen").into(),
        Source::Date(DateKind::Unknown) => tr("Plausibelstes Datum im Dokument").into(),
        Source::Date(_) => tr("Unsicher: kein ausgewiesenes Dokumentdatum gefunden").into(),
        Source::Letterhead { sender_line: true } => tr("Aus dem Briefkopf, bestätigt durch die Absenderzeile").into(),
        Source::Letterhead { sender_line: false } => tr("Aus dem Briefkopf (größte Schrift)").into(),
        Source::Topic(TopicSource::SubjectLine) => tr("Aus der Betreffzeile").into(),
        Source::Topic(TopicSource::LineItem) => tr("Aus der ersten Rechnungsposition").into(),
        Source::InvoiceSignals(s) if s.is_empty() => tr("Keine Rechnungsmerkmale gefunden").into(),
        Source::Knowledge {
            kennung: Some(k),
            dokumente,
        } => trf(
            "Gelernt über {} ({} Dokumente)",
            &[&crate::settings_ui::id_label(*k), dokumente],
        ),
        Source::Knowledge {
            kennung: None,
            dokumente,
        } => trf("Gelernt über den Namen im Briefkopf ({} Dokumente)", &[dokumente]),
        Source::KnownTopic { anzahl } => trf("Häufigstes Thema dieses Absenders ({}×)", &[anzahl]),
        Source::InvoiceSignals(s) => {
            let list = s.iter().map(|w| format!("„{w}“")).collect::<Vec<_>>().join(", ");
            trf("Gefunden: {}", &[&list])
        }
    }
}

fn source_line(ui: &mut egui::Ui, source: &Source) {
    ui.label(RichText::new(describe(source)).small().weak());
}

fn result_box(ui: &mut egui::Ui, e: &Entry, c: &FormCtx<'_>) {
    let ctx = ui.ctx().clone();
    egui::Frame::group(ui.style())
        .corner_radius(9)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(tr("Neuer Dateiname")).small().strong().weak());
            ui.label(RichText::new(&e.original).monospace().small().strikethrough().weak());
            let Some(template) = c.template else {
                ui.colored_label(
                    theme::bad(&ctx),
                    tr("Die Dateinamen-Vorlage ist fehlerhaft, bitte in den Einstellungen korrigieren."),
                );
                return;
            };
            let name = template.file_name(&e.fields, c.naming);
            ui.add(egui::Label::new(RichText::new(&name).monospace().strong()).wrap());
            match c.target_dir {
                Some(dir) => ui.label(RichText::new(format!("→ {}", dir.display())).small().weak()),
                None => ui.label(RichText::new(tr("Bleibt im Eingangsordner")).small().weak()),
            };
            let mut problems = Vec::new();
            if e.fields.datum.is_none() {
                problems.push(tr("Datum fehlt oder ist ungültig").to_string());
            }
            if e.fields.unternehmen.trim().is_empty() {
                problems.push(tr("Unternehmen fehlt").to_string());
            }
            if e.is_open() && c.taken.contains(&name) {
                problems.push(tr("Name existiert bereits → wird mit _2 gespeichert").to_string());
            }
            if !problems.is_empty() {
                ui.colored_label(theme::bad(&ctx), RichText::new(problems.join(" · ")).small());
            }
        });
}

/// Aufklappbar: was beim Umbenennen ins PDF bzw. als Finder-Tag geschrieben wird; Titel und Stichwörter änderbar.
fn metadata_section(ui: &mut egui::Ui, e: &mut Entry, c: &FormCtx<'_>) {
    let m = c.meta;
    if !(m.info || m.xmp || m.finder_tags || m.embed_ocr) {
        return;
    }
    let (title, keywords, tags) = c.meta_preview;
    let edited = e.meta_title.is_some() || e.meta_keywords.is_some();
    let header = if edited {
        tr("PDF-Metadaten (bearbeitet)")
    } else {
        tr("PDF-Metadaten")
    };
    egui::CollapsingHeader::new(RichText::new(header).strong())
        .id_salt("metadaten")
        .show(ui, |ui| {
            if m.info || m.xmp {
                ui.label(RichText::new(tr("Titel")).small().weak());
                let mut t = title.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut t).desired_width(f32::INFINITY))
                    .changed()
                {
                    e.meta_title = Some(t);
                }
                ui.label(RichText::new(tr("Stichwörter (kommagetrennt)")).small().weak());
                let mut k = keywords.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut k).desired_width(f32::INFINITY))
                    .changed()
                {
                    e.meta_keywords = Some(k);
                }
            }
            if m.finder_tags && cfg!(target_os = "macos") {
                ui.label(RichText::new(tr("Finder-Tags")).small().weak());
                ui.horizontal_wrapped(|ui| {
                    for t in tags {
                        badge(ui, t, theme::accent(ui.ctx()));
                    }
                });
            }
            let is_scan = matches!(&e.analysis, AnalysisState::Ready(a) if matches!(a.source, TextSource::Ocr { .. }));
            if m.embed_ocr && is_scan {
                ui.label(
                    RichText::new(tr(
                        "Die erkannte Schrift wird als unsichtbare Textebene eingebettet (Volltextsuche).",
                    ))
                    .small()
                    .weak(),
                );
            }
            if edited && ui.small_button(tr("Automatisch")).clicked() {
                e.meta_title = None;
                e.meta_keywords = None;
            }
        });
}
