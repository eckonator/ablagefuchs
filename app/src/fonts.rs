// SPDX-License-Identifier: MIT OR Apache-2.0
//! Schriften: Inter für die Oberfläche (kräftiger als die feine egui-Standardschrift Ubuntu Light),
//! dahinter die egui-Schriften und ein Ausschnitt aus DejaVu Sans für Symbole, die beiden fehlen
//! (↵ ↓ ↑ ⌥ ⌘ ✔ ⚠ ◀ ▶ …). Lizenzen: assets/fonts/Inter-LICENSE.txt (OFL-1.1),
//! assets/fonts/DejaVu-LICENSE.txt (Bitstream Vera, frei weitergebbar).

use std::sync::Arc;

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};

pub fn install(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "dejavu-symbole".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/DejaVuSans-Symbole.ttf"
        ))),
    );
    // Inter vorn: normale Zeichen kommen daraus, Emoji und Symbole weiter aus den Rückfallschriften
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".into());
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("dejavu-symbole".into());
    }
    ctx.set_fonts(fonts);
}
