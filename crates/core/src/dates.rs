// SPDX-License-Identifier: MIT OR Apache-2.0
//! Datumsangaben finden, ihre Bedeutung bestimmen (Dokumentdatum, Fälligkeit, Zeitraum …)
//! und das wahrscheinlichste Dokumentdatum auswählen.

use std::sync::LazyLock;

use chrono::{Datelike, NaiveDate};
use regex::Regex;
use serde::Serialize;

use crate::layout::Layout;
use crate::model::Rect;

/// Bedeutung eines gefundenen Datums.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DateKind {
    /// „Datum:“, „Rechnung vom“, „Stuttgart, 15. September 2026“
    Document,
    Due,
    PeriodStart,
    PeriodEnd,
    /// Leistungs-/Behandlungsdatum in einer Tabelle
    Service,
    /// Bekanntgabe, Geburtsdatum, Vertragsbeginn …
    Other,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct DateCandidate {
    pub date: NaiveDate,
    pub kind: DateKind,
    pub page: u16,
    pub rect: Rect,
    /// interne Punktzahl, höher = eher das Dokumentdatum
    pub score: i32,
}

const MONTHS: [(&str, u32); 31] = [
    ("januar", 1),
    ("january", 1),
    ("jan", 1),
    ("februar", 2),
    ("february", 2),
    ("feb", 2),
    ("märz", 3),
    ("maerz", 3),
    ("march", 3),
    ("mär", 3),
    ("mar", 3),
    ("april", 4),
    ("apr", 4),
    ("mai", 5),
    ("may", 5),
    ("juni", 6),
    ("june", 6),
    ("jun", 6),
    ("juli", 7),
    ("july", 7),
    ("jul", 7),
    ("august", 8),
    ("aug", 8),
    ("september", 9),
    ("sept", 9),
    ("sep", 9),
    ("oktober", 10),
    ("october", 10),
    ("okt", 10),
    ("oct", 10),
    ("november", 11),
];

fn month_number(name: &str) -> Option<u32> {
    let n = name.trim_end_matches('.').to_lowercase();
    match n.as_str() {
        "nov" => return Some(11),
        "dezember" | "december" | "dez" | "dec" => return Some(12),
        _ => {}
    }
    MONTHS.iter().find(|(m, _)| *m == n).map(|(_, v)| *v)
}

/// Deutsche Monatsnamen für Themen wie „Mobilfunk September“.
pub fn month_name_de(month: u32) -> &'static str {
    [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ][(month as usize).clamp(1, 12) - 1]
}

/// Datum aus einem PDF-Datumsfeld wie `D:20261002143000+02'00'` (nur der Tag, Zeitzone wie angegeben).
pub fn parse_pdf_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    let s = s.strip_prefix("D:").unwrap_or(s);
    let digits = s.get(..8)?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    NaiveDate::from_ymd_opt(
        digits[..4].parse().ok()?,
        digits[4..6].parse().ok()?,
        digits[6..8].parse().ok()?,
    )
}

static NUMERIC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{1,2})\.\s?(\d{1,2})\.\s?(\d{4}|\d{2})\b").unwrap());
static ISO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{4})-(\d{2})-(\d{2})\b").unwrap());
static SLASH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{1,2})/(\d{1,2})/(\d{4})\b").unwrap());
static TEXTUAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d{1,2})\.?\s+([a-zäöü]{3,9}\.?)\s+(\d{4})\b").unwrap());
/// „Stuttgart, “ bzw. „Musterstadt, den “ vor dem Datum
static PLACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-ZÄÖÜ][\p{L}\. -]{1,40},\s*(den\s+)?$").unwrap());

