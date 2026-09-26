// SPDX-License-Identifier: MIT OR Apache-2.0
//! Umbenennen und Verschieben mit Protokoll (Journal) und Rückgängig.
//!
//! Grundsätze: nie überschreiben (Kollisionen bekommen `_2`, `_3` …), auf anderem Laufwerk erst kopieren
//! und vergleichen, dann löschen. Jede Änderung steht als eine JSON-Zeile in `journal.jsonl`.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::naming::dedupe;

/// Unterordner im Eingangsordner für Journal und spätere Originalkopien.
pub const DATA_DIR: &str = ".ablagefuchs";
pub const JOURNAL_FILE: &str = "journal.jsonl";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Umbenannt,
    Rueckgaengig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// fortlaufende Nummer innerhalb der Datei
    pub id: u64,
    /// Unix-Zeit in Sekunden
    pub zeit: u64,
    pub aktion: Action,
    pub von: PathBuf,
    pub nach: PathBuf,
    /// bei `Rueckgaengig`: die zurückgenommene Nummer
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bezug: Option<u64>,
    /// unveränderte Kopie des Originals (vor dem Schreiben von Metadaten), falls gesichert
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sicherung: Option<PathBuf>,
}

/// Unterordner für Originalkopien.
pub const BACKUP_DIR: &str = "originale";

#[derive(Debug, Clone)]
pub struct Journal {
    path: PathBuf,
}

impl Journal {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Journal { path: path.into() }
    }

    /// Journal im Eingangsordner; ist der nicht beschreibbar, im Ausweichordner (Einstellungsordner).
    pub fn for_inbox(inbox: &Path, fallback_dir: Option<&Path>) -> Self {
        let dir = inbox.join(DATA_DIR);
        if std::fs::create_dir_all(&dir).is_ok() && is_writable(&dir) {
            return Journal::new(dir.join(JOURNAL_FILE));
        }
        let fallback = fallback_dir.map(Path::to_path_buf).unwrap_or_else(std::env::temp_dir);
        let _ = std::fs::create_dir_all(&fallback);
        Journal::new(fallback.join(JOURNAL_FILE))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> Vec<JournalEntry> {
        let Ok(f) = std::fs::File::open(&self.path) else {
            return Vec::new();
        };
        BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| serde_json::from_str(&l).ok())
            .collect()
    }

    fn append(
        &self,
        aktion: Action,
        von: &Path,
        nach: &Path,
        bezug: Option<u64>,
        sicherung: Option<PathBuf>,
    ) -> std::io::Result<JournalEntry> {
        let id = self.entries().last().map_or(1, |e| e.id + 1);
        let zeit = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let entry = JournalEntry {
            id,
            zeit,
            aktion,
            von: von.to_path_buf(),
            nach: nach.to_path_buf(),
            bezug,
            sicherung,
        };
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(&entry).map_err(std::io::Error::other)?)?;
        f.sync_all()?;
        Ok(entry)
    }

    /// Benennt `from` in `target_dir/name` um (Kollisionen → `_2` …) und protokolliert es.
    /// Mit `backup_dir` wird vorher eine unveränderte Kopie dort abgelegt (für späteres Schreiben von Metadaten).
    pub fn rename(
        &self,
        from: &Path,
        target_dir: &Path,
        name: &str,
        backup_dir: Option<&Path>,
    ) -> std::io::Result<JournalEntry> {
        std::fs::create_dir_all(target_dir)?;
        let sicherung = match backup_dir {
            Some(dir) => {
                std::fs::create_dir_all(dir)?;
                let original = from
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let copy = dir.join(dedupe(&original, |n| dir.join(n).exists()));
                std::fs::copy(from, &copy)?;
                Some(copy)
            }
            None => None,
        };
        let name = dedupe(name, |n| {
            let candidate = target_dir.join(n);
            candidate.exists() && candidate != from
        });
        let to = target_dir.join(name);
        if to != from {
            move_file(from, &to)?;
        }
        self.append(Action::Umbenannt, from, &to, None, sicherung)
    }

    /// Nimmt die Umbenennung mit der Nummer `id` zurück.
    pub fn undo(&self, id: u64) -> std::io::Result<JournalEntry> {
        let entries = self.entries();
        let entry = entries
            .iter()
            .find(|e| e.id == id && e.aktion == Action::Umbenannt)
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, format!("Eintrag {id} fehlt im Journal"))
            })?;
        if entries.iter().any(|e| e.bezug == Some(id)) {
            return Err(std::io::Error::other(format!(
                "Eintrag {id} wurde schon zurückgenommen"
            )));
        }
        if entry.von.exists() && entry.von != entry.nach {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("{} existiert wieder, nicht überschrieben", entry.von.display()),
            ));
        }
        match entry.sicherung.as_ref().filter(|s| s.is_file()) {
            // Original gesichert: unveränderten Inhalt zurückholen, geänderte Datei entfernen
            Some(backup) => {
                move_file(backup, &entry.von)?;
                if entry.nach != entry.von {
                    std::fs::remove_file(&entry.nach)?;
                }
            }
            None if entry.von != entry.nach => move_file(&entry.nach, &entry.von)?,
            None => {}
        }
        self.append(Action::Rueckgaengig, &entry.nach, &entry.von, Some(id), None)
    }
}

