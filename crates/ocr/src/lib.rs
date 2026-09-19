// SPDX-License-Identifier: MIT OR Apache-2.0
//! Texterkennung (OCR) für Seiten ohne Textebene. Je System die eingebaute Engine:
//! Apple Vision (macOS), Windows.Media.Ocr (Windows), Tesseract als externes Programm (Linux).
//! Die Implementierungen folgen in Meilenstein M4.

use af_core::Word;

/// Ein Seitenbild als RGBA, Zeile für Zeile.
pub struct RgbaImage<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

pub trait OcrEngine: Send + Sync {
    /// Anzeigename, z. B. „Apple Vision“
    fn name(&self) -> &'static str;
    /// Ob die Engine auf diesem Rechner nutzbar ist (Sprachpaket, Programm installiert …)
    fn available(&self) -> bool;
    /// Erkennt die Wörter eines Seitenbilds. `langs` z. B. `["de", "en"]`.
    fn recognize(&self, image: &RgbaImage, langs: &[&str]) -> anyhow::Result<Vec<Word>>;
}
