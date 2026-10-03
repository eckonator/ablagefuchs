// SPDX-License-Identifier: MIT OR Apache-2.0
//! Einstellungsfenster: Allgemein (Sprache, Aussehen, Grafik), Dateiname, Ordner, Über.

use af_core::naming::{NameFields, NamingOptions, Spaces, Template, TemplateError};
use chrono::NaiveDate;
use eframe::egui::{self, RichText};

use crate::app::{Action, AnalysisState, Entry};
use crate::i18n::{tr, trf};
use crate::settings::{GraphicsSetting, LangSetting, OcrSetting, Settings, ThemeSetting};
use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    General,
    Naming,
    Folders,
    Knowledge,
    Metadata,
    About,
}

#[derive(Default)]
pub struct SettingsUi {
    pub open: bool,
    page: Page,
    template_buf: String,
    folder_buf: String,
    /// „Alles vergessen“ wurde einmal geklickt und wartet auf Bestätigung
    confirm_forget: bool,
    /// Wissen: Suche, gewähltes Unternehmen, Eingabe für den neuen Namen, Ziel zum Zusammenführen
    search: String,
    selected: Option<usize>,
    rename_buf: String,
    merge_target: Option<usize>,
    /// Mögliche Doppel mit dem Wissensstand, für den sie berechnet wurden
    duplicates: Option<(u64, Vec<(usize, usize)>)>,
}

/// Was die Seite „Wissen“ anzeigt.
pub struct KnowledgeCtx<'a> {
    pub knowledge: &'a af_core::knowledge::Knowledge,
    /// (erledigt, gesamt, aktuelle Datei), solange gelernt wird
    pub progress: Option<(usize, usize, String)>,
    pub last: Option<&'a af_ocr::archive::LearnStats>,
    pub file: Option<&'a std::path::Path>,
    /// Stand des Wissens (ändert sich bei jeder Änderung)
    pub rev: u64,
}

impl SettingsUi {
    pub fn open_page(&mut self, name: &str, settings: &Settings) {
        self.open = true;
        self.page = match name {
            "name" | "dateiname" => Page::Naming,
            "ordner" => Page::Folders,
            "wissen" => Page::Knowledge,
            "metadaten" => Page::Metadata,
            "ueber" | "über" => Page::About,
            _ => Page::General,
        };
        self.template_buf = settings.template.clone();
        self.folder_buf = settings.folder_template.clone();
    }

    /// Eingabefelder neu aus den Einstellungen füllen (nach einem Import).
    pub fn reload(&mut self, settings: &Settings) {
        self.template_buf = settings.template.clone();
        self.folder_buf = settings.folder_template.clone();
    }
}

const PRESETS: [(&str, &str); 4] = [
    ("Standard", "{datum}_{unternehmen}_-_{thema}{rechnung?:_Rechnung}"),
    ("Rechnung vorne", "{datum}_{rechnung?:Rechnung_}{unternehmen}_-_{thema}"),
    ("Ohne Thema", "{datum}_{unternehmen}{rechnung?:_Rechnung}"),
    (
        "Jahr-Monat",
        "{jahr}-{monat}_{unternehmen}_-_{thema}{rechnung?:_Rechnung}",
    ),
];
const TOKENS: [&str; 7] = [
    "{datum}",
    "{jahr}",
    "{monat}",
    "{tag}",
    "{unternehmen}",
    "{thema}",
    "{rechnung?:_Rechnung}",
];

pub fn show(
    ctx: &egui::Context,
    st: &mut SettingsUi,
    settings: &mut Settings,
    entries: &[Entry],
    k: &KnowledgeCtx<'_>,
    actions: &mut Vec<Action>,
) {
    let mut changed = false;
    show_window(ctx, st, settings, entries, k, actions, &mut changed);
    if changed {
        actions.push(Action::SettingsChanged);
    }
}

