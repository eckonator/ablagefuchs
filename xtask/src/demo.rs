// SPDX-License-Identifier: MIT OR Apache-2.0
//! Erzeugt Beispiel-Scans fiktiver Firmen (dieselben wie in `docs/mockup.html`).
//! Für Tests, Demos und README-Screenshots; echte Dokumente kommen nie ins Repo.
//!
//! Die PDFs haben eine Textebene (Helvetica, WinAnsiEncoding), nur `scan_…_0007.pdf` nicht:
//! sie simuliert einen Scan ohne Text, der später per OCR erkannt werden muss.

use std::path::Path;

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId, Stream, StringFormat, dictionary};

const W: f32 = 595.0;
const H: f32 = 842.0;
const LEFT: f32 = 60.0;
const INFO_X: f32 = 380.0;

/// Eine Seite als Liste von Zeichenbefehlen. Koordinaten von oben links in Punkt.
#[derive(Default)]
struct Page {
    ops: Vec<Operation>,
}

impl Page {
    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, s: &str) -> &mut Self {
        let font = if bold { "F2" } else { "F1" };
        self.ops.extend([
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![font.into(), size.into()]),
            Operation::new("Td", vec![x.into(), (H - y).into()]),
            Operation::new("Tj", vec![Object::String(win_ansi(s), StringFormat::Literal)]),
            Operation::new("ET", vec![]),
        ]);
        self
    }
    fn t(&mut self, x: f32, y: f32, s: &str) -> &mut Self {
        self.text(x, y, 10.5, false, s)
    }
    fn b(&mut self, x: f32, y: f32, s: &str) -> &mut Self {
        self.text(x, y, 10.5, true, s)
    }
    fn small(&mut self, x: f32, y: f32, s: &str) -> &mut Self {
        self.text(x, y, 7.5, false, s)
    }
    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) -> &mut Self {
        self.ops.extend([
            Operation::new("m", vec![x1.into(), (H - y1).into()]),
            Operation::new("l", vec![x2.into(), (H - y2).into()]),
            Operation::new("S", vec![]),
        ]);
        self
    }
    /// Fließtext, eine Zeile je Eintrag. Gibt die y-Position nach dem Absatz zurück.
    fn para(&mut self, y: f32, lines: &[&str]) -> f32 {
        for (i, l) in lines.iter().enumerate() {
            self.t(LEFT, y + i as f32 * 14.0, l);
        }
        y + lines.len() as f32 * 14.0 + 10.0
    }
    /// Tabelle mit rechtsbündiger letzter Spalte (ungefähr, Helvetica ≈ 0,5 em je Zeichen).
    fn table(&mut self, mut y: f32, rows: &[(&str, &str)], bold_last: bool) -> f32 {
        for (i, (label, value)) in rows.iter().enumerate() {
            let bold = bold_last && i == rows.len() - 1;
            self.text(LEFT, y, 10.0, bold, label);
            let x = W - LEFT - value.chars().count() as f32 * 5.2;
            self.text(x, y, 10.0, bold, value);
            self.line(LEFT, y + 5.0, W - LEFT, y + 5.0);
            y += 18.0;
        }
        y + 10.0
    }
}

/// Briefkopf, Absenderzeile, Anschrift und Infoblock nach DIN 5008 (vereinfacht).
fn letter(company: &str, company_size: f32, address: &[&str], info: &[&str]) -> Page {
    let mut p = Page::default();
    p.text(LEFT, 70.0, company_size, true, company);
    for (i, l) in address.iter().enumerate() {
        p.small(INFO_X, 62.0 + i as f32 * 10.0, l);
    }
    // Absenderzeile über dem Anschriftenfeld; Firmenname nicht doppelt, wenn er schon in der Adresse steht
    let sender = if address[0].starts_with(company) {
        address.join(" · ")
    } else {
        format!("{company} · {}", address.join(" · "))
    };
    p.small(LEFT, 140.0, &sender);
    p.line(LEFT, 144.0, 300.0, 144.0);
    for (i, l) in ["Herrn", "Markus Beispiel", "Lindenallee 12", "76133 Musterstadt"]
        .iter()
        .enumerate()
    {
        p.t(LEFT, 162.0 + i as f32 * 14.0, l);
    }
    for (i, l) in info.iter().enumerate() {
        p.text(INFO_X, 250.0 + i as f32 * 13.0, 9.0, false, l);
    }
    p
}

