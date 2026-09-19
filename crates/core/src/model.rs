// SPDX-License-Identifier: MIT OR Apache-2.0
//! Text einer Seite mit Positionen, egal ob aus der Textebene des PDFs oder aus der OCR.

use serde::{Deserialize, Serialize};

/// Rechteck in PDF-Punkten (1/72 Zoll), Ursprung oben links.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn center_y(&self) -> f32 {
        self.y + self.h / 2.0
    }
    /// Kleinstes Rechteck, das beide umschließt.
    pub fn union(&self, o: &Rect) -> Rect {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Rect {
            x,
            y,
            w: self.right().max(o.right()) - x,
            h: self.bottom().max(o.bottom()) - y,
        }
    }
}

/// Ein Wort mit Lage und Schriftmerkmalen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub bbox: Rect,
    pub font_size: f32,
    pub bold: bool,
}

/// Alle Wörter einer Seite.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PageText {
    /// Seitennummer ab 1
    pub page: u16,
    pub width: f32,
    pub height: f32,
    pub words: Vec<Word>,
}

/// Woher der Text stammt. OCR-Text bekommt später eine geringere Sicherheit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextSource {
    Embedded,
    Ocr { engine: String, mean_confidence: f32 },
}
