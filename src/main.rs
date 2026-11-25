#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod package_map;
mod analyzer;
mod builder;
mod ui;

use eframe::egui::{self, Visuals};
use ui::Py2ExeApp;

fn main() -> eframe::Result<()> {
    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport = egui::ViewportBuilder::default()
        .with_inner_size([720.0, 680.0])
        .with_min_inner_size([550.0, 500.0])
        .with_title("Halium — Автоматическая компиляция Python в EXE")
        .with_drag_and_drop(true);

    eframe::run_native(
        "Halium",
        native_options,
        Box::new(|cc| {
            // Применяем современную темную тему
            cc.egui_ctx.set_visuals(Visuals::dark());
            Ok(Box::new(Py2ExeApp::default()))
        }),
    )
}
