// SPDX-License-Identifier: MIT OR Apache-2.0
//! Metadaten und unsichtbare Textebene: schreiben, wieder lesen, Text an der richtigen Stelle finden.

use af_core::{PageText, Rect, Word};
use af_pdf::{PdfMeta, WriteOptions, write_metadata};
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};

/// Leere A4-Seite ohne Text (wie ein Scan ohne Textebene), Ressourcen absichtlich am Seitenbaum (geerbt).
fn blank_pdf(path: &std::path::Path) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content = Content {
        operations: vec![Operation::new("n", vec![])],
    };
    let stream = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => stream });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Count" => 1, "Kids" => vec![page.into()],
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {},
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    doc.save(path).unwrap();
}

fn decode(o: &Object) -> String {
    let bytes = o.as_str().unwrap();
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..].chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16(&units).unwrap()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[test]
fn writes_metadata_and_invisible_text() {
    let file = std::env::temp_dir().join(format!("af-meta-test-{}.pdf", std::process::id()));
    blank_pdf(&file);
    let meta = PdfMeta {
        title: "Krankenkasse Süd – Beitragsbescheinigung 2025".into(),
        author: "Krankenkasse Süd".into(),
        subject: "Beitragsbescheinigung 2025".into(),
        keywords: vec!["Krankenkasse Süd".into(), "Beitragsbescheinigung".into(), "2025".into()],
        created: Some("2026-09-15".into()),
    };
    let words = vec![
        Word {
            text: "Beitragsbescheinigung".into(),
            bbox: Rect {
                x: 100.0,
                y: 300.0,
                w: 160.0,
                h: 14.0,
            },
            font_size: 14.0,
            bold: false,
        },
        Word {
            text: "Süd".into(),
            bbox: Rect {
                x: 270.0,
                y: 300.0,
                w: 30.0,
                h: 14.0,
            },
            font_size: 14.0,
            bold: false,
        },
    ];
    let ocr = vec![PageText {
        page: 1,
        width: 595.0,
        height: 842.0,
        words,
    }];
    write_metadata(&file, &meta, &ocr, WriteOptions { info: true, xmp: true }).unwrap();

    let doc = Document::load(&file).unwrap();
    let info_id = doc.trailer.get(b"Info").unwrap().as_reference().unwrap();
    let info = doc.get_dictionary(info_id).unwrap();
    assert_eq!(decode(info.get(b"Title").unwrap()), meta.title);
    assert_eq!(decode(info.get(b"Author").unwrap()), "Krankenkasse Süd");
    assert_eq!(decode(info.get(b"CreationDate").unwrap()), "D:20260915120000+00'00'");
    let xmp_id = doc.catalog().unwrap().get(b"Metadata").unwrap().as_reference().unwrap();
    let xmp = String::from_utf8(doc.get_object(xmp_id).unwrap().as_stream().unwrap().content.clone()).unwrap();
    assert!(xmp.contains("<rdf:li>Beitragsbescheinigung</rdf:li>") && xmp.contains("Krankenkasse Süd"));

    // Die unsichtbare Textebene muss pdfium wiederfinden – an der richtigen Stelle
    if af_pdf::pdfium().is_ok() {
        // Das Erstellungsdatum liest pdfium aus den Metadaten zurück
        let created = af_pdf::created_date(&file).unwrap();
        assert!(created.from_pdf);
        assert_eq!(created.date, chrono::NaiveDate::from_ymd_opt(2026, 9, 15).unwrap());
        let pages = af_pdf::extract_text(&file, 1).unwrap();
        let w = pages[0]
            .words
            .iter()
            .find(|w| w.text == "Beitragsbescheinigung")
            .expect("Text eingebettet");
        assert!(
            (w.bbox.x - 100.0).abs() < 3.0 && (w.bbox.y - 300.0).abs() < 6.0,
            "{:?}",
            w.bbox
        );
        assert!(pages[0].words.iter().any(|w| w.text == "Süd"));
    }
    std::fs::remove_file(&file).ok();
}