fn show_window(
    ctx: &egui::Context,
    st: &mut SettingsUi,
    settings: &mut Settings,
    entries: &[Entry],
    k: &KnowledgeCtx<'_>,
    actions: &mut Vec<Action>,
    changed: &mut bool,
) {
    if !st.open {
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.open = false;
        return;
    }
    let mut open = st.open;
    egui::Window::new(tr("Einstellungen"))
        .id(egui::Id::new("einstellungen"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .fixed_size(egui::vec2(640.0, 520.0))
        // Mehr Abstand zum Fensterrand (Wunsch Markus)
        .frame(egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::symmetric(24, 20)))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (page, label) in [
                    (Page::General, "Allgemein"),
                    (Page::Naming, "Dateiname"),
                    (Page::Folders, "Ordner"),
                    (Page::Knowledge, "Wissen"),
                    (Page::Metadata, "Metadaten"),
                    (Page::About, "Über"),
                ] {
                    if ui
                        .selectable_label(st.page == page, RichText::new(tr(label)).size(14.0))
                        .clicked()
                    {
                        st.page = page;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.set_width(ui.available_width());
                match st.page {
                    Page::General => general(ui, settings, actions, changed),
                    Page::Naming => naming(ui, st, settings, entries, changed),
                    Page::Folders => folders(ui, st, settings, changed),
                    Page::Knowledge => knowledge(ui, st, settings, k, actions, changed),
                    Page::Metadata => metadata(ui, settings, changed),
                    Page::About => about(ui),
                }
            });
        });
    st.open = open;
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(text).strong());
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().weak());
}

fn general(ui: &mut egui::Ui, s: &mut Settings, actions: &mut Vec<Action>, changed: &mut bool) {
    heading(ui, tr("Sprache"));
    ui.horizontal(|ui| {
        for (v, label) in [
            (LangSetting::System, tr("System")),
            (LangSetting::De, "Deutsch"),
            (LangSetting::En, "English"),
        ] {
            if ui.radio_value(&mut s.language, v, label).changed() {
                *changed = true;
            }
        }
    });
    hint(ui, tr("Wirkt sofort, ohne Neustart."));

    heading(ui, tr("Aussehen"));
    ui.horizontal(|ui| {
        for (v, label) in [
            (ThemeSetting::System, tr("System")),
            (ThemeSetting::Light, tr("Hell")),
            (ThemeSetting::Dark, tr("Dunkel")),
        ] {
            if ui.radio_value(&mut s.theme, v, label).changed() {
                *changed = true;
            }
        }
    });

    heading(ui, tr("Texterkennung für Scans"));
    let native = if cfg!(target_os = "macos") {
        tr("Automatisch (Apple Vision)")
    } else if cfg!(windows) {
        tr("Automatisch (Windows-Texterkennung)")
    } else {
        tr("Automatisch (Tesseract)")
    };
    ui.horizontal(|ui| {
        for (v, label) in [
            (OcrSetting::Auto, native),
            (OcrSetting::Tesseract, tr("Immer Tesseract")),
        ] {
            if ui.radio_value(&mut s.ocr, v, label).changed() {
                *changed = true;
            }
        }
    });
    // Verfügbarkeit nur einmal je Auswahl prüfen (Tesseract wäre sonst ein Programmstart pro Bild)
    let key = egui::Id::new(("ocr-verfuegbar", s.ocr as u8));
    let available = match ui.data(|d| d.get_temp::<bool>(key)) {
        Some(a) => a,
        None => {
            let a = af_ocr::engine(s.ocr.preference()).is_some();
            ui.data_mut(|d| d.insert_temp(key, a));
            a
        }
    };
    if !available {
        ui.colored_label(
            theme::warn(ui.ctx()),
            tr("Keine Texterkennung verfügbar. Bitte Tesseract installieren (z. B. tesseract-ocr und tesseract-ocr-deu)."),
        );
    } else {
        hint(
            ui,
            tr("Wird nur für Scans ohne Textebene gebraucht. Ergebnisse werden zwischengespeichert."),
        );
    }

    heading(ui, tr("Stapelmodus"));
    ui.horizontal(|ui| {
        ui.label(tr("Sicherheitsschwelle"));
        if ui
            .add(egui::Slider::new(&mut s.threshold, 70..=100).suffix(" %"))
            .changed()
        {
            *changed = true;
        }
    });
    hint(
        ui,
        tr(
            "Dateien, bei denen jedes Feld mindestens so sicher erkannt ist, gelten als sicher: nicht unter „Prüfen“ und im Stapelmodus ohne Rückfrage umbenannt.",
        ),
    );

    heading(ui, tr("Auf ein anderes Gerät übertragen"));
    ui.horizontal(|ui| {
        if ui.button(tr("Exportieren …")).clicked() {
            actions.push(Action::ExportBackup);
        }
        if ui.button(tr("Importieren …")).clicked() {
            actions.push(Action::ImportBackup);
        }
    });
    hint(
        ui,
        tr(
            "Sichert alle Einstellungen und das gelernte Wissen in eine Datei. Beim Import wird das Wissen mit dem vorhandenen zusammengeführt; Grafik und zuletzt geöffnete Ordner bleiben, Ordner werden nur übernommen, wenn es sie auf diesem Gerät gibt.",
        ),
    );

    heading(ui, tr("Updates"));
    let mut check = s.update_check.unwrap_or(false);
    if ui
        .checkbox(&mut check, tr("Beim Start bei GitHub nach einer neuen Version suchen"))
        .changed()
    {
        s.update_check = Some(check);
        *changed = true;
    }
    hint(
        ui,
        tr("Gesendet wird nur die Frage nach der neuesten Version, keine Daten über Dokumente oder Ordner."),
    );

    heading(ui, tr("Grafik"));
    let auto = if cfg!(windows) {
        tr("Automatisch (DirectX 12)")
    } else {
        tr("Automatisch")
    };
    ui.horizontal(|ui| {
        for (v, label) in [
            (GraphicsSetting::Auto, auto),
            (GraphicsSetting::Wgpu, "wgpu"),
            (GraphicsSetting::OpenGl, "OpenGL"),
        ] {
            if ui.radio_value(&mut s.graphics, v, label).changed() {
                s.opengl_fallback = false;
                *changed = true;
            }
        }
    });
    hint(
        ui,
        tr(
            "Wirkt nach einem Neustart. Startet die Grafik nicht, wechselt Ablagefuchs beim nächsten Start automatisch auf OpenGL.",
        ),
    );
    if s.opengl_fallback && s.graphics == GraphicsSetting::Auto {
        hint(ui, tr("Zurzeit aktiv: OpenGL (Ausweichweg nach einem Grafikproblem)."));
    }
}