fn footer(p: &mut Page, left: &str, page: usize, pages: usize) {
    p.line(LEFT, H - 50.0, W - LEFT, H - 50.0);
    p.small(LEFT, H - 38.0, left);
    p.small(W - LEFT - 50.0, H - 38.0, &format!("Seite {page} von {pages}"));
}

/// Weitere Seiten eines mehrseitigen Dokuments.
fn follow_page(title: &str, footer_left: &str, page: usize, pages: usize) -> Page {
    let mut p = Page::default();
    p.b(LEFT, 70.0, title);
    p.para(100.0, &["Erläuterungen und Hinweise zu diesem Schreiben."]);
    footer(&mut p, footer_left, page, pages);
    p
}

fn documents() -> Vec<(&'static str, Vec<Page>)> {
    let mut docs = Vec::new();

    // 1: Stromrechnung, eindeutig
    let f = "Stadtwerke Musterstadt GmbH · USt-IdNr. DE123456789 · IBAN DE12 6605 0101 0000 1234 56";
    let mut p = letter(
        "Stadtwerke Musterstadt",
        18.0,
        &["Stadtwerke Musterstadt GmbH", "Werkstraße 4", "76131 Musterstadt"],
        &[
            "Kundennummer: 4711-0815",
            "Rechnungsnummer: 2026-883412",
            "Datum: 28.09.2026",
        ],
    );
    p.text(LEFT, 310.0, 13.0, true, "Jahresabrechnung Strom 2025/2026");
    let y = p.para(
        340.0,
        &[
            "Sehr geehrter Herr Beispiel,",
            "",
            "für den Abrechnungszeitraum vom 01.09.2025 bis 31.08.2026 erhalten Sie Ihre Jahresabrechnung.",
        ],
    );
    let y = p.table(
        y,
        &[
            ("Arbeitspreis Strom (2.417 kWh)", "773,44 €"),
            ("Grundpreis (12 Monate)", "155,40 €"),
            ("abzgl. geleistete Abschläge", "-876,00 €"),
            ("Rechnungsbetrag (inkl. 19 % USt)", "52,84 €"),
        ],
        true,
    );
    p.para(
        y,
        &[
            "Der Betrag wird am 20.10.2026 von Ihrem Konto abgebucht.",
            "Ihr neuer monatlicher Abschlag beträgt ab November 75,00 €.",
            "",
            "Mit freundlichen Grüßen",
            "Ihre Stadtwerke Musterstadt",
        ],
    );
    footer(&mut p, f, 1, 2);
    docs.push((
        "scan_20261002_0001.pdf",
        vec![p, follow_page("Erläuterungen zur Jahresabrechnung", f, 2, 2)],
    ));

    // 2: Beitragsbescheinigung, keine Rechnung
    let mut p = letter(
        "Krankenkasse Süd",
        18.0,
        &["Postfach 10 20 30", "70100 Stuttgart"],
        &["Versichertennummer: K123456789", "Stuttgart, 15. September 2026"],
    );
    p.text(LEFT, 310.0, 12.0, true, "Beitragsbescheinigung für das Jahr 2025");
    let y = p.para(
        340.0,
        &[
            "Sehr geehrter Herr Beispiel,",
            "",
            "für Ihre Einkommensteuererklärung bescheinigen wir Ihnen die im Zeitraum 01.01.2025 bis 31.12.2025",
            "gezahlten Beiträge zur Kranken- und Pflegeversicherung:",
        ],
    );
    let y = p.table(
        y,
        &[
            ("Krankenversicherung (Basisabsicherung)", "4.812,60 €"),
            ("Pflegeversicherung", "1.022,40 €"),
        ],
        false,
    );
    p.para(
        y,
        &[
            "Diese Daten wurden auch elektronisch an Ihr Finanzamt übermittelt.",
            "",
            "Freundliche Grüße",
            "Ihre Krankenkasse Süd",
        ],
    );
    footer(&mut p, "Krankenkasse Süd · Körperschaft des öffentlichen Rechts", 1, 1);
    docs.push(("scan_20261002_0002.pdf", vec![p]));

    // 3: Mobilfunkrechnung, Firmenname mit Rechtsform
    let f = "NetzWelt Kommunikation GmbH · USt-IdNr. DE987654321 · netzwelt.example";
    let mut p = letter(
        "NetzWelt Kommunikation GmbH",
        16.0,
        &["Kundenservice: 0800 123 456", "netzwelt.example"],
        &[
            "Rechnung vom 01.10.2026",
            "Kundenkonto: 99887766",
            "Rechnungsnummer: RN-2026-10-554",
        ],
    );
    p.text(LEFT, 310.0, 13.0, true, "Ihre Rechnung für Mobilfunk");
    let y = p.para(340.0, &["Leistungszeitraum: 01.09.2026 – 30.09.2026"]);
    let y = p.table(
        y,
        &[
            ("Tarif NetzWelt Unlimited M", "25,20 €"),
            ("Roaming EU", "0,00 €"),
            ("USt 19 %", "4,79 €"),
            ("Gesamtbetrag", "29,99 €"),
        ],
        true,
    );
    p.para(
        y,
        &["Wir buchen den Betrag frühestens am 08.10.2026 von Ihrem Konto ab."],
    );
    footer(&mut p, f, 1, 3);
    docs.push((
        "scan_20261002_0003.pdf",
        vec![
            p,
            follow_page("Einzelverbindungsnachweis", f, 2, 3),
            follow_page("Allgemeine Hinweise", f, 3, 3),
        ],
    ));

    // 4: Steuerbescheid, Zahlbetrag aber keine Rechnung
    let f = "Rechtsbehelfsbelehrung siehe Seite 4";
    let mut p = letter(
        "Finanzamt Musterstadt",
        18.0,
        &["Schlossplatz 1", "76131 Musterstadt"],
        &["Steuernummer 35/123/45678", "Datum 22.09.2026"],
    );
    p.text(
        LEFT,
        310.0,
        12.0,
        true,
        "Bescheid für 2025 über Einkommensteuer und Solidaritätszuschlag",
    );
    let y = p.para(340.0, &["Der Bescheid gilt mit dem 25.09.2026 als bekannt gegeben."]);
    let y = p.table(
        y,
        &[
            ("festgesetzt werden", "8.214,00 €"),
            ("abzüglich Steuerabzug vom Lohn", "8.903,00 €"),
            ("verbleibende Beträge", "-689,00 €"),
        ],
        true,
    );
    p.para(
        y,
        &[
            "Der Erstattungsbetrag wird auf Ihr Konto überwiesen.",
            "Etwaige Zahlungen sind bis zum 27.10.2026 zu leisten.",
        ],
    );
    footer(&mut p, f, 1, 4);
    let mut pages = vec![p];
    for (i, t) in [
        "Berechnung der Einkommensteuer",
        "Erläuterungen",
        "Rechtsbehelfsbelehrung",
    ]
    .iter()
    .enumerate()
    {
        pages.push(follow_page(t, f, i + 2, 4));
    }
    docs.push(("scan_20261002_0004.pdf", pages));

    // 5: Arztrechnung, neuer Absender
    let mut p = letter(
        "Zahnarztpraxis Dr. Weber",
        15.0,
        &["Marktplatz 7", "76137 Musterstadt"],
        &["Rechnung Nr. 2026/1187", "Datum: 30.09.2026"],
    );
    p.text(LEFT, 310.0, 12.0, true, "Liquidation nach GOZ");
    let y = p.table(
        340.0,
        &[
            ("03.09.2026  Professionelle Zahnreinigung (GOZ 1040)", "96,48 €"),
            ("03.09.2026  Befundaufnahme", "12,94 €"),
            ("Gesamtbetrag", "109,42 €"),
        ],
        true,
    );
    p.para(y, &["Bitte überweisen Sie den Betrag bis zum 30.10.2026."]);
    footer(&mut p, "Bankverbindung: Sparkasse Musterstadt", 1, 1);
    docs.push(("scan_20261002_0005.pdf", vec![p]));

    // 6: Nebenkostenabrechnung, Grenzfall Rechnung
    let f = "Hausverwaltung Lindner · Bahnhofstraße 22 · 76137 Musterstadt";
    let mut p = letter(
        "Hausverwaltung Lindner",
        18.0,
        &["Bahnhofstraße 22", "76137 Musterstadt"],
        &["Musterstadt, 19.09.2026", "Objekt: Lindenallee 12"],
    );
    p.text(LEFT, 310.0, 12.0, true, "Nebenkostenabrechnung für das Jahr 2025");
    let y = p.para(
        340.0,
        &[
            "Sehr geehrter Herr Beispiel,",
            "",
            "anbei erhalten Sie die Betriebskostenabrechnung für den Zeitraum 01.01.–31.12.2025.",
        ],
    );
    let y = p.table(
        y,
        &[
            ("Ihre Kosten gesamt", "2.184,30 €"),
            ("Ihre Vorauszahlungen", "2.040,00 €"),
            ("Nachzahlung", "144,30 €"),
        ],
        true,
    );
    p.para(y, &["Bitte überweisen Sie die Nachzahlung bis zum 31.10.2026."]);
    footer(&mut p, f, 1, 3);
    docs.push((
        "scan_20261002_0006.pdf",
        vec![
            p,
            follow_page("Aufstellung der Kosten", f, 2, 3),
            follow_page("Verteilerschlüssel", f, 3, 3),
        ],
    ));

    // 7: handschriftlicher Gruß ohne Textebene (nur Linien, wie ein Bild)
    let mut p = Page::default();
    for i in 0..5 {
        let y = 120.0 + i as f32 * 34.0;
        let mut x = LEFT + 10.0;
        while x < 420.0 - i as f32 * 40.0 {
            p.line(x, y, x + 6.0, y - 10.0)
                .line(x + 6.0, y - 10.0, x + 11.0, y + 2.0);
            x += 13.0;
        }
    }
    for (x1, y1, x2, y2) in [
        (LEFT, 400.0, 400.0, 400.0),
        (400.0, 400.0, 400.0, 560.0),
        (400.0, 560.0, LEFT, 560.0),
        (LEFT, 560.0, LEFT, 400.0),
    ] {
        p.line(x1, y1, x2, y2);
    }
    docs.push(("scan_20261002_0007.pdf", vec![p]));

    docs
}

