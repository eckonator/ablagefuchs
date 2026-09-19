// SPDX-License-Identifier: MIT OR Apache-2.0
//! Schriften: die egui-Standardschrift plus ein Ausschnitt aus DejaVu Sans für Symbole, die ihr fehlen
//! (↵ ↓ ↑ ⌥ ⌘ ✔ ⚠ ◀ ▶ …). Lizenz: assets/fonts/DejaVu-LICENSE.txt (Bitstream Vera, frei weitergebbar).

use std::sync::Arc;

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};

pub fn install(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "dejavu-symbole".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/DejaVuSans-Symbole.ttf"
        ))),
    );
    // Als Rückfall hinten anhängen: normale Zeichen kommen weiter aus der Standardschrift
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("dejavu-symbole".into());
    }
    ctx.set_fonts(fonts);
}