fn template_error(e: &TemplateError) -> String {
    match e {
        TemplateError::UnknownPlaceholder {
            name,
            suggestion: Some(s),
        } => trf(
            "Unbekannter Platzhalter {} – meintest du {}?",
            &[&format!("{{{name}}}"), &format!("{{{s}}}")],
        ),
        TemplateError::UnknownPlaceholder { name, suggestion: None } => {
            trf("Unbekannter Platzhalter {}", &[&format!("{{{name}}}")])
        }
        TemplateError::Unclosed => tr("Eine geschweifte Klammer { wird nicht geschlossen").into(),
        TemplateError::Empty => tr("Die Vorlage ist leer").into(),
    }
}

fn naming(ui: &mut egui::Ui, st: &mut SettingsUi, s: &mut Settings, entries: &[Entry], changed: &mut bool) {
    heading(ui, tr("Vorlage"));
    let resp = ui.add(
        egui::TextEdit::singleline(&mut st.template_buf)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY),
    );
    let mut apply = resp.changed();
    ui.horizontal_wrapped(|ui| {
        for t in TOKENS {
            if ui.small_button(RichText::new(t).monospace()).clicked() {
                st.template_buf.push_str(t);
                apply = true;
            }
        }
    });
    hint(
        ui,
        tr("Platzhalter in {…}. Bedingter Text wie {rechnung?:_Rechnung} erscheint nur, wenn das Feld gesetzt ist."),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("Vorlagen:"));
        for (name, t) in PRESETS {
            if ui.selectable_label(st.template_buf == t, tr(name)).clicked() {
                st.template_buf = t.to_string();
                apply = true;
            }
        }
    });
    match Template::parse(&st.template_buf) {
        Ok(_) => {
            if apply && s.template != st.template_buf {
                s.template = st.template_buf.clone();
                *changed = true;
            }
        }
        Err(e) => {
            ui.colored_label(theme::bad(ui.ctx()), template_error(&e));
        }
    }

    heading(ui, tr("Zeichen im Dateinamen"));
    let mut n: NamingOptions = s.naming.clone();
    egui::Grid::new("zeichen")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr("Umlaute"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut n.transliterate_umlauts, true, tr("umschreiben (ae oe ue ss)"));
                ui.radio_value(&mut n.transliterate_umlauts, false, tr("beibehalten (ä ö ü ß)"));
            });
            ui.end_row();
            ui.label(tr("Leerzeichen"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut n.spaces, Spaces::Underscore, "_");
                ui.radio_value(&mut n.spaces, Spaces::Dash, "-");
                ui.radio_value(&mut n.spaces, Spaces::Remove, tr("entfernen"));
                ui.radio_value(&mut n.spaces, Spaces::Keep, tr("beibehalten"));
            });
            ui.end_row();
            ui.label(tr("Sonderzeichen"));
            ui.vertical(|ui| {
                ui.radio_value(&mut n.strict, true, tr("nur A–Z, 0–9, _ und - (é → e, & → und)"));
                ui.radio_value(&mut n.strict, false, tr("erlauben (nur verbotene Zeichen entfernen)"));
            });
            ui.end_row();
        });
    if n != s.naming {
        s.naming = n;
        *changed = true;
    }

    heading(ui, tr("Beispiele"));
    if let Ok(t) = Template::parse(&s.template) {
        let mut samples: Vec<NameFields> = entries
            .iter()
            .filter_map(|e| match &e.analysis {
                AnalysisState::Ready(a) if a.suggestion.is_complete() => Some(e.fields.clone()),
                _ => None,
            })
            .take(4)
            .collect();
        if samples.is_empty() {
            samples.push(NameFields {
                datum: NaiveDate::from_ymd_opt(2026, 9, 28),
                unternehmen: "Stadtwerke Musterstadt".into(),
                thema: "Jahresabrechnung Strom".into(),
                rechnung: true,
            });
        }
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for f in samples {
                ui.label(RichText::new(t.file_name(&f, &s.naming)).monospace().small());
            }
        });
    }
}

