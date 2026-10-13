// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ablagefuchs: gescannte PDFs sichten und sinnvoll umbenennen.

// Unter Windows im Release kein zusätzliches Konsolenfenster öffnen
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cache;
mod devtools;
mod diagnose;
mod file_list;
mod fonts;
mod form;
mod i18n;
mod learning;
mod preview;
mod settings;
mod settings_ui;
mod tags;
mod theme;
mod update;
mod worker;

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use eframe::egui;

use crate::devtools::DevTools;
use crate::settings::{GraphicsSetting, LangSetting, Settings, ThemeSetting};

fn main() -> eframe::Result {
    let dev = DevTools::from_env();
    let isolated = dev.isolated();
    diagnose::init(!isolated);
    let mut settings = Settings::load();
    if let Some(l) = &dev.lang {
        settings.language = if l == "en" { LangSetting::En } else { LangSetting::De };
    }
    if let Some(t) = &dev.theme {
        settings.theme = if t == "dunkel" || t == "dark" {
            ThemeSetting::Dark
        } else {
            ThemeSetting::Light
        };
    }
    // Entwicklerhilfe (nur Debug): Ordner für „Aus einem Ordner lernen“ vorgeben
    if cfg!(debug_assertions)
        && let Some(dir) = std::env::var_os("AF_LERNORDNER")
    {
        settings.learn_folder = Some(PathBuf::from(dir));
    }
    i18n::apply_setting(settings.language);
    // Optional: ein Ordner als Argument wird beim Start geöffnet
    let start_dir = std::env::args_os().nth(1).map(PathBuf::from).filter(|p| p.is_dir());

    // Kam der letzte Start mit wgpu nicht bis zum ersten Bild, ist vermutlich der Grafiktreiber abgestürzt
    // (das kann Rust nicht abfangen): ab jetzt OpenGL
    if settings.graphics == GraphicsSetting::Auto
        && !settings.opengl_fallback
        && std::env::var_os("AF_RENDERER").is_none()
        && diagnose::unfinished_graphics_start().is_some_and(|r| r == "wgpu")
    {
        log::warn!("Der letzte Start mit wgpu kam nicht bis zum ersten Bild, nehme ab jetzt OpenGL");
        settings.opengl_fallback = true;
        settings.save();
    }
    let renderer = match std::env::var("AF_RENDERER").as_deref() {
        Ok("glow" | "opengl") => eframe::Renderer::Glow,
        Ok("wgpu") => eframe::Renderer::Wgpu,
        _ if settings.graphics == GraphicsSetting::OpenGl => eframe::Renderer::Glow,
        _ if settings.graphics == GraphicsSetting::Auto && settings.opengl_fallback => eframe::Renderer::Glow,
        _ => eframe::Renderer::Wgpu,
    };
    let app_created = Rc::new(Cell::new(false));
    let mut dev = Some(dev);
    let mut result = run(renderer, settings.clone(), &mut dev, start_dir.clone(), &app_created);
    // Scheitert wgpu schon beim Einrichten (Treiber, virtuelle Maschinen, Remote Desktop): OpenGL versuchen
    if let Err(e) = &result
        && renderer == eframe::Renderer::Wgpu
        && !app_created.get()
    {
        log::error!("Grafik über wgpu nicht verfügbar ({e}), versuche OpenGL");
        settings.opengl_fallback = true;
        if !isolated {
            settings.save();
        }
        result = run(eframe::Renderer::Glow, settings, &mut dev, start_dir, &app_created);
    }
    if let Err(e) = &result {
        log::error!("Programm beendet mit Fehler: {e}");
        if cfg!(windows) && !app_created.get() {
            diagnose::show_error(&i18n::trf("Das Fenster ließ sich nicht öffnen:\n{}", &[e]));
        }
    }
    result
}

fn run(
    renderer: eframe::Renderer,
    settings: Settings,
    dev: &mut Option<DevTools>,
    start_dir: Option<PathBuf>,
    app_created: &Rc<Cell<bool>>,
) -> eframe::Result {
    log::info!("Grafik: {renderer}");
    diagnose::graphics_starting(&renderer.to_string());
    let size = DevTools::window_size().unwrap_or([1440.0, 900.0]);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ablagefuchs")
            .with_inner_size(size)
            .with_min_inner_size([1100.0, 650.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon-256.png")).unwrap_or_default(),
            ),
        renderer,
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    let created = Rc::clone(app_created);
    let dev = dev.take().unwrap_or_default();
    eframe::run_native(
        "Ablagefuchs",
        options,
        Box::new(move |cc| {
            created.set(true);
            Ok(Box::new(app::App::new(cc, settings, dev, start_dir)))
        }),
    )
}

/// wgpu-Einstellungen. Unter Windows nur DirectX 12: Vulkan-Treiber (etwa Intel UHD 620) stürzten
/// bei FadeWerk DJ beim Start sporadisch ab, ohne dass sich das abfangen ließ. `WGPU_BACKEND` überschreibt das.
fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew, wgpu};
    let mut options = WgpuConfiguration::default();
    if cfg!(windows) {
        let mut setup = WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::DX12);
        options.wgpu_setup = WgpuSetup::CreateNew(setup);
    }
    options
}
