// SPDX-License-Identifier: MIT OR Apache-2.0
//! Einstellungen und Wissen in eine Datei sichern und auf einem anderen Gerät übernehmen.
//! Die Datei liegt nur dort, wo der Nutzer sie speichert; übertragen wird nichts.

use std::path::Path;

use af_core::knowledge::Knowledge;
use af_ocr::archive::LearnLog;
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use crate::settings::Settings;

const FORMAT: &str = "ablagefuchs-sicherung";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Backup {
    format: String,
    version: u32,
    /// Zeitpunkt des Exports (nur zur Information)
    exportiert: String,
    app_version: String,
    einstellungen: Settings,
    wissen: Knowledge,
    /// schon gelernte Archivdateien (damit ein Lernlauf auf dem anderen Gerät nichts doppelt zählt)
    #[serde(default)]
    lernstand: LearnLog,
}

/// Vorgeschlagener Dateiname beim Export.
pub fn default_file_name() -> String {
    format!("Ablagefuchs-Sicherung-{}.json", now().format("%Y-%m-%d"))
}

fn now() -> chrono::DateTime<chrono::Utc> {
    std::time::SystemTime::now().into()
}

pub fn export(path: &Path, settings: &Settings, knowledge: &Knowledge, log: &LearnLog) -> anyhow::Result<()> {
    let backup = Backup {
        format: FORMAT.into(),
        version: VERSION,
        exportiert: now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        einstellungen: settings.clone(),
        wissen: knowledge.clone(),
        lernstand: log.clone(),
    };
    crate::settings::write_atomic(path, &serde_json::to_string_pretty(&backup)?)?;
    Ok(())
}

/// Liest eine Sicherung: (Einstellungen, Wissen, Lernstand).
pub fn import(path: &Path) -> anyhow::Result<(Settings, Knowledge, LearnLog)> {
    let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    let backup: Backup = serde_json::from_str(&text).context("keine Ablagefuchs-Sicherung")?;
    if backup.format != FORMAT {
        bail!("keine Ablagefuchs-Sicherung");
    }
    if backup.version > VERSION {
        bail!("Sicherung stammt aus einer neueren Version ({})", backup.app_version);
    }
    Ok((backup.einstellungen, backup.wissen, backup.lernstand))
}

/// Übernimmt importierte Einstellungen. Was nur zu diesem Gerät gehört, bleibt: Grafik, zuletzt geöffnete
/// Ordner, Update-Frage. Ordnerpfade werden nur übernommen, wenn es sie hier gibt (anderes System, anderes
/// Laufwerk).
pub fn apply(current: &Settings, imported: Settings) -> Settings {
    let existing = |p: Option<std::path::PathBuf>, fallback: &Option<std::path::PathBuf>| {
        p.filter(|p| p.is_dir()).or_else(|| fallback.clone())
    };
    Settings {
        graphics: current.graphics,
        opengl_fallback: current.opengl_fallback,
        recent_inboxes: current.recent_inboxes.clone(),
        update_check: current.update_check,
        archive_root: existing(imported.archive_root, &current.archive_root),
        learn_folder: existing(imported.learn_folder, &current.learn_folder),
        ..imported
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::GraphicsSetting;

    #[test]
    fn round_trip_and_device_settings() {
        let dir = std::env::temp_dir().join(format!("af-transfer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sicherung.json");

        let there = Settings {
            template: "{datum}_{unternehmen}".into(),
            graphics: GraphicsSetting::OpenGl,
            archive_root: Some("/gibt/es/hier/nicht".into()),
            learn_folder: Some(dir.clone()),
            ..Settings::default()
        };
        let mut knowledge = Knowledge::default();
        knowledge.unternehmen.push(af_core::knowledge::Company {
            name: "NetzWelt".into(),
            ..Default::default()
        });
        export(&file, &there, &knowledge, &LearnLog::default()).unwrap();

        let (settings, k, _) = import(&file).unwrap();
        assert_eq!(k, knowledge);
        let here = Settings {
            archive_root: Some("/hier".into()),
            ..Settings::default()
        };
        let merged = apply(&here, settings);
        assert_eq!(merged.template, "{datum}_{unternehmen}");
        assert_eq!(merged.graphics, GraphicsSetting::Auto, "Grafik gehört zum Gerät");
        assert_eq!(
            merged.archive_root,
            Some("/hier".into()),
            "fehlender Ordner wird nicht übernommen"
        );
        assert_eq!(merged.learn_folder, Some(dir.clone()));

        std::fs::write(&file, "{\"irgendwas\": 1}").unwrap();
        assert!(import(&file).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