/// Schreibt alle Demo-PDFs nach `dir`.
pub fn write_all(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, pages) in documents() {
        let path = dir.join(name);
        save(pages, &path)?;
        println!("{}", path.display());
    }
    // Ein echter „Scan“: Brief als Bild ohne Textebene, damit die Texterkennung etwas zu tun hat
    let path = dir.join("scan_20261002_0008.pdf");
    scanned(scan_letter(), &path)?;
    println!("{}", path.display());
    Ok(())
}

/// Beitragsrechnung einer Kfz-Versicherung, wird nur als Bild abgelegt.
fn scan_letter() -> Page {
    let mut p = letter(
        "Kfz-Versicherung Nord",
        18.0,
        &["Hafenstraße 9", "20457 Hamburg"],
        &[
            "Versicherungsschein: KV-1188-42",
            "Rechnungsnummer: BR-2026-7781",
            "Hamburg, 05.10.2026",
        ],
    );
    p.text(LEFT, 310.0, 13.0, true, "Ihre Beitragsrechnung für das Jahr 2027");
    let y = p.para(
        340.0,
        &[
            "Sehr geehrter Herr Beispiel,",
            "",
            "für Ihren Pkw MS-AB 123 berechnen wir folgende Beiträge:",
        ],
    );
    let y = p.table(
        y,
        &[
            ("Haftpflicht", "312,40 €"),
            ("Vollkasko", "298,10 €"),
            ("Versicherungsteuer 19 %", "116,00 €"),
            ("Gesamtbetrag", "726,50 €"),
        ],
        true,
    );
    p.para(
        y,
        &[
            "Der Betrag ist zahlbar bis zum 01.01.2027.",
            "",
            "Mit freundlichen Grüßen",
            "Ihre Kfz-Versicherung Nord",
        ],
    );
    footer(&mut p, "Kfz-Versicherung Nord AG · Hafenstraße 9 · 20457 Hamburg", 1, 1);
    p
}

