// SPDX-License-Identifier: MIT OR Apache-2.0
//! Sprachen der Oberfläche (Bauplan §7.6).
//!
//! Der deutsche Text im Code ist zugleich der Schlüssel: `tr("Ordner wählen …")` liefert auf Englisch
//! "Choose folder …". Platzhalter `{}` werden mit `trf` der Reihe nach ersetzt. Fehlt eine Übersetzung,
//! bleibt der deutsche Text (und ein Test schlägt fehl).

use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::settings::LangSetting;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::De
    }
}

/// Einstellung „System“ folgt der Systemsprache: Deutsch, wenn sie mit „de“ beginnt, sonst Englisch.
pub fn apply_setting(s: LangSetting) {
    set_lang(match s {
        LangSetting::De => Lang::De,
        LangSetting::En => Lang::En,
        LangSetting::System => {
            if sys_locale::get_locale().is_some_and(|l| l.to_lowercase().starts_with("de")) {
                Lang::De
            } else {
                Lang::En
            }
        }
    });
}

/// Übersetzt einen festen Text.
pub fn tr(de: &'static str) -> &'static str {
    if !de.chars().any(char::is_alphabetic) {
        return de;
    }
    match lang() {
        Lang::De => de,
        Lang::En => en(de).unwrap_or_else(|| {
            if cfg!(debug_assertions) {
                log::debug!("Übersetzung fehlt: {de:?}");
            }
            de
        }),
    }
}

/// Übersetzt einen Text mit `{}`-Platzhaltern und setzt die Werte der Reihe nach ein.
pub fn trf(de: &'static str, args: &[&dyn Display]) -> String {
    let template = tr(de);
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut rest = template;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(a) = args.next() {
            out.push_str(&a.to_string());
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

fn en(de: &str) -> Option<&'static str> {
    Some(match de {
        "{} / {} erledigt" => "{} / {} done",
        "Ablagefuchs arbeitet vollständig offline. Es werden keine Daten übertragen." => {
            "Ablagefuchs works completely offline. No data is transmitted."
        }
        "Ablagefuchs wird ohne jegliche Gewährleistung bereitgestellt." => {
            "Ablagefuchs is provided without any warranty."
        }
        "Ablagefuchs wurde wegen eines Fehlers beendet:\n\n{}\n({})" => {
            "Ablagefuchs was closed because of an error:\n\n{}\n({})"
        }
        "Alle Dateien sind erledigt." => "All files are done.",
        "Apache-Lizenz 2.0 anzeigen" => "Show Apache License 2.0",
        "Archivordner wählen" => "Choose archive folder",
        "Archivordner:" => "Archive folder:",
        "Aus dem Briefkopf (größte Schrift)" => "From the letterhead (largest font)",
        "Aus dem Briefkopf, bestätigt durch die Absenderzeile" => "From the letterhead, confirmed by the sender line",
        "Aus der Betreffzeile" => "From the subject line",
        "Aus der ersten Rechnungsposition" => "From the first invoice item",
        "Aussehen" => "Appearance",
        "Automatisch (DirectX 12)" => "Automatic (DirectX 12)",
        "Automatisch" => "Automatic",
        "beibehalten (ä ö ü ß)" => "keep (ä ö ü ß)",
        "beibehalten" => "keep",
        "Beispiele" => "Examples",
        "Bleibt im Eingangsordner" => "Stays in the inbox folder",
        "Das Fenster ließ sich nicht öffnen:\n{}" => "The window could not be opened:\n{}",
        "Datei nicht lesbar: {}" => "File not readable: {}",
        "Dateiliste ein/aus (⌘L)" => "Toggle file list (⌘L)",
        "Dateiliste" => "File list",
        "Datenschutz" => "Privacy",
        "Datum fehlt oder ist ungültig" => "Date missing or invalid",
        "Datum" => "Date",
        "Diagnose" => "Diagnostics",
        "Die Dateinamen-Vorlage ist fehlerhaft, bitte in den Einstellungen korrigieren." => {
            "The file name template is invalid, please fix it in the settings."
        }
        "Die vollständige Liste liegt jedem Download als THIRD-PARTY-LICENSES bei." => {
            "The full list ships with every download as THIRD-PARTY-LICENSES."
        }
        "Die Vorlage ist leer" => "The template is empty",
        "Dokumentdatum" => "document date",
        "Dunkel" => "Dark",
        "Eine geschweifte Klammer { wird nicht geschlossen" => "A curly brace { is not closed",
        "Eingangsordner wählen" => "Choose inbox folder",
        "Einstellungen" => "Settings",
        "Einstellungsordner öffnen" => "Open settings folder",
        "Einzelheiten stehen in der Logdatei:\n{}" => "Details are in the log file:\n{}",
        "entfernen" => "remove",
        "erlauben (nur verbotene Zeichen entfernen)" => "allow (only remove forbidden characters)",
        "Erledigt: {}" => "Done: {}",
        "Fälligkeit" => "due date",
        "Fehler melden" => "Report a bug",
        "Feld wählen" => "select field",
        "Fremdbestandteile" => "Third-party components",
        "Gefunden: {}" => "Found: {}",
        "Grafik" => "Graphics",
        "Hell" => "Light",
        "Im Dokument als Datum ausgewiesen" => "Marked as the date in the document",
        "In das Archiv verschieben" => "Move to the archive",
        "In diesem Ordner liegen keine PDF-Dateien." => "There are no PDF files in this folder.",
        "Jede Umbenennung steht in .ablagefuchs/journal.jsonl im Eingangsordner und lässt sich rückgängig machen." => {
            "Every rename is logged in .ablagefuchs/journal.jsonl in the inbox folder and can be undone."
        }
        "JJJJ-MM-TT" => "YYYY-MM-DD",
        "Keine Rechnungsmerkmale gefunden" => "No invoice indicators found",
        "Leerzeichen" => "Spaces",
        "Leistungsdatum" => "service date",
        "Markierungen" => "Highlights",
        "Ordner wird überwacht – neue Scans erscheinen automatisch" => {
            "Watching folder – new scans appear automatically"
        }
        "nicht mehr vorhanden" => "no longer present",
        "Kein Text" => "No text",
        "Nichts erkannt (Foto, Handschrift oder leere Seite?)" => {
            "Nothing recognized (photo, handwriting or blank page?)"
        }
        "Nichts erkannt. Für Scans wird Tesseract benötigt (tesseract-ocr, tesseract-ocr-deu)." => {
            "Nothing recognized. Scans need Tesseract (tesseract-ocr, tesseract-ocr-deu)."
        }
        "Texterkennung: {} · Sicherheit {} %" => "Text recognition: {} · confidence {} %",
        "Automatisch (Apple Vision)" => "Automatic (Apple Vision)",
        "Automatisch (Tesseract)" => "Automatic (Tesseract)",
        "Automatisch (Windows-Texterkennung)" => "Automatic (Windows OCR)",
        "Immer Tesseract" => "Always Tesseract",
        "Keine Texterkennung verfügbar. Bitte Tesseract installieren (z. B. tesseract-ocr und tesseract-ocr-deu)." => {
            "No text recognition available. Please install Tesseract (e.g. tesseract-ocr and tesseract-ocr-deu)."
        }
        "Texterkennung für Scans" => "Text recognition for scans",
        "Wird nur für Scans ohne Textebene gebraucht. Ergebnisse werden zwischengespeichert." => {
            "Only needed for scans without a text layer. Results are cached."
        }
        "Lizenz" => "License",
        "Lizenziert unter MIT oder Apache-2.0, nach Wahl." => "Licensed under MIT or Apache-2.0, at your option.",
        "Logdatei anzeigen" => "Show log file",
        "manuell" => "manual",
        "MIT-Lizenz anzeigen" => "Show MIT license",
        "Nach dem Umbenennen" => "After renaming",
        "Name existiert bereits → wird mit _2 gespeichert" => "Name already exists → will be saved with _2",
        "Neuer Dateiname" => "New file name",
        "nicht gewählt" => "not chosen",
        "nicht lesbar" => "not readable",
        "nichts erkannt" => "nothing recognized",
        "Nichts gefunden – bitte von Hand eintragen" => "Nothing found – please enter manually",
        "nur A–Z, 0–9, _ und - (é → e, & → und)" => "only A–Z, 0–9, _ and - (é → e, & → und)",
        "Ohne Verschieben bleibt die Datei im Eingangsordner und wird nur umbenannt." => {
            "Without moving, the file stays in the inbox folder and is only renamed."
        }
        "Ordner nicht lesbar: {}" => "Folder not readable: {}",
        "Ordner wählen …" => "Choose folder …",
        "Platzhalter in {…}. Bedingter Text wie {rechnung?:_Rechnung} erscheint nur, wenn das Feld gesetzt ist." => {
            "Placeholders in {…}. Conditional text like {rechnung?:_Rechnung} only appears if the field is set."
        }
        "Plausibelstes Datum im Dokument" => "Most plausible date in the document",
        "Protokoll" => "Journal",
        "Quellcode" => "Source code",
        "Rechnung" => "Invoice",
        "Rückgängig fehlgeschlagen: {}" => "Undo failed: {}",
        "Rückgängig gemacht" => "Undone",
        "rückgängig" => "undo",
        "Rückgängig" => "Undo",
        "Schließen" => "Close",
        "Seite {} / {}" => "Page {} / {}",
        "Sonderzeichen" => "Special characters",
        "sonstiges Datum" => "other date",
        "Sprache" => "Language",
        "System" => "System",
        "Text" => "Text",
        "Textebene vorhanden" => "Text layer present",
        "Thema" => "Topic",
        "übernehmen" => "accept",
        "überspringen / nächste" => "skip / next",
        "Überspringen" => "Skip",
        "Umbenannt → {}" => "Renamed → {}",
        "Umbenennen & weiter" => "Rename & next",
        "Umbenennen fehlgeschlagen: {}" => "Rename failed: {}",
        "Umbenennung rückgängig machen" => "Undo rename",
        "Umlaute" => "Umlauts",
        "umschreiben (ae oe ue ss)" => "transliterate (ae oe ue ss)",
        "Unbekannter Platzhalter {} – meintest du {}?" => "Unknown placeholder {} – did you mean {}?",
        "Unbekannter Platzhalter {}" => "Unknown placeholder {}",
        "Unsicher: kein ausgewiesenes Dokumentdatum gefunden" => "Uncertain: no marked document date found",
        "Unternehmen fehlt" => "Company missing",
        "Unternehmen" => "Company",
        "Unterordner-Vorlage:" => "Subfolder template:",
        "Version {}" => "Version {}",
        "vorherige" => "previous",
        "Vorlage" => "Template",
        "Vorlagen:" => "Presets:",
        "Vorschlag zurücksetzen" => "Reset suggestion",
        "Wähle den Ordner, in den dein Scanner die Dokumente legt." => {
            "Choose the folder your scanner saves documents to."
        }
        "Wählen …" => "Choose …",
        "wird gelesen …" => "reading …",
        "Wirkt nach einem Neustart. Startet die Grafik nicht, wechselt Ablagefuchs beim nächsten Start automatisch auf OpenGL." => {
            "Takes effect after a restart. If graphics fail to start, Ablagefuchs switches to OpenGL on the next launch."
        }
        "Wirkt sofort, ohne Neustart." => "Takes effect immediately, no restart needed.",
        "Zeichen im Dateinamen" => "Characters in the file name",
        "Zeitraum bis" => "period to",
        "Zeitraum von" => "period from",
        "Zuletzt benutzt" => "Recently used",
        "Zurzeit aktiv: OpenGL (Ausweichweg nach einem Grafikproblem)." => {
            "Currently active: OpenGL (fallback after a graphics problem)."
        }
        "Alle" => "All",
        "Offen" => "Open",
        "Prüfen" => "Review",
        "Erledigt" => "Done",
        "Allgemein" => "General",
        "Dateiname" => "File name",
        "Ordner" => "Folders",
        "Über" => "About",
        "Standard" => "Default",
        "Rechnung vorne" => "Invoice first",
        "Ohne Thema" => "Without topic",
        "Jahr-Monat" => "Year-month",
        "egui-Schriften (Ubuntu, Noto Emoji, Emoji Icon Font)" => "egui fonts (Ubuntu, Noto Emoji, Emoji Icon Font)",
        "DejaVu Sans (Ausschnitt für Symbole)" => "DejaVu Sans (subset for symbols)",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Alle `tr("…")`/`trf("…")`-Texte im Quellcode müssen eine englische Übersetzung haben.
    #[test]
    fn every_text_is_translated() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let call = regex_lite_find;
        let mut missing = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let src = std::fs::read_to_string(&path).unwrap();
            for text in call(&src) {
                if text.chars().any(char::is_alphabetic) && en(&text).is_none() {
                    missing.push(format!("{}: {text:?}", path.file_name().unwrap().to_string_lossy()));
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "Übersetzung fehlt für:\n{}", missing.join("\n"));
    }

    /// Findet Zeichenketten in `tr("…")`, `trf("…"` sowie in Tabellen mit `(Wert, "…")`, die später übersetzt werden.
    fn regex_lite_find(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for marker in ["tr(\"", "trf(\""] {
            let mut rest = src;
            while let Some(i) = rest.find(marker) {
                // nicht mitten in einem längeren Namen wie „attr(“
                let before = rest[..i].chars().last();
                rest = &rest[i + marker.len()..];
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                if let Some(end) = find_string_end(rest) {
                    out.push(unescape(&rest[..end]));
                }
            }
        }
        out
    }

    fn find_string_end(s: &str) -> Option<usize> {
        let mut escaped = false;
        for (i, c) in s.char_indices() {
            match c {
                '\\' if !escaped => escaped = true,
                '"' if !escaped => return Some(i),
                _ => escaped = false,
            }
        }
        None
    }

    fn unescape(s: &str) -> String {
        s.replace("\\n", "\n").replace("\\\"", "\"").replace("\\\\", "\\")
    }

    #[test]
    fn placeholders_are_filled_in_order() {
        set_lang(Lang::De);
        assert_eq!(trf("{} / {} erledigt", &[&3, &7]), "3 / 7 erledigt");
    }
}
