// SPDX-License-Identifier: MIT OR Apache-2.0
//! Gespeicherte Einstellungen als JSON im Konfigurationsordner des Systems
//! (macOS: ~/Library/Application Support/Ablagefuchs/einstellungen.json).
//! Fehlende Felder fallen auf Standardwerte zurück.

use std::path::{Path, PathBuf};

use af_core::naming::{DEFAULT_TEMPLATE, NamingOptions};
use serde::{Deserialize, Serialize};

pub const APP_DIR: &str = "Ablagefuchs";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LangSetting {
    #[default]
    System,
    De,
    En,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeSetting {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GraphicsSetting {
    /// wgpu (Windows: DirectX 12), bei Problemen automatisch OpenGL
    #[default]
    Auto,
    Wgpu,
    OpenGl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OcrSetting {
    /// Systemeigene Texterkennung (Apple Vision, Windows OCR), sonst Tesseract
    #[default]
    Auto,
    /// Immer Tesseract (gleiche Ergebnisse auf allen Systemen)
    Tesseract,
}

impl OcrSetting {
    pub fn preference(self) -> af_ocr::Preference {
        match self {
            OcrSetting::Auto => af_ocr::Preference::Auto,
            OcrSetting::Tesseract => af_ocr::Preference::Tesseract,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: LangSetting,
    pub theme: ThemeSetting,
    pub graphics: GraphicsSetting,
    pub ocr: OcrSetting,
    /// Automatisch gesetzt, wenn wgpu nicht startete; gilt bei `GraphicsSetting::Auto`
    pub opengl_fallback: bool,
    pub template: String,
    pub naming: NamingOptions,
    /// Nach dem Umbenennen ins Archiv verschieben
    pub move_to_archive: bool,
    pub archive_root: Option<PathBuf>,
    pub folder_template: String,
    /// zuletzt geöffnete Eingangsordner, neuester zuerst
    pub recent_inboxes: Vec<PathBuf>,
    pub show_file_list: bool,
    /// Fundstellen in der Vorschau farbig markieren
    pub show_highlights: bool,
    pub zoom: f32,
    /// Ab dieser Sicherheit (jedes Feld) gilt ein Vorschlag als sicher: kein „Prüfen“, Stapelmodus erlaubt
    pub threshold: u8,
    /// Beim Umbenennen aus den bestätigten Feldern lernen
    pub learn_from_corrections: bool,
    /// Ordner, aus dem zuletzt gelernt wurde
    pub learn_folder: Option<PathBuf>,
    /// Beim Lernen aus einem Ordner auch Scans ohne Textebene lesen (langsam)
    pub learn_with_ocr: bool,
    /// Metadaten beim Umbenennen (Bauplan §8)
    pub meta: MetaSettings,
    /// Beim Start bei GitHub nach einer neuen Version fragen; `None` = noch nicht gefragt
    pub update_check: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetaSettings {
    /// PDF-Info: Titel, Autor, Stichwörter, Erstelldatum = Dokumentdatum
    pub info: bool,
    pub xmp: bool,
    /// Finder-Tags (macOS): Unternehmen, Jahr, „Rechnung“
    pub finder_tags: bool,
    /// Bei Scans die erkannte Schrift als unsichtbare Textebene einbetten (Volltextsuche)
    pub embed_ocr: bool,
    /// Unveränderte Kopie in `.ablagefuchs/originale/` behalten (Rückgängig holt sie zurück)
    pub keep_originals: bool,
    pub title_template: String,
}

impl Default for MetaSettings {
    fn default() -> Self {
        MetaSettings {
            info: true,
            xmp: true,
            finder_tags: true,
            embed_ocr: true,
            keep_originals: false,
            title_template: "{unternehmen} – {thema}".into(),
        }
    }
}

impl MetaSettings {
    /// Wird die Datei beim Umbenennen überhaupt verändert?
    pub fn modifies_file(&self) -> bool {
        self.info || self.xmp || self.embed_ocr
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: LangSetting::System,
            theme: ThemeSetting::System,
            graphics: GraphicsSetting::Auto,
            ocr: OcrSetting::Auto,
            opengl_fallback: false,
            template: DEFAULT_TEMPLATE.to_string(),
            naming: NamingOptions::default(),
            move_to_archive: false,
            archive_root: None,
            folder_template: "{jahr}/{unternehmen}".to_string(),
            recent_inboxes: Vec::new(),
            show_file_list: true,
            show_highlights: true,
            threshold: 90,
            learn_from_corrections: true,
            learn_folder: None,
            learn_with_ocr: false,
            meta: MetaSettings::default(),
            update_check: None,

            zoom: 1.0,
        }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_DIR))
}

/// Gelerntes Wissen; liegt nur hier, nie im Projekt oder anderswo.
pub fn knowledge_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("wissen.json"))
}

fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("einstellungen.json"))
}

/// Erst in eine Zwischendatei schreiben, dann umbenennen: Ein Absturz hinterlässt nie eine halbe Datei.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(tmp, path)
}

impl Settings {
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Settings::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                log::warn!("Einstellungen nicht lesbar ({e}), nehme Standardwerte");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self) {
        let Some(path) = settings_path() else { return };
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = write_atomic(&path, &json) {
                    log::error!("Einstellungen nicht gespeichert: {e}");
                }
            }
            Err(e) => log::error!("Einstellungen nicht gespeichert: {e}"),
        }
    }

    pub fn remember_inbox(&mut self, dir: &Path) {
        self.recent_inboxes.retain(|d| d != dir);
        self.recent_inboxes.insert(0, dir.to_path_buf());
        self.recent_inboxes.truncate(8);
    }
}
