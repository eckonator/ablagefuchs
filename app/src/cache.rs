// SPDX-License-Identifier: MIT OR Apache-2.0
//! Cache für Texterkennungs-Ergebnisse (OCR dauert, Textebenen nicht). Schlüssel ist der Inhalt der Datei
//! (BLAKE3), damit Umbenennen oder Verschieben den Cache nicht ungültig macht.
//! Ablage: Cache-Ordner des Systems, `Ablagefuchs/ocr/v1/<hash>.json`.

use std::path::{Path, PathBuf};

use af_core::{PageText, TextSource};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Cached {
    pages: Vec<PageText>,
    source: TextSource,
}

fn dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join(crate::settings::APP_DIR).join("ocr").join("v1"))
}

/// Inhalts-Prüfsumme einer Datei als Hex-Text.
pub fn key(path: &Path) -> Option<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(std::fs::File::open(path).ok()?).ok()?;
    Some(hasher.finalize().to_hex().to_string())
}

pub fn load(key: &str) -> Option<(Vec<PageText>, TextSource)> {
    let text = std::fs::read_to_string(dir()?.join(format!("{key}.json"))).ok()?;
    let c: Cached = serde_json::from_str(&text).ok()?;
    Some((c.pages, c.source))
}

pub fn store(key: &str, pages: &[PageText], source: &TextSource) {
    let Some(dir) = dir() else { return };
    let c = Cached {
        pages: pages.to_vec(),
        source: source.clone(),
    };
    match serde_json::to_string(&c) {
        Ok(json) => {
            if let Err(e) = crate::settings::write_atomic(&dir.join(format!("{key}.json")), &json) {
                log::warn!("OCR-Cache nicht gespeichert: {e}");
            }
        }
        Err(e) => log::warn!("OCR-Cache nicht gespeichert: {e}"),
    }
}
