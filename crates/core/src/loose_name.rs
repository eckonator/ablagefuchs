// SPDX-License-Identifier: MIT OR Apache-2.0
//! Archivnamen ohne festes Schema lesen, z. B. `2025-03-14_Stadtwerke_Musterstadt_Abschlag.pdf`:
//! Wo die Firma aufhört und das Thema beginnt, steht nicht im Namen. Deshalb zählt eine Wortfolge nur dann
//! als Firma, wenn der Brief selbst sie bestätigt (Briefkopf, Absender- oder Fußzeile) oder eine eindeutige
//! Kennung auf ein schon bekanntes Unternehmen dieses Namens zeigt. Sonst wird nichts gelernt.

use chrono::NaiveDate;

use crate::company;
use crate::knowledge::normalize_name;
use crate::layout::{Layout, Line, Zone};
use crate::naming::NameFields;

/// Längste Wortfolge, die als Firma geprüft wird.
const MAX_RUN: usize = 5;

/// Wörter, die als bloßer Anfang eines Briefkopfs nichts über die Firma sagen („Die Techniker“).
const WEAK_PREFIX: [&str; 6] = ["die", "der", "das", "ihre", "ihr", "sehr"];

/// Ein Archivname, zerlegt in Datum und Wörter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LooseName {
    pub datum: Option<NaiveDate>,
    pub words: Vec<String>,
}

impl LooseName {
    /// Zerlegt einen Dateinamen an `_` und Leerzeichen. Führende Datums- und Zahlenteile fallen weg
    /// (das Datum wird gemerkt), „TelekomRechnung“ wird zu „Telekom“ + „Rechnung“ getrennt,
    /// „Stromrechnung“ bleibt ein Wort. `None`, wenn kein Wort mit Buchstaben übrig bleibt.
    pub fn parse(file_name: &str) -> Option<LooseName> {
        let stem = match file_name.len().checked_sub(4) {
            Some(i) if file_name.is_char_boundary(i) && file_name[i..].eq_ignore_ascii_case(".pdf") => &file_name[..i],
            _ => file_name,
        };
        let mut parts: Vec<&str> = stem.split(['_', ' ']).filter(|w| !w.is_empty()).collect();
        let mut datum = None;
        while let Some(first) = parts.first() {
            if !first.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '.') {
                break;
            }
            if datum.is_none() {
                datum = NaiveDate::parse_from_str(first, "%Y-%m-%d").ok();
            }
            parts.remove(0);
        }
        let words: Vec<String> = parts.into_iter().flat_map(split_camel_invoice).collect();
        words
            .iter()
            .any(|w| w.chars().any(char::is_alphabetic))
            .then_some(LooseName { datum, words })
    }

    /// Sucht die längste (bei Gleichstand früheste) Wortfolge, die `is_company` als Firma bestätigt,
    /// und teilt den Namen damit in Unternehmen und Thema. `is_company` bekommt die Vergleichsform
    /// ([`normalize_name`]) und gibt zurück, ob nur ein Anfang des Firmennamens passt (`Some(true)`)
    /// oder der ganze (`Some(false)`).
    pub fn split(&self, is_company: impl Fn(&str) -> Option<bool>) -> Option<NameFields> {
        let n = self.words.len();
        for len in (1..=MAX_RUN.min(n)).rev() {
            for start in 0..=n - len {
                let run = &self.words[start..start + len];
                if run.iter().any(|w| is_invoice_word(w)) {
                    continue;
                }
                let norm = normalize_name(&run.join(" "));
                if norm.chars().filter(|c| c.is_alphabetic()).count() < 3 {
                    continue;
                }
                let ok = match is_company(&norm) {
                    Some(false) => true,
                    // Nur ein Anfang des Briefkopfs: kurze oder nichtssagende Wörter reichen nicht
                    Some(true) => norm.len() >= 4 && !WEAK_PREFIX.contains(&norm.as_str()),
                    None => false,
                };
                if !ok {
                    continue;
                }
                let thema: Vec<&str> = self
                    .words
                    .iter()
                    .enumerate()
                    .filter(|(i, w)| (*i < start || *i >= start + len) && !w.eq_ignore_ascii_case("rechnung"))
                    .map(|(_, w)| w.as_str())
                    .collect();
                return Some(NameFields {
                    datum: self.datum,
                    unternehmen: run.join(" "),
                    thema: thema.join(" "),
                    rechnung: self.words.iter().any(|w| is_invoice_word(w)),
                });
            }
        }
        None
    }
}

