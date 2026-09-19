// SPDX-License-Identifier: MIT OR Apache-2.0
//! Prüft, dass pdfium gefunden und geladen wird und ein PDF öffnen kann.
//! Ohne pdfium wird der Test übersprungen, außer `AF_REQUIRE_PDFIUM=1` ist gesetzt (CI).

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};

/// Kleinstes gültiges PDF mit `pages` leeren A4-Seiten.
fn tiny_pdf(pages: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let kids: Vec<Object> = (0..pages)
        .map(|_| {
            let content = Content {
                operations: vec![Operation::new("n", vec![])],
            };
            let stream = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => pages_id, "Contents" => stream,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            })
            .into()
        })
        .collect();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => pages as i64, "Kids" => kids }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn pdfium_opens_pdf() {
    if let Err(e) = af_pdf::pdfium() {
        assert!(std::env::var_os("AF_REQUIRE_PDFIUM").is_none(), "pdfium fehlt: {e}");
        eprintln!("Übersprungen: {e}");
        return;
    }
    let file = std::env::temp_dir().join(format!("af-pdfium-test-{}.pdf", std::process::id()));
    std::fs::write(&file, tiny_pdf(3)).unwrap();
    let count = af_pdf::page_count(&file);
    std::fs::remove_file(&file).ok();
    assert_eq!(count.unwrap(), 3);
}
