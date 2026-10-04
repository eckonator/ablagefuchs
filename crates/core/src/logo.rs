// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fingerabdruck des Briefkopfs: erkennt Absender, deren Name nur als Bild-Logo gedruckt ist.
//!
//! Aus dem oberen Teil der ersten Seite wird der bedruckte Bereich ausgeschnitten (so stören verrutschte
//! oder leicht anders skalierte Scans nicht) und auf ein grobes Raster verkleinert. Zwei Briefköpfe gelten
//! als gleich, wenn ihre Raster stark korrelieren und der bedruckte Bereich ähnliche Proportionen hat.
//! Gespeichert wird nur dieses Raster (576 Graustufen in 4 Bit), kein Bild des Briefs.

use serde::{Deserialize, Serialize};

use crate::model::Rect;

/// Raster: 48 × 12 Zellen
pub const COLS: usize = 48;
pub const ROWS: usize = 12;
/// Bis hierhin (Anteil der Seitenhöhe) wird nach dem Briefkopf gesucht
const SEARCH: f32 = 0.25;
/// Rand, der bei Scans oft dunkle Kanten hat
const BORDER: f32 = 0.03;
/// Eine leere Zone dieser Höhe (Anteil der Seitenhöhe) beendet den Briefkopf
const GAP: f32 = 0.018;
/// Geprüfte Schräglagen: ±1,5° in Schritten von 0,1°
const SKEW_STEPS: i32 = 15;
/// Ab dieser Ähnlichkeit gelten zwei Briefköpfe als gleich (abgestimmt an simulierten Scans:
/// gleicher Briefkopf verdreht/verschoben/verrauscht ≥ 0,91; verschiedene Firmen mit fast gleicher
/// Textvorlage erreichen das vereinzelt, deshalb zusätzlich [`MARGIN`])
pub const SAME: f32 = 0.90;
/// So weit muss das beste Unternehmen vor dem zweitbesten liegen
pub const MARGIN: f32 = 0.03;
/// Fast gleiche Fingerabdrücke werden nicht doppelt gespeichert
const DUPLICATE: f32 = 0.97;
/// Höchstens so viele Briefkopf-Varianten je Unternehmen (z. B. nach einem neuen Logo)
pub const MAX_PER_COMPANY: usize = 6;