/// Rendert die Seite mit pdfium als Graustufenbild (200 dpi) und legt nur das Bild in ein neues PDF.
fn scanned(page: Page, path: &Path) -> anyhow::Result<()> {
    let tmp = path.with_extension("text.pdf");
    save(vec![page], &tmp)?;
    let rendered = af_pdf::render_page(&tmp, 0, 1654);
    std::fs::remove_file(&tmp)?;
    let img = rendered?;
    // Graustufen mit leichtem Papierton, wie ein Scanner sie liefert
    let gray: Vec<u8> = img
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|px| {
            let l = (u32::from(px[0]) * 30 + u32::from(px[1]) * 59 + u32::from(px[2]) * 11) / 100;
            (l as u8).min(246)
        })
        .collect();
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut z, &gray)?;
    let data = z.finish()?;

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let image = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => img.width as i64, "Height" => img.height as i64,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8, "Filter" => "FlateDecode",
        },
        data,
    ));
    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new("cm", vec![W.into(), 0.into(), 0.into(), H.into(), 0.into(), 0.into()]),
            Operation::new("Do", vec!["Scan".into()]),
            Operation::new("Q", vec![]),
        ],
    };
    let stream = doc.add_object(Stream::new(dictionary! {}, content.encode()?));
    let resources = doc.add_object(dictionary! { "XObject" => dictionary! { "Scan" => image } });
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => stream, "Resources" => resources,
        "MediaBox" => vec![0.into(), 0.into(), W.into(), H.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => 1, "Kids" => vec![page_id.into()] }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    doc.save(path)?;
    Ok(())
}