/// „Rechnung“, „Stromrechnung“, „Abschlagsrechnung“ …
fn is_invoice_word(word: &str) -> bool {
    word.to_lowercase().ends_with("rechnung")
}

/// „TelekomRechnung“ → „Telekom“, „Rechnung“ (nur bei großem R hinter einem Kleinbuchstaben).
fn split_camel_invoice(word: &str) -> Vec<String> {
    if let Some(i) = word.rfind("Rechnung")
        && i > 0
        && word[..i].chars().last().is_some_and(char::is_lowercase)
    {
        return vec![word[..i].to_string(), word[i..].to_string()];
    }
    vec![word.to_string()]
}

/// Liegt eine Zeile im Anschriftenfeld? Gleiche linke Kante wie „Herrn“/„Frau“, bis etwa fünf Zeilen darunter.
fn address_window<'a>(page1: &[&'a Line]) -> impl Fn(&Line) -> bool + 'a {
    let recipients: Vec<&'a Line> = page1
        .iter()
        .copied()
        .filter(|l| company::is_recipient_line(&l.text))
        .collect();
    move |l: &Line| {
        recipients.iter().any(|r| {
            let dy = l.bbox.y - r.bbox.y;
            (l.bbox.x - r.bbox.x).abs() < 20.0 && (0.0..=r.bbox.h.max(8.0) * 6.0).contains(&dy)
        })
    }
}

