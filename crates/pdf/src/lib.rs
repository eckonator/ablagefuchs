// SPDX-License-Identifier: MIT OR Apache-2.0
//! PDF-Zugriff für Ablagefuchs. Rendern und Text mit Positionen über pdfium,
//! Metadaten schreiben über lopdf (ab M6).

mod meta;

pub use meta::{PdfMeta, WriteOptions, write_metadata};

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, anyhow};
use pdfium_render::prelude::*;

static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

/// Gemeinsame pdfium-Instanz. Beim ersten Aufruf wird die Bibliothek gesucht und geladen.
pub fn pdfium() -> anyhow::Result<&'static Pdfium> {
    if let Some(p) = PDFIUM.get() {
        return Ok(p);
    }
    let bindings = bind()?;
    Ok(PDFIUM.get_or_init(|| Pdfium::new(bindings)))
}

/// Anzahl der Seiten eines PDFs.
pub fn page_count(path: &Path) -> anyhow::Result<usize> {
    let doc = pdfium()?
        .load_pdf_from_file(path, None)
        .with_context(|| format!("PDF lässt sich nicht öffnen: {}", path.display()))?;
    Ok(usize::try_from(doc.pages().len()).unwrap_or(0))
}

/// Erstellungsdatum: `CreationDate` aus den PDF-Metadaten, sonst das Änderungsdatum der Datei
/// (bei einem frischen Scan der Scantag).
pub fn created_date(path: &Path) -> Option<af_core::Created> {
    let from_pdf = pdfium()
        .ok()
        .and_then(|p| p.load_pdf_from_file(path, None).ok())
        .and_then(|doc| {
            doc.metadata()
                .get(PdfDocumentMetadataTagType::CreationDate)
                .and_then(|t| af_core::dates::parse_pdf_date(t.value()))
        });
    if let Some(date) = from_pdf {
        return Some(af_core::Created { date, from_pdf: true });
    }
    let secs = std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let date = chrono::DateTime::from_timestamp(i64::try_from(secs).ok()?, 0)?.date_naive();
    Some(af_core::Created { date, from_pdf: false })
}

/// Orte, an denen nach der pdfium-Bibliothek gesucht wird, in dieser Reihenfolge:
/// `AF_PDFIUM` (Datei oder Ordner), neben dem Programm, `../Frameworks` im macOS-App-Bundle,
/// `vendor/pdfium/<plattform>` im Quellbaum (von `cargo xtask fetch-pdfium`).
/// Danach versucht [`bind`] noch die Systembibliothek.
pub fn search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(p) = std::env::var_os("AF_PDFIUM").map(PathBuf::from) {
        paths.push(if p.is_dir() {
            Pdfium::pdfium_platform_library_name_at_path(&p)
        } else {
            p
        });
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        paths.push(Pdfium::pdfium_platform_library_name_at_path(&dir));
        paths.push(Pdfium::pdfium_platform_library_name_at_path(&dir.join("../Frameworks")));
    }
    let vendor = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/pdfium")
        .join(platform());
    for sub in ["lib", "bin"] {
        paths.push(Pdfium::pdfium_platform_library_name_at_path(&vendor.join(sub)));
    }
    paths
}

fn bind() -> anyhow::Result<Box<dyn PdfiumLibraryBindings>> {
    for path in search_paths() {
        if path.is_file() {
            log::debug!("Lade pdfium: {}", path.display());
            return Pdfium::bind_to_library(&path)
                .map_err(|e| anyhow!("pdfium aus {} nicht ladbar: {e}", path.display()));
        }
    }
    Pdfium::bind_to_system_library()
        .map_err(|e| anyhow!("pdfium nicht gefunden ({e}). Für die Entwicklung: `cargo xtask fetch-pdfium` ausführen."))
}

/// Plattformkennung wie bei den pdfium-Binärpaketen (`mac-arm64`, `win-x64`, …).
pub fn platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "mac-arm64",
        ("macos", _) => "mac-x64",
        ("windows", "aarch64") => "win-arm64",
        ("windows", _) => "win-x64",
        ("linux", "aarch64") => "linux-arm64",
        _ => "linux-x64",
    }
}

