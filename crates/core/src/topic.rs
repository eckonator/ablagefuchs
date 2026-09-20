// SPDX-License-Identifier: MIT OR Apache-2.0
//! Thema aus der Betreffzeile ableiten, ersatzweise aus der ersten Rechnungsposition.

use std::sync::LazyLock;

use chrono::{Datelike, NaiveDate};
use regex::Regex;
use serde::Serialize;

use crate::dates::{DateCandidate, DateKind, month_name_de};
use crate::layout::{Layout, Zone};
use crate::model::Rect;

/// Höchstens so viele Wörter im Thema.
const MAX_WORDS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TopicSource {
    SubjectLine,
    LineItem,
}

#[derive(Debug, Clone)]
pub struct TopicResult {
    pub text: String,
    pub confidence: u8,
    pub source: TopicSource,
    pub page: u16,
    pub rect: Rect,
}

static BETREFF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(betreff|betr\.|subject)\s*:\s*").unwrap());
/// „Bescheid für 2025 über Einkommensteuer …“ → „Einkommensteuerbescheid 2025“
static BESCHEID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^bescheid\s+(?:für\s+(?:das\s+jahr\s+)?(\d{4})\s+)?über\s+(?:die\s+|den\s+)?(\p{L}+)").unwrap()
});
/// „Beitragsbescheinigung für das Jahr 2025“ → „Beitragsbescheinigung 2025“
static FOR_YEAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(.+?)\s+(?:für\s+das\s+(?:jahr|kalenderjahr|wirtschaftsjahr)|für|zum\s+jahr)\s+(\d{4})\b.*$")
        .unwrap()
});
static POSSESSIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(ihre|ihr|ihren|unsere|unser)\s+").unwrap());
static INVOICE_FOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(rechnung|abrechnung)\s+(für|über|zu)\s+").unwrap());
static YEAR_RANGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+\d{4}\s*/\s*\d{2,4}$").unwrap());
static SALUTATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(sehr geehrte|guten tag|hallo|liebe|dear)").unwrap());
static AMOUNT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-?[\d.]+,\d{2}\s?€?$|€$").unwrap());
static LEADING_DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{1,2}\.\d{1,2}\.\d{2,4}\s+").unwrap());
static TRAILING_PARENS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\([^)]*\)$").unwrap());

/// Wörter, die nur „Rechnung“ sagen und kein Thema sind.
const INVOICE_ONLY: [&str; 12] = [
    "ihre",
    "ihr",
    "rechnung",
    "liquidation",
    "honorarrechnung",
    "nach",
    "gemäß",
    "goz",
    "goä",
    "nr",
    "nr.",
    "nummer",
];
/// Tabellenzeilen, die keine Leistung beschreiben.
const NOT_ITEMS: [&str; 12] = [
    "gesamt",
    "summe",
    "betrag",
    "ust",
    "mwst",
    "netto",
    "brutto",
    "position",
    "abzgl",
    "zwischensumme",
    "leistung",
    "datum",
];

pub fn detect(layout: &Layout, is_invoice: bool, dates: &[DateCandidate]) -> Option<TopicResult> {
    let mut result = from_subject(layout).or_else(|| from_line_item(layout))?;
    if is_invoice && let Some(month) = single_month_period(dates) {
        let has_month = (1..=12).any(|m| result.text.contains(month_name_de(m)));
        if !has_month {
            result.text = format!("{} {}", result.text, month_name_de(month));
        }
    }
    Some(result)
}

fn from_subject(layout: &Layout) -> Option<TopicResult> {
    let body = layout.body_font_size();
    let (_, height) = layout.page_size(1);
    let salutation_y = layout
        .lines
        .iter()
        .find(|l| l.page == 1 && SALUTATION.is_match(&l.text))
        .map_or(f32::MAX, |l| l.bbox.y);
    let line = layout.lines.iter().find(|l| {
        l.page == 1
            && l.zone == Zone::Body
            && l.bbox.y > height * 0.25
            && l.bbox.y < salutation_y
            && ((l.bold && l.font_size >= body * 0.95) || l.font_size >= body * 1.1 || BETREFF.is_match(&l.text))
    })?;
    let (text, transformed) = normalize(&line.text);
    let text = text?;
    Some(TopicResult {
        text,
        confidence: if transformed { 85 } else { 80 },
        source: TopicSource::SubjectLine,
        page: line.page,
        rect: line.bbox,
    })
}