/// Fügt `fp` zu `list` hinzu, außer es gibt schon einen fast gleichen; älteste Varianten fallen weg.
pub fn add_to(list: &mut Vec<Fingerprint>, fp: Fingerprint) -> bool {
    if list.iter().any(|f| f.similarity(&fp) >= DUPLICATE) {
        return false;
    }
    list.push(fp);
    if list.len() > MAX_PER_COMPANY {
        list.remove(0);
    }
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fingerprint {
    /// eine Hex-Ziffer (0–f, Tintenanteil) je Zelle, zeilenweise
    pub raster: String,
    /// Breite / Höhe des bedruckten Bereichs
    pub seitenverhaeltnis: f32,
}

/// Ergebnis für eine Seite: Fingerabdruck und Lage des bedruckten Kopfbereichs (für die Markierung).
#[derive(Debug, Clone)]
pub struct Header {
    pub fingerprint: Fingerprint,
    /// in Punkt, wie die Wörter der Seite
    pub rect: Rect,
}

impl Fingerprint {
    /// Aus einer Seite in Graustufen (`gray[y * width + x]`, 0 = schwarz). `page_pt`: Seitengröße in Punkt.
    /// `None`, wenn oben kaum etwas gedruckt ist.
    pub fn from_gray(gray: &[u8], width: usize, height: usize, page_pt: (f32, f32)) -> Option<Header> {
        if width < 50 || height < 50 || gray.len() < width * height {
            return None;
        }
        let x0 = (width as f32 * BORDER) as usize;
        let x1 = width - x0;
        let y0 = (height as f32 * BORDER * 0.5) as usize;
        let y1 = (height as f32 * SEARCH) as usize;
        // Schwelle aus Papier- und Druckfarbe dieses Scans (heller/dunkler Scan, Grauschleier)
        let mut hist = [0usize; 256];
        for y in y0..y1 {
            for &v in &gray[y * width + x0..y * width + x1] {
                hist[usize::from(v)] += 1;
            }
        }
        let total: usize = hist.iter().sum();
        let percentile = |p: f32| {
            let mut acc = 0;
            hist.iter()
                .position(|&n| {
                    acc += n;
                    acc as f32 >= total as f32 * p
                })
                .unwrap_or(255) as f32
        };
        let (paper, dark) = (percentile(0.5), percentile(0.002));
        if paper - dark < 60.0 {
            return None;
        }
        let threshold = (paper + dark) / 2.0;
        // bedruckte Pixel (schwarz-weiß: Kantenglättung und Helligkeit sollen nicht zählen)
        let mut ink: Vec<(f32, f32, f32)> = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                if f32::from(gray[y * width + x]) < threshold {
                    ink.push((x as f32, y as f32, 1.0));
                }
            }
        }
        if ink.len() < 50 {
            return None;
        }
        // Schräglage: der Winkel, bei dem die Zeilen am schärfsten waagerecht liegen (Projektionsprofil)
        let rows = y1 - y0 + width / 10 + 2;
        let profile = |slope: f32| {
            let mut h = vec![0f32; rows];
            for &(x, y, w) in &ink {
                let r = (y - y0 as f32 - (x - width as f32 / 2.0) * slope + width as f32 / 20.0) as usize;
                if r < rows {
                    h[r] += w;
                }
            }
            h
        };
        let slope = (-SKEW_STEPS..=SKEW_STEPS)
            .map(|i| (i as f32 * 0.1).to_radians().tan())
            .map(|s| (s, profile(s).iter().map(|v| v * v).sum::<f32>()))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(0.0, |(s, _)| s);
        let straight = |x: f32, y: f32| y - (x - width as f32 / 2.0) * slope;
        // Briefkopf = erster Block von oben bis zu einer deutlichen Lücke; Zeilen mit nur ein paar Pixeln
        // (Staub, Lochungen) zählen nicht
        let h = profile(slope);
        let min_row = 2.0;
        let gap = (height as f32 * GAP) as usize;
        let first = h.iter().position(|&v| v >= min_row)?;
        let mut last = first;
        let mut empty = 0;
        for (r, &v) in h.iter().enumerate().skip(first) {
            if v >= min_row {
                last = r;
                empty = 0;
            } else {
                empty += 1;
                if empty > gap {
                    break;
                }
            }
        }
        let offset = y0 as f32 - width as f32 / 20.0;
        let (top, bottom) = (first as f32 + offset, last as f32 + 1.0 + offset);
        let block: Vec<(f32, f32, f32)> = ink
            .iter()
            .map(|&(x, y, w)| (x, straight(x, y), w))
            .filter(|&(_, y, _)| y >= top && y < bottom)
            .collect();
        if block.len() < 30 || bottom - top < 3.0 {
            return None;
        }
        // waagerechte Ausdehnung ohne vereinzelte Ausreißer
        let mut xs: Vec<f32> = block.iter().map(|p| p.0).collect();
        xs.sort_by(f32::total_cmp);
        let (left, right) = (xs[xs.len() / 200], xs[xs.len() - 1 - xs.len() / 200] + 1.0);
        let (bw, bh) = (right - left, bottom - top);
        if bw < 10.0 {
            return None;
        }
        let mut cells = [0f32; COLS * ROWS];
        for &(x, y, w) in &block {
            if x < left || x >= right {
                continue;
            }
            // anteilig auf die vier nächsten Zellen (weich, damit Pixel an Zellgrenzen nicht springen)
            let fc = ((x - left) / bw) * COLS as f32 - 0.5;
            let fr = ((y - top) / bh) * ROWS as f32 - 0.5;
            let (c0, r0) = (fc.floor(), fr.floor());
            let (tc, tr) = (fc - c0, fr - r0);
            for (dc, wc) in [(0, 1.0 - tc), (1, tc)] {
                for (dr, wr) in [(0, 1.0 - tr), (1, tr)] {
                    let (c, r) = (c0 as i32 + dc, r0 as i32 + dr);
                    let c = c.clamp(0, COLS as i32 - 1) as usize;
                    let r = r.clamp(0, ROWS as i32 - 1) as usize;
                    cells[r * COLS + c] += w * wc * wr;
                }
            }
        }
        // auf den dunkelsten Wert strecken, damit Scanner-Helligkeit und Auflösung keine Rolle spielen
        let max = cells.iter().copied().fold(0.0f32, f32::max).max(1e-6);
        let raster: String = cells
            .iter()
            .map(|v| char::from_digit(((v / max) * 15.0).round() as u32, 16).unwrap_or('0'))
            .collect();
        let (sx, sy) = (page_pt.0 / width as f32, page_pt.1 / height as f32);
        Some(Header {
            fingerprint: Fingerprint {
                raster,
                seitenverhaeltnis: bw / bh,
            },
            rect: Rect {
                x: left * sx,
                y: top * sy,
                w: bw * sx,
                h: bh * sy,
            },
        })
    }

    /// Aus RGBA-Pixeln (wie sie die PDF-Darstellung liefert).
    pub fn from_rgba(rgba: &[u8], width: usize, height: usize, page_pt: (f32, f32)) -> Option<Header> {
        let gray: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| ((u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000) as u8)
            .collect();
        Fingerprint::from_gray(&gray, width, height, page_pt)
    }

    fn values(&self) -> Vec<f32> {
        self.raster
            .chars()
            .map(|c| c.to_digit(16).unwrap_or(0) as f32)
            .collect()
    }

    /// Ähnlichkeit −1…1 (Korrelation der Raster, bester Wert bei bis zu einer Zelle Versatz);
    /// 0, wenn die Proportionen deutlich abweichen.
    pub fn similarity(&self, other: &Fingerprint) -> f32 {
        let ratio = self.seitenverhaeltnis / other.seitenverhaeltnis.max(1e-6);
        if !(0.75..=1.33).contains(&ratio) {
            return 0.0;
        }
        let (a, b) = (self.values(), other.values());
        if a.len() != COLS * ROWS || b.len() != COLS * ROWS {
            return 0.0;
        }
        let mut best = f32::MIN;
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                best = best.max(correlation(&a, &b, dx, dy));
            }
        }
        best
    }
}

