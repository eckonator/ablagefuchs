// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hintergrund-Thread für alles, was dauert: Text lesen, erkennen, Seiten rendern.
//! Die Oberfläche schickt Aufträge und holt Ergebnisse pro Bild ab, ohne je zu warten.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use af_core::{Context, PageText, Suggestion};
use chrono::NaiveDate;
use eframe::egui;

/// Für die Erkennung reichen die ersten beiden Seiten.
const ANALYZE_PAGES: usize = 2;

pub enum Job {
    Analyze {
        id: u64,
        path: PathBuf,
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
    /// Text mit Positionen der ersten Seiten (für Markierungen in M3)
    #[allow(dead_code)]
    pub text: Vec<PageText>,
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
    tx: Sender<Job>,
    rx: Receiver<Done>,
}

impl Worker {
    pub fn spawn(ctx: egui::Context) -> Self {
        let (tx, jobs) = mpsc::channel::<Job>();
        let (done, rx) = mpsc::channel::<Done>();
        std::thread::Builder::new()
            .name("af-worker".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let result = match job {
                        Job::Analyze { id, path } => Done::Analyzed {
                            id,
                            result: analyze(&path).map(Box::new),
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
        Worker { tx, rx }
    }

    pub fn send(&self, job: Job) {
        let _ = self.tx.send(job);
    }

    pub fn try_recv(&self) -> Option<Done> {
        self.rx.try_recv().ok()
    }
}

fn analyze(path: &std::path::Path) -> Result<Analysis, String> {
    let pages = af_pdf::page_count(path).map_err(|e| format!("{e:#}"))?;
    let text = af_pdf::extract_text(path, ANALYZE_PAGES).map_err(|e| format!("{e:#}"))?;
    let ctx = Context {
        reference_date: modified_date(path),
        ocr_quality: None,
    };
    let suggestion = af_core::analyze(&text, &ctx);
    Ok(Analysis {
        pages,
        suggestion,
        text,
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