/// Alle Zeilen der ersten Seite außer der Empfängeranschrift, in Vergleichsform.
pub fn page_texts(layout: &Layout) -> Vec<String> {
    let page1: Vec<&Line> = layout.lines.iter().filter(|l| l.page == 1).collect();
    let in_address = address_window(&page1);
    page1
        .iter()
        .filter(|l| !in_address(l))
        .map(|l| normalize_name(&l.text))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Wie eine Wortfolge (Vergleichsform) im Brief vorkommt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mention {
    No,
    /// fast gleich geschrieben (Lesefehler der Texterkennung), nur bei mindestens 6 Zeichen
    Similar,
    /// wörtlich, als ganze Wörter
    Exact,
}

pub fn mention(texts: &[String], norm: &str) -> Mention {
    let needle = format!(" {norm} ");
    if texts.iter().any(|t| format!(" {t} ").contains(&needle)) {
        return Mention::Exact;
    }
    if norm.len() < 6 {
        return Mention::No;
    }
    let n = norm.split(' ').count();
    let similar = texts.iter().any(|t| {
        let tokens: Vec<&str> = t.split(' ').collect();
        tokens.len() >= n
            && tokens
                .windows(n)
                .any(|w| strsim::jaro_winkler(&w.join(" "), norm) >= 0.9)
    });
    if similar { Mention::Similar } else { Mention::No }
}

/// Textstellen der ersten Seite, die den Absender nennen: alle Zeilen im Kopfbereich (Logo-Schrift, Briefkopf,
/// Absenderzeile), die kleine Absenderzeile über der Anschrift und die Fußzeile, jeweils an `·`, `|` und `,` in
/// Teile zerlegt, in Vergleichsform. Fließtext und Empfängeranschrift („Herrn …“ und die Zeilen darunter) zählen nicht.
pub fn sender_texts(layout: &Layout) -> Vec<String> {
    let body = layout.body_font_size();
    let page1: Vec<&Line> = layout.lines.iter().filter(|l| l.page == 1).collect();
    let in_address = address_window(&page1);
    let mut lines: Vec<&str> = Vec::new();
    let letterhead = company::detect(layout);
    if let Some(c) = &letterhead {
        lines.push(&c.name);
    }
    for l in page1 {
        let sender_line = l.font_size < body && l.text.contains(" · ") && l.zone != Zone::Body;
        let header = l.zone == Zone::Header && !in_address(l);
        if sender_line || header || l.zone == Zone::Footer {
            lines.push(&l.text);
        }
    }
    let mut out: Vec<String> = lines
        .iter()
        .flat_map(|l| l.split(['·', '|', ',']))
        .map(normalize_name)
        .filter(|s| !s.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Warum ein freier Name nicht bestätigt wurde (nur für die Statistik, ohne Inhalte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Miss {
    /// auf Seite 1 kaum Text (Texterkennung fand nichts Brauchbares)
    NoText,
    /// eine Wortfolge des Namens steht wörtlich auf Seite 1, aber nur im Fließtext oder in der Anschrift
    InBody,
    /// steht nur ähnlich da (Lesefehler der Texterkennung?)
    Similar,
    /// nirgends (Logo als Bild, Abkürzung, anderer Name)
    Missing,
}

impl LooseName {
    /// Ordnet einen nicht bestätigten Namen einer [`Miss`]-Art zu (`texts` aus [`page_texts`]).
    pub fn miss_reason(&self, texts: &[String]) -> Miss {
        let letters: usize = texts
            .iter()
            .map(|t| t.chars().filter(|c| c.is_alphabetic()).count())
            .sum();
        if letters < 40 {
            return Miss::NoText;
        }
        let best = self
            .candidate_runs()
            .iter()
            .map(|r| mention(texts, r))
            .max()
            .unwrap_or(Mention::No);
        match best {
            Mention::Exact => Miss::InBody,
            Mention::Similar => Miss::Similar,
            Mention::No => Miss::Missing,
        }
    }

    /// Wortfolgen, die als Firma in Frage kommen (Vergleichsform).
    fn candidate_runs(&self) -> Vec<String> {
        let n = self.words.len();
        let mut out = Vec::new();
        for len in 1..=MAX_RUN.min(n) {
            for start in 0..=n - len {
                let run = &self.words[start..start + len];
                if run.iter().any(|w| is_invoice_word(w)) {
                    continue;
                }
                let norm = normalize_name(&run.join(" "));
                if norm.chars().filter(|c| c.is_alphabetic()).count() >= 4 {
                    out.push(norm);
                }
            }
        }
        out
    }
}

/// Prüft eine Wortfolge gegen Absendertexte: ganzer Text gleich (`Some(false)`) oder nur dessen Anfang
/// (`Some(true)`, z. B. „Stadtwerke“ zu „Stadtwerke Musterstadt“).
pub fn matches_any<'a>(norm: &str, texts: impl IntoIterator<Item = &'a str>) -> Option<bool> {
    let prefix = format!("{norm} ");
    let mut found = None;
    for t in texts {
        if t == norm {
            return Some(false);
        }
        if t.starts_with(&prefix) {
            found = Some(true);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split_with(name: &str, texts: &[&str]) -> Option<NameFields> {
        let texts: Vec<String> = texts.iter().map(|t| normalize_name(t)).collect();
        LooseName::parse(name)?.split(|n| matches_any(n, texts.iter().map(String::as_str)))
    }

    #[test]
    fn words_and_date() {
        let n = LooseName::parse("2025-03-14_Stadtwerke_Musterstadt_Abschlag.pdf").unwrap();
        assert_eq!(n.datum, NaiveDate::from_ymd_opt(2025, 3, 14));
        assert_eq!(n.words, ["Stadtwerke", "Musterstadt", "Abschlag"]);
        let n = LooseName::parse("2025_NetzWelt_Rechnung_12345.PDF").unwrap();
        assert_eq!(n.datum, None);
        assert_eq!(n.words, ["NetzWelt", "Rechnung", "12345"]);
        assert_eq!(
            LooseName::parse("2025-03-14_TelekomRechnung.pdf").unwrap().words,
            ["Telekom", "Rechnung"]
        );
        assert_eq!(
            LooseName::parse("2025-03-14_Stromrechnung.pdf").unwrap().words,
            ["Stromrechnung"]
        );
        assert_eq!(LooseName::parse("2025-03-14.pdf"), None);
        assert_eq!(LooseName::parse("2025-03-14_7.pdf"), None);
    }

    #[test]
    fn company_confirmed_by_letter() {
        let f = split_with(
            "2025-03-14_Stadtwerke_Musterstadt_Abschlag_Rechnung.pdf",
            &["Stadtwerke Musterstadt GmbH", "USt-IdNr. DE123456789"],
        )
        .unwrap();
        assert_eq!(f.unternehmen, "Stadtwerke Musterstadt");
        assert_eq!(f.thema, "Abschlag");
        assert!(f.rechnung);
        assert_eq!(f.datum, NaiveDate::from_ymd_opt(2025, 3, 14));
    }

    #[test]
    fn short_name_and_topic_first() {
        // Nutzer schreibt nur den Anfang des Briefkopfs, Thema vorne
        let f = split_with(
            "2025-03-14_Teilrechnung_Stadtwerke.pdf",
            &["Stadtwerke Musterstadt GmbH"],
        )
        .unwrap();
        assert_eq!(f.unternehmen, "Stadtwerke");
        assert_eq!(f.thema, "Teilrechnung");
        assert!(f.rechnung);
        let f = split_with("2025-03-14_NetzWeltRechnung.pdf", &["NetzWelt Kommunikation"]).unwrap();
        assert_eq!(f.unternehmen, "NetzWelt");
        assert_eq!(f.thema, "");
    }

    #[test]
    fn nothing_without_confirmation() {
        assert_eq!(
            split_with("2025-03-14_Stadtwerke_Abschlag.pdf", &["NetzWelt Kommunikation"]),
            None
        );
        // „Die“ ist kein Firmenname, nur weil der Briefkopf so anfängt
        assert_eq!(split_with("2025-03-14_Die_Abrechnung.pdf", &["Die Techniker"]), None);
        // ein Thema, das zufällig wie ein Rechnungswort aussieht, wird nie Firma
        assert_eq!(split_with("2025-03-14_Rechnung.pdf", &["Rechnung"]), None);
    }

    /// Seite aus (Text, x, y, Schriftgröße).
    fn page(lines: &[(&str, f32, f32, f32)]) -> Layout {
        let mut words = Vec::new();
        for &(text, x0, y, size) in lines {
            let mut x = x0;
            for w in text.split(' ') {
                let width = w.chars().count() as f32 * size * 0.5;
                words.push(crate::Word {
                    text: w.into(),
                    bbox: crate::Rect {
                        x,
                        y,
                        w: width,
                        h: size,
                    },
                    font_size: size,
                    bold: false,
                });
                x += width + size * 0.3;
            }
        }
        Layout::new(&[crate::PageText {
            page: 1,
            width: 595.0,
            height: 842.0,
            words,
        }])
    }

    const BODY: &str = "Sehr geehrter Herr Beispiel, anbei erhalten Sie die Abrechnung für den vergangenen Zeitraum.";

    #[test]
    fn every_header_line_counts_but_not_the_recipient() {
        let layout = page(&[
            ("RECHNUNG", 60.0, 40.0, 26.0),
            ("Autohaus Becker", 400.0, 50.0, 9.0),
            ("Herrn", 60.0, 120.0, 10.0),
            ("Max Beispiel", 60.0, 132.0, 10.0),
            (BODY, 60.0, 300.0, 10.0),
            (BODY, 60.0, 320.0, 10.0),
        ]);
        let texts = sender_texts(&layout);
        assert!(texts.contains(&"autohaus becker".to_string()), "{texts:?}");
        assert!(!texts.contains(&"max beispiel".to_string()), "{texts:?}");
    }

    #[test]
    fn reasons_for_unconfirmed_names() {
        let name = LooseName::parse("2025-03-14_Autohaus_Becker_Inspektion.pdf").unwrap();
        assert_eq!(
            name.miss_reason(&page_texts(&page(&[("Becker", 60.0, 40.0, 20.0)]))),
            Miss::NoText
        );
        let in_body = page(&[
            (BODY, 60.0, 300.0, 10.0),
            ("Ihr Autohaus Becker Team", 60.0, 340.0, 10.0),
        ]);
        assert_eq!(name.miss_reason(&page_texts(&in_body)), Miss::InBody);
        let misread = page(&[(BODY, 60.0, 300.0, 10.0), ("Autohcus Beoker", 60.0, 340.0, 10.0)]);
        assert_eq!(name.miss_reason(&page_texts(&misread)), Miss::Similar);
        assert_eq!(
            name.miss_reason(&page_texts(&page(&[(BODY, 60.0, 300.0, 10.0)]))),
            Miss::Missing
        );
    }
}
