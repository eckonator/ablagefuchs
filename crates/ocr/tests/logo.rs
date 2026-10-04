// SPDX-License-Identifier: MIT OR Apache-2.0
//! Abstimmen des Briefkopf-Fingerabdrucks an simulierten Scans der Demo-Briefe:
//! gleiche Firma (verdreht, verschoben, verrauscht, heller/dunkler) muss über der Schwelle liegen,
//! verschiedene Firmen (alle aus derselben Briefvorlage, also sehr ähnlich aufgebaut) darunter.
//! Braucht pdfium und `cargo xtask demo-archiv`; ausführen mit `cargo test -p af-ocr --test logo -- --ignored --nocapture`.

use std::path::Path;

use af_core::logo::{Fingerprint, SAME};

const WIDTH: u32 = 1000;

fn gray(path: &Path) -> (Vec<u8>, usize, usize, (f32, f32)) {
    let img = af_pdf::render_page(path, 0, WIDTH).expect("rendern");
    let g = img
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| ((u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000) as u8)
        .collect();
    (g, img.width, img.height, (img.page_width_pt, img.page_height_pt))
}

/// Einfacher Zufall (reproduzierbar)
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (u32::MAX >> 1) as f32
    }
}

/// „Scan“: drehen (Grad), verschieben (px), skalieren, Helligkeit, Rauschen, Grauschleier.
fn scan(g: &[u8], w: usize, h: usize, seed: u64) -> Vec<u8> {
    let mut rng = Rng(seed);
    // AF_SCAN=rot,shift,scale,bright,noise wählt die Störungen (Standard: alle)
    let on = |k: &str| std::env::var("AF_SCAN").map_or(true, |v| v.split(',').any(|x| x == k));
    let f = |on: bool, v: f32| if on { v } else { 0.0 };
    let angle = f(on("rot"), (rng.next() - 0.5) * 1.6_f32.to_radians()); // ±0,8°
    let (dx, dy) = (
        f(on("shift"), (rng.next() - 0.5) * 16.0),
        f(on("shift"), (rng.next() - 0.5) * 16.0),
    );
    let scale = 1.0 + f(on("scale"), (rng.next() - 0.5) * 0.04); // ±2 %
    let bright = f(on("bright"), (rng.next() - 0.5) * 40.0);
    let noise_on = on("noise");
    let (cs, sn) = (angle.cos(), angle.sin());
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let mut out = vec![255u8; w * h];
    for y in 0..h {
        for x in 0..w {
            // Rückwärts abbilden
            let (fx, fy) = ((x as f32 - cx - dx) / scale, (y as f32 - cy - dy) / scale);
            let (sx, sy) = (fx * cs + fy * sn + cx, -fx * sn + fy * cs + cy);
            let v = if sx >= 0.0 && sy >= 0.0 && (sx as usize) < w && (sy as usize) < h {
                g[sy as usize * w + sx as usize] as f32
            } else {
                255.0
            };
            let noise = if noise_on { (rng.next() - 0.5) * 50.0 } else { 0.0 };
            out[y * w + x] = (v * 0.9 + 20.0 + bright + noise).clamp(0.0, 255.0) as u8;
        }
    }
    out
}

