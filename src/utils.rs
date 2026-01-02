// utils.rs - Funkcje pomocnicze
use anyhow::{anyhow, Context, Result};
use eframe::egui;
use std::path::Path;

/// Ładuje teksturę z pliku
pub fn load_texture_from_path(
    ctx: &egui::Context,
    path: &Path,
    name: &str,
) -> Result<egui::TextureHandle> {
    let data = std::fs::read(path).context("Nie mozna odczytac pliku tekstury")?;
    load_texture_from_memory(ctx, &data, name)
}

/// Ładuje teksturę z pamięci (dane PNG)
pub fn load_texture_from_memory(
    ctx: &egui::Context,
    data: &[u8],
    name: &str,
) -> Result<egui::TextureHandle> {
    let image = image::load_from_memory(data)
        .context("Nie mozna zdekodowac obrazu")?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let pixels = image.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Ok(ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR))
}

/// Oblicza skalowany rozmiar podglądu
pub fn scaled_preview_size(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (max_width, max_width * 9 / 16);
    }
    let aspect = width as f32 / height as f32;
    let out_w = max_width;
    let out_h = (out_w as f32 / aspect) as u32;
    (out_w, out_h)
}

/// Clampuje offset timeline
pub fn clamp_offset(offset: f32, duration: f32, window: f32) -> f32 {
    if duration <= window {
        0.0
    } else {
        offset.clamp(0.0, duration - window)
    }
}

/// Snap time do siatki (obecnie brak snappingu)
pub fn snap_time(time: f32, _zoom: f32) -> f32 {
    time
}
