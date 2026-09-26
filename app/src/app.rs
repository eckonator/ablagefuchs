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

/// Ab dieser Sicherheit (jedes Feld) gilt ein Vorschlag als sicher; darunter „Prüfen“ (Einstellung).
static THRESHOLD: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(90);

pub fn threshold() -> u8 {
    THRESHOLD.load(std::sync::atomic::Ordering::Relaxed)
}

fn set_threshold(t: u8) {
    THRESHOLD.store(t.clamp(50, 100), std::sync::atomic::Ordering::Relaxed);
}
/// Höchstens so viele Erkennungen gleichzeitig in der Warteschlange: alle offenen Dateien werden nach und
/// nach im Hintergrund erkannt (für Filter und Stapelmodus), die aktuelle wartet aber nie lange.
const MAX_PENDING: usize = 2;

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
    /// Seitengröße in Punkt, um Fundstellen (in Punkt) aufs Bild umzurechnen
    pub page_size_pt: egui::Vec2,
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
    /// Datei ist von außen verschwunden (gelöscht, verschoben)
    pub gone: bool,
    /// Größe und Änderungszeit beim letzten Lesen: ändern sie sich, wird neu gelesen
    file_state: FileState,
    /// Stand des Wissens bei der Erkennung (ändert es sich, werden unbearbeitete Dateien neu erkannt)
    analyzed_rev: u64,
    /// Im Formular überschriebener Titel bzw. Stichwörter (sonst aus den Feldern)
    pub meta_title: Option<String>,
    pub meta_keywords: Option<String>,
    /// Andere Datei mit gleichem Inhalt/Text (doppelt gescannt?), wird je Bild aktualisiert
    pub duplicate: Option<String>,
}

type FileState = (u64, Option<std::time::SystemTime>);

