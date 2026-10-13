// SPDX-License-Identifier: MIT OR Apache-2.0
//! Zustand und Logik der App. Die Oberflächenmodule zeichnen nur und melden [`Action`]s zurück;
//! ausgeführt werden sie hier.

use std::path::{Path, PathBuf};

use af_core::Suggestion;
use af_core::files::Journal;
use af_core::naming::{NameFields, Template, TemplateError};
use chrono::NaiveDate;
use eframe::egui::{self, Event, Key, KeyboardShortcut, Modifiers};

use crate::devtools::DevTools;
use crate::i18n::{tr, trf};
use crate::settings::{Settings, ThemeSetting};
use crate::settings_ui::SettingsUi;
use crate::worker::{Analysis, Done, Job, Worker};
use crate::{file_list, form, preview, settings, settings_ui, theme};

/// Ab dieser Sicherheit (jedes Feld) gilt ein Vorschlag als sicher; darunter „Prüfen“.
pub const THRESHOLD: u8 = 90;
/// So viele Dateien nach der aktuellen werden im Voraus erkannt.
const PREFETCH: usize = 3;

pub enum AnalysisState {
    Missing,
    Pending,
    Ready(Box<Analysis>),
    Failed(String),
}

pub struct Preview {
    pub page: usize,
    pub width: u32,
    pub texture: egui::TextureHandle,
}

pub struct Entry {
    /// eindeutig über alle Ordnerwechsel, damit späte Ergebnisse nicht beim Falschen landen
    pub id: u64,
    pub path: PathBuf,
    /// Dateiname beim Einlesen
    pub original: String,
    /// erledigt; mit Journal-Nummer, wenn in dieser Sitzung umbenannt (dann rückgängig machbar)
    pub done: Option<Option<u64>>,
    pub analysis: AnalysisState,
    pub fields: NameFields,
    /// Datum als Text im Eingabefeld (JJJJ-MM-TT, auch TT.MM.JJJJ wird verstanden)
    pub date_text: String,
    pub page: usize,
    pub preview: Option<Preview>,
    pub render_pending: Option<(usize, u32)>,
}

impl Entry {
    pub fn suggestion(&self) -> Option<&Suggestion> {
        match &self.analysis {
            AnalysisState::Ready(a) => Some(&a.suggestion),
            _ => None,
        }
    }

    pub fn pages(&self) -> usize {
        match &self.analysis {
            AnalysisState::Ready(a) => a.pages,
            _ => 1,
        }
    }

    pub fn is_open(&self) -> bool {
        self.done.is_none()
    }

