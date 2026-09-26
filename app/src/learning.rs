// SPDX-License-Identifier: MIT OR Apache-2.0
//! Lernen aus einem Archivordner im Hintergrund (rekursiv, mit Fortschritt und Abbrechen).
//! Alles bleibt lokal: Dateinamen und erste Seiten werden auf diesem Rechner gelesen, das Ergebnis
//! landet nur in `wissen.json` im Einstellungsordner.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use af_core::knowledge::Knowledge;
use af_core::naming::Template;
use af_ocr::archive::LearnStats;
use eframe::egui;

pub struct LearnJob {
    pub cancel: Arc<AtomicBool>,
    /// (erledigt, gesamt, aktueller Dateiname)
    pub progress: Arc<Mutex<(usize, usize, String)>>,
    pub result: Receiver<(Knowledge, LearnStats)>,
}

impl LearnJob {
    pub fn spawn(
        ctx: egui::Context,
        root: PathBuf,
        template: Template,
        mut knowledge: Knowledge,
        ocr: Option<af_ocr::Preference>,
    ) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new((0, 0, String::new())));
        let (tx, result) = mpsc::channel();
        let (c, p) = (Arc::clone(&cancel), Arc::clone(&progress));
        std::thread::Builder::new()
            .name("af-lernen".into())
            .spawn(move || {
                log::info!("Lerne aus {}", root.display());
                let stats = af_ocr::archive::learn_folder(&root, &template, &mut knowledge, ocr, |pr| {
                    if let Ok(mut g) = p.lock() {
                        let name = pr
                            .current
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        *g = (pr.done, pr.total, name);
                    }
                    ctx.request_repaint();
                    !c.load(Ordering::Relaxed)
                });
                log::info!("Lernen fertig: {stats:?}");
                let _ = tx.send((knowledge, stats));
                ctx.request_repaint();
            })
            .expect("Lern-Thread startet");
        LearnJob {
            cancel,
            progress,
            result,
        }
    }
}