/// Betreff zu einem kurzen Thema. Gibt `None` zurück, wenn nur „Rechnung“ o. Ä. übrig bleibt.
pub fn normalize(subject: &str) -> (Option<String>, bool) {
    let mut s = BETREFF.replace(subject.trim(), "").trim().to_string();
    let original = s.clone();
    if let Some(c) = BESCHEID.captures(&s) {
        let year = c.get(1).map(|y| format!(" {}", y.as_str())).unwrap_or_default();
        return (Some(format!("{}bescheid{year}", &c[2])), true);
    }
    if let Some(c) = FOR_YEAR.captures(&s) {
        s = format!("{} {}", &c[1], &c[2]);
    }
    loop {
        let next = POSSESSIVE.replace(&s, "").to_string();
        let next = INVOICE_FOR.replace(&next, "").to_string();
        if next == s {
            break;
        }
        s = next;
    }
    s = YEAR_RANGE.replace(&s, "").trim().to_string();
    let meaningful = s
        .split_whitespace()
        .filter(|w| !INVOICE_ONLY.contains(&w.to_lowercase().as_str()) && !w.chars().all(|c| c.is_ascii_digit()))
        .count();
    if meaningful == 0 {
        return (None, false);
    }
    let words: Vec<&str> = s.split_whitespace().take(MAX_WORDS).collect();
    let s = words.join(" ");
    let transformed = s != original;
    (Some(s), transformed)
}

fn from_line_item(layout: &Layout) -> Option<TopicResult> {
    for (i, line) in layout.lines.iter().enumerate() {
        if line.page != 1 || line.zone != Zone::Body || AMOUNT.is_match(&line.text) {
            continue;
        }
        let row = layout.row(i);
        // Bezeichnung links, Betrag ganz rechts in derselben Zeile
        let has_amount = row.len() >= 2 && row[0] == i && AMOUNT.is_match(&layout.lines[*row.last().unwrap()].text);
        if !has_amount {
            continue;
        }
        let label = LEADING_DATE.replace(&line.text, "");
        let label = TRAILING_PARENS.replace(&label, "").trim().to_string();
        let lower = label.to_lowercase();
        if label.is_empty() || NOT_ITEMS.iter().any(|w| lower.contains(w)) {
            continue;
        }
        let text = label.split_whitespace().take(MAX_WORDS).collect::<Vec<_>>().join(" ");
        return Some(TopicResult {
            text,
            confidence: 65,
            source: TopicSource::LineItem,
            page: line.page,
            rect: line.bbox,
        });
    }
    None
}

/// Umfasst ein Zeitraum genau einen Kalendermonat (01.09. bis 30.09.), dessen Nummer.
fn single_month_period(dates: &[DateCandidate]) -> Option<u32> {
    let start = dates.iter().find(|d| d.kind == DateKind::PeriodStart)?;
    let end = dates.iter().find(|d| {
        d.kind == DateKind::PeriodEnd && d.page == start.page && (d.rect.center_y() - start.rect.center_y()).abs() < 3.0
    })?;
    let (s, e) = (start.date, end.date);
    let last_day = NaiveDate::from_ymd_opt(s.year(), s.month(), 1)?
        .checked_add_months(chrono::Months::new(1))?
        .pred_opt()?;
    (s.day() == 1 && e == last_day).then_some(s.month())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> Option<String> {
        normalize(s).0
    }

    #[test]
    fn subjects() {
        assert_eq!(
            n("Jahresabrechnung Strom 2025/2026").as_deref(),
            Some("Jahresabrechnung Strom")
        );
        assert_eq!(
            n("Beitragsbescheinigung für das Jahr 2025").as_deref(),
            Some("Beitragsbescheinigung 2025")
        );
        assert_eq!(n("Ihre Rechnung für Mobilfunk").as_deref(), Some("Mobilfunk"));
        assert_eq!(
            n("Bescheid für 2025 über Einkommensteuer und Solidaritätszuschlag").as_deref(),
            Some("Einkommensteuerbescheid 2025")
        );
        assert_eq!(
            n("Betreff: Kündigung Ihres Vertrags").as_deref(),
            Some("Kündigung Ihres Vertrags")
        );
        assert_eq!(n("Liquidation nach GOZ"), None);
        assert_eq!(n("Ihre Rechnung Nr. 4711"), None);
    }
}
