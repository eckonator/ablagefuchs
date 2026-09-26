// SPDX-License-Identifier: MIT OR Apache-2.0
//! Metadaten und unsichtbare Textebene schreiben, damit Spotlight bzw. die Windows-Suche
//! Dokumente über Titel, Stichwörter und Volltext findet.
//!
//! Geschrieben wird in eine Zwischendatei im selben Ordner, die dann die Originaldatei ersetzt.
//! Verschlüsselte oder nicht lesbare PDFs bleiben unverändert (Fehler zurück, Datei unangetastet).

use std::path::Path;

use af_core::PageText;
use anyhow::{Context, anyhow, bail};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};

/// Was ins PDF geschrieben wird.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PdfMeta {
    pub title: String,
    /// Absender
    pub author: String,
    pub subject: String,
    pub keywords: Vec<String>,
    /// Dokumentdatum `JJJJ-MM-TT`
    pub created: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOptions {
    /// Info-Dictionary (Titel, Autor, Stichwörter, Erstelldatum)
    pub info: bool,
    /// XMP-Paket (für Spotlight und Windows-Suche)
    pub xmp: bool,
}

/// Name der Schrift für die unsichtbare Textebene in den Seitenressourcen.
const OCR_FONT: &str = "AblagefuchsOCR";

/// Schreibt Metadaten und – falls `ocr_pages` Wörter enthält – eine unsichtbare Textebene.
pub fn write_metadata(path: &Path, meta: &PdfMeta, ocr_pages: &[PageText], opts: WriteOptions) -> anyhow::Result<()> {
    let mut doc = Document::load(path).with_context(|| format!("PDF nicht lesbar: {}", path.display()))?;
    if doc.is_encrypted() {
        bail!("PDF ist verschlüsselt, Metadaten werden nicht geschrieben");
    }
    if opts.info {
        set_info(&mut doc, meta)?;
    }
    if opts.xmp {
        set_xmp(&mut doc, meta)?;
    }
    if ocr_pages.iter().any(|p| !p.words.is_empty()) {
        add_text_layer(&mut doc, ocr_pages)?;
    }
    save_atomic(&mut doc, path)
}

fn save_atomic(doc: &mut Document, path: &Path) -> anyhow::Result<()> {
    let dir = path.parent().ok_or_else(|| anyhow!("kein Ordner"))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.ablagefuchs.tmp"));
    {
        let mut f = std::fs::File::create(&tmp)?;
        doc.save_to(&mut f).context("PDF nicht geschrieben")?;
        f.sync_all()?;
    }
    // Zur Sicherheit: die neue Datei muss wieder lesbar sein, bevor sie das Original ersetzt
    if let Err(e) = Document::load(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        bail!("geschriebenes PDF nicht lesbar, Original bleibt: {e}");
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// PDF-Textzeichenkette: ASCII direkt, sonst UTF-16BE mit Byte-Order-Mark.
fn text_string(s: &str) -> Object {
    if s.is_ascii() {
        Object::String(s.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            bytes.extend_from_slice(&u.to_be_bytes());
        }
        Object::String(bytes, StringFormat::Hexadecimal)
    }
}

fn pdf_date(iso: &str) -> Option<String> {
    let digits: String = iso.chars().filter(char::is_ascii_digit).collect();
    (digits.len() == 8).then(|| format!("D:{digits}120000+00'00'"))
}

fn set_info(doc: &mut Document, meta: &PdfMeta) -> anyhow::Result<()> {
    let info_id = match doc.trailer.get(b"Info").and_then(Object::as_reference) {
        Ok(id) => id,
        Err(_) => {
            let id = doc.add_object(Dictionary::new());
            doc.trailer.set("Info", id);
            id
        }
    };
    let info = doc.get_dictionary_mut(info_id)?;
    info.set("Title", text_string(&meta.title));
    info.set("Author", text_string(&meta.author));
    info.set("Subject", text_string(&meta.subject));
    info.set("Keywords", text_string(&meta.keywords.join(", ")));
    info.set(
        "Producer",
        text_string(&format!("Ablagefuchs {}", env!("CARGO_PKG_VERSION"))),
    );
    if let Some(d) = meta.created.as_deref().and_then(pdf_date) {
        info.set("CreationDate", Object::String(d.into_bytes(), StringFormat::Literal));
    }
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn set_xmp(doc: &mut Document, meta: &PdfMeta) -> anyhow::Result<()> {
    let keywords: String = meta
        .keywords
        .iter()
        .map(|k| format!("<rdf:li>{}</rdf:li>", xml_escape(k)))
        .collect::<Vec<_>>()
        .join("");
    let created = meta
        .created
        .as_deref()
        .map(|d| format!("<xmp:CreateDate>{d}T12:00:00Z</xmp:CreateDate>"))
        .unwrap_or_default();
    let xml = format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\">\
<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{title}</rdf:li></rdf:Alt></dc:title>\
<dc:creator><rdf:Seq><rdf:li>{author}</rdf:li></rdf:Seq></dc:creator>\
<dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{subject}</rdf:li></rdf:Alt></dc:description>\
<dc:subject><rdf:Bag>{keywords}</rdf:Bag></dc:subject>\
<pdf:Keywords>{kw}</pdf:Keywords><pdf:Producer>Ablagefuchs {version}</pdf:Producer>{created}\
</rdf:Description></rdf:RDF></x:xmpmeta>\n<?xpacket end=\"w\"?>",
        title = xml_escape(&meta.title),
        author = xml_escape(&meta.author),
        subject = xml_escape(&meta.subject),
        kw = xml_escape(&meta.keywords.join(", ")),
        version = env!("CARGO_PKG_VERSION"),
    );
    // XMP bleibt unkomprimiert, damit Suchprogramme es ohne PDF-Kenntnisse finden
    let mut stream = Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        xml.into_bytes(),
    );
    stream.allows_compression = false;
    let id = doc.add_object(stream);
    doc.catalog_mut()?.set("Metadata", id);
    Ok(())
}

/// WinAnsiEncoding (Windows-1252) für die Standardschrift Helvetica.
fn win_ansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            '\u{20}'..='\u{7e}' | '\u{a0}'..='\u{ff}' => c as u8,
            '€' => 0x80,
            '„' => 0x84,
            '…' => 0x85,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '–' => 0x96,
            '—' => 0x97,
            _ => b'?',
        })
        .collect()
}

