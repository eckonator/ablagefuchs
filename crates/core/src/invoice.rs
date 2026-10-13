// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ist das Dokument eine Rechnung? Punktwertung über Schlüsselwörter (Bauplan §5.3).

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::layout::Layout;

/// Ab dieser Punktzahl gilt ein Dokument als Rechnung.
const THRESHOLD: i32 = 40;

#[derive(Debug, Clone, Serialize)]
pub struct InvoiceResult {
    pub is_invoice: bool,
    pub confidence: u8,
    /// gefundene Merkmale, für die Herkunftsanzeige („„Rechnungsnummer“ gefunden“)
    pub signals: Vec<String>,
    pub score: i32,
}

struct Signal {
    re: Regex,
    label: &'static str,
    points: i32,
}

static SIGNALS: LazyLock<Vec<Signal>> = LazyLock::new(|| {
    let s = |re: &str, label, points| Signal {
        re: Regex::new(&format!("(?i){re}")).unwrap(),
        label,
        points,
    };
    vec![
        s(r"rechnungs-?\s?(nummer|nr)", "Rechnungsnummer", 35),
        s(r"\brechnung\b", "Rechnung", 25),
        s(r"rechnungsbetrag", "Rechnungsbetrag", 25),
        s(r"\bliquidation\b", "Liquidation", 25),
        s(r"\b(ust|mwst|umsatzsteuer|mehrwertsteuer)\b", "USt", 15),
        s(r"zahlbar|zahlungsziel", "zahlbar", 10),
        s(r"gesamtbetrag|endbetrag|zu zahlen", "Gesamtbetrag", 10),
        s(r"nachzahlung", "Nachzahlung", 20),
        s(r"überweisen", "überweisen", 10),
        s(r"abgebucht|buchen .* ab\b|lastschrift", "Abbuchung", 5),
        // Gegenanzeigen
        s(r"\bbescheid\b", "Bescheid", -40),
        s(r"bescheinigung", "Bescheinigung", -40),
        s(r"\bangebot\b", "Angebot", -30),
        s(r"gutschrift", "Gutschrift", -30),
        s(r"kontoauszug", "Kontoauszug", -40),
    ]
});

/// Bewertet die ersten beiden Seiten.
pub fn detect(layout: &Layout) -> InvoiceResult {
    let text: String = layout
        .lines
        .iter()
        .filter(|l| l.page <= 2)
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut score = 0;
    let mut signals = Vec::new();
    for s in SIGNALS.iter() {
        if s.re.is_match(&text) {
            score += s.points;
            signals.push(s.label.to_string());
        }
    }
    let is_invoice = score >= THRESHOLD;
    // Je weiter von der Schwelle entfernt, desto sicherer; ganz ohne Merkmale ziemlich sicher „keine Rechnung“
    let distance = (score - THRESHOLD).abs();
    let confidence = if signals.is_empty() {
        90
    } else {
        (55 + distance).min(99)
    } as u8;
    InvoiceResult {
        is_invoice,
        confidence,
        signals,
        score,
    }
}
