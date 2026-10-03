// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hilfen für die Entwicklung, nur in Debug-Builds aktiv:
//!
//! - `AF_DEMO=1` öffnet `demo/Eingang` (vorher `cargo xtask demo-pdfs`)
//! - `AF_SCREENSHOT=/pfad/bild.bmp` speichert nach ~3 s ein Bild des Fensters und beendet die App
//! - `AF_SIZE=1600x1000`, `AF_LANG=de|en`, `AF_THEME=hell|dunkel`
//! - `AF_SETTINGS=allgemein|name|ordner|ueber` öffnet die Einstellungen auf dieser Seite
//! - `AF_FILE=3` wählt die dritte Datei
//! - `AF_WISSEN=/pfad/wissen.json` Wissensdatei für Entwicklerläufe (sonst ohne Wissen); `AF_LERNORDNER=…` Lernordner
//! - `AF_KEYS=enter,enter,cmd+z,alt+r,cmd+down` drückt nacheinander diese Tasten (Ablauf ohne Maus testen);
//!   dazu `text:Abc`, `move:x:y`, `press:x:y`, `release:x:y` (Maus in Punkt, für Klicks und Ziehen), `scroll:-300`
//! - Bei diesen Schaltern wird nichts gespeichert.

use std::io::Write;
use std::path::{Path, PathBuf};

use std::collections::VecDeque;

use eframe::egui::{self, ColorImage, Event, Key, Modifiers, RawInput, ViewportCommand};

const SCREENSHOT_AFTER_FRAMES: u64 = 60;
const SCREENSHOT_AFTER_SECS: f32 = 3.0;

#[derive(Default)]
pub struct DevTools {
    screenshot: Option<PathBuf>,
    pub demo: bool,
    pub lang: Option<String>,
    pub theme: Option<String>,
    pub settings_page: Option<String>,
    pub file: Option<usize>,
    keys: VecDeque<String>,
    key_frame: u64,
    last_key_frame: u64,
    frame: u64,
    started: Option<std::time::Instant>,
    requested: bool,
}