/// Unsichtbarer Text (Darstellungsmodus 3) an den Positionen der OCR-Wörter.
fn add_text_layer(doc: &mut Document, pages: &[PageText]) -> anyhow::Result<()> {
    let font_id =
        doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
    let page_ids = doc.get_pages();
    for page in pages.iter().filter(|p| !p.words.is_empty()) {
        let Some(&page_id) = page_ids.get(&u32::from(page.page)) else {
            continue;
        };
        let (x0, y0, height) = media_box(doc, page_id).unwrap_or((0.0, 0.0, page.height));
        ensure_font(doc, page_id, font_id)?;
        let mut content = String::from("q\n");
        for w in &page.words {
            let text = win_ansi(&w.text);
            if text.is_empty() || w.bbox.w <= 0.0 || w.bbox.h <= 0.0 {
                continue;
            }
            let size = (w.bbox.h * 0.85).max(1.0);
            // Breite anpassen: Helvetica ist im Mittel etwa eine halbe Schriftgröße breit je Zeichen
            let natural = 0.52 * size * text.len() as f32;
            let scale = (100.0 * w.bbox.w / natural).clamp(10.0, 1000.0);
            let x = x0 + w.bbox.x;
            let y = y0 + height - (w.bbox.y + w.bbox.h) + 0.2 * w.bbox.h;
            let literal = escape_literal(&text);
            content.push_str(&format!(
                "BT 3 Tr /{OCR_FONT} {size:.2} Tf {scale:.1} Tz 1 0 0 1 {x:.2} {y:.2} Tm ({literal}) Tj ET\n"
            ));
        }
        content.push_str("Q\n");
        doc.add_page_contents(page_id, content.into_bytes())?;
    }
    Ok(())
}

/// Bytes für eine PDF-Literal-Zeichenkette (Klammern und Backslash maskiert, Rest als Oktal-Escape).
fn escape_literal(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(b as char);
            }
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out
}

fn media_box(doc: &Document, page_id: ObjectId) -> Option<(f32, f32, f32)> {
    let mut id = page_id;
    // MediaBox kann vom übergeordneten Seitenbaum geerbt sein
    for _ in 0..16 {
        let dict = doc.get_dictionary(id).ok()?;
        if let Ok(arr) = dict.get(b"MediaBox").and_then(Object::as_array) {
            let n: Vec<f32> = arr.iter().filter_map(|o| o.as_float().ok()).collect();
            if n.len() == 4 {
                return Some((n[0], n[1], n[3] - n[1]));
            }
        }
        id = dict.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

/// Trägt die OCR-Schrift in die Ressourcen der Seite ein. Geerbte Ressourcen werden vorher auf die Seite
/// kopiert, damit nichts verloren geht.
fn ensure_font(doc: &mut Document, page_id: ObjectId, font_id: ObjectId) -> anyhow::Result<()> {
    let has_own = doc.get_dictionary(page_id)?.has(b"Resources");
    if !has_own {
        let (_, inherited) = doc.get_page_resources(page_id)?;
        if let Some(&res_id) = inherited.first() {
            let copy = doc.get_dictionary(res_id)?.clone();
            doc.get_dictionary_mut(page_id)?.set("Resources", copy);
        }
    }
    // Font-Eintrag: direkt im Ressourcen-Dictionary oder als Verweis
    let font_ref = {
        let res = doc.get_or_create_resources(page_id)?.as_dict()?;
        match res.get(b"Font") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };
    match font_ref {
        Some(id) => doc.get_dictionary_mut(id)?.set(OCR_FONT, font_id),
        None => {
            let res = doc.get_or_create_resources(page_id)?.as_dict_mut()?;
            if !res.has(b"Font") {
                res.set("Font", Dictionary::new());
            }
            res.get_mut(b"Font")?.as_dict_mut()?.set(OCR_FONT, font_id);
        }
    }
    Ok(())
}
