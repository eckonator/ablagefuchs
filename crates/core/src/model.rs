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
