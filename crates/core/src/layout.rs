// SPDX-License-Identifier: MIT OR Apache-2.0
//! Aus losen Wörtern werden Zeilen in Lesereihenfolge, jeweils mit Zone (Briefkopf, Text, Fußzeile).
//! Wörter mit großem Abstand auf gleicher Höhe (zweite Spalte, Beträge in Tabellen) bilden eigene Zeilen;
//! [`Layout::row`] findet sie wieder.

use serde::Serialize;

use crate::model::{PageText, Rect};

/// Grober Bereich einer Seite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Zone {
    /// Oberste 20 %: Briefkopf, Logo, Absender
    Header,
    Body,
    /// Unterste 10 %: Bankverbindung, USt-IdNr., Seitenzahl
    Footer,
}

#[derive(Debug, Clone, Serialize)]
pub struct Line {
    /// Seitennummer ab 1
    pub page: u16,
    pub text: String,
    pub bbox: Rect,
    /// größte Schrift in der Zeile
    pub font_size: f32,
    /// überwiegend fett
    pub bold: bool,
    pub zone: Zone,
    /// Lage der Wörter: Zeichenbereich in `text` und Rechteck
    #[serde(skip)]
    pub spans: Vec<(std::ops::Range<usize>, Rect)>,
}

impl Line {
    /// Rechteck um den Text im Zeichenbereich `range` (Byte-Offsets in `text`).
    pub fn rect_of(&self, range: std::ops::Range<usize>) -> Rect {
        let mut out: Option<Rect> = None;
        for (r, rect) in &self.spans {
            if r.start < range.end && range.start < r.end {
                out = Some(out.map_or(*rect, |o| o.union(rect)));
            }
        }
        out.unwrap_or(self.bbox)
    }
}

/// Alle Zeilen eines Dokuments plus Seitengrößen.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub lines: Vec<Line>,
    /// (Breite, Höhe) je Seite, Index = Seitennummer − 1
    pub pages: Vec<(f32, f32)>,
}

impl Layout {
    pub fn new(pages: &[PageText]) -> Self {
        let mut layout = Layout::default();
        for page in pages {
            layout.pages.push((page.width, page.height));
            layout.lines.extend(page_lines(page));
        }
        layout
    }

    pub fn page_size(&self, page: u16) -> (f32, f32) {
        self.pages
            .get(usize::from(page).saturating_sub(1))
            .copied()
            .unwrap_or((595.0, 842.0))
    }

    /// Zeilen derselben Tabellen-/Textzeile wie `idx` (gleiche Höhe), von links nach rechts, inklusive `idx`.
    pub fn row(&self, idx: usize) -> Vec<usize> {
        let l = &self.lines[idx];
        let cy = l.bbox.center_y();
        let mut row: Vec<usize> = (0..self.lines.len())
            .filter(|&i| {
                let o = &self.lines[i];
                o.page == l.page && (o.bbox.center_y() - cy).abs() < l.bbox.h.max(o.bbox.h) * 0.5
            })
            .collect();
        row.sort_by(|a, b| self.lines[*a].bbox.x.total_cmp(&self.lines[*b].bbox.x));
        row
    }

    /// Typische Schriftgröße des Fließtexts (Median über alle Zeilen).
    pub fn body_font_size(&self) -> f32 {
        let mut sizes: Vec<f32> = self.lines.iter().map(|l| l.font_size).collect();
        if sizes.is_empty() {
            return 10.0;
        }
        sizes.sort_by(f32::total_cmp);
        sizes[sizes.len() / 2]
    }
}

fn page_lines(page: &PageText) -> Vec<Line> {
    let mut words: Vec<usize> = (0..page.words.len())
        .filter(|&i| !page.words[i].text.trim().is_empty())
        .collect();
    words.sort_by(|&a, &b| {
        let (wa, wb) = (&page.words[a], &page.words[b]);
        wa.bbox
            .center_y()
            .total_cmp(&wb.bbox.center_y())
            .then(wa.bbox.x.total_cmp(&wb.bbox.x))
    });

    // Wörter zu Zeilen gruppieren: gleiche Höhe (Mitte innerhalb der halben Wortgröße)
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for w in words {
        let word = &page.words[w];
        let fits = rows.last().is_some_and(|row| {
            let first = &page.words[row[0]];
            (word.bbox.center_y() - first.bbox.center_y()).abs() < first.bbox.h.max(word.bbox.h) * 0.5
        });
        if fits {
            rows.last_mut().unwrap().push(w);
        } else {
            rows.push(vec![w]);
        }
    }

    let mut lines = Vec::new();
    for mut row in rows {
        row.sort_by(|&a, &b| page.words[a].bbox.x.total_cmp(&page.words[b].bbox.x));
        // Große Lücken (mehr als drei Schriftgrößen) trennen Spalten
        let mut segment: Vec<usize> = Vec::new();
        for w in row {
            if let Some(&last) = segment.last() {
                let (prev, word) = (&page.words[last], &page.words[w]);
                let gap = word.bbox.x - prev.bbox.right();
                if gap > prev.font_size.max(word.font_size) * 3.0 {
                    lines.push(make_line(page, &segment));
                    segment.clear();
                }
            }
            segment.push(w);
        }
        if !segment.is_empty() {
            lines.push(make_line(page, &segment));
        }
    }
    lines.sort_by(|a, b| {
        a.bbox
            .center_y()
            .total_cmp(&b.bbox.center_y())
            .then(a.bbox.x.total_cmp(&b.bbox.x))
    });
    lines
}

fn make_line(page: &PageText, words: &[usize]) -> Line {
    let mut text = String::new();
    let mut spans = Vec::new();
    let mut bbox = page.words[words[0]].bbox;
    let mut font_size: f32 = 0.0;
    let mut bold_chars = 0;
    let mut all_chars = 0;
    for &w in words {
        let word = &page.words[w];
        if !text.is_empty() {
            text.push(' ');
        }
        let start = text.len();
        text.push_str(word.text.trim());
        spans.push((start..text.len(), word.bbox));
        bbox = bbox.union(&word.bbox);
        font_size = font_size.max(word.font_size);
        let n = word.text.chars().count();
        all_chars += n;
        if word.bold {
            bold_chars += n;
        }
    }
    let rel_y = bbox.center_y() / page.height.max(1.0);
    let zone = if rel_y < 0.2 {
        Zone::Header
    } else if rel_y > 0.9 {
        Zone::Footer
    } else {
        Zone::Body
    };
    Line {
        page: page.page,
        text,
        bbox,
        font_size,
        bold: bold_chars * 2 > all_chars,
        zone,
        spans,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Word;

    fn word(text: &str, x: f32, y: f32) -> Word {
        Word {
            text: text.into(),
            bbox: Rect {
                x,
                y,
                w: text.len() as f32 * 5.0,
                h: 10.0,
            },
            font_size: 10.0,
            bold: false,
        }
    }

    #[test]
    fn groups_words_into_lines_and_columns() {
        let page = PageText {
            page: 1,
            width: 595.0,
            height: 842.0,
            words: vec![
                word("Gesamtbetrag", 60.0, 400.0),
                word("29,99", 500.0, 401.0),
                word("€", 530.0, 400.0),
                word("Hallo", 60.0, 380.0),
                word("Welt", 90.0, 381.0),
            ],
        };
        let layout = Layout::new(&[page]);
        let texts: Vec<&str> = layout.lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["Hallo Welt", "Gesamtbetrag", "29,99 €"]);
        assert_eq!(layout.row(1), vec![1, 2]);
    }
}
