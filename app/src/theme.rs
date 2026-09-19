// SPDX-License-Identifier: MIT OR Apache-2.0
//! Farben. Feldfarben gelten in Hell und Dunkel; Ampelfarben je nach Modus.

use eframe::egui::{Color32, Context, Visuals};

pub const DATUM: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xed);
pub const UNTERNEHMEN: Color32 = Color32::from_rgb(0x1f, 0x9d, 0x55);
pub const THEMA: Color32 = Color32::from_rgb(0xc9, 0x85, 0x00);
pub const RECHNUNG: Color32 = Color32::from_rgb(0xd6, 0x45, 0x45);

fn dark(ctx: &Context) -> bool {
    ctx.global_style().visuals.dark_mode
}

pub fn ok(ctx: &Context) -> Color32 {
    if dark(ctx) {
        Color32::from_rgb(0x3e, 0xc2, 0x7a)
    } else {
        Color32::from_rgb(0x1f, 0x9d, 0x55)
    }
}

pub fn warn(ctx: &Context) -> Color32 {
    if dark(ctx) {
        Color32::from_rgb(0xe3, 0xa7, 0x2f)
    } else {
        Color32::from_rgb(0xb0, 0x74, 0x00)
    }
}

pub fn bad(ctx: &Context) -> Color32 {
    if dark(ctx) {
        Color32::from_rgb(0xef, 0x6b, 0x6b)
    } else {
        Color32::from_rgb(0xc6, 0x35, 0x35)
    }
}

pub fn accent(ctx: &Context) -> Color32 {
    if dark(ctx) {
        Color32::from_rgb(0x5b, 0x8f, 0xf9)
    } else {
        Color32::from_rgb(0x2f, 0x6f, 0xed)
    }
}

pub fn muted(ctx: &Context) -> Color32 {
    ctx.global_style().visuals.weak_text_color()
}

/// Hintergrund hinter der Seitenvorschau („Schreibtisch“).
pub fn desk(ctx: &Context) -> Color32 {
    if dark(ctx) {
        Color32::from_rgb(0x18, 0x18, 0x1b)
    } else {
        Color32::from_rgb(0xe7, 0xe7, 0xec)
    }
}

/// Ampelfarbe für eine Sicherheit 0–100.
pub fn confidence(ctx: &Context, c: u8, threshold: u8) -> Color32 {
    if c >= threshold {
        ok(ctx)
    } else if c >= 70 {
        warn(ctx)
    } else {
        bad(ctx)
    }
}

/// Etwas ruhigere Grundoptik als der egui-Standard.
pub fn apply(ctx: &Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = eframe::egui::vec2(8.0, 6.0);
        style.spacing.button_padding = eframe::egui::vec2(10.0, 5.0);
        let v: &mut Visuals = &mut style.visuals;
        v.widgets.noninteractive.corner_radius = 6.into();
        v.widgets.inactive.corner_radius = 6.into();
        v.widgets.hovered.corner_radius = 6.into();
        v.widgets.active.corner_radius = 6.into();
    });
}