/// Korrelation von `a` mit dem um (dx, dy) Zellen verschobenen `b`, über die überlappenden Zellen.
fn correlation(a: &[f32], b: &[f32], dx: i32, dy: i32) -> f32 {
    let mut pairs = Vec::with_capacity(COLS * ROWS);
    for r in 0..ROWS as i32 {
        for c in 0..COLS as i32 {
            let (br, bc) = (r + dy, c + dx);
            if (0..ROWS as i32).contains(&br) && (0..COLS as i32).contains(&bc) {
                pairs.push((a[(r * COLS as i32 + c) as usize], b[(br * COLS as i32 + bc) as usize]));
            }
        }
    }
    let n = pairs.len() as f32;
    let (ma, mb) = (
        pairs.iter().map(|p| p.0).sum::<f32>() / n,
        pairs.iter().map(|p| p.1).sum::<f32>() / n,
    );
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (x, y) in &pairs {
        cov += (x - ma) * (y - mb);
        va += (x - ma) * (x - ma);
        vb += (y - mb) * (y - mb);
    }
    if va <= 0.0 || vb <= 0.0 {
        return 0.0;
    }
    cov / (va.sqrt() * vb.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Weiße A4-Seite (595 × 842 pt bei 1 px/pt) mit schwarzen Rechtecken (x, y, w, h).
    fn page(blocks: &[(usize, usize, usize, usize)]) -> Vec<u8> {
        let (w, h) = (595, 842);
        let mut g = vec![255u8; w * h];
        for &(bx, by, bw, bh) in blocks {
            for y in by..by + bh {
                for x in bx..bx + bw {
                    g[y * w + x] = 0;
                }
            }
        }
        g
    }

    fn fp(blocks: &[(usize, usize, usize, usize)]) -> Option<Fingerprint> {
        Fingerprint::from_gray(&page(blocks), 595, 842, (595.0, 842.0)).map(|h| h.fingerprint)
    }

    #[test]
    fn same_letterhead_shifted_is_similar() {
        // Logo links, Absenderzeile rechts
        let logo = [(60, 40, 120, 40), (70, 85, 60, 8), (400, 45, 120, 6), (400, 55, 100, 6)];
        let a = fp(&logo).unwrap();
        // derselbe Briefkopf 7 px nach rechts und 5 px nach unten verrutscht
        let shifted: Vec<_> = logo.iter().map(|&(x, y, w, h)| (x + 7, y + 5, w, h)).collect();
        let b = fp(&shifted).unwrap();
        assert!(a.similarity(&b) > 0.95, "{}", a.similarity(&b));
        // anderer Briefkopf: Logo rechts, Text links
        let other = fp(&[(60, 45, 200, 10), (60, 60, 150, 6), (420, 30, 110, 55)]).unwrap();
        assert!(a.similarity(&other) < 0.5, "{}", a.similarity(&other));
    }

    #[test]
    fn empty_header_has_no_fingerprint() {
        assert!(fp(&[]).is_none());
        // Text erst weiter unten (Anschrift, Brieftext)
        assert!(fp(&[(60, 300, 300, 10)]).is_none());
    }
}