fn folders(ui: &mut egui::Ui, st: &mut SettingsUi, s: &mut Settings, changed: &mut bool) {
    heading(ui, tr("Nach dem Umbenennen"));
    if ui
        .checkbox(&mut s.move_to_archive, tr("In das Archiv verschieben"))
        .changed()
    {
        *changed = true;
    }
    ui.add_enabled_ui(s.move_to_archive, |ui| {
        ui.horizontal(|ui| {
            ui.label(tr("Archivordner:"));
            let text = s
                .archive_root
                .as_ref()
                .map_or_else(|| tr("nicht gewählt").to_string(), |p| p.display().to_string());
            ui.label(RichText::new(text).monospace());
            if ui.button(tr("Wählen …")).clicked()
                && let Some(dir) = rfd::FileDialog::new()
                    .set_title(tr("Archivordner wählen"))
                    .pick_folder()
            {
                s.archive_root = Some(dir);
                *changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label(tr("Unterordner-Vorlage:"));
            if ui
                .add(egui::TextEdit::singleline(&mut st.folder_buf).font(egui::TextStyle::Monospace))
                .changed()
                && Template::parse(&st.folder_buf).is_ok()
            {
                s.folder_template = st.folder_buf.clone();
                *changed = true;
            }
        });
        if let Err(e) = Template::parse(&st.folder_buf) {
            ui.colored_label(theme::bad(ui.ctx()), template_error(&e));
        }
    });
    hint(
        ui,
        tr("Ohne Verschieben bleibt die Datei im Eingangsordner und wird nur umbenannt."),
    );

    heading(ui, tr("Protokoll"));
    hint(
        ui,
        tr("Jede Umbenennung steht in .ablagefuchs/journal.jsonl im Eingangsordner und lässt sich rückgängig machen."),
    );
}

const THIRD_PARTY: [(&str, &str); 8] = [
    ("PDFium", "Apache-2.0 / BSD-3-Clause"),
    ("egui / eframe", "MIT / Apache-2.0"),
    (
        "egui-Schriften (Ubuntu, Noto Emoji, Emoji Icon Font)",
        "Ubuntu Font Licence / OFL-1.1",
    ),
    ("Inter (Oberflächenschrift)", "OFL-1.1"),
    ("DejaVu Sans (Ausschnitt für Symbole)", "Bitstream Vera / Public Domain"),
    ("pdfium-render, lopdf", "MIT / Apache-2.0"),
    ("regex, chrono, serde, rfd, dirs", "MIT / Apache-2.0"),
    ("unicode-normalization, sys-locale, env_logger", "MIT / Apache-2.0"),
];

fn about(ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        // App-Icon (beim Start als Textur geladen)
        if let Some(tex) = ui.data(|d| d.get_temp::<egui::TextureHandle>(egui::Id::new("app-icon"))) {
            ui.add(egui::Image::new((tex.id(), egui::vec2(80.0, 80.0))));
        }
        ui.vertical(|ui| {
            ui.heading("Ablagefuchs");
            let hash = env!("AF_GIT_HASH");
            let version = if hash.is_empty() {
                env!("CARGO_PKG_VERSION").to_string()
            } else {
                format!("{} ({hash})", env!("CARGO_PKG_VERSION"))
            };
            ui.label(RichText::new(trf("Version {}", &[&version])).weak());
            ui.label(format!("© 2026 {}", env!("CARGO_PKG_AUTHORS")));
        });
    });

    heading(ui, tr("Lizenz"));
    ui.label(tr("Lizenziert unter MIT oder Apache-2.0, nach Wahl."));
    egui::CollapsingHeader::new(tr("MIT-Lizenz anzeigen"))
        .show(ui, |ui| license_text(ui, include_str!("../../LICENSE-MIT")));
    egui::CollapsingHeader::new(tr("Apache-Lizenz 2.0 anzeigen"))
        .show(ui, |ui| license_text(ui, include_str!("../../LICENSE-APACHE")));
    hint(ui, tr("Ablagefuchs wird ohne jegliche Gewährleistung bereitgestellt."));

    heading(ui, tr("Quellcode"));
    let repo = env!("CARGO_PKG_REPOSITORY");
    ui.horizontal(|ui| {
        ui.hyperlink_to(repo.trim_start_matches("https://"), repo);
        ui.hyperlink_to(tr("Fehler melden"), format!("{repo}/issues"));
    });

    heading(ui, tr("Fremdbestandteile"));
    egui::Grid::new("fremd")
        .num_columns(2)
        .striped(true)
        .spacing([24.0, 4.0])
        .show(ui, |ui| {
            for (name, license) in THIRD_PARTY {
                ui.label(tr(name));
                ui.label(RichText::new(license).weak());
                ui.end_row();
            }
        });
    hint(
        ui,
        tr("Die vollständige Liste liegt jedem Download als THIRD-PARTY-LICENSES bei."),
    );

    heading(ui, tr("Datenschutz"));
    ui.label(tr(
        "Ablagefuchs arbeitet offline: Dokumente, Dateinamen, Ordner und Gelerntes verlassen diesen Rechner nie.",
    ));
    hint(
        ui,
        tr(
            "Einzige Ausnahme, nur wenn eingeschaltet: Beim Start fragt Ablagefuchs bei GitHub nach der neuesten Version. Dabei sieht GitHub wie bei jedem Webseitenaufruf deine IP-Adresse.",
        ),
    );

    heading(ui, tr("Diagnose"));
    ui.horizontal(|ui| {
        if ui.button(tr("Logdatei anzeigen")).clicked()
            && let Some(p) = crate::diagnose::log_path()
        {
            crate::diagnose::open_in_system(&p);
        }
        if ui.button(tr("Einstellungsordner öffnen")).clicked()
            && let Some(p) = crate::settings::config_dir()
        {
            let _ = std::fs::create_dir_all(&p);
            crate::diagnose::open_in_system(&p);
        }
    });
}