/// Liest den Text der ersten `max_pages` Seiten als Wörter mit Positionen (Ursprung oben links, in Punkt).
pub fn extract_text(path: &Path, max_pages: usize) -> anyhow::Result<Vec<af_core::PageText>> {
    let doc = pdfium()?
        .load_pdf_from_file(path, None)
        .with_context(|| format!("PDF lässt sich nicht öffnen: {}", path.display()))?;
    let mut pages = Vec::new();
    for (i, page) in doc.pages().iter().enumerate().take(max_pages) {
        let (width, height) = (page.width().value, page.height().value);
        let text = page
            .text()
            .map_err(|e| anyhow!("Text von Seite {} nicht lesbar: {e}", i + 1))?;
        let mut words = Vec::new();
        let mut current: Option<WordBuilder> = None;
        for ch in text.chars().iter() {
            let Some(c) = ch.unicode_char() else { continue };
            if c.is_whitespace() || c.is_control() {
                words.extend(current.take().map(WordBuilder::finish));
                continue;
            }
            let Ok(b) = ch.loose_bounds() else { continue };
            let rect = af_core::Rect {
                x: b.left().value,
                y: height - b.top().value,
                w: b.width().value,
                h: b.height().value,
            };
            let size = ch.scaled_font_size().value;
            // Neues Wort, wenn ein Zeichen deutlich abseits liegt (ohne Leerzeichen dazwischen)
            if let Some(w) = &current {
                let gap = rect.x - w.rect.right();
                if gap > size * 0.3 || gap < -size || (rect.center_y() - w.rect.center_y()).abs() > size * 0.5 {
                    words.extend(current.take().map(WordBuilder::finish));
                }
            }
            let bold = is_bold(&ch);
            match &mut current {
                Some(w) => w.push(c, rect, size, bold),
                None => current = Some(WordBuilder::new(c, rect, size, bold)),
            }
        }
        words.extend(current.take().map(WordBuilder::finish));
        pages.push(af_core::PageText {
            page: u16::try_from(i + 1).unwrap_or(u16::MAX),
            width,
            height,
            words,
        });
    }
    Ok(pages)
}

fn is_bold(ch: &PdfPageTextChar) -> bool {
    let name = ch.font_name().to_lowercase();
    ["bold", "black", "heavy", "semibold", "demi"]
        .iter()
        .any(|w| name.contains(w))
        || ch.font_is_bold_reenforced()
        || matches!(
            ch.font_weight(),
            Some(
                PdfFontWeight::Weight600
                    | PdfFontWeight::Weight700Bold
                    | PdfFontWeight::Weight800
                    | PdfFontWeight::Weight900
            )
        )
}

struct WordBuilder {
    text: String,
    rect: af_core::Rect,
    size: f32,
    bold_chars: usize,
    chars: usize,
}

impl WordBuilder {
    fn new(c: char, rect: af_core::Rect, size: f32, bold: bool) -> Self {
        WordBuilder {
            text: c.to_string(),
            rect,
            size,
            bold_chars: usize::from(bold),
            chars: 1,
        }
    }
    fn push(&mut self, c: char, rect: af_core::Rect, size: f32, bold: bool) {
        self.text.push(c);
        self.rect = self.rect.union(&rect);
        self.size = self.size.max(size);
        self.chars += 1;
        self.bold_chars += usize::from(bold);
    }
    fn finish(self) -> af_core::Word {
        af_core::Word {
            text: self.text,
            bbox: self.rect,
            font_size: self.size,
            bold: self.bold_chars * 2 > self.chars,
        }
    }
}

/// Gerenderte Seite als RGBA-Bild plus Seitengröße in Punkt (für die Umrechnung der Markierungen).
pub struct RenderedPage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub page_width_pt: f32,
    pub page_height_pt: f32,
}

/// Rendert Seite `index` (ab 0) mit `target_width` Pixeln Breite.
pub fn render_page(path: &Path, index: usize, target_width: u32) -> anyhow::Result<RenderedPage> {
    let doc = pdfium()?
        .load_pdf_from_file(path, None)
        .with_context(|| format!("PDF lässt sich nicht öffnen: {}", path.display()))?;
    let page = doc
        .pages()
        .get(i32::try_from(index).unwrap_or(i32::MAX))
        .map_err(|e| anyhow!("Seite {} fehlt: {e}", index + 1))?;
    let config = PdfRenderConfig::new().set_target_width(i32::try_from(target_width).unwrap_or(1200));
    let bitmap = page
        .render_with_config(&config)
        .map_err(|e| anyhow!("Seite {} nicht darstellbar: {e}", index + 1))?;
    Ok(RenderedPage {
        width: usize::try_from(bitmap.width()).unwrap_or(0),
        height: usize::try_from(bitmap.height()).unwrap_or(0),
        rgba: bitmap.as_rgba_bytes(),
        page_width_pt: page.width().value,
        page_height_pt: page.height().value,
    })
}