fn save(pages: Vec<Page>, path: &Path) -> anyhow::Result<()> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = |doc: &mut Document, base: &str| -> ObjectId {
        doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => base, "Encoding" => "WinAnsiEncoding",
        })
    };
    let regular = font(&mut doc, "Helvetica");
    let bold = font(&mut doc, "Helvetica-Bold");
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => regular, "F2" => bold } });
    let count = pages.len() as i64;
    let mut kids = Vec::new();
    for page in pages {
        let content = Content { operations: page.ops };
        let stream = doc.add_object(Stream::new(dictionary! {}, content.encode()?));
        let id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => stream, "Resources" => resources,
            "MediaBox" => vec![0.into(), 0.into(), W.into(), H.into()],
        });
        kids.push(id.into());
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => count, "Kids" => kids }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    doc.compress();
    doc.save(path)?;
    Ok(())
}

/// Wandelt Text in WinAnsiEncoding (Windows-1252) um, das die PDF-Standardschriften verstehen.
fn win_ansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            '\u{20}'..='\u{7e}' | '\u{a0}'..='\u{ff}' => c as u8,
            '€' => 0x80,
            '‚' => 0x82,
            '„' => 0x84,
            '…' => 0x85,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            _ => b'?',
        })
        .collect()
}

/// Ein Brief für das Demo-Archiv.
struct ArchiveLetter {
    path: &'static str,
    company: &'static str,
    address: &'static [&'static str],
    info: &'static [&'static str],
    subject: &'static str,
    items: &'static [(&'static str, &'static str)],
    footer: &'static str,
}