fn license_text(ui: &mut egui::Ui, text: &str) {
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .id_salt(text.len())
        .show(ui, |ui| {
            ui.label(RichText::new(text).monospace().small());
        });
}

fn knowledge(
    ui: &mut egui::Ui,
    st: &mut SettingsUi,
    s: &mut Settings,
    k: &KnowledgeCtx<'_>,
    actions: &mut Vec<Action>,
    changed: &mut bool,
) {
    hint(
        ui,
        tr(
            "Ablagefuchs lernt Schreibweisen, Kennungen (USt-IdNr., IBAN, Steuernummer) und Themen. Alles bleibt auf diesem Rechner: kein Netzwerk, keine KI.",
        ),
    );
    heading(ui, tr("Aus Korrekturen lernen"));
    if ui
        .checkbox(
            &mut s.learn_from_corrections,
            tr("Beim Umbenennen aus meinen Angaben lernen"),
        )
        .changed()
    {
        *changed = true;
    }

    heading(ui, tr("Aus einem Ordner lernen"));
    hint(
        ui,
        tr(
            "Liest alle PDFs im Ordner und in allen Unterordnern: Dateiname und erste Seite. Namen nach der Vorlage gelten direkt; bei anderen Namen muss der Brief die Firma bestätigen (Briefkopf, Fußzeile oder bekannte USt-IdNr./IBAN).",
        ),
    );
    ui.horizontal(|ui| {
        ui.label(tr("Ordner:"));
        let text = s
            .learn_folder
            .as_ref()
            .map_or_else(|| tr("nicht gewählt").to_string(), |p| p.display().to_string());
        ui.add(egui::Label::new(RichText::new(text).monospace()).truncate());
        if ui
            .add_enabled(k.progress.is_none(), egui::Button::new(tr("Wählen …")))
            .clicked()
            && let Some(dir) = rfd::FileDialog::new()
                .set_title(tr("Ordner zum Lernen wählen"))
                .pick_folder()
        {
            s.learn_folder = Some(dir);
            *changed = true;
        }
    });
    if ui
        .checkbox(&mut s.learn_with_ocr, tr("Auch Scans ohne Textebene lesen (langsam)"))
        .changed()
    {
        *changed = true;
    }
    match &k.progress {
        Some((done, total, name)) => {
            ui.horizontal(|ui| {
                let frac = if *total == 0 { 0.0 } else { *done as f32 / *total as f32 };
                ui.add(
                    egui::ProgressBar::new(frac)
                        .desired_width(300.0)
                        .text(format!("{done} / {total}")),
                );
                if ui.button(tr("Abbrechen")).clicked() {
                    actions.push(Action::CancelLearning);
                }
            });
            ui.add(egui::Label::new(RichText::new(name).small().weak()).truncate());
        }
        None => {
            if ui
                .add_enabled(s.learn_folder.is_some(), egui::Button::new(tr("Lernen starten")))
                .clicked()
            {
                actions.push(Action::StartLearning);
            }
        }
    }
    if let Some(l) = k.last {
        hint(
            ui,
            &trf(
                "Zuletzt: {} PDFs gefunden, {} neu gelernt, {} schon bekannt, {} ohne Firma im Namen, {} Firma nicht im Brief bestätigt, {} nicht lesbar",
                &[
                    &l.files,
                    &l.learned,
                    &l.already,
                    &l.skipped_name,
                    &l.unconfirmed,
                    &l.errors,
                ],
            ),
        );
        if l.unconfirmed > 0 {
            hint(
                ui,
                &trf(
                    "Nicht bestätigt: {} ohne erkannten Text, {} Firma nur im Fließtext, {} ähnlich geschrieben (Lesefehler?), {} gar nicht gefunden (Logo, Abkürzung)",
                    &[&l.miss_no_text, &l.miss_in_body, &l.miss_similar, &l.miss_missing],
                ),
            );
        }
        if !l.unconfirmed_files.is_empty()
            && ui
                .add_enabled(
                    k.progress.is_none(),
                    egui::Button::new(trf("Nicht gelernte Dateien durchgehen ({})", &[&l.unconfirmed_files.len()])),
                )
                .on_hover_text(tr(
                    "Öffnet die Dateien wie einen Eingangsordner. Du bestätigst die Firma, Ablagefuchs lernt daraus; umbenannt wird nichts.",
                ))
                .clicked()
        {
            actions.push(Action::ReviewUnlearned);
        }
    }

    let learning = k.progress.is_some();
    heading(ui, &trf("Bekannte Unternehmen ({})", &[&k.knowledge.unternehmen.len()]));
    if k.knowledge.unternehmen.is_empty() {
        hint(ui, tr("Noch nichts gelernt."));
    } else {
        if learning {
            hint(ui, tr("Während des Lernens kann nichts bearbeitet werden."));
        }
        duplicates(ui, st, k, actions, learning);
        ui.horizontal(|ui| {
            ui.label(tr("Suchen:"));
            ui.add(egui::TextEdit::singleline(&mut st.search).desired_width(220.0));
        });
        ui.add_space(4.0);
        company_table(ui, st, k);
        if let Some(i) = st.selected.filter(|&i| i < k.knowledge.unternehmen.len()) {
            company_editor(ui, st, k, i, actions, learning);
        }
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if let Some(f) = k.file
            && ui.button(tr("Wissensdatei anzeigen")).clicked()
        {
            let dir = f.parent().unwrap_or(f);
            crate::diagnose::open_in_system(dir);
        }
        let label = if st.confirm_forget {
            tr("Wirklich alles vergessen?")
        } else {
            tr("Alles vergessen")
        };
        if ui
            .add_enabled(
                !k.knowledge.unternehmen.is_empty() && !learning,
                egui::Button::new(label),
            )
            .clicked()
        {
            if st.confirm_forget {
                actions.push(Action::ForgetKnowledge);
                st.selected = None;
            }
            st.confirm_forget = !st.confirm_forget;
        }
    });
}