    /// Offen und unsicher (oder nicht lesbar): gehört in den Filter „Prüfen“.
    pub fn needs_check(&self) -> bool {
        self.is_open()
            && match &self.analysis {
                AnalysisState::Ready(a) => !a.suggestion.is_complete() || a.suggestion.min_confidence() < THRESHOLD,
                AnalysisState::Failed(_) => true,
                _ => false,
            }
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filter {
    All,
    Open,
    Check,
    Done,
}

impl Filter {
    pub fn matches(self, e: &Entry) -> bool {
        match self {
            Filter::All => true,
            Filter::Open => e.is_open(),
            Filter::Check => e.needs_check(),
            Filter::Done => !e.is_open(),
        }
    }
}

/// Was die Oberfläche auslösen kann.
pub enum Action {
    Select(usize),
    Accept,
    Skip,
    Previous,
    Undo,
    ResetFields,
    SetFilter(Filter),
    ChooseFolder,
    OpenFolder(PathBuf),
    ToggleList,
    OpenSettings,
    Page(usize),
    Zoom(f32),
    SettingsChanged,
}

/// Feld, das im nächsten Bild den Fokus bekommen soll.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FieldFocus {
    Datum,
    Unternehmen,
    Thema,
}

impl FieldFocus {
    pub fn id(self) -> egui::Id {
        egui::Id::new(match self {
            FieldFocus::Datum => "feld-datum",
            FieldFocus::Unternehmen => "feld-unternehmen",
            FieldFocus::Thema => "feld-thema",
        })
    }
}

struct Toast {
    text: String,
    undo: bool,
    error: bool,
    until: f64,
}

pub struct App {
    pub settings: Settings,
    persist: bool,
    dev: DevTools,
    pub inbox: Option<PathBuf>,
    journal: Option<Journal>,
    pub entries: Vec<Entry>,
    pub current: Option<usize>,
    pub filter: Filter,
    worker: Worker,
    next_id: u64,
    pub template: Result<Template, TemplateError>,
    settings_ui: SettingsUi,
    toast: Option<Toast>,
    /// (Eintrag, Journal-Nummer) in Reihenfolge der Umbenennungen dieser Sitzung
    undo_stack: Vec<(usize, u64)>,
    focus: Option<FieldFocus>,
    preview_width: f32,
    applied_theme: Option<ThemeSetting>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        settings: Settings,
        dev: DevTools,
        start_dir: Option<PathBuf>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        crate::fonts::install(&cc.egui_ctx);
        let persist = !dev.isolated();
        let template = Template::parse(&settings.template);
        let mut app = App {
            persist,
            inbox: None,
            journal: None,
            entries: Vec::new(),
            current: None,
            filter: Filter::All,
            worker: Worker::spawn(cc.egui_ctx.clone()),
            next_id: 1,
            template,
            settings_ui: SettingsUi::default(),
            toast: None,
            undo_stack: Vec::new(),
            focus: None,
            preview_width: 600.0,
            applied_theme: None,
            settings,
            dev,
        };
        if let Some(page) = app.dev.settings_page.clone() {
            app.settings_ui.open_page(&page, &app.settings);
        }
        let dir = if app.dev.demo {
            Some(DevTools::demo_dir())
        } else {
            start_dir
                .or_else(|| app.settings.recent_inboxes.first().cloned())
                .filter(|d| d.is_dir())
        };
        if let Some(dir) = dir {
            app.open_inbox(dir);
        }
        if let Some(n) = app.dev.file
            && n < app.entries.len()
        {
            app.current = Some(n);
        }
        app
    }

    fn save_settings(&self) {
        if self.persist {
            self.settings.save();
        }
    }

