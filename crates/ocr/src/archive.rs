// SPDX-License-Identifier: MIT OR Apache-2.0
//! Wissen aus einem Archivordner lernen (rekursiv, mit allen Unterordnern). Läuft vollständig lokal:
//! Dateinamen werden mit der Vorlage zurückgelesen, dazu Kennungen und Briefkopf der ersten Seite.

use std::path::{Path, PathBuf};

use af_core::knowledge::{Knowledge, find_ids, normalize_name};
use af_core::layout::Layout;
use af_core::naming::Template;

use crate::Preference;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LearnStats {
    /// gefundene PDFs
    pub files: usize,
    /// daraus gelernt
    pub learned: usize,
    /// Dateiname passt nicht zur Vorlage
    pub skipped_name: usize,
    /// nicht lesbar
    pub errors: usize,
}

/// Fortschritt für die Anzeige.
pub struct Progress<'a> {
    pub done: usize,
    pub total: usize,
    pub current: &'a Path,
}

/// Alle PDFs unter `root`, rekursiv und sortiert. Versteckte Ordner (z. B. `.ablagefuchs`) bleiben außen vor,
/// symbolischen Links wird nicht gefolgt (keine Endlosschleifen).
pub fn collect_pdfs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.filter_map(Result::ok) {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(ft) = entry.file_type() else { continue };
            let path = entry.path();
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Lernt aus allen benannten PDFs unter `root`. `ocr`: auch Scans ohne Textebene lesen (langsam).
/// `progress` wird je Datei aufgerufen; gibt es `false` zurück, wird abgebrochen.
pub fn learn_folder(
    root: &Path,
    template: &Template,
    knowledge: &mut Knowledge,
    ocr: Option<Preference>,
    mut progress: impl FnMut(Progress<'_>) -> bool,
) -> LearnStats {
    let files = collect_pdfs(root);
    let mut stats = LearnStats {
        files: files.len(),
        ..LearnStats::default()
    };
    for (i, path) in files.iter().enumerate() {
        if !progress(Progress {
            done: i,
            total: files.len(),
            current: path,
        }) {
            break;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(mut fields) = template.parse_name(&name).filter(|f| !f.unternehmen.trim().is_empty()) else {
            stats.skipped_name += 1;
            continue;
        };
        let text = match ocr {
            Some(pref) => crate::read_text(path, 1, pref).map(|(t, _)| t),
            None => af_pdf::extract_text(path, 1),
        };
        let pages = match text {
            Ok(p) => p,
            Err(e) => {
                log::warn!("{}: {e:#}", path.display());
                stats.errors += 1;
                continue;
            }
        };
        let layout = Layout::new(&pages);
        // Nur ein deutlich gestalteter Briefkopf taugt als Alias (sonst lernt man z. B. „Herrn“)
        let letterhead = af_core::company::detect(&layout)
            .filter(|c| c.confidence >= 60)
            .map(|c| c.name);
        // Dateinamen kennen keine Umlaute („Sued“): stimmt der Briefkopf überein, dessen Schreibweise nehmen
        if let Some(l) = &letterhead
            && normalize_name(l) == normalize_name(&fields.unternehmen)
        {
            fields.unternehmen = l.clone();
        }
        let ids = find_ids(&layout);
        knowledge.learn(&fields, &ids, letterhead.as_deref(), None);
        stats.learned += 1;
    }
    stats
}