impl DevTools {
    pub fn from_env() -> Self {
        if !cfg!(debug_assertions) {
            return Self::default();
        }
        Self {
            screenshot: std::env::var_os("AF_SCREENSHOT").map(PathBuf::from),
            demo: std::env::var_os("AF_DEMO").is_some(),
            lang: std::env::var("AF_LANG").ok(),
            theme: std::env::var("AF_THEME").ok(),
            settings_page: std::env::var("AF_SETTINGS").ok(),
            file: std::env::var("AF_FILE")
                .ok()
                .and_then(|n| n.parse::<usize>().ok())
                .map(|n| n.saturating_sub(1)),
            keys: std::env::var("AF_KEYS")
                .map(|k| k.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            ..Self::default()
        }
    }

    /// Entwicklerläufe speichern nichts, damit die echten Einstellungen unberührt bleiben.
    pub fn isolated(&self) -> bool {
        self.screenshot.is_some()
            || self.demo
            || self.lang.is_some()
            || self.theme.is_some()
            || self.settings_page.is_some()
            || self.file.is_some()
            || !self.keys.is_empty()
    }

    /// Speist die nächste Taste aus `AF_KEYS` ein: erst nach ~1,5 s (Erkennung fertig), dann alle 30 Bilder.
    pub fn inject_keys(&mut self, raw: &mut RawInput) {
        self.key_frame += 1;
        if self.keys.is_empty() || self.key_frame < 90 || !self.key_frame.is_multiple_of(30) {
            return;
        }
        let Some(spec) = self.keys.pop_front() else { return };
        self.last_key_frame = self.key_frame;
        let pointer = |rest: &str| -> Option<egui::Pos2> {
            let (x, y) = rest.split_once(':')?;
            Some(egui::pos2(x.parse().ok()?, y.parse().ok()?))
        };
        for (prefix, pressed) in [("press:", Some(true)), ("release:", Some(false)), ("move:", None)] {
            if let Some(pos) = spec.strip_prefix(prefix).and_then(pointer) {
                log::info!("AF_KEYS: {spec}");
                raw.events.push(Event::PointerMoved(pos));
                if let Some(pressed) = pressed {
                    raw.events.push(Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
                return;
            }
        }
        // `scroll:dy`: Mausrad an der aktuellen Zeigerposition (negativ = nach unten blättern)
        if let Some(dy) = spec.strip_prefix("scroll:").and_then(|d| d.parse::<f32>().ok()) {
            raw.events.push(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            });
            return;
        }
        if let Some(text) = spec.strip_prefix("text:") {
            raw.events.push(Event::Text(text.to_string()));
            return;
        }
        let mut modifiers = Modifiers::NONE;
        let mut name = spec.as_str();
        while let Some((m, rest)) = name.split_once('+') {
            match m {
                "cmd" => modifiers |= Modifiers::COMMAND,
                "alt" => modifiers |= Modifiers::ALT,
                "shift" => modifiers |= Modifiers::SHIFT,
                _ => {}
            }
            name = rest;
        }
        let key = match name {
            "enter" => Key::Enter,
            "esc" => Key::Escape,
            "down" => Key::ArrowDown,
            "up" => Key::ArrowUp,
            "tab" => Key::Tab,
            other => match Key::from_name(&other.to_uppercase()).or_else(|| Key::from_name(other)) {
                Some(k) => k,
                None => {
                    log::warn!("AF_KEYS: unbekannte Taste {other:?}");
                    return;
                }
            },
        };
        log::info!("AF_KEYS: {spec}");
        for pressed in [true, false] {
            raw.events.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
        }
    }

    /// Ordner mit den Demo-PDFs im Quellbaum.
    pub fn demo_dir() -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../demo/Eingang");
        dir.canonicalize().unwrap_or(dir)
    }

    pub fn window_size() -> Option<[f32; 2]> {
        let s = std::env::var("AF_SIZE").ok().filter(|_| cfg!(debug_assertions))?;
        let (w, h) = s.split_once('x')?;
        Some([w.parse().ok()?, h.parse().ok()?])
    }

    pub fn on_frame(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        self.frame += 1;
        let started = *self.started.get_or_insert_with(std::time::Instant::now);
        // Erst wenn alle eingespeisten Tasten verarbeitet sind (plus etwas Zeit zum Zeichnen)
        if !self.requested
            && self.keys.is_empty()
            && self.key_frame > self.last_key_frame + 20
            && self.frame >= SCREENSHOT_AFTER_FRAMES
            && started.elapsed().as_secs_f32() >= SCREENSHOT_AFTER_SECS
        {
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(Default::default()));
            self.requested = true;
        }
        ctx.request_repaint();
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            match save_bmp(&path, &image) {
                Ok(()) => log::info!("Bildschirmfoto gespeichert: {}", path.display()),
                Err(e) => log::error!("Bildschirmfoto nicht gespeichert: {e}"),
            }
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

/// Schreibt ein unkomprimiertes 24-Bit-BMP.
fn save_bmp(path: &Path, img: &ColorImage) -> std::io::Result<()> {
    let [w, h] = img.size;
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(b"BM")?;
    f.write_all(&(size as u32).to_le_bytes())?;
    f.write_all(&[0; 4])?;
    f.write_all(&54u32.to_le_bytes())?;
    f.write_all(&40u32.to_le_bytes())?;
    f.write_all(&(w as i32).to_le_bytes())?;
    f.write_all(&(h as i32).to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&24u16.to_le_bytes())?;
    f.write_all(&[0; 24])?;
    let pad = vec![0u8; row - w * 3];
    for y in (0..h).rev() {
        for x in 0..w {
            let c = img.pixels[y * w + x];
            f.write_all(&[c.b(), c.g(), c.r()])?;
        }
        f.write_all(&pad)?;
    }
    f.flush()
}
