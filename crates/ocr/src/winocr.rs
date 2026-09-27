// SPDX-License-Identifier: MIT OR Apache-2.0
//! Windows.Media.Ocr: in Windows 10/11 eingebaut, braucht das Sprachpaket der gewünschten Sprache
//! (sonst nimmt es die Sprachen des Benutzerprofils).

use af_core::{Rect, Word};
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine as WinEngine;
use windows::Storage::Streams::DataWriter;
use windows::core::HSTRING;

use crate::{OcrEngine, OcrPage, RgbaImage};

pub struct WindowsOcr;

fn engine() -> Option<WinEngine> {
    let german = Language::CreateLanguage(&HSTRING::from("de-DE")).ok();
    german
        .and_then(|l| WinEngine::TryCreateFromLanguage(&l).ok())
        .or_else(|| WinEngine::TryCreateFromUserProfileLanguages().ok())
}

impl OcrEngine for WindowsOcr {
    fn name(&self) -> &'static str {
        "Windows OCR"
    }

    fn available(&self) -> bool {
        engine().is_some()
    }

    fn recognize(&self, image: &RgbaImage) -> anyhow::Result<OcrPage> {
        let engine = engine().ok_or_else(|| anyhow::anyhow!("Windows OCR: keine Sprache verfügbar"))?;
        // Windows erwartet BGRA
        let mut bgra = image.pixels.to_vec();
        for px in bgra.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        let writer = DataWriter::new()?;
        writer.WriteBytes(&bgra)?;
        let buffer = writer.DetachBuffer()?;
        let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            i32::try_from(image.width)?,
            i32::try_from(image.height)?,
            BitmapAlphaMode::Premultiplied,
        )?;
        let result = engine.RecognizeAsync(&bitmap)?.join()?;
        let mut words = Vec::new();
        for line in result.Lines()? {
            for word in line.Words()? {
                let r = word.BoundingRect()?;
                let bbox = Rect {
                    x: r.X,
                    y: r.Y,
                    w: r.Width,
                    h: r.Height,
                };
                words.push(Word {
                    text: word.Text()?.to_string(),
                    bbox,
                    font_size: bbox.h,
                    bold: false,
                });
            }
        }
        // Windows OCR liefert keine Sicherheit je Wort; gute Erkennung annehmen
        Ok(OcrPage {
            words,
            mean_confidence: 0.9,
        })
    }
}