/// Fiktives Archiv, wie es nach einiger Zeit mit Ablagefuchs aussehen könnte: Unterordner in mehreren Ebenen,
/// Dateinamen nach der Standardvorlage in der Schreibweise des Nutzers („NetzWelt“) und eine alte Datei,
/// die nicht zur Vorlage passt.
pub fn write_archive(dir: &Path) -> anyhow::Result<()> {
    const NETZWELT: &str = "NetzWelt Kommunikation GmbH · USt-IdNr. DE987654321 · netzwelt.example";
    const STADTWERKE: &str = "Stadtwerke Musterstadt GmbH · USt-IdNr. DE123456789";
    let letters = [
        ArchiveLetter {
            path: "2025/NetzWelt/2025-08-01_NetzWelt_-_Mobilfunk_Juli_Rechnung.pdf",
            company: "NetzWelt Kommunikation GmbH",
            address: &["Kundenservice: 0800 123 456", "netzwelt.example"],
            info: &["Rechnung vom 01.08.2025", "Rechnungsnummer: RN-2025-08-112"],
            subject: "Ihre Rechnung für Mobilfunk",
            items: &[("Tarif NetzWelt Unlimited M", "25,20 €"), ("Gesamtbetrag", "29,99 €")],
            footer: NETZWELT,
        },
        ArchiveLetter {
            path: "2025/NetzWelt/2025-09-01_NetzWelt_-_Mobilfunk_August_Rechnung.pdf",
            company: "NetzWelt Kommunikation GmbH",
            address: &["Kundenservice: 0800 123 456", "netzwelt.example"],
            info: &["Rechnung vom 01.09.2025", "Rechnungsnummer: RN-2025-09-087"],
            subject: "Ihre Rechnung für Mobilfunk",
            items: &[("Tarif NetzWelt Unlimited M", "25,20 €"), ("Gesamtbetrag", "29,99 €")],
            footer: NETZWELT,
        },
        ArchiveLetter {
            path: "2025/Wohnen/Strom/2025-09-26_Stadtwerke_Musterstadt_-_Jahresabrechnung_Strom_Rechnung.pdf",
            company: "Stadtwerke Musterstadt",
            address: &["Stadtwerke Musterstadt GmbH", "Werkstraße 4", "76131 Musterstadt"],
            info: &["Rechnungsnummer: 2025-771203", "Datum: 26.09.2025"],
            subject: "Jahresabrechnung Strom 2024/2025",
            items: &[
                ("Arbeitspreis Strom", "741,10 €"),
                ("Rechnungsbetrag (inkl. 19 % USt)", "38,20 €"),
            ],
            footer: STADTWERKE,
        },
        ArchiveLetter {
            path: "2025/Wohnen/Strom/Abschlag/2025-11-03_Stadtwerke_Musterstadt_-_Abschlagsaenderung.pdf",
            company: "Stadtwerke Musterstadt",
            address: &["Stadtwerke Musterstadt GmbH", "Werkstraße 4", "76131 Musterstadt"],
            info: &["Datum: 03.11.2025"],
            subject: "Änderung Ihres Abschlags",
            items: &[("Neuer monatlicher Abschlag", "75,00 €")],
            footer: STADTWERKE,
        },
        ArchiveLetter {
            path: "2024/Steuer/2024-10-02_Finanzamt_Musterstadt_-_Einkommensteuerbescheid_2023.pdf",
            company: "Finanzamt Musterstadt",
            address: &["Schlossplatz 1", "76131 Musterstadt"],
            info: &["Steuernummer 35/123/45678", "Datum 02.10.2024"],
            subject: "Bescheid für 2023 über Einkommensteuer",
            items: &[("festgesetzt werden", "7.902,00 €")],
            footer: "Finanzamt Musterstadt",
        },
        ArchiveLetter {
            path: "2025/Versicherungen/Krankenkasse/2025-02-10_Krankenkasse_Sued_-_Beitragsbescheinigung_2024.pdf",
            company: "Krankenkasse Süd",
            address: &["Postfach 10 20 30", "70100 Stuttgart"],
            info: &["Stuttgart, 10. Februar 2025"],
            subject: "Beitragsbescheinigung für das Jahr 2024",
            items: &[("Krankenversicherung", "4.611,20 €")],
            footer: "Krankenkasse Süd · Körperschaft des öffentlichen Rechts",
        },
        ArchiveLetter {
            path: "2025/Wohnen/2025-09-15_Hausverwaltung_Lindner_-_Nebenkostenabrechnung_2024.pdf",
            company: "Hausverwaltung Lindner",
            address: &["Bahnhofstraße 22", "76137 Musterstadt"],
            info: &["Musterstadt, 15.09.2025"],
            subject: "Nebenkostenabrechnung für das Jahr 2024",
            items: &[("Nachzahlung", "88,10 €")],
            footer: "Hausverwaltung Lindner · Bahnhofstraße 22 · 76137 Musterstadt",
        },
        ArchiveLetter {
            path: "alt/Scan vom März 2019.pdf",
            company: "Irgendwer GmbH",
            address: &["Weg 1", "12345 Ort"],
            info: &["Datum: 01.03.2019"],
            subject: "Alter Brief",
            items: &[],
            footer: "Irgendwer GmbH",
        },
    ];
    for l in letters {
        let mut p = letter(l.company, 18.0, l.address, l.info);
        p.text(LEFT, 310.0, 12.0, true, l.subject);
        let y = p.para(340.0, &["Sehr geehrter Herr Beispiel,"]);
        p.table(y, l.items, true);
        footer(&mut p, l.footer, 1, 1);
        let path = dir.join(l.path);
        std::fs::create_dir_all(path.parent().unwrap())?;
        save(vec![p], &path)?;
        println!("{}", path.display());
    }
    Ok(())
}