/// Mögliche Doppel (werden nur neu berechnet, wenn sich das Wissen ändert).
fn duplicates(ui: &mut egui::Ui, st: &mut SettingsUi, k: &KnowledgeCtx<'_>, actions: &mut Vec<Action>, learning: bool) {
    if st.duplicates.as_ref().is_none_or(|(rev, _)| *rev != k.rev) {
        st.duplicates = Some((k.rev, k.knowledge.likely_duplicates()));
    }
    let Some((_, pairs)) = &st.duplicates else { return };
    if pairs.is_empty() {
        return;
    }
    let companies = &k.knowledge.unternehmen;
    egui::CollapsingHeader::new(trf("Mögliche Doppel ({})", &[&pairs.len()]))
        .id_salt("doppel")
        .default_open(true)
        .show(ui, |ui| {
            hint(
                ui,
                tr("Zusammenführen behält den Namen des Unternehmens mit mehr Dokumenten; der andere wird zur weiteren Schreibweise."),
            );
            for &(a, b) in pairs.iter().take(8) {
                let (Some(ca), Some(cb)) = (companies.get(a), companies.get(b)) else { continue };
                // in das häufigere zusammenführen
                let (from, into) = if ca.anzahl >= cb.anzahl { (b, a) } else { (a, b) };
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(format!("{} ({}) ↔ {} ({})", ca.name, ca.anzahl, cb.name, cb.anzahl))
                            .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!learning, egui::Button::new(tr("Kein Doppel")).small()).clicked() {
                            actions.push(Action::DismissDuplicate(ca.name.clone(), cb.name.clone()));
                        }
                        if ui
                            .add_enabled(!learning, egui::Button::new(tr("Zusammenführen")).small())
                            .on_hover_text(trf("→ {}", &[&companies[into].name]))
                            .clicked()
                        {
                            actions.push(Action::MergeCompanies { from, into });
                            st.selected = None;
                        }
                    });
                });
            }
            if pairs.len() > 8 {
                hint(ui, &trf("… und {} weitere", &[&(pairs.len() - 8)]));
            }
        });
}