/// Alle Daten im Dokument (nur die ersten beiden Seiten zählen für die Auswahl).
pub fn find_dates(layout: &Layout, reference: Option<NaiveDate>) -> Vec<DateCandidate> {
    let mut out = Vec::new();
    for line in &layout.lines {
        let mut found: Vec<(std::ops::Range<usize>, NaiveDate)> = Vec::new();
        for caps in NUMERIC.captures_iter(&line.text) {
            let year: i32 = caps[3].parse().unwrap_or(0);
            let year = if caps[3].len() == 2 { 2000 + year } else { year };
            if let Some(d) = NaiveDate::from_ymd_opt(year, caps[2].parse().unwrap_or(0), caps[1].parse().unwrap_or(0)) {
                found.push((caps.get(0).unwrap().range(), d));
            }
        }
        for caps in ISO.captures_iter(&line.text) {
            if let Some(d) = NaiveDate::from_ymd_opt(
                caps[1].parse().unwrap_or(0),
                caps[2].parse().unwrap_or(0),
                caps[3].parse().unwrap_or(0),
            ) {
                found.push((caps.get(0).unwrap().range(), d));
            }
        }
        for caps in SLASH.captures_iter(&line.text) {
            if let Some(d) = NaiveDate::from_ymd_opt(
                caps[3].parse().unwrap_or(0),
                caps[2].parse().unwrap_or(0),
                caps[1].parse().unwrap_or(0),
            ) {
                found.push((caps.get(0).unwrap().range(), d));
            }
        }
        for caps in TEXTUAL.captures_iter(&line.text) {
            if let Some(m) = month_number(&caps[2])
                && let Some(d) = NaiveDate::from_ymd_opt(caps[3].parse().unwrap_or(0), m, caps[1].parse().unwrap_or(0))
            {
                found.push((caps.get(0).unwrap().range(), d));
            }
        }
        found.sort_by_key(|(r, _)| r.start);

        let lower = line.text.to_lowercase();
        let in_range =
            found.len() >= 2 && (lower.contains(" bis ") || line.text.contains('–') || lower.contains(" - "));
        let (width, height) = layout.page_size(line.page);
        for (i, (range, date)) in found.iter().enumerate() {
            let before = lower[..range.start].trim_end();
            let kind = classify(&line.text[..range.start], before, &lower, i, in_range);
            let rect = line.rect_of(range.clone());
            let mut score = match kind {
                DateKind::Document => 60,
                DateKind::Unknown => 25,
                _ => 5,
            };
            if line.page == 1 && rect.y < height * 0.45 {
                score += 15;
            }
            if rect.x > width * 0.5 {
                score += 10;
            }
            if line.page > 1 {
                score -= 40;
            }
            if let Some(r) = reference {
                if *date > r + chrono::Days::new(31) {
                    score -= 30;
                }
                if date.year() < r.year() - 2 {
                    score -= 20;
                }
            }
            out.push(DateCandidate {
                date: *date,
                kind,
                page: line.page,
                rect,
                score,
            });
        }
    }
    out
}

fn classify(original_before: &str, before: &str, line: &str, index: usize, in_range: bool) -> DateKind {
    let ends = |words: &[&str]| words.iter().any(|w| before.ends_with(w));
    if ends(&[
        "datum:",
        "datum",
        "rechnungsdatum:",
        "rechnungsdatum",
        "vom",
        "bescheid vom",
        "stand:",
    ]) && !before.contains("geburt")
        && !before.contains("zeitraum")
    {
        return DateKind::Document;
    }
    if PLACE.is_match(original_before.trim_start()) {
        return DateKind::Document;
    }
    if in_range || line.contains("zeitraum") {
        return if index == 0 {
            DateKind::PeriodStart
        } else {
            DateKind::PeriodEnd
        };
    }
    let due = [
        "fällig",
        "zahlbar",
        "bis zum",
        "bis spätestens",
        "abgebucht",
        "abbuchung",
        "frühestens am",
        "zahlungsziel",
    ];
    if due.iter().any(|w| line.contains(w)) {
        return DateKind::Due;
    }
    if ["bekannt gegeben", "geburt", "beginn", "gültig"]
        .iter()
        .any(|w| line.contains(w))
    {
        return DateKind::Other;
    }
    // Datum am Zeilenanfang mit Text dahinter: Tabellenzeile (Leistungsdatum)
    if before.is_empty() && line.len() > 14 {
        return DateKind::Service;
    }
    DateKind::Unknown
}