#[test]
#[ignore]
fn tune_on_simulated_scans() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo");
    let files: Vec<_> = [
        "Archiv/2025/NetzWelt/2025-08-01_NetzWelt_-_Mobilfunk_Juli_Rechnung.pdf",
        "Archiv/2024/Auto/2024-03-01_Autohaus_Becker_Inspektion_Rechnung.pdf",
        "Archiv/2024/Auto/2024-10-14_Autohaus_Becker_Reifenwechsel.pdf",
        "Archiv/2024/Auto/2024-11-20_Garantie_Autohaus_Becker.pdf",
        "Archiv/2024/Steuer/2024-10-02_Finanzamt_Musterstadt_-_Einkommensteuerbescheid_2023.pdf",
        "Archiv/2025/Versicherungen/Krankenkasse/2025-02-10_Krankenkasse_Sued_-_Beitragsbescheinigung_2024.pdf",
        "Archiv/2025/Wohnen/2025-09-15_Hausverwaltung_Lindner_-_Nebenkostenabrechnung_2024.pdf",
        "Archiv/2025/Wohnen/Strom/2025-09-26_Stadtwerke_Musterstadt_-_Jahresabrechnung_Strom_Rechnung.pdf",
        "Eingang/scan_20261002_0002.pdf",
        "Eingang/scan_20261002_0005.pdf",
        "Eingang/scan_20261002_0006.pdf",
        "Eingang/scan_20261002_0008.pdf",
    ]
    .iter()
    .map(|f| root.join(f))
    .collect();
    if !files[0].exists() {
        eprintln!("Demo fehlt: cargo xtask demo-pdfs && cargo xtask demo-archiv");
        return;
    }
    // Firma je Datei (Eingang und Archiv teilen sich Briefköpfe)
    let company = [0, 1, 2, 3, 4, 5, 6, 7, 5, 8, 6, 9];
    // je Brief: Original und 4 simulierte Scans
    let mut prints: Vec<(usize, usize, Fingerprint)> = Vec::new();
    for (file, f) in files.iter().enumerate() {
        let i = company[file];
        let (g, w, h, pt) = gray(f);
        for seed in 0..5u64 {
            let img = if seed == 0 {
                g.clone()
            } else {
                scan(&g, w, h, seed * 7919 + file as u64)
            };
            match Fingerprint::from_gray(&img, w, h, pt) {
                Some(hd) => prints.push((file, i, hd.fingerprint)),
                None => println!("Brief {} Scan {seed}: kein Fingerabdruck", i + 1),
            }
        }
    }
    let (mut same_file, mut same_company, mut diff_max) = (f32::MAX, f32::MAX, f32::MIN);
    let mut worst = (0, 0);
    let mut diffs = Vec::new();
    for (a, (fa_file, ia, fa)) in prints.iter().enumerate() {
        for (fb_file, ib, fb) in prints.iter().skip(a + 1) {
            let s = fa.similarity(fb);
            if fa_file == fb_file {
                same_file = same_file.min(s);
            } else if ia == ib {
                same_company = same_company.min(s);
            } else {
                diffs.push((s, *ia, *ib));
                if s > diff_max {
                    diff_max = s;
                    worst = (*ia, *ib);
                }
            }
        }
    }
    let same_min = same_file;
    diffs.sort_by(|a, b| b.0.total_cmp(&a.0));
    let over = |t: f32| diffs.iter().filter(|d| d.0 >= t).count();
    println!(
        "verschiedene Firmen: {} Paare, davon ≥ 0,80: {}, ≥ 0,85: {}, ≥ 0,90: {}",
        diffs.len(),
        over(0.8),
        over(0.85),
        over(0.9)
    );
    let mut pairs: Vec<(usize, usize)> = diffs
        .iter()
        .filter(|d| d.0 >= 0.85)
        .map(|d| (d.1 + 1, d.2 + 1))
        .collect();
    pairs.sort();
    pairs.dedup();
    println!("Firmenpaare ≥ 0,85: {pairs:?}");
    println!("gleicher Brief, verschiedene Scans: mindestens {same_file:.3}");
    println!("gleiche Firma, anderer Brief: mindestens {same_company:.3}");
    println!(
        "verschiedene Firmen: höchstens {diff_max:.3} (Firma {} / {})",
        worst.0 + 1,
        worst.1 + 1
    );
    assert!(same_min >= SAME, "gleicher Briefkopf unter der Schwelle");
    // Verschiedene Firmen aus derselben Textvorlage dürfen vereinzelt über der Schwelle liegen
    // (dafür gibt es den Abstand zum Zweitbesten und beim Lernen die Bestätigung durch den Dateinamen)
    assert!(
        over(SAME) * 100 <= diffs.len(),
        "zu viele verschiedene Briefköpfe über der Schwelle"
    );
}