/// Tabelle der Unternehmen (gefiltert über die Suche); Klick auf eine Zeile wählt sie zum Bearbeiten.
fn company_table(ui: &mut egui::Ui, st: &mut SettingsUi, k: &KnowledgeCtx<'_>) {
    // Feste Spaltenbreiten: lange Namen und Kennungslisten werden gekürzt (Rest im Tooltip),
    // statt das Fenster zu verbreitern
    let spacing = 14.0;
    let docs = 70.0;
    let rest = (ui.available_width() - docs - 3.0 * spacing).max(150.0);
    let widths = [rest * 0.4, rest * 0.3, rest * 0.3, docs];
    let cell = |ui: &mut egui::Ui, width: f32, text: RichText, hover: &str| {
        ui.allocate_ui_with_layout(
            egui::vec2(width, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(width);
                let r = ui.add(egui::Label::new(text).truncate());
                if !hover.is_empty() {
                    r.on_hover_text(hover);
                }
            },
        );
    };
    let needle = st.search.trim().to_lowercase();
    let selected = st.selected;
    egui::Grid::new("wissen")
        .num_columns(4)
        .striped(true)
        .spacing([spacing, 4.0])
        .show(ui, |ui| {
            for (label, width) in ["Name", "Kennungen", "Themen", "Dokumente"].into_iter().zip(widths) {
                cell(ui, width, RichText::new(tr(label)).small().strong(), "");
            }
            ui.end_row();
            for (i, c) in k.knowledge.unternehmen.iter().enumerate() {
                if !needle.is_empty()
                    && !std::iter::once(&c.name)
                        .chain(c.aliase.iter())
                        .any(|n| n.to_lowercase().contains(&needle))
                {
                    continue;
                }
                // Name als auswählbarer Eintrag: Klick öffnet den Bearbeiten-Bereich darunter
                ui.allocate_ui_with_layout(
                    egui::vec2(widths[0], ui.spacing().interact_size.y),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(widths[0]);
                        let name = if selected == Some(i) {
                            RichText::new(&c.name).strong().color(theme::accent(ui.ctx()))
                        } else {
                            RichText::new(&c.name)
                        };
                        let r = ui
                            .add(egui::Label::new(name).truncate().sense(egui::Sense::click()))
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        let r = if c.aliase.is_empty() {
                            r
                        } else {
                            r.on_hover_text(c.aliase.join(", "))
                        };
                        if r.clicked() {
                            st.selected = if selected == Some(i) { None } else { Some(i) };
                            st.rename_buf = c.name.clone();
                            st.merge_target = None;
                        }
                    },
                );
                let ids: Vec<&str> = c
                    .kennungen
                    .iter()
                    .filter(|(_, v)| !v.is_empty())
                    .map(|(kind, _)| id_label(*kind))
                    .collect();
                let ids = ids.join(", ");
                cell(ui, widths[1], RichText::new(&ids).small().weak(), &ids);
                let topics = c.topics().into_iter().take(5).collect::<Vec<_>>().join(", ");
                cell(ui, widths[2], RichText::new(&topics).small().weak(), &topics);
                cell(ui, widths[3], RichText::new(c.anzahl.to_string()), "");
                ui.end_row();
            }
        });
}