    fn open_inbox(&mut self, dir: PathBuf) {
        let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
                .collect(),
            Err(e) => {
                self.show_error(trf("Ordner nicht lesbar: {}", &[&e]));
                return;
            }
        };
        files.sort();
        self.entries = files
            .into_iter()
            .map(|path| {
                let original = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                // Schon nach der Vorlage benannt: gilt als erledigt
                let done = self
                    .template
                    .as_ref()
                    .ok()
                    .and_then(|t| t.parse_name(&original))
                    .map(|_| None);
                let id = self.next_id;
                self.next_id += 1;
                Entry {
                    id,
                    path,
                    original,
                    done,
                    analysis: AnalysisState::Missing,
                    fields: NameFields::default(),
                    date_text: String::new(),
                    page: 0,
                    preview: None,
                    render_pending: None,
                }
            })
            .collect();
        self.journal = Some(Journal::for_inbox(&dir, settings::config_dir().as_deref()));
        self.undo_stack.clear();
        self.current = self
            .entries
            .iter()
            .position(Entry::is_open)
            .or(if self.entries.is_empty() { None } else { Some(0) });
        self.settings.remember_inbox(&dir);
        self.inbox = Some(dir);
        self.save_settings();
    }

    fn show_error(&mut self, text: String) {
        log::error!("{text}");
        self.toast = Some(Toast {
            text,
            undo: false,
            error: true,
            until: f64::MAX,
        });
    }

    fn show_info(&mut self, ctx: &egui::Context, text: String, undo: bool) {
        let now = ctx.input(|i| i.time);
        self.toast = Some(Toast {
            text,
            undo,
            error: false,
            until: now + 5.0,
        });
    }

    // ---------- Hintergrundarbeit ----------

    fn poll_worker(&mut self, ctx: &egui::Context) {
        while let Some(done) = self.worker.try_recv() {
            match done {
                Done::Analyzed { id, result } => {
                    let Some(e) = self.entries.iter_mut().find(|e| e.id == id) else {
                        continue;
                    };
                    match result {
                        Ok(a) => {
                            e.fields = a.suggestion.fields();
                            e.date_text = e
                                .fields
                                .datum
                                .map(|d| d.format("%Y-%m-%d").to_string())
                                .unwrap_or_default();
                            e.analysis = AnalysisState::Ready(a);
                        }
                        Err(err) => {
                            log::warn!("{}: {err}", e.path.display());
                            e.analysis = AnalysisState::Failed(err);
                        }
                    }
                }
                Done::Rendered {
                    id,
                    page,
                    width,
                    result,
                } => {
                    let Some(e) = self.entries.iter_mut().find(|e| e.id == id) else {
                        continue;
                    };
                    e.render_pending = None;
                    match result {
                        Ok(img) => {
                            let image = egui::ColorImage::from_rgba_unmultiplied([img.width, img.height], &img.rgba);
                            let texture =
                                ctx.load_texture(format!("seite-{id}-{page}"), image, egui::TextureOptions::LINEAR);
                            e.preview = Some(Preview { page, width, texture });
                        }
                        Err(err) => log::warn!("Vorschau {}: {err}", e.path.display()),
                    }
                }
            }
        }
    }

    /// Aktuelle Datei und die nächsten offenen erkennen lassen, Vorschau in passender Größe rendern.
    fn schedule_jobs(&mut self, ctx: &egui::Context) {
        let Some(cur) = self.current else { return };
        let n = self.entries.len();
        let mut wanted = vec![cur];
        wanted.extend(
            (1..n)
                .map(|k| (cur + k) % n)
                .filter(|&i| self.entries[i].is_open())
                .take(PREFETCH),
        );
        for i in wanted {
            let e = &mut self.entries[i];
            if matches!(e.analysis, AnalysisState::Missing) {
                e.analysis = AnalysisState::Pending;
                self.worker.send(Job::Analyze {
                    id: e.id,
                    path: e.path.clone(),
                });
            }
        }
        // Breite in Stufen, damit beim Ziehen am Fenster nicht ständig neu gerendert wird
        let px = self.preview_width * ctx.pixels_per_point() * self.settings.zoom;
        let width = ((px / 256.0).ceil() * 256.0).clamp(512.0, 3072.0) as u32;
        let e = &mut self.entries[cur];
        let good = e.preview.as_ref().is_some_and(|p| p.page == e.page && p.width >= width);
        if !good && e.render_pending != Some((e.page, width)) {
            e.render_pending = Some((e.page, width));
            self.worker.send(Job::Render {
                id: e.id,
                path: e.path.clone(),
                page: e.page,
                width,
            });
        }
    }

    // ---------- Aktionen ----------

    fn select(&mut self, i: usize) {
        if i < self.entries.len() {
            self.current = Some(i);
        }
    }

    fn next_open_after(&self, i: usize) -> Option<usize> {
        let n = self.entries.len();
        (1..=n).map(|k| (i + k) % n).find(|&j| self.entries[j].is_open())
    }

    fn accept(&mut self, ctx: &egui::Context) {
        let Some(i) = self.current else { return };
        let Ok(template) = &self.template else {
            self.show_error(
                tr("Die Dateinamen-Vorlage ist fehlerhaft, bitte in den Einstellungen korrigieren.").into(),
            );
            return;
        };
        let e = &self.entries[i];
        if !e.is_open() || e.fields.datum.is_none() || e.fields.unternehmen.trim().is_empty() {
            return;
        }
        let name = template.file_name(&e.fields, &self.settings.naming);
        let target_dir = self.target_dir(&e.path, &e.fields);
        let Some(journal) = &self.journal else { return };
        match journal.rename(&e.path, &target_dir, &name) {
            Ok(entry) => {
                let new_name = entry
                    .nach
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let e = &mut self.entries[i];
                e.path = entry.nach.clone();
                e.done = Some(Some(entry.id));
                self.undo_stack.push((i, entry.id));
                self.show_info(ctx, trf("Umbenannt → {}", &[&new_name]), true);
                match self.next_open_after(i) {
                    Some(next) => self.current = Some(next),
                    None => self.show_info(ctx, tr("Alle Dateien sind erledigt.").into(), true),
                }
            }
            Err(err) => self.show_error(trf("Umbenennen fehlgeschlagen: {}", &[&err])),
        }
    }

    /// Zielordner: Archiv mit Unterordner-Vorlage oder der Ordner der Datei.
    fn target_dir(&self, path: &Path, fields: &NameFields) -> PathBuf {
        let here = path.parent().map(Path::to_path_buf).unwrap_or_default();
        if !self.settings.move_to_archive {
            return here;
        }
        let Some(root) = &self.settings.archive_root else {
            return here;
        };
        let mut dir = root.clone();
        if let Ok(t) = Template::parse(&self.settings.folder_template) {
            for part in t.render(fields, &self.settings.naming).split('/') {
                if !part.is_empty() && part != "." && part != ".." {
                    dir.push(part);
                }
            }
        }
        dir
    }

    fn undo(&mut self, ctx: &egui::Context) {
        let Some((i, id)) = self.undo_stack.pop() else { return };
        let Some(journal) = &self.journal else { return };
        match journal.undo(id) {
            Ok(entry) => {
                let e = &mut self.entries[i];
                e.path = entry.nach;
                e.done = None;
                self.current = Some(i);
                self.show_info(ctx, tr("Rückgängig gemacht").into(), false);
            }
            Err(err) => self.show_error(trf("Rückgängig fehlgeschlagen: {}", &[&err])),
        }
    }

    fn choose_folder(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title(tr("Eingangsordner wählen"));
        if let Some(dir) = &self.inbox {
            dialog = dialog.set_directory(dir);
        }
        if let Some(dir) = dialog.pick_folder() {
            self.open_inbox(dir);
        }
    }

    fn run(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Select(i) => self.select(i),
            Action::Accept => self.accept(ctx),
            Action::Skip => {
                if let Some(i) = self.current {
                    self.select((i + 1) % self.entries.len().max(1));
                }
            }
            Action::Previous => {
                if let Some(i) = self.current {
                    let n = self.entries.len().max(1);
                    self.select((i + n - 1) % n);
                }
            }
            Action::Undo => self.undo(ctx),
            Action::ResetFields => {
                if let Some(e) = self.current.and_then(|i| self.entries.get_mut(i))
                    && let Some(s) = e.suggestion()
                {
                    e.fields = s.fields();
                    e.date_text = e
                        .fields
                        .datum
                        .map(|d| d.format("%Y-%m-%d").to_string())
                        .unwrap_or_default();
                }
            }
            Action::SetFilter(f) => self.filter = f,
            Action::ChooseFolder => self.choose_folder(),
            Action::OpenFolder(dir) => self.open_inbox(dir),
            Action::ToggleList => {
                self.settings.show_file_list = !self.settings.show_file_list;
                self.save_settings();
            }
            Action::OpenSettings => self.settings_ui.open_page("allgemein", &self.settings),
            Action::Page(p) => {
                if let Some(e) = self.current.and_then(|i| self.entries.get_mut(i)) {
                    e.page = p.min(e.pages().saturating_sub(1));
                }
            }
            Action::Zoom(z) => {
                self.settings.zoom = z.clamp(0.5, 2.5);
                self.save_settings();
            }
            Action::SettingsChanged => {
                self.template = Template::parse(&self.settings.template);
                crate::i18n::apply_setting(self.settings.language);
                self.save_settings();
            }
        }
    }

    // ---------- Tastatur ----------

    fn shortcuts(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if self.settings_ui.open {
            return;
        }
        let cmd = |key| KeyboardShortcut::new(Modifiers::COMMAND, key);
        let alt = |key| KeyboardShortcut::new(Modifiers::ALT, key);
        let mut alt_used = false;
        ctx.input_mut(|i| {
            if i.consume_shortcut(&cmd(Key::ArrowDown)) {
                actions.push(Action::Skip);
            }
            if i.consume_shortcut(&cmd(Key::ArrowUp)) {
                actions.push(Action::Previous);
            }
            if i.consume_shortcut(&cmd(Key::L)) {
                actions.push(Action::ToggleList);
            }
            if i.consume_shortcut(&cmd(Key::Comma)) {
                actions.push(Action::OpenSettings);
            }
            if i.consume_shortcut(&cmd(Key::O)) {
                actions.push(Action::ChooseFolder);
            }
            for (key, field) in [
                (Key::Num1, FieldFocus::Datum),
                (Key::Num2, FieldFocus::Unternehmen),
                (Key::Num3, FieldFocus::Thema),
            ] {
                if i.consume_shortcut(&alt(key)) {
                    self.focus = Some(field);
                    alt_used = true;
                }
            }
            if i.consume_shortcut(&alt(Key::R)) {
                if let Some(e) = self.current.and_then(|c| self.entries.get_mut(c)) {
                    e.fields.rechnung = !e.fields.rechnung;
                }
                alt_used = true;
            }
            // ⌥ + Taste erzeugt auf dem Mac zusätzlich ein Zeichen („¡“, „®“), das nicht im Feld landen soll
            if alt_used {
                i.events.retain(|e| !matches!(e, Event::Text(_)));
            }
            if i.key_pressed(Key::Enter) && i.modifiers.is_none() {
                actions.push(Action::Accept);
            }
        });
        // ⌘Z ohne Fokus in einem Textfeld: letzte Umbenennung zurücknehmen
        if ctx.memory(|m| m.focused().is_none()) && ctx.input_mut(|i| i.consume_shortcut(&cmd(Key::Z))) {
            actions.push(Action::Undo);
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme == Some(self.settings.theme) {
            return;
        }
        self.applied_theme = Some(self.settings.theme);
        ctx.set_theme(match self.settings.theme {
            ThemeSetting::System => egui::ThemePreference::System,
            ThemeSetting::Light => egui::ThemePreference::Light,
            ThemeSetting::Dark => egui::ThemePreference::Dark,
        });
    }

    // ---------- Zeichnen ----------

    fn toolbar(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal_centered(|ui| {
            if ui
                .selectable_label(self.settings.show_file_list, "☰")
                .on_hover_text(tr("Dateiliste ein/aus (⌘L)"))
                .clicked()
            {
                actions.push(Action::ToggleList);
            }
            let label = self
                .inbox
                .as_ref()
                .map_or_else(|| tr("Ordner wählen …").to_string(), |d| d.display().to_string());
            ui.menu_button(format!("📁 {label}"), |ui| {
                if ui.button(tr("Ordner wählen …")).clicked() {
                    actions.push(Action::ChooseFolder);
                }
                if !self.settings.recent_inboxes.is_empty() {
                    ui.separator();
                    ui.label(egui::RichText::new(tr("Zuletzt benutzt")).small().weak());
                    for dir in &self.settings.recent_inboxes {
                        if ui.button(dir.display().to_string()).clicked() {
                            actions.push(Action::OpenFolder(dir.clone()));
                        }
                    }
                }
            });
            if !self.entries.is_empty() {
                let done = self.entries.iter().filter(|e| !e.is_open()).count();
                let total = self.entries.len();
                ui.add(egui::ProgressBar::new(done as f32 / total as f32).desired_width(140.0));
                ui.label(egui::RichText::new(trf("{} / {} erledigt", &[&done, &total])).weak());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!("⚙ {}", tr("Einstellungen"))).clicked() {
                    actions.push(Action::OpenSettings);
                }
            });
        });
    }

    fn statusbar(ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            let mac = cfg!(target_os = "macos");
            let (cmd, alt) = if mac { ("⌘", "⌥") } else { ("Strg+", "Alt+") };
            let items = [
                ("↵".to_string(), tr("übernehmen")),
                (format!("{cmd}↓"), tr("überspringen / nächste")),
                (format!("{cmd}↑"), tr("vorherige")),
                (format!("{alt}1–3"), tr("Feld wählen")),
                (format!("{alt}R"), tr("Rechnung")),
                (format!("{cmd}Z"), tr("rückgängig")),
                (format!("{cmd}L"), tr("Dateiliste")),
                (format!("{cmd},"), tr("Einstellungen")),
            ];
            for (key, label) in items {
                ui.label(egui::RichText::new(key).monospace().small());
                ui.label(egui::RichText::new(label).small().weak());
                ui.add_space(6.0);
            }
        });
    }

    fn empty_state(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.heading("Ablagefuchs");
            ui.add_space(8.0);
            let text = if self.inbox.is_some() {
                tr("In diesem Ordner liegen keine PDF-Dateien.")
            } else {
                tr("Wähle den Ordner, in den dein Scanner die Dokumente legt.")
            };
            ui.label(text);
            ui.add_space(12.0);
            if ui
                .button(egui::RichText::new(format!("📁 {}", tr("Ordner wählen …"))).size(16.0))
                .clicked()
            {
                actions.push(Action::ChooseFolder);
            }
        });
    }

    fn toast(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let now = ctx.input(|i| i.time);
        if self.toast.as_ref().is_some_and(|t| now > t.until) {
            self.toast = None;
        }
        let Some(t) = &self.toast else { return };
        let mut close = false;
        egui::Area::new(egui::Id::new("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -44.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let fill = if t.error {
                    theme::bad(ctx)
                } else {
                    egui::Color32::from_rgb(0x1c, 0x1c, 0x1f)
                };
                egui::Frame::new()
                    .fill(fill)
                    .corner_radius(10)
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&t.text).color(egui::Color32::WHITE));
                            if t.undo && !self.undo_stack.is_empty() && ui.button(tr("Rückgängig")).clicked() {
                                actions.push(Action::Undo);
                            }
                            if t.error && ui.button(tr("Schließen")).clicked() {
                                close = true;
                            }
                        });
                    });
            });
        if close {
            self.toast = None;
        } else if self.toast.as_ref().is_some_and(|t| t.until < f64::MAX) {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.dev.inject_keys(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.cumulative_frame_nr() >= 2 {
            crate::diagnose::graphics_running();
        }
        self.apply_theme(&ctx);
        self.poll_worker(&ctx);
        let mut actions = Vec::new();
        self.shortcuts(&ctx, &mut actions);

        egui::Panel::top("toolbar")
            .exact_size(48.0)
            .show(ui, |ui| self.toolbar(ui, &mut actions));
        egui::Panel::bottom("statusbar")
            .exact_size(26.0)
            .show(ui, Self::statusbar);

        if self.entries.is_empty() {
            egui::CentralPanel::default().show(ui, |ui| self.empty_state(ui, &mut actions));
        } else {
            if self.settings.show_file_list {
                egui::Panel::left("dateiliste")
                    .resizable(true)
                    .default_size(270.0)
                    .min_size(200.0)
                    .show(ui, |ui| {
                        file_list::show(ui, &self.entries, self.current, self.filter, &mut actions);
                    });
            }
            let focus = self.focus.take();
            let threshold = THRESHOLD;
            if let Some(cur) = self.current {
                let target = {
                    let e = &self.entries[cur];
                    self.target_dir(&e.path, &e.fields)
                };
                let shown_target = self.settings.move_to_archive.then_some(target);
                let other_names: Vec<String> = self
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(i, e)| *i != cur && !e.is_open())
                    .map(|(_, e)| e.file_name())
                    .collect();
                let fctx = form::FormCtx {
                    template: self.template.as_ref().ok(),
                    naming: &self.settings.naming,
                    threshold,
                    target_dir: shown_target.as_deref(),
                    taken: &other_names,
                    can_undo: !self.undo_stack.is_empty(),
                };
                egui::Panel::right("formular").exact_size(380.0).show(ui, |ui| {
                    form::show(ui, &mut self.entries[cur], &fctx, focus, &mut actions);
                });
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::desk(&ctx)))
                .show(ui, |ui| {
                    let entry = self.current.map(|i| &self.entries[i]);
                    self.preview_width = preview::show(ui, entry, self.settings.zoom, &mut actions);
                });
        }

        let mut settings_changed = false;
        settings_ui::show(
            &ctx,
            &mut self.settings_ui,
            &mut self.settings,
            &self.entries,
            &mut settings_changed,
        );
        if settings_changed {
            actions.push(Action::SettingsChanged);
        }
        self.toast(&ctx, &mut actions);

        for a in actions {
            self.run(&ctx, a);
        }
        self.schedule_jobs(&ctx);
        self.dev.on_frame(&ctx);
    }
}

/// Datum aus dem Eingabefeld: `2026-09-28`, `28.09.2026` oder `28.9.26`.
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(s, "%d.%m.%Y"))
        .or_else(|_| NaiveDate::parse_from_str(s, "%d.%m.%y"))
        .ok()
}