fn file_state(path: &Path) -> FileState {
    std::fs::metadata(path).map_or((0, None), |m| (m.len(), m.modified().ok()))
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
        self.done.is_none() && !self.gone
    }

    /// Offen und unsicher (oder nicht lesbar): gehört in den Filter „Prüfen“.
    pub fn needs_check(&self) -> bool {
        self.is_open()
            && match &self.analysis {
                AnalysisState::Ready(a) => {
                    !a.suggestion.is_complete()
                        || a.suggestion.min_confidence() < threshold()
                        || self.duplicate.is_some()
                }
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
            Filter::Done => e.done.is_some(),
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
    /// Datum aus dem Dokument übernehmen (Klick auf ein unterstrichenes Datum)
    SetDate(NaiveDate),
    /// Feld fokussieren (Klick auf eine Markierung)
    Focus(FieldFocus),
    /// Text in der Vorschau markiert: Menü an dieser Stelle anbieten
    Selection {
        text: String,
        pos: egui::Pos2,
    },
    ApplySelection(FieldFocus),
    CloseSelection,
    ToggleHighlights,
    /// Stapelmodus: alle sicheren Vorschläge umbenennen
    Batch,
    /// Lernen aus dem gewählten Ordner starten (rekursiv)
    StartLearning,
    CancelLearning,
    /// Ein gelerntes Unternehmen vergessen
    ForgetCompany(usize),
    /// Gesamtes Wissen löschen
    ForgetKnowledge,
    /// Thema aus den bekannten Themen übernehmen
    SetTopic(String),
    /// Aktuelle Datei in den Papierkorb (doppelt gescannt)
    TrashCurrent,
}

/// Ausgewählter Text aus der Vorschau, der als Feld übernommen werden kann.
struct SelectionMenu {
    text: String,
    pos: egui::Pos2,
    /// Fläche des Menüs im letzten Bild (Klick daneben schließt es)
    rect: Option<egui::Rect>,
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
    /// Umbenennungen dieser Sitzung als Gruppen (Stapelmodus = eine Gruppe): (Eintrag, Journal-Nummer)
    undo_stack: Vec<Vec<(usize, u64)>>,
    /// Gelerntes Wissen; geteilt mit dem Analyse-Thread
    pub knowledge: std::sync::Arc<af_core::knowledge::Knowledge>,
    knowledge_rev: u64,
    knowledge_file: Option<PathBuf>,
    learning: Option<crate::learning::LearnJob>,
    /// Korrekturen, die während des Lernens aus einem Ordner anfallen (werden danach nachgetragen)
    pending_corrections: Vec<(NameFields, af_core::LearnInfo, Option<String>)>,
    pub last_learn: Option<af_ocr::archive::LearnStats>,
    focus: Option<FieldFocus>,
    preview_width: f32,
    /// Ordnerüberwachung: Zeitpunkt der letzten Prüfung und neue Dateien, die noch geschrieben werden könnten
    last_poll: f64,
    pending_files: std::collections::HashMap<PathBuf, FileState>,
    applied_theme: Option<ThemeSetting>,
    selection: Option<SelectionMenu>,
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
        // ⌘ +/− zoomt die Vorschau, nicht die ganze Oberfläche
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
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
            knowledge: std::sync::Arc::new(af_core::knowledge::Knowledge::default()),
            knowledge_rev: 1,
            knowledge_file: None,
            learning: None,
            pending_corrections: Vec::new(),
            last_learn: None,
            focus: None,
            preview_width: 600.0,
            last_poll: 0.0,
            pending_files: std::collections::HashMap::new(),
            applied_theme: None,
            selection: None,
            settings,
            dev,
        };
        set_threshold(app.settings.threshold);
        // Wissen: im Normalbetrieb aus dem Einstellungsordner, bei Entwicklerläufen nur mit AF_WISSEN
        app.knowledge_file = if persist {
            settings::knowledge_path()
        } else {
            std::env::var_os("AF_WISSEN").map(PathBuf::from)
        };
        if let Some(path) = &app.knowledge_file {
            match af_core::knowledge::Knowledge::load(path) {
                Ok(k) => app.knowledge = std::sync::Arc::new(k),
                Err(err) => log::error!("Wissen nicht lesbar ({}): {err}", path.display()),
            }
        }
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
        self.entries = files.into_iter().map(|path| self.new_entry(path)).collect();
        self.journal = Some(Journal::for_inbox(&dir, settings::config_dir().as_deref()));
        self.undo_stack.clear();
        self.pending_files.clear();
        self.last_poll = 0.0;
        self.current = self
            .entries
            .iter()
            .position(Entry::is_open)
            .or(if self.entries.is_empty() { None } else { Some(0) });
        self.settings.remember_inbox(&dir);
        self.inbox = Some(dir);
        self.save_settings();
    }

    fn new_entry(&mut self, path: PathBuf) -> Entry {
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
            original,
            done,
            analysis: AnalysisState::Missing,
            fields: NameFields::default(),
            date_text: String::new(),
            page: 0,
            preview: None,
            render_pending: None,
            gone: false,
            analyzed_rev: 0,
            meta_title: None,
            meta_keywords: None,
            duplicate: None,
            file_state: file_state(&path),
            path,
        }
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
                            let page_size_pt = egui::vec2(img.page_width_pt, img.page_height_pt);
                            e.preview = Some(Preview {
                                page,
                                width,
                                texture,
                                page_size_pt,
                            });
                        }
                        Err(err) => log::warn!("Vorschau {}: {err}", e.path.display()),
                    }
                }
            }
        }
    }

    /// Ordnerüberwachung per Abfrage alle 2 s (zuverlässiger als Dateisystem-Ereignisse auf NAS-Freigaben).
    /// Neue PDFs erscheinen erst, wenn sich Größe und Änderungszeit zwischen zwei Abfragen nicht mehr ändern,
    /// der Scanner also fertig geschrieben hat. Verschwundene Dateien werden als solche markiert.
    fn watch_inbox(&mut self, ctx: &egui::Context) {
        let Some(dir) = self.inbox.clone() else { return };
        let now = ctx.input(|i| i.time);
        ctx.request_repaint_after(std::time::Duration::from_secs(2));
        if now - self.last_poll < 2.0 {
            return;
        }
        self.last_poll = now;
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let known: std::collections::HashSet<PathBuf> = self.entries.iter().map(|e| e.path.clone()).collect();
        let mut seen = std::collections::HashSet::new();
        let mut added = Vec::new();
        for path in rd.filter_map(|e| e.ok().map(|e| e.path())) {
            if !path.is_file()
                || !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
                || known.contains(&path)
            {
                continue;
            }
            seen.insert(path.clone());
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            let state = (meta.len(), meta.modified().ok());
            match self.pending_files.get(&path) {
                Some(prev) if *prev == state && state.0 > 0 => {
                    self.pending_files.remove(&path);
                    added.push(path);
                }
                _ => {
                    self.pending_files.insert(path, state);
                }
            }
        }
        self.pending_files.retain(|p, _| seen.contains(p));
        added.sort();
        for path in added {
            log::info!("Neue Datei im Eingang: {}", path.display());
            let entry = self.new_entry(path);
            self.entries.push(entry);
            if self.current.is_none() {
                self.current = Some(self.entries.len() - 1);
            }
        }
        for e in &mut self.entries {
            if e.done.is_some() {
                continue;
            }
            e.gone = !e.path.exists();
            // Nachträglich verändert (Scanner schrieb mit Pause weiter, Datei ersetzt): neu lesen
            let state = file_state(&e.path);
            if !e.gone && state != e.file_state {
                log::info!("Datei verändert, lese neu: {}", e.path.display());
                e.file_state = state;
                e.analysis = AnalysisState::Missing;
                e.preview = None;
                e.render_pending = None;
            }
        }
    }

    /// Aktuelle Datei und die nächsten offenen erkennen lassen, Vorschau in passender Größe rendern.
    fn schedule_jobs(&mut self, ctx: &egui::Context) {
        let Some(cur) = self.current else { return };
        let n = self.entries.len();
        let mut wanted = vec![cur];
        wanted.extend((1..n).map(|k| (cur + k) % n).filter(|&i| self.entries[i].is_open()));
        // Neues Wissen: Dateien, deren Vorschlag der Nutzer nicht verändert hat, neu erkennen
        for e in &mut self.entries {
            if e.is_open() && e.analyzed_rev != 0 && e.analyzed_rev != self.knowledge_rev {
                let untouched = match &e.analysis {
                    AnalysisState::Ready(a) => a.suggestion.fields() == e.fields,
                    _ => false,
                };
                if untouched {
                    e.analysis = AnalysisState::Missing;
                }
                e.analyzed_rev = 0;
            }
        }
        let mut pending = self
            .entries
            .iter()
            .filter(|e| matches!(e.analysis, AnalysisState::Pending))
            .count();
        for i in wanted {
            let e = &mut self.entries[i];
            if matches!(e.analysis, AnalysisState::Missing) && (i == cur || pending < MAX_PENDING) {
                pending += 1;
                e.analysis = AnalysisState::Pending;
                e.analyzed_rev = self.knowledge_rev;
                self.worker.send(Job::Analyze {
                    id: e.id,
                    path: e.path.clone(),
                    ocr: self.settings.ocr.preference(),
                    knowledge: std::sync::Arc::clone(&self.knowledge),
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
            self.selection = None;
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
        let template = template.clone();
        match self.rename_entry(i, &template) {
            Some(Ok((new_name, id, warning))) => {
                self.undo_stack.push(vec![(i, id)]);
                match warning {
                    None => self.show_info(ctx, trf("Umbenannt → {}", &[&new_name]), true),
                    Some(w) => self.show_info(
                        ctx,
                        trf("Umbenannt → {} (Metadaten nicht geschrieben: {})", &[&new_name, &w]),
                        true,
                    ),
                }
                match self.next_open_after(i) {
                    Some(next) => self.current = Some(next),
                    None => self.show_info(ctx, tr("Alle Dateien sind erledigt.").into(), true),
                }
            }
            Some(Err(err)) => self.show_error(trf("Umbenennen fehlgeschlagen: {}", &[&err])),
            None => {}
        }
    }

    /// Benennt einen Eintrag um, protokolliert es, schreibt Metadaten und lernt aus den bestätigten Feldern.
    /// `None`, wenn der Eintrag nicht bereit ist (erledigt, Datum oder Unternehmen fehlt).
    /// Im Erfolgsfall: neuer Name, Journal-Nummer und ggf. ein Hinweis, falls die Metadaten nicht geschrieben wurden.
    fn rename_entry(
        &mut self,
        i: usize,
        template: &Template,
    ) -> Option<std::io::Result<(String, u64, Option<String>)>> {
        let e = &self.entries[i];
        if !e.is_open() || e.fields.datum.is_none() || e.fields.unternehmen.trim().is_empty() {
            return None;
        }
        let name = template.file_name(&e.fields, &self.settings.naming);
        let target_dir = self.target_dir(&e.path, &e.fields);
        let meta_settings = self.settings.meta.clone();
        // Original nur sichern, wenn die Datei überhaupt verändert wird
        let backup_dir = (meta_settings.keep_originals && meta_settings.modifies_file())
            .then(|| {
                self.inbox
                    .as_ref()
                    .map(|d| d.join(af_core::files::DATA_DIR).join(af_core::files::BACKUP_DIR))
            })
            .flatten();
        let journal = self.journal.as_ref()?;
        let entry = match journal.rename(&e.path, &target_dir, &name, backup_dir.as_deref()) {
            Ok(entry) => entry,
            Err(err) => return Some(Err(err)),
        };
        let new_name = entry
            .nach
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let warning = self.write_file_metadata(i, &entry.nach);
        // Lernen: bestätigte Felder, Kennungen und Briefkopf dieses Dokuments
        let e = &self.entries[i];
        if self.settings.learn_from_corrections
            && let Some(s) = e.suggestion()
        {
            let replaced = (s.unternehmen.value != e.fields.unternehmen && !s.unternehmen.value.is_empty())
                .then(|| s.unternehmen.value.clone());
            let item = (e.fields.clone(), s.learn.clone(), replaced);
            self.learn_correction(item);
        }
        let e = &mut self.entries[i];
        e.path = entry.nach.clone();
        e.done = Some(Some(entry.id));
        Some(Ok((new_name, entry.id, warning)))
    }

    /// Titel, Stichwörter, XMP, OCR-Textebene und Finder-Tags. Fehler verhindern das Umbenennen nicht,
    /// sie werden als Hinweis zurückgegeben.
    fn write_file_metadata(&self, i: usize, path: &Path) -> Option<String> {
        let m = &self.settings.meta;
        let e = &self.entries[i];
        let mut problems = Vec::new();
        if m.modifies_file() {
            let ocr_pages: &[af_core::PageText] = match &e.analysis {
                AnalysisState::Ready(a) if m.embed_ocr && matches!(a.source, af_core::TextSource::Ocr { .. }) => {
                    &a.text
                }
                _ => &[],
            };
            let opts = af_pdf::WriteOptions {
                info: m.info,
                xmp: m.xmp,
            };
            if (m.info || m.xmp || !ocr_pages.is_empty())
                && let Err(err) = af_pdf::write_metadata(path, &self.pdf_meta(e), ocr_pages, opts)
            {
                log::warn!("Metadaten für {}: {err:#}", path.display());
                problems.push(format!("{err:#}"));
            }
        }
        if m.finder_tags
            && let Err(err) = crate::tags::add_finder_tags(path, &finder_tags(&e.fields))
        {
            log::warn!("Finder-Tags für {}: {err}", path.display());
            problems.push(err.to_string());
        }
        (!problems.is_empty()).then(|| problems.join("; "))
    }

    /// Andere Datei im Ordner mit gleichem Inhalt oder gleichem Text (doppelt gescannt?).
    fn duplicate_of(&self, i: usize) -> Option<String> {
        let AnalysisState::Ready(a) = &self.entries[i].analysis else {
            return None;
        };
        self.entries.iter().enumerate().find_map(|(j, other)| {
            if j == i || other.gone {
                return None;
            }
            let AnalysisState::Ready(b) = &other.analysis else {
                return None;
            };
            let same_bytes = a.content_key.is_some() && a.content_key == b.content_key;
            let same_text = a.text_key.is_some() && a.text_key == b.text_key;
            (same_bytes || same_text).then(|| other.file_name())
        })
    }

    /// Metadaten aus den Feldern (oder den im Formular überschriebenen Werten).
    pub fn pdf_meta(&self, e: &Entry) -> af_pdf::PdfMeta {
        let f = &e.fields;
        let title = e
            .meta_title
            .clone()
            .unwrap_or_else(|| default_title(&self.settings.meta.title_template, f));
        let keywords = match &e.meta_keywords {
            Some(k) => k
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            None => default_keywords(f),
        };
        af_pdf::PdfMeta {
            title,
            author: f.unternehmen.clone(),
            subject: f.thema.clone(),
            keywords,
            created: f.datum.map(|d| d.format("%Y-%m-%d").to_string()),
        }
    }

    fn learn_correction(&mut self, (fields, info, replaced): (NameFields, af_core::LearnInfo, Option<String>)) {
        if self.learning.is_some() {
            self.pending_corrections.push((fields, info, replaced));
            return;
        }
        let relevant = std::sync::Arc::make_mut(&mut self.knowledge).learn(
            &fields,
            &info.ids,
            info.letterhead.as_deref(),
            replaced.as_deref(),
        );
        // Nur wenn sich die Zuordnung ändern kann, andere Dateien neu erkennen; gespeichert wird immer
        if relevant {
            self.knowledge_changed();
        } else {
            self.save_knowledge();
        }
    }

    fn knowledge_changed(&mut self) {
        self.knowledge_rev += 1;
        self.save_knowledge();
    }

    fn save_knowledge(&self) {
        if let Some(path) = &self.knowledge_file
            && let Err(err) = self.knowledge.save(path)
        {
            log::error!("Wissen nicht gespeichert ({}): {err}", path.display());
        }
    }

    /// Stapelmodus: alle offenen, vollständigen Vorschläge ab der Schwelle umbenennen (eine Rückgängig-Gruppe).
    fn batch(&mut self, ctx: &egui::Context) {
        let Ok(template) = self.template.clone() else { return };
        let mut group = Vec::new();
        let mut failed = 0;
        for i in self.batch_candidates() {
            match self.rename_entry(i, &template) {
                Some(Ok((_, id, _))) => group.push((i, id)),
                Some(Err(err)) => {
                    log::error!("Stapelmodus: {err}");
                    failed += 1;
                }
                None => {}
            }
        }
        if group.is_empty() {
            return;
        }
        let n = group.len();
        self.undo_stack.push(group);
        if failed > 0 {
            self.show_error(trf(
                "{} Dateien umbenannt, {} fehlgeschlagen (siehe Logdatei)",
                &[&n, &failed],
            ));
        } else {
            self.show_info(ctx, trf("{} Dateien im Stapel umbenannt", &[&n]), true);
        }
        if let Some(next) = self.entries.iter().position(Entry::is_open) {
            self.current = Some(next);
        }
    }

    /// Offene Dateien, deren Vorschlag vollständig und in jedem Feld sicher genug ist (unverändert vom Nutzer).
    pub fn batch_candidates(&self) -> Vec<usize> {
        (0..self.entries.len())
            .filter(|&i| {
                let e = &self.entries[i];
                e.is_open()
                    && e.duplicate.is_none()
                    && match &e.analysis {
                        AnalysisState::Ready(a) => {
                            a.suggestion.is_complete()
                                && a.suggestion.min_confidence() >= threshold()
                                && a.suggestion.fields() == e.fields
                        }
                        _ => false,
                    }
            })
            .collect()
    }

    fn poll_learning(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.learning else { return };
        let Ok((knowledge, stats)) = job.result.try_recv() else {
            return;
        };
        self.learning = None;
        self.knowledge = std::sync::Arc::new(knowledge);
        for item in std::mem::take(&mut self.pending_corrections) {
            std::sync::Arc::make_mut(&mut self.knowledge).learn(
                &item.0,
                &item.1.ids,
                item.1.letterhead.as_deref(),
                item.2.as_deref(),
            );
        }
        self.knowledge_changed();
        self.show_info(
            ctx,
            trf(
                "Gelernt aus {} von {} Dateien – {} Unternehmen bekannt",
                &[&stats.learned, &stats.files, &self.knowledge.unternehmen.len()],
            ),
            false,
        );
        self.last_learn = Some(stats);
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
        let Some(group) = self.undo_stack.pop() else { return };
        let Some(journal) = &self.journal else { return };
        let mut errors = Vec::new();
        let mut first = None;
        // rückwärts, damit Namensfolgen (…_2) sauber zurückgehen
        for (i, id) in group.into_iter().rev() {
            match journal.undo(id) {
                Ok(entry) => {
                    let e = &mut self.entries[i];
                    e.path = entry.nach;
                    e.done = None;
                    first = Some(i);
                }
                Err(err) => errors.push(err.to_string()),
            }
        }
        if let Some(i) = first {
            self.current = Some(i);
        }
        if errors.is_empty() {
            self.show_info(ctx, tr("Rückgängig gemacht").into(), false);
        } else {
            self.show_error(trf("Rückgängig fehlgeschlagen: {}", &[&errors.join("; ")]));
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
            Action::SetDate(d) => {
                if let Some(e) = self.current.and_then(|i| self.entries.get_mut(i)) {
                    e.fields.datum = Some(d);
                    e.date_text = d.format("%Y-%m-%d").to_string();
                }
            }
            Action::Focus(f) => self.focus = Some(f),
            Action::Selection { text, pos } => self.selection = Some(SelectionMenu { text, pos, rect: None }),
            Action::CloseSelection => self.selection = None,
            Action::ApplySelection(field) => {
                if let Some(sel) = self.selection.take()
                    && let Some(e) = self.current.and_then(|i| self.entries.get_mut(i))
                {
                    match field {
                        FieldFocus::Datum => {
                            if let Some(d) = find_date(&sel.text) {
                                e.fields.datum = Some(d);
                                e.date_text = d.format("%Y-%m-%d").to_string();
                            }
                        }
                        FieldFocus::Unternehmen => e.fields.unternehmen = sel.text,
                        FieldFocus::Thema => e.fields.thema = sel.text,
                    }
                    self.focus = Some(field);
                }
            }
            Action::Batch => self.batch(ctx),
            Action::StartLearning => {
                if self.learning.is_none()
                    && let (Some(root), Ok(template)) = (self.settings.learn_folder.clone(), self.template.clone())
                {
                    let ocr = self.settings.learn_with_ocr.then(|| self.settings.ocr.preference());
                    self.learning = Some(crate::learning::LearnJob::spawn(
                        ctx.clone(),
                        root,
                        template,
                        (*self.knowledge).clone(),
                        ocr,
                    ));
                }
            }
            Action::CancelLearning => {
                if let Some(job) = &self.learning {
                    job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            Action::ForgetCompany(i) => {
                let k = std::sync::Arc::make_mut(&mut self.knowledge);
                if i < k.unternehmen.len() {
                    k.unternehmen.remove(i);
                    self.knowledge_changed();
                }
            }
            Action::ForgetKnowledge => {
                self.knowledge = std::sync::Arc::new(af_core::knowledge::Knowledge::default());
                self.knowledge_changed();
            }
            Action::TrashCurrent => {
                if let Some(i) = self.current
                    && let Some(e) = self.entries.get_mut(i)
                {
                    match trash::delete(&e.path) {
                        Ok(()) => {
                            e.gone = true;
                            let name = e.file_name();
                            self.show_info(ctx, trf("In den Papierkorb gelegt: {}", &[&name]), false);
                            if let Some(next) = self.next_open_after(i) {
                                self.current = Some(next);
                            }
                        }
                        Err(err) => self.show_error(trf("Papierkorb nicht möglich: {}", &[&err])),
                    }
                }
            }
            Action::SetTopic(t) => {
                if let Some(e) = self.current.and_then(|i| self.entries.get_mut(i)) {
                    e.fields.thema = t;
                }
            }
            Action::ToggleHighlights => {
                self.settings.show_highlights = !self.settings.show_highlights;
                self.save_settings();
            }
            Action::SettingsChanged => {
                set_threshold(self.settings.threshold);
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
        // Vorher abfragen: innerhalb von `input_mut` darf der Kontext nicht noch einmal gesperrt werden (Deadlock)
        let completing = form::completion_active(ctx);
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
            if i.consume_shortcut(&cmd(Key::Plus)) || i.consume_shortcut(&cmd(Key::Equals)) {
                actions.push(Action::Zoom(self.settings.zoom + 0.1));
            }
            if i.consume_shortcut(&cmd(Key::Minus)) {
                actions.push(Action::Zoom(self.settings.zoom - 0.1));
            }
            if i.consume_shortcut(&cmd(Key::Num0)) {
                actions.push(Action::Zoom(1.0));
            }
            if self.selection.is_some() && i.key_pressed(Key::Escape) {
                actions.push(Action::CloseSelection);
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
            // Ist in einer Vorschlagsliste ein Eintrag gewählt, übernimmt ↵ diesen statt die Datei umzubenennen
            if i.key_pressed(Key::Enter) && i.modifiers.is_none() && !completing {
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
                let done = self.entries.iter().filter(|e| e.done.is_some()).count();
                let total = self.entries.len();
                ui.add(egui::ProgressBar::new(done as f32 / total as f32).desired_width(140.0));
                ui.label(egui::RichText::new(trf("{} / {} erledigt", &[&done, &total])).weak());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!("⚙ {}", tr("Einstellungen"))).clicked() {
                    actions.push(Action::OpenSettings);
                }
                let n = self.batch_candidates().len();
                let batch = ui
                    .add_enabled(n > 0, egui::Button::new(trf("⚡ Sichere übernehmen ({})", &[&n])))
                    .on_hover_text(trf(
                        "Alle offenen Dateien umbenennen, bei denen jedes Feld mindestens {} % Sicherheit hat",
                        &[&threshold()],
                    ));
                if batch.clicked() {
                    actions.push(Action::Batch);
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

    /// Bekannte Unternehmen und Themen für die Autovervollständigung: aus allen erkannten und
    /// schon benannten Dateien dieses Ordners (ab M5 zusätzlich aus dem gelernten Wissen).
    fn completions(&self) -> (Vec<String>, Vec<String>) {
        let mut companies = std::collections::BTreeSet::new();
        let mut topics = std::collections::BTreeSet::new();
        for e in &self.entries {
            if let Some(s) = e.suggestion() {
                companies.insert(s.unternehmen.value.clone());
                topics.insert(s.thema.value.clone());
            }
            companies.insert(e.fields.unternehmen.clone());
            topics.insert(e.fields.thema.clone());
            if let Ok(t) = &self.template
                && let Some(f) = t.parse_name(&e.file_name())
            {
                companies.insert(f.unternehmen);
                topics.insert(f.thema);
            }
        }
        companies.extend(self.knowledge.company_names());
        topics.extend(self.knowledge.topic_names());
        let clean =
            |set: std::collections::BTreeSet<String>| set.into_iter().filter(|s| !s.trim().is_empty()).collect();
        (clean(companies), clean(topics))
    }

    /// Kleines Menü an der Stelle einer Textauswahl in der Vorschau.
    fn selection_menu(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(sel) = &mut self.selection else { return };
        // Klick daneben schließt das Menü
        if let Some(rect) = sel.rect
            && ctx.input(|i| i.pointer.any_pressed())
            && ctx.pointer_interact_pos().is_some_and(|p| !rect.contains(p))
        {
            actions.push(Action::CloseSelection);
            return;
        }
        let is_date = find_date(&sel.text).is_some();
        let resp = egui::Area::new(egui::Id::new("auswahl-menue"))
            .fixed_pos(sel.pos + egui::vec2(0.0, 8.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(&ctx.global_style()).show(ui, |ui| {
                    ui.set_max_width(320.0);
                    ui.add(egui::Label::new(egui::RichText::new(format!("„{}“", sel.text)).weak()).truncate());
                    ui.horizontal(|ui| {
                        if ui.button(format!("→ {}", tr("Unternehmen"))).clicked() {
                            actions.push(Action::ApplySelection(FieldFocus::Unternehmen));
                        }
                        if ui.button(format!("→ {}", tr("Thema"))).clicked() {
                            actions.push(Action::ApplySelection(FieldFocus::Thema));
                        }
                        if is_date && ui.button(format!("→ {}", tr("Datum"))).clicked() {
                            actions.push(Action::ApplySelection(FieldFocus::Datum));
                        }
                    });
                });
            });
        sel.rect = Some(resp.response.rect);
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
        self.poll_learning(&ctx);
        for i in 0..self.entries.len() {
            let d = if self.entries[i].is_open() {
                self.duplicate_of(i)
            } else {
                None
            };
            self.entries[i].duplicate = d;
        }
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
            let mut hovered = None;
            let threshold = threshold();
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
                let (companies, topics) = self.completions();
                let duplicate = self.entries[cur].duplicate.clone();
                let meta_preview = {
                    let e = &self.entries[cur];
                    let m = self.pdf_meta(e);
                    (m.title, m.keywords.join(", "), finder_tags(&e.fields))
                };
                let fctx = form::FormCtx {
                    template: self.template.as_ref().ok(),
                    naming: &self.settings.naming,
                    threshold,
                    target_dir: shown_target.as_deref(),
                    taken: &other_names,
                    can_undo: !self.undo_stack.is_empty(),
                    companies: &companies,
                    topics: &topics,
                    duplicate: duplicate.as_deref(),
                    meta: &self.settings.meta,
                    meta_preview: &meta_preview,
                };
                egui::Panel::right("formular").exact_size(380.0).show(ui, |ui| {
                    hovered = form::show(ui, &mut self.entries[cur], &fctx, focus, &mut actions);
                });
            }
            // Fokussiertes Feld hat Vorrang vor dem nur überfahrenen
            let focused = [FieldFocus::Datum, FieldFocus::Unternehmen, FieldFocus::Thema]
                .into_iter()
                .find(|f| ctx.memory(|m| m.has_focus(f.id())));
            let pctx = preview::PreviewCtx {
                zoom: self.settings.zoom,
                show_highlights: self.settings.show_highlights,
                active: focused.or(hovered),
            };
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::desk(&ctx)))
                .show(ui, |ui| {
                    let entry = self.current.map(|i| &self.entries[i]);
                    self.preview_width = preview::show(ui, entry, &pctx, &mut actions);
                });
            self.selection_menu(&ctx, &mut actions);
        }

        let learn_progress = self
            .learning
            .as_ref()
            .and_then(|j| j.progress.lock().ok().map(|p| p.clone()));
        let kctx = settings_ui::KnowledgeCtx {
            knowledge: &self.knowledge,
            progress: learn_progress,
            last: self.last_learn.as_ref(),
            file: self.knowledge_file.as_deref(),
        };
        settings_ui::show(
            &ctx,
            &mut self.settings_ui,
            &mut self.settings,
            &self.entries,
            &kctx,
            &mut actions,
        );
        self.toast(&ctx, &mut actions);

        for a in actions {
            self.run(&ctx, a);
        }
        self.watch_inbox(&ctx);
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

/// Datum irgendwo in einem markierten Text, z. B. „Datum: 28.09.2026“.
pub fn find_date(s: &str) -> Option<NaiveDate> {
    parse_date(s).or_else(|| {
        s.split_whitespace().find_map(|w| {
            let w = w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-');
            parse_date(w).or_else(|| parse_date(w.trim_end_matches('.')))
        })
    })
}

/// Titel nach Vorlage, ohne Bereinigung für Dateinamen (Umlaute, Leerzeichen bleiben).
pub fn default_title(template: &str, f: &NameFields) -> String {
    let opts = af_core::naming::NamingOptions {
        transliterate_umlauts: false,
        spaces: af_core::naming::Spaces::Keep,
        strict: false,
    };
    let t = Template::parse(template)
        .map(|t| t.render(f, &opts))
        .unwrap_or_default();
    t.trim().trim_end_matches(['–', '-']).trim().to_string()
}

/// Stichwörter: Unternehmen, Thema, Jahr und ggf. „Rechnung“.
pub fn default_keywords(f: &NameFields) -> Vec<String> {
    let mut k = vec![f.unternehmen.trim().to_string(), f.thema.trim().to_string()];
    if let Some(d) = f.datum {
        k.push(d.format("%Y").to_string());
    }
    if f.rechnung {
        k.push("Rechnung".into());
    }
    k.retain(|s| !s.is_empty());
    k
}

/// Finder-Tags: Unternehmen, Jahr und ggf. „Rechnung“.
pub fn finder_tags(f: &NameFields) -> Vec<String> {
    let mut t = vec![f.unternehmen.trim().to_string()];
    if let Some(d) = f.datum {
        t.push(d.format("%Y").to_string());
    }
    if f.rechnung {
        t.push("Rechnung".into());
    }
    t.retain(|s| !s.is_empty());
    t
}