/// Bearbeiten-Bereich für das gewählte Unternehmen: umbenennen, zusammenführen, vergessen.
fn company_editor(
    ui: &mut egui::Ui,
    st: &mut SettingsUi,
    k: &KnowledgeCtx<'_>,
    i: usize,
    actions: &mut Vec<Action>,
    learning: bool,
) {
    let companies = &k.knowledge.unternehmen;
    let c = &companies[i];
    ui.add_space(6.0);
    egui::Frame::group(ui.style()).corner_radius(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add_enabled_ui(!learning, |ui| {
            ui.label(RichText::new(trf("„{}“ bearbeiten", &[&c.name])).strong());
            if !c.aliase.is_empty() {
                hint(ui, &trf("Weitere Schreibweisen: {}", &[&c.aliase.join(", ")]));
            }
            ui.horizontal(|ui| {
                ui.label(tr("Name:"));
                let edit = ui.add(egui::TextEdit::singleline(&mut st.rename_buf).desired_width(260.0));
                let enter = edit.lost_focus() && ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                let new = st.rename_buf.trim();
                if (ui
                    .add_enabled(!new.is_empty() && new != c.name, egui::Button::new(tr("Umbenennen")))
                    .clicked()
                    || enter)
                    && !new.is_empty()
                    && new != c.name
                {
                    actions.push(Action::RenameCompany(i, new.to_string()));
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Zusammenführen mit:"));
                let current = st
                    .merge_target
                    .and_then(|j| companies.get(j))
                    .map_or_else(|| tr("Unternehmen wählen …").to_string(), |t| t.name.clone());
                egui::ComboBox::from_id_salt("zusammenfuehren")
                    .selected_text(current)
                    .width(260.0)
                    .height(300.0)
                    .show_ui(ui, |ui| {
                        let mut others: Vec<(usize, &str)> = companies
                            .iter()
                            .enumerate()
                            .filter(|&(j, _)| j != i)
                            .map(|(j, o)| (j, o.name.as_str()))
                            .collect();
                        others.sort_by_key(|(_, n)| n.to_lowercase());
                        for (j, name) in others {
                            ui.selectable_value(&mut st.merge_target, Some(j), name);
                        }
                    });
                if let Some(j) = st.merge_target.filter(|&j| j < companies.len() && j != i)
                    && ui
                        .button(tr("Zusammenführen"))
                        .on_hover_text(trf(
                            "„{}“ wird zu einer Schreibweise von „{}“",
                            &[&c.name, &companies[j].name],
                        ))
                        .clicked()
                {
                    actions.push(Action::MergeCompanies { from: i, into: j });
                    st.selected = None;
                    st.merge_target = None;
                }
            });
            if ui.button(tr("Vergessen")).clicked() {
                actions.push(Action::ForgetCompany(i));
                st.selected = None;
            }
        });
    });
}

pub fn id_label(kind: af_core::knowledge::IdKind) -> &'static str {
    use af_core::knowledge::IdKind;
    match kind {
        IdKind::UstId => tr("USt-IdNr."),
        IdKind::Iban => "IBAN",
        IdKind::Steuernummer => tr("Steuernummer"),
        IdKind::Domain => tr("Domain"),
    }
}

fn metadata(ui: &mut egui::Ui, s: &mut Settings, changed: &mut bool) {
    hint(
        ui,
        tr(
            "Wird beim Umbenennen geschrieben, damit Spotlight bzw. die Windows-Suche Dokumente über Titel, Stichwörter und Inhalt findet.",
        ),
    );
    heading(ui, tr("Ins PDF schreiben"));
    let m = &mut s.meta;
    let mut any = false;
    any |= ui
        .checkbox(
            &mut m.info,
            tr("PDF-Info (Titel, Autor, Stichwörter, Erstelldatum = Dokumentdatum)"),
        )
        .changed();
    any |= ui.checkbox(&mut m.xmp, tr("XMP-Metadaten")).changed();
    any |= ui
        .checkbox(
            &mut m.embed_ocr,
            tr("Bei Scans die erkannte Schrift als unsichtbare Textebene einbetten (Volltextsuche)"),
        )
        .changed();
    if cfg!(target_os = "macos") {
        any |= ui
            .checkbox(
                &mut m.finder_tags,
                tr("Finder-Tags setzen (Unternehmen, Jahr, Rechnung)"),
            )
            .changed();
    }
    heading(ui, tr("Titel-Vorlage"));
    if ui
        .add(
            egui::TextEdit::singleline(&mut m.title_template)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY),
        )
        .changed()
    {
        any = true;
    }
    if let Err(e) = Template::parse(&m.title_template) {
        ui.colored_label(theme::bad(ui.ctx()), template_error(&e));
    }
    heading(ui, tr("Originale"));
    any |= ui
        .checkbox(
            &mut m.keep_originals,
            tr("Unveränderte Kopie in .ablagefuchs/originale/ im Eingangsordner behalten"),
        )
        .changed();
    hint(
        ui,
        tr("Mit Kopie stellt „Rückgängig“ auch den ursprünglichen Inhalt wieder her, sonst nur den Dateinamen."),
    );
    if any {
        *changed = true;
    }
}
