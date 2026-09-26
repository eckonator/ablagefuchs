// SPDX-License-Identifier: MIT OR Apache-2.0
//! Führt die Einzelerkennungen zu einem Vorschlag zusammen: je Feld Wert, Sicherheit, Herkunft
//! und Fundstellen für die Markierungen in der Vorschau.

use chrono::NaiveDate;
use serde::Serialize;

use crate::dates::{self, DateCandidate, DateKind};
use crate::knowledge::{FoundId, IdKind, Knowledge, Match, find_ids};
use crate::layout::Layout;
use crate::model::{PageText, Rect};
use crate::naming::NameFields;
use crate::topic::TopicSource;
use crate::{company, invoice, topic};

/// Rahmenbedingungen der Analyse.
#[derive(Debug, Clone, Default)]
pub struct Context<'a> {
    /// Bezugsdatum, meist das Scan- bzw. Änderungsdatum der Datei. Daten weit danach sind unplausibel.
    pub reference_date: Option<NaiveDate>,
    /// Mittlere OCR-Sicherheit 0–1, wenn der Text aus der Texterkennung stammt (senkt alle Sicherheiten)
    pub ocr_quality: Option<f32>,
    /// Gelerntes Wissen (Schreibweisen, Kennungen, Themen), falls vorhanden
    pub knowledge: Option<&'a Knowledge>,
}

/// Woher ein Vorschlag stammt. Die Oberfläche übersetzt das in einen Satz.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "art", content = "daten")]
pub enum Source {
    Nothing,
    Date(DateKind),
    Letterhead {
        sender_line: bool,
    },
    Topic(TopicSource),
    InvoiceSignals(Vec<String>),
    /// aus dem gelernten Wissen: über eine Kennung (USt-IdNr., IBAN …) oder, ohne Kennung, über den Namen
    Knowledge {
        kennung: Option<IdKind>,
        dokumente: u32,
    },
    /// kein Betreff gefunden: häufigstes Thema dieses Absenders
    KnownTopic {
        anzahl: u32,
    },
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
    /// bekannte Themen des erkannten Unternehmens, häufigstes zuerst (Chips im Formular)
    pub known_topics: Vec<String>,
    /// Angaben zum Lernen, wenn der Nutzer die Datei umbenennt
    #[serde(skip)]
    pub learn: LearnInfo,
}

/// Was beim Umbenennen ins Wissen übernommen wird.
#[derive(Debug, Clone, Default)]
pub struct LearnInfo {
    pub ids: Vec<FoundId>,
    /// Name aus dem Briefkopf (ohne Wissen), wird zum Alias
    pub letterhead: Option<String>,
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

pub fn analyze(pages: &[PageText], ctx: &Context<'_>) -> Suggestion {
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

    let letterhead = company::detect(&layout);
    let ids = find_ids(&layout);
    let known = ctx
        .knowledge
        .and_then(|k| k.match_document(&layout, &ids).map(|m| (k, m)));
    let mut unternehmen = match letterhead.clone() {
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

    // Gelerntes Wissen: Schreibweise des Nutzers und höhere Sicherheit
    let mut company_info: Option<&crate::knowledge::Company> = None;
    if let Some((k, m)) = &known {
        let c = &k.unternehmen[m.company()];
        company_info = Some(c);
        let (confidence, kennung, page, rect) = match m {
            Match::ById {
                kind,
                agreeing,
                page,
                rect,
                ..
            } => (97 + (*agreeing as u8 - 1).min(2), Some(*kind), *page, *rect),
            Match::ByName { page, rect, .. } => (90, None, *page, *rect),
        };
        let mut highlights = unternehmen.highlights.clone();
        if !highlights.iter().any(|h| h.page == page && h.rect == rect) {
            highlights.push(Highlight { page, rect });
        }
        unternehmen = Field {
            value: c.name.clone(),
            confidence,
            source: Source::Knowledge {
                kennung,
                dokumente: c.anzahl,
            },
            highlights,
        };
    }

    let mut thema = match topic::detect(&layout, inv.is_invoice, &dates) {
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

    let mut rechnung = rechnung;
    if let Some(c) = company_info {
        let known_topics = c.topics();
        // „Mobilfunk September“ entspricht „Mobilfunk Juli“, „Beitragsbescheinigung 2025“ der von 2024
        let same = |a: &str, b: &str| topic_pattern(a) == topic_pattern(b);
        if thema.value.is_empty() {
            if let Some(t) = known_topics.first() {
                thema = Field {
                    value: (*t).to_string(),
                    confidence: 70,
                    source: Source::KnownTopic {
                        anzahl: c.themen.get(*t).copied().unwrap_or(0),
                    },
                    highlights: Vec::new(),
                };
            }
        } else if known_topics.iter().any(|t| same(t, &thema.value)) {
            // Diesen Betreff hat der Nutzer bei diesem Absender schon so benannt
            thema.confidence = thema.confidence.max(95);
        }
        // Rechnung: passt die Erkennung zur Geschichte des Absenders?
        if c.anzahl >= 3 {
            let share = c.rechnungen as f32 / c.anzahl as f32;
            let agrees = (share >= 0.9 && rechnung.value) || (share <= 0.1 && !rechnung.value);
            let disagrees = (share >= 0.9 && !rechnung.value) || (share <= 0.1 && rechnung.value);
            if agrees {
                rechnung.confidence = rechnung.confidence.max(95);
            } else if disagrees {
                rechnung.confidence = rechnung.confidence.min(70);
            }
        }
    }
    let known_topics = company_info
        .map(|c| c.topics().into_iter().map(String::from).collect())
        .unwrap_or_default();
    // Nur ein deutlich gestalteter Briefkopf taugt als Alias (sonst lernt man z. B. „Herrn“)
    let learn = LearnInfo {
        ids,
        letterhead: letterhead.filter(|c| c.confidence >= 60).map(|c| c.name),
    };

    let mut s = Suggestion {
        datum,
        unternehmen,
        thema,
        rechnung,
        dates,
        known_topics,
        learn,
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

/// Thema ohne Jahreszahlen und Monatsnamen, zum Vergleich mit bekannten Themen.
fn topic_pattern(topic: &str) -> String {
    topic
        .split_whitespace()
        .filter(|w| !w.chars().all(|c| c.is_ascii_digit() || c == '/' || c == '-'))
        .filter(|w| !(1..=12).any(|m| dates::month_name_de(m).eq_ignore_ascii_case(w)))
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}
