// SPDX-License-Identifier: MIT OR Apache-2.0
//! Texterkennung (OCR) für Seiten ohne Textebene. Je System die eingebaute Engine:
//! Apple Vision (macOS), Windows.Media.Ocr (Windows), sonst Tesseract als externes Programm.

pub mod archive;
mod tesseract;
#[cfg(target_os = "macos")]
mod vision;
#[cfg(windows)]
mod winocr;

use af_core::{PageText, Rect, Word};

/// Ein Seitenbild als RGBA, Zeile für Zeile.
pub struct RgbaImage<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

/// Ergebnis einer Seite: Wörter in Bildpixeln (Ursprung oben links) und mittlere Sicherheit 0–1.
pub struct OcrPage {
    pub words: Vec<Word>,
    pub mean_confidence: f32,
}

pub trait OcrEngine: Send + Sync {
    /// Anzeigename, z. B. „Apple Vision“
    fn name(&self) -> &'static str;
    /// Ob die Engine auf diesem Rechner nutzbar ist (Sprachpaket, Programm installiert …)
    fn available(&self) -> bool;
    /// Erkennt die Wörter eines Seitenbilds.
    fn recognize(&self, image: &RgbaImage) -> anyhow::Result<OcrPage>;
}

/// Welche Engine bevorzugt wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preference {
    /// Systemeigene Engine, sonst Tesseract
    Auto,
    /// Immer Tesseract (gleiche Ergebnisse auf allen Systemen)
    Tesseract,
}

/// Die beste verfügbare Engine oder `None`, wenn es keine gibt (dann bleibt der Scan ohne Text).
pub fn engine(pref: Preference) -> Option<Box<dyn OcrEngine>> {
    let mut candidates: Vec<Box<dyn OcrEngine>> = Vec::new();
    if pref == Preference::Auto {
        #[cfg(target_os = "macos")]
        candidates.push(Box::new(vision::Vision));
        #[cfg(windows)]
        candidates.push(Box::new(winocr::WindowsOcr));
    }
    candidates.push(Box::new(tesseract::Tesseract));
    candidates.into_iter().find(|e| e.available())
}

/// Rechnet OCR-Wörter (Bildpixel) in Seitenkoordinaten (Punkt) um, wie sie die Textebene liefert.
pub fn to_page_text(ocr: &OcrPage, image_width: u32, page: u16, page_width_pt: f32, page_height_pt: f32) -> PageText {
    let scale = page_width_pt / image_width.max(1) as f32;
    let words = ocr
        .words
        .iter()
        .map(|w| {
            let bbox = Rect {
                x: w.bbox.x * scale,
                y: w.bbox.y * scale,
                w: w.bbox.w * scale,
                h: w.bbox.h * scale,
            };
            // Schriftgröße schätzen: Höhe des Wortkastens. Für den Vergleich „größer als Fließtext“ reicht das.
            Word {
                text: w.text.clone(),
                bbox,
                font_size: bbox.h,
                bold: false,
            }
        })
        .collect();
    PageText {
        page,
        width: page_width_pt,
        height: page_height_pt,
        words,
    }
}

/// PNG für Engines, die ein Bild als Datei bzw. Datenblock erwarten.
pub(crate) fn encode_png(image: &RgbaImage) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.write_header()?.write_image_data(image.pixels)?;
    Ok(out)
}

/// Teilt eine erkannte Zeile in Wörter auf, wenn die Engine nur Zeilen liefert: Breite anteilig nach Zeichen.
#[allow(dead_code)]
pub(crate) fn split_line(text: &str, line: Rect) -> Vec<Word> {
    let total = text.chars().count().max(1) as f32;
    let mut out = Vec::new();
    let mut offset = 0usize;
    for part in text.split(' ') {
        let n = part.chars().count();
        if n > 0 {
            let x = line.x + line.w * offset as f32 / total;
            let w = line.w * n as f32 / total;
            out.push(Word {
                text: part.to_string(),
                bbox: Rect {
                    x,
                    y: line.y,
                    w,
                    h: line.h,
                },
                font_size: line.h,
                bold: false,
            });
        }
        offset += n + 1;
    }
    out
}

/// Unter so vielen Zeichen auf Seite 1 gilt ein PDF als Scan ohne Textebene.
const MIN_TEXT_CHARS: usize = 30;
/// Auflösung für die Texterkennung.
const OCR_DPI: f32 = 300.0;

/// Liest den Text der ersten `max_pages` Seiten: aus der Textebene, sonst per Texterkennung.
/// Gibt zurück, woher der Text stammt (für die Sicherheit und die Anzeige).
pub fn read_text(
    path: &std::path::Path,
    max_pages: usize,
    pref: Preference,
) -> anyhow::Result<(Vec<PageText>, af_core::TextSource)> {
    let pages = af_pdf::extract_text(path, max_pages)?;
    let chars: usize = pages
        .first()
        .map_or(0, |p| p.words.iter().map(|w| w.text.chars().count()).sum());
    if chars >= MIN_TEXT_CHARS {
        return Ok((pages, af_core::TextSource::Embedded));
    }
    let Some(engine) = engine(pref) else {
        log::info!("{}: kein Text und keine Texterkennung verfügbar", path.display());
        return Ok((pages, af_core::TextSource::Embedded));
    };
    let started = std::time::Instant::now();
    let mut out = Vec::new();
    let mut conf = 0.0;
    for (i, p) in pages.iter().enumerate() {
        let width_px = ((p.width / 72.0 * OCR_DPI) as u32).clamp(800, 3600);
        let img = af_pdf::render_page(path, i, width_px)?;
        let rgba = RgbaImage {
            width: img.width as u32,
            height: img.height as u32,
            pixels: &img.rgba,
        };
        let ocr = engine.recognize(&rgba)?;
        conf += ocr.mean_confidence;
        out.push(to_page_text(
            &ocr,
            rgba.width,
            p.page,
            img.page_width_pt,
            img.page_height_pt,
        ));
    }
    let mean_confidence = if out.is_empty() { 0.0 } else { conf / out.len() as f32 };
    log::info!(
        "{}: Texterkennung mit {} in {:.1} s, Sicherheit {:.0} %",
        path.display(),
        engine.name(),
        started.elapsed().as_secs_f32(),
        mean_confidence * 100.0
    );
    Ok((
        out,
        af_core::TextSource::Ocr {
            engine: engine.name().to_string(),
            mean_confidence,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_coordinates() {
        let ocr = OcrPage {
            words: vec![Word {
                text: "Rechnung".into(),
                bbox: Rect {
                    x: 500.0,
                    y: 1000.0,
                    w: 250.0,
                    h: 50.0,
                },
                font_size: 0.0,
                bold: false,
            }],
            mean_confidence: 0.9,
        };
        // 2480 px Breite = A4 bei 300 dpi → 595 pt
        let p = to_page_text(&ocr, 2480, 1, 595.0, 842.0);
        let b = p.words[0].bbox;
        assert!((b.x - 120.0).abs() < 0.1 && (b.w - 60.0).abs() < 0.1 && (p.words[0].font_size - 12.0).abs() < 0.1);
    }

    #[test]
    fn split_line_proportionally() {
        let words = split_line(
            "ab cd",
            Rect {
                x: 0.0,
                y: 0.0,
                w: 50.0,
                h: 10.0,
            },
        );
        assert_eq!(words.len(), 2);
        assert_eq!(words[1].text, "cd");
        assert!((words[1].bbox.x - 30.0).abs() < 0.01);
    }
}