/// Bestes Dokumentdatum und Sicherheit 0–100.
pub fn best_document_date(candidates: &[DateCandidate]) -> Option<(&DateCandidate, u8)> {
    let mut sorted: Vec<&DateCandidate> = candidates.iter().collect();
    sorted.sort_by_key(|c| std::cmp::Reverse(c.score));
    let best = *sorted.first()?;
    // Abstand zum zweitbesten anderen Datum
    let runner_up = sorted.iter().find(|c| c.date != best.date).map_or(0, |c| c.score);
    let margin = best.score - runner_up;
    let base = match best.kind {
        DateKind::Document => 85,
        DateKind::Unknown => 55,
        _ => 30,
    };
    let confidence = (base + margin / 4).clamp(5, 98) as u8;
    Some((best, confidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PageText, Word};

    #[test]
    fn pdf_dates() {
        let d = |y, m, t| NaiveDate::from_ymd_opt(y, m, t);
        assert_eq!(parse_pdf_date("D:20261002143000+02'00'"), d(2026, 10, 2));
        assert_eq!(parse_pdf_date("D:20261002"), d(2026, 10, 2));
        assert_eq!(parse_pdf_date("20250131120000Z"), d(2025, 1, 31));
        assert_eq!(parse_pdf_date("D:2026"), None);
        assert_eq!(parse_pdf_date("D:20261302"), None);
        assert_eq!(parse_pdf_date(""), None);
    }

    fn layout(lines: &[(&str, f32, f32)]) -> Layout {
        let mut words = Vec::new();
        for (text, x, y) in lines {
            let mut cx = *x;
            for w in text.split(' ') {
                words.push(Word {
                    text: w.into(),
                    bbox: Rect {
                        x: cx,
                        y: *y,
                        w: w.chars().count() as f32 * 5.0,
                        h: 10.0,
                    },
                    font_size: 10.0,
                    bold: false,
                });
                cx += w.chars().count() as f32 * 5.0 + 3.0;
            }
        }
        Layout::new(&[PageText {
            page: 1,
            width: 595.0,
            height: 842.0,
            words,
        }])
    }

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn formats() {
        let l = layout(&[
            ("a 28.09.2026", 60.0, 300.0),
            ("b 1.2.26", 60.0, 320.0),
            ("c 2026-03-04", 60.0, 340.0),
            ("d 15. September 2026", 60.0, 360.0),
            ("e 3 Okt. 2026", 60.0, 380.0),
            ("f 31.02.2026", 60.0, 400.0),
        ]);
        let dates: Vec<NaiveDate> = find_dates(&l, None).iter().map(|c| c.date).collect();
        assert_eq!(
            dates,
            [
                date("2026-09-28"),
                date("2026-02-01"),
                date("2026-03-04"),
                date("2026-09-15"),
                date("2026-10-03")
            ]
        );
    }

    #[test]
    fn picks_document_date_over_due_and_period() {
        let l = layout(&[
            ("Stuttgart, 15. September 2026", 380.0, 250.0),
            ("im Zeitraum 01.01.2025 bis 31.12.2025", 60.0, 340.0),
            ("Bitte zahlen Sie bis zum 30.10.2026.", 60.0, 400.0),
        ]);
        let c = find_dates(&l, Some(date("2026-10-13")));
        let kinds: Vec<DateKind> = c.iter().map(|c| c.kind).collect();
        assert_eq!(
            kinds,
            [
                DateKind::Document,
                DateKind::PeriodStart,
                DateKind::PeriodEnd,
                DateKind::Due
            ]
        );
        let (best, conf) = best_document_date(&c).unwrap();
        assert_eq!(best.date, date("2026-09-15"));
        assert!(conf >= 90, "{conf}");
    }
}
