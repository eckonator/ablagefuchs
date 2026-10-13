// SPDX-License-Identifier: MIT OR Apache-2.0
//! Golden-Tests: Text der Demo-PDFs (von pdfium gelesen und als JSON gespeichert) → erwarteter Dateiname.
//! Neu erzeugen nach Änderungen an den Demo-PDFs:
//! `for i in 1 2 3 4 5 6 7 8; do ablagefuchs-cli text --json demo/Eingang/scan_20261002_000$i.pdf > crates/core/tests/fixtures/demo/scan_20261002_000$i.json; done`

use af_core::naming::{DEFAULT_TEMPLATE, NamingOptions, Template};
use af_core::{Context, PageText, Source, analyze};
use chrono::NaiveDate;

/// Dieselben Erwartungen wie `xtask/demo-expected.tsv` (dort für den Weg über pdfium in der CI).
const EXPECTED: &str = include_str!("../../../xtask/demo-expected.tsv");
/// Der Scan ohne Textebene; Fixture mit dem Ergebnis von Apple Vision.
const EXPECTED_OCR: &str = include_str!("../../../xtask/demo-expected-ocr.tsv");

#[test]
fn demo_documents_get_expected_names() {
    let template = Template::parse(DEFAULT_TEMPLATE).unwrap();
    let ctx = Context {
        reference_date: NaiveDate::from_ymd_opt(2026, 10, 13),
        ocr_quality: None,
        knowledge: None,
    };
    for line in EXPECTED.lines().chain(EXPECTED_OCR.lines()) {
        let (pdf, expected) = line.split_once('\t').unwrap();
        let fixture = format!(
            "{}/tests/fixtures/demo/{}",
            env!("CARGO_MANIFEST_DIR"),
            pdf.replace(".pdf", ".json")
        );
        let pages: Vec<PageText> = serde_json::from_str(&std::fs::read_to_string(&fixture).unwrap()).unwrap();
        let s = analyze(&pages, &ctx);
        if expected == "-" {
            assert!(!s.is_complete(), "{pdf}: sollte keinen Vorschlag haben");
            assert_eq!(s.min_confidence(), 0);
            continue;
        }
        assert_eq!(
            template.file_name(&s.fields(), &NamingOptions::default()),
            expected,
            "{pdf}"
        );
        // Markierungen zeigen auf die Fundstellen
        assert!(
            !s.datum.highlights.is_empty() && !s.unternehmen.highlights.is_empty(),
            "{pdf}"
        );
    }
}

#[test]
fn uncertain_cases_are_flagged() {
    let ctx = Context {
        reference_date: NaiveDate::from_ymd_opt(2026, 10, 13),
        ocr_quality: None,
        knowledge: None,
    };
    let load = |n: u8| -> Vec<PageText> {
        let f = format!(
            "{}/tests/fixtures/demo/scan_20261002_000{n}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap()
    };
    // Nebenkostenabrechnung: Nachzahlung, aber keine Rechnungsnummer → keine Rechnung, aber unsicher
    let s = analyze(&load(6), &ctx);
    assert!(!s.rechnung.value && s.rechnung.confidence < 90);
    // Arztrechnung ohne Betreff: Thema aus der Rechnungsposition, deshalb unsicherer
    let s = analyze(&load(5), &ctx);
    assert_eq!(s.thema.source, Source::Topic(af_core::topic::TopicSource::LineItem));
    assert!(s.thema.confidence < 80);
    // OCR-Qualität senkt die Sicherheit
    let ocr = Context {
        ocr_quality: Some(0.8),
        ..ctx.clone()
    };
    assert!(analyze(&load(1), &ocr).datum.confidence < analyze(&load(1), &ctx).datum.confidence);
}
