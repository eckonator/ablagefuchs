// SPDX-License-Identifier: MIT OR Apache-2.0
//! Führt die Einzelerkennungen zu einem Vorschlag zusammen: je Feld Wert, Sicherheit, Herkunft
//! und Fundstellen für die Markierungen in der Vorschau.

use chrono::NaiveDate;
use serde::Serialize;

use crate::dates::{self, DateCandidate, DateKind};
use crate::layout::Layout;
use crate::model::{PageText, Rect};
use crate::naming::NameFields;
use crate::topic::TopicSource;
use crate::{company, invoice, topic};

/// Rahmenbedingungen der Analyse.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Bezugsdatum, meist das Scan- bzw. Änderungsdatum der Datei. Daten weit danach sind unplausibel.
    pub reference_date: Option<NaiveDate>,
    /// Mittlere OCR-Sicherheit 0–1, wenn der Text aus der Texterkennung stammt (senkt alle Sicherheiten)
    pub ocr_quality: Option<f32>,
}

/// Woher ein Vorschlag stammt. Die Oberfläche übersetzt das in einen Satz.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "art", content = "daten")]
pub enum Source {
    Nothing,
    Date(DateKind),
    Letterhead { sender_line: bool },
    Topic(TopicSource),
    InvoiceSignals(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Highlight {
    pub page: u16,
    pub rect: Rect,
}

#[derive(Debug, Clone, Serialize)]
pub struct Field<T> {
    pub value: T,
    /// 0–100
    pub confidence: u8,
    pub source: Source,
    pub highlights: Vec<Highlight>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub datum: Field<Option<NaiveDate>>,
    pub unternehmen: Field<String>,
    pub thema: Field<String>,
    pub rechnung: Field<bool>,
    /// alle gefundenen Daten (Chips im Formular)
    pub dates: Vec<DateCandidate>,
}

impl Suggestion {
    pub fn fields(&self) -> NameFields {
        NameFields {
            datum: self.datum.value,
            unternehmen: self.unternehmen.value.clone(),
            thema: self.thema.value.clone(),
            rechnung: self.rechnung.value,
        }
    }

    /// Ohne Datum und Unternehmen gibt es keinen brauchbaren Namen: dann von Hand ausfüllen.
    pub fn is_complete(&self) -> bool {
        self.datum.value.is_some() && !self.unternehmen.value.trim().is_empty()
    }

    /// Sicherheit der ganzen Datei = schwächstes Feld (entscheidet über „Prüfen“ und den Stapelmodus).
    pub fn min_confidence(&self) -> u8 {
        [
            self.datum.confidence,
            self.unternehmen.confidence,
            self.thema.confidence,
            self.rechnung.confidence,
        ]
        .into_iter()
        .min()
        .unwrap_or(0)
    }
}

fn empty<T>(value: T) -> Field<T> {
    Field {
        value,
        confidence: 0,
        source: Source::Nothing,
        highlights: Vec::new(),
    }
}

pub fn analyze(pages: &[PageText], ctx: &Context) -> Suggestion {
    let layout = Layout::new(pages);
    let dates = dates::find_dates(&layout, ctx.reference_date);

    let datum = match dates::best_document_date(&dates) {
        Some((best, confidence)) => Field {
            value: Some(best.date),
            confidence,
            source: Source::Date(best.kind),
            highlights: vec![Highlight {
                page: best.page,
                rect: best.rect,
            }],
        },
        None => empty(None),
    };

    let inv = invoice::detect(&layout);
    let rechnung = Field {
        value: inv.is_invoice,
        confidence: if layout.lines.is_empty() { 0 } else { inv.confidence },
        source: Source::InvoiceSignals(inv.signals.clone()),
        highlights: Vec::new(),
    };

    let unternehmen = match company::detect(&layout) {
        Some(c) => Field {
            value: c.name,
            confidence: c.confidence,
            source: Source::Letterhead {
                sender_line: c.confirmed_by_sender_line,
            },
            highlights: vec![Highlight {
                page: c.page,
                rect: c.rect,
            }],
        },
        None => empty(String::new()),
    };

    let thema = match topic::detect(&layout, inv.is_invoice, &dates) {
        Some(t) => Field {
            value: t.text,
            confidence: t.confidence,
            source: Source::Topic(t.source),
            highlights: vec![Highlight {
                page: t.page,
                rect: t.rect,
            }],
        },
        None => empty(String::new()),
    };

    let mut s = Suggestion {
        datum,
        unternehmen,
        thema,
        rechnung,
        dates,
    };
    if let Some(q) = ctx.ocr_quality {
        let q = q.clamp(0.0, 1.0);
        for c in [
            &mut s.datum.confidence,
            &mut s.unternehmen.confidence,
            &mut s.thema.confidence,
            &mut s.rechnung.confidence,
        ] {
            *c = (f32::from(*c) * q).round() as u8;
        }
    }
    s
}
