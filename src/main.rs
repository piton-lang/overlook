#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Overlook: an application for exploring Piton specifications.

mod application;
mod specbase;
#[cfg(test)]
mod testing;
mod ui;

use application::{Application, APP_NAME};
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };

    let directory = application::directory_from_args();
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| {
            let app = Application::new(cc);
            let app = match &directory {
                Some(dir) => app.with_directory(dir),
                None => app,
            };
            Ok(Box::new(app))
        }),
    )
}
