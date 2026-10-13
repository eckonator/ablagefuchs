// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ablagefuchs: gescannte PDFs sichten und sinnvoll umbenennen.
//! Stand M0: leeres Fenster, aber schon mit der Grafikwahl aus FadeWerk DJ (Windows nur DirectX 12,
//! Ausweichweg OpenGL). Einstellungen und Startmarkierung folgen in M2.

// Unter Windows im Release kein zusätzliches Konsolenfenster öffnen
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::Cell;
use std::rc::Rc;

use eframe::egui;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // AF_RENDERER=glow|wgpu erzwingt das Grafik-Backend
    let renderer = match std::env::var("AF_RENDERER").as_deref() {
        Ok("glow" | "opengl") => eframe::Renderer::Glow,
        _ => eframe::Renderer::Wgpu,
    };
    let app_created = Rc::new(Cell::new(false));
    let mut result = run(renderer, &app_created);
    // Scheitert wgpu schon beim Einrichten (Treiber, virtuelle Maschinen, Remote Desktop): OpenGL versuchen
    if let Err(e) = &result
        && renderer == eframe::Renderer::Wgpu
        && !app_created.get()
    {
        log::error!("Grafik über wgpu nicht verfügbar ({e}), versuche OpenGL");
        result = run(eframe::Renderer::Glow, &app_created);
    }
    result
}

fn run(renderer: eframe::Renderer, app_created: &Rc<Cell<bool>>) -> eframe::Result {
    log::info!("Grafik: {renderer}");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ablagefuchs")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([1100.0, 650.0]),
        renderer,
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    let created = Rc::clone(app_created);
    eframe::run_native(
        "Ablagefuchs",
        options,
        Box::new(move |_cc| {
            created.set(true);
            Ok(Box::new(AblagefuchsApp))
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

struct AblagefuchsApp;

impl eframe::App for AblagefuchsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                ui.heading(format!("Ablagefuchs {} – Grundgerüst", env!("CARGO_PKG_VERSION")));
            });
        });
    }
}
