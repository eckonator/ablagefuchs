// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hintergrund-Threads für alles, was dauert. Zwei getrennte Threads, damit das Rendern der aktuellen
//! Seite nie hinter einer langen Texterkennung warten muss:
//! - `af-analyse`: Text lesen (Textebene oder OCR, mit Cache) und erkennen
//! - `af-render`: Seiten für die Vorschau rendern

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use af_core::{Context, PageText, Suggestion, TextSource};
use chrono::NaiveDate;
use eframe::egui;

/// Für die Erkennung reichen die ersten beiden Seiten.
const ANALYZE_PAGES: usize = 2;

pub enum Job {
    Analyze {
        id: u64,
        path: PathBuf,
        ocr: af_ocr::Preference,
        knowledge: std::sync::Arc<af_core::knowledge::Knowledge>,
    },
    Render {
        id: u64,
        path: PathBuf,
        page: usize,
        width: u32,
    },
}

pub struct Analysis {
    pub pages: usize,
    pub suggestion: Suggestion,
    /// Text mit Positionen der ersten Seiten (für die Textauswahl in der Vorschau)
    pub text: Vec<PageText>,
    pub source: TextSource,
}

impl Analysis {
    /// Ganz ohne Text (Scan, aber keine Texterkennung verfügbar oder nichts erkannt)
    pub fn no_text(&self) -> bool {
        self.text.iter().all(|p| p.words.is_empty())
    }
}

pub enum Done {
    Analyzed {
        id: u64,
        result: Result<Box<Analysis>, String>,
    },
    Rendered {
        id: u64,
        page: usize,
        width: u32,
        result: Result<af_pdf::RenderedPage, String>,
    },
}

pub struct Worker {
    analyze: Sender<Job>,
    render: Sender<Job>,
    rx: Receiver<Done>,
}

impl Worker {
    pub fn spawn(ctx: egui::Context) -> Self {
        let (done, rx) = mpsc::channel::<Done>();
        let analyze = spawn_thread("af-analyse", ctx.clone(), done.clone());
        let render = spawn_thread("af-render", ctx, done);
        Worker { analyze, render, rx }
    }

    pub fn send(&self, job: Job) {
        let _ = match job {
            Job::Analyze { .. } => self.analyze.send(job),
            Job::Render { .. } => self.render.send(job),
        };
    }

    pub fn try_recv(&self) -> Option<Done> {
        self.rx.try_recv().ok()
    }
}

fn spawn_thread(name: &str, ctx: egui::Context, done: Sender<Done>) -> Sender<Job> {
    let (tx, jobs) = mpsc::channel::<Job>();
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            while let Ok(job) = jobs.recv() {
                let result = match job {
                    Job::Analyze {
                        id,
                        path,
                        ocr,
                        knowledge,
                    } => Done::Analyzed {
                        id,
                        result: analyze(&path, ocr, &knowledge).map(Box::new),
                    },
                    Job::Render { id, path, page, width } => Done::Rendered {
                        id,
                        page,
                        width,
                        result: af_pdf::render_page(&path, page, width).map_err(|e| format!("{e:#}")),
                    },
                };
                if done.send(result).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        })
        .expect("Hintergrund-Thread startet");
    tx
}

fn analyze(
    path: &std::path::Path,
    ocr: af_ocr::Preference,
    knowledge: &af_core::knowledge::Knowledge,
) -> Result<Analysis, String> {
    let pages = af_pdf::page_count(path).map_err(|e| format!("{e:#}"))?;
    let key = crate::cache::key(path);
    let (text, source) = match key.as_deref().and_then(crate::cache::load) {
        Some(hit) => hit,
        None => {
            let (text, source) = af_ocr::read_text(path, ANALYZE_PAGES, ocr).map_err(|e| format!("{e:#}"))?;
            // Nur Texterkennung lohnt den Cache; die Textebene ist schnell gelesen
            if let (Some(k), TextSource::Ocr { .. }) = (&key, &source) {
                crate::cache::store(k, &text, &source);
            }
            (text, source)
        }
    };
    let ocr_quality = match &source {
        TextSource::Ocr { mean_confidence, .. } => Some(*mean_confidence),
        TextSource::Embedded => None,
    };
    let ctx = Context {
        reference_date: modified_date(path),
        ocr_quality,
        knowledge: Some(knowledge),
    };
    let suggestion = af_core::analyze(&text, &ctx);
    Ok(Analysis {
        pages,
        suggestion,
        text,
        source,
    })
}

/// Änderungsdatum der Datei (meist der Scantag) als Bezugsdatum für die Plausibilität.
fn modified_date(p: &std::path::Path) -> Option<NaiveDate> {
    let secs = std::fs::metadata(p)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(chrono::DateTime::from_timestamp(i64::try_from(secs).ok()?, 0)?.date_naive())
}
