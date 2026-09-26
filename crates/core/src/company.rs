// SPDX-License-Identifier: MIT OR Apache-2.0
//! Absender erkennen. Bis zum gelernten Wissen (M5) nur über die Gestaltung des Briefkopfs:
//! größte bzw. fette Schrift oben, Adresszeilen ausgenommen.

use std::sync::LazyLock;

use regex::Regex;

use crate::layout::{Layout, Zone};
use crate::model::Rect;

#[derive(Debug, Clone)]
pub struct CompanyResult {
    pub name: String,
    pub confidence: u8,
    pub page: u16,
    pub rect: Rect,
    /// zusätzlich in der Absenderzeile über der Anschrift gefunden
    pub confirmed_by_sender_line: bool,
}

/// Zeilen, die nach Adresse oder Kontakt aussehen, kommen als Firmenname nicht in Frage.
static ADDRESS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b\d{5}\b|straße|strasse|str\.|platz\b|postfach|allee\b|weg\b|telefon|tel\.|fax|e-?mail|@|www\.|https?:|\.(de|com|example|net|org)\b|kundenservice|hotline|\d{3,}\s?\d{3,}",
    )
    .unwrap()
});

/// Anrede- und Empfängerzeilen gehören zum Anschriftenfeld, nicht zum Absender.
static RECIPIENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(herrn?|frau|familie|eheleute|an|z\.\s?hd\.?)(\s|$)").unwrap());

/// Plausibler Text für Namen und Themen: mindestens drei verschiedene Buchstaben
/// (filtert Texterkennungs-Unsinn wie „VVVVVVVVI“ aus Handschrift oder Linien).
pub fn plausible(text: &str) -> bool {
    let mut letters: Vec<char> = text
        .chars()
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect();
    letters.sort_unstable();
    letters.dedup();
    letters.len() >= 3
}

/// Rechtsformen am Ende, die im Dateinamen stören („NetzWelt Kommunikation GmbH“ → „NetzWelt Kommunikation“).
static LEGAL_FORM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[\s,]+(gmbh\s*&\s*co\.?\s*kg|gmbh|mbh|ag|kg|ohg|gbr|e\.\s?v\.|ug\s*\(haftungsbeschränkt\)|ug|ltd\.?|inc\.?|llc|se|s\.a\.|b\.v\.)$",
    )
    .unwrap()
});

pub fn strip_legal_form(name: &str) -> String {
    let mut s = name.trim().to_string();
    // mehrfach, z. B. „… GmbH & Co. KG“ in Varianten
    loop {
        let stripped = LEGAL_FORM.replace(&s, "").trim().to_string();
        if stripped == s || stripped.is_empty() {
            return s;
        }
        s = stripped;
    }
}

pub fn detect(layout: &Layout) -> Option<CompanyResult> {
    let body = layout.body_font_size();
    let (width, _) = layout.page_size(1);
    let best = layout
        .lines
        .iter()
        .filter(|l| l.page == 1 && l.zone == Zone::Header)
        .filter(|l| !ADDRESS.is_match(&l.text) && !RECIPIENT.is_match(&l.text) && plausible(&l.text))
        .map(|l| {
            let mut score = l.font_size * 2.0;
            if l.bold {
                score += 10.0;
            }
            if l.bbox.x < width * 0.5 {
                score += 5.0;
            }
            (score, l)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, l)| l)?;

    let name = strip_legal_form(&best.text);
    // Absenderzeile: kleine Zeile mit „ · “ über dem Anschriftenfeld
    let confirmed = layout
        .lines
        .iter()
        .filter(|l| l.page == 1 && l.font_size < body && l.text.contains(" · "))
        .any(|l| l.text.starts_with(&name) || l.text.starts_with(&best.text));
    let mut confidence: i32 = if best.font_size >= body * 1.3 { 65 } else { 45 };
    if best.bold {
        confidence += 5;
    }
    if confirmed {
        confidence += 5;
    }
    Some(CompanyResult {
        name,
        confidence: confidence.min(75) as u8,
        page: best.page,
        rect: best.bbox,
        confirmed_by_sender_line: confirmed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_forms() {
        assert_eq!(
            strip_legal_form("NetzWelt Kommunikation GmbH"),
            "NetzWelt Kommunikation"
        );
        assert_eq!(strip_legal_form("Muster GmbH & Co. KG"), "Muster");
        assert_eq!(strip_legal_form("Förderverein Schule e. V."), "Förderverein Schule");
        assert_eq!(strip_legal_form("AG"), "AG");
        assert_eq!(strip_legal_form("Stadtwerke Musterstadt"), "Stadtwerke Musterstadt");
    }
}