/// Verschiebt eine Datei, ohne ein Ziel zu überschreiben. Über Laufwerksgrenzen: kopieren, vergleichen, löschen.
pub fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() && !same_file(from, to) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} existiert schon", to.display()),
        ));
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            std::fs::copy(from, to)?;
            if std::fs::read(from)? != std::fs::read(to)? {
                let _ = std::fs::remove_file(to);
                return Err(std::io::Error::other(
                    "Kopie weicht vom Original ab, Original bleibt erhalten",
                ));
            }
            std::fs::remove_file(from)
        }
        Err(e) => Err(e),
    }
}

/// Auf Systemen ohne Unterscheidung von Groß/Klein ist „a.pdf“ → „A.pdf“ dieselbe Datei.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".schreibtest");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(probe);
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("af-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn rename_collision_and_undo() {
        let dir = temp_dir("rename");
        let journal = Journal::for_inbox(&dir, None);
        std::fs::write(dir.join("scan1.pdf"), b"eins").unwrap();
        std::fs::write(dir.join("scan2.pdf"), b"zwei").unwrap();

        let a = journal.rename(&dir.join("scan1.pdf"), &dir, "Ziel.pdf", None).unwrap();
        let b = journal.rename(&dir.join("scan2.pdf"), &dir, "Ziel.pdf", None).unwrap();
        assert_eq!(a.nach, dir.join("Ziel.pdf"));
        assert_eq!(b.nach, dir.join("Ziel_2.pdf"));
        assert_eq!(std::fs::read(dir.join("Ziel_2.pdf")).unwrap(), b"zwei");

        journal.undo(b.id).unwrap();
        assert!(dir.join("scan2.pdf").exists() && !dir.join("Ziel_2.pdf").exists());
        assert!(journal.undo(b.id).is_err(), "zweimal zurücknehmen geht nicht");
        assert_eq!(journal.entries().len(), 3);
        assert_eq!(journal.path(), dir.join(DATA_DIR).join(JOURNAL_FILE));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn move_into_subfolder() {
        let dir = temp_dir("move");
        let journal = Journal::for_inbox(&dir, None);
        std::fs::write(dir.join("scan.pdf"), b"x").unwrap();
        let e = journal
            .rename(&dir.join("scan.pdf"), &dir.join("2026/Firma"), "Brief.pdf", None)
            .unwrap();
        assert!(e.nach.ends_with("2026/Firma/Brief.pdf") && e.nach.exists());
        journal.undo(e.id).unwrap();
        assert!(dir.join("scan.pdf").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn undo_refuses_to_overwrite() {
        let dir = temp_dir("overwrite");
        let journal = Journal::for_inbox(&dir, None);
        std::fs::write(dir.join("scan.pdf"), b"alt").unwrap();
        let e = journal.rename(&dir.join("scan.pdf"), &dir, "Neu.pdf", None).unwrap();
        std::fs::write(dir.join("scan.pdf"), b"neuer Scan mit gleichem Namen").unwrap();
        assert!(journal.undo(e.id).is_err());
        assert_eq!(std::fs::read(dir.join("Neu.pdf")).unwrap(), b"alt");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn backup_restores_original_content() {
        let dir = temp_dir("backup");
        let journal = Journal::for_inbox(&dir, None);
        std::fs::write(dir.join("scan.pdf"), b"original").unwrap();
        let backups = dir.join(DATA_DIR).join(BACKUP_DIR);
        let e = journal
            .rename(&dir.join("scan.pdf"), &dir, "Neu.pdf", Some(&backups))
            .unwrap();
        // Metadaten schreiben verändert die Datei
        std::fs::write(dir.join("Neu.pdf"), b"mit Metadaten").unwrap();
        journal.undo(e.id).unwrap();
        assert_eq!(std::fs::read(dir.join("scan.pdf")).unwrap(), b"original");
        assert!(!dir.join("Neu.pdf").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
