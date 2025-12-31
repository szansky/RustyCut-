use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::egui;
use egui::load::SizedTexture;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::Read;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, atomic::AtomicU64,
};
use std::thread;

fn main() -> Result<()> {
    let options = eframe::NativeOptions::default();
    if let Err(err) = eframe::run_native(
        "Rust Video Editor (Simple)",
        options,
        Box::new(|_cc| Box::new(VideoEditorApp::default())),
    ) {
        return Err(anyhow!(err.to_string()));
    }
    Ok(())
}

struct VideoEditorApp {
    input_path: String,
    output_path: String,
    clips: Vec<Clip>,
    duration: f32,
    video_width: u32,
    video_height: u32,
    video_fps: f32,
    playhead: f32,
    mark_in: Option<f32>,
    mark_out: Option<f32>,
    selected_clip: Option<usize>,
    preview_texture: Option<egui::TextureHandle>,
    waveform_texture: Option<egui::TextureHandle>,
    thumb_textures: Vec<egui::TextureHandle>,
    thumb_times: Vec<f32>,
    temp_dir: Option<PathBuf>,
    last_preview_time: Option<Instant>,
    last_preview_playhead: f32,
    is_playing: bool,
    last_tick: Option<Instant>,
    playback_thread: Option<thread::JoinHandle<()>>,
    playback_stop: Option<Arc<AtomicBool>>,
    playback_frames: Arc<Mutex<Option<egui::ColorImage>>>,
    audio_thread: Option<thread::JoinHandle<()>>,
    audio_stop: Option<Arc<AtomicBool>>,
    audio_stream: Option<cpal::Stream>,
    audio_buffer: Arc<Mutex<VecDeque<i16>>>,
    audio_samples_played: Arc<AtomicU64>,
    audio_sample_rate: u32,
    audio_channels: u16,
    dragging_playhead: bool,
    was_dragging_playhead: bool,
    timeline_zoom: f32,
    timeline_offset: f32,
    last_drag_preview_time: Option<Instant>,
    last_drag_preview_playhead: f32,
    live_drag_preview: bool,
    tool: Tool,
    dragging_timeline: bool,
    dragging_fade: Option<FadeDrag>,
    ripple_delete: bool,
    status: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Clip {
    start: f32,
    end: f32,
    fade_in: f32,
    fade_out: f32,
}

#[derive(Serialize, Deserialize)]
struct ProjectData {
    input_path: String,
    clips: Vec<Clip>,
    duration: f32,
    video_width: u32,
    video_height: u32,
    video_fps: f32,
}

#[derive(Clone, Copy)]
enum FadeKind {
    In,
    Out,
}

#[derive(Clone, Copy)]
struct FadeDrag {
    clip_idx: usize,
    kind: FadeKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Hand,
    Scissors,
}

impl eframe::App for VideoEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut user_seeked = false;

        // Skroty klawiszowe
        if ctx.input(|i| i.key_pressed(egui::Key::A)) {
            self.tool = Tool::Hand;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::B)) {
            self.tool = Tool::Scissors;
        }

        if self.is_playing {
            let now = Instant::now();
            let dt = if let Some(last) = self.last_tick {
                now.duration_since(last).as_secs_f32()
            } else {
                0.0
            };
            self.last_tick = Some(now);
            if self.duration > 0.0 && dt > 0.0 {
                self.playhead = (self.playhead + dt).min(self.duration);
                if self.playhead >= self.duration {
                    self.stop_playback();
                }
            }
            ctx.request_repaint();
        }

        if self.is_playing {
            if let Some(frame) = self.take_latest_frame() {
                if let Some(tex) = &mut self.preview_texture {
                    tex.set(frame, egui::TextureOptions::LINEAR);
                } else {
                    self.preview_texture =
                        Some(ctx.load_texture("preview_playback", frame, egui::TextureOptions::LINEAR));
                }
            }
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Plik", |ui| {
                    if ui.button("Nowy projekt").clicked() {
                        self.input_path.clear();
                        self.output_path.clear();
                        self.clips.clear();
                        self.duration = 0.0;
                        self.playhead = 0.0;
                        self.stop_playback();
                        self.thumb_textures.clear();
                        self.thumb_times.clear();
                        self.preview_texture = None;
                        self.waveform_texture = None;
                        self.status = "Nowy projekt utworzony".to_string();
                        ui.close_menu();
                    }
                    if ui.button("Otworz projekt...").clicked() {
                        self.load_project_dialog(ctx);
                        ui.close_menu();
                    }
                    if ui.button("Zapisz projekt...").clicked() {
                        self.save_project_as();
                        ui.close_menu();
                    }
                });
            });
        });

        // Panel dolny: Timeline
        egui::TopBottomPanel::bottom("timeline_panel")
            .resizable(true)
            .min_height(150.0)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.label("Timeline:");
                    if draw_timeline(ui, self) {
                        user_seeked = true;
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(format!("Playhead: {:.2}s", self.playhead));
                        if ui.button("Mark In").clicked() {
                            self.mark_in = Some(self.playhead);
                        }
                        if ui.button("Mark Out").clicked() {
                            self.mark_out = Some(self.playhead);
                        }
                        if ui.button("Dodaj klip").clicked() {
                            if let (Some(start), Some(end)) = (self.mark_in, self.mark_out) {
                                if end > start {
                                    self.clips.push(Clip {
                                        start,
                                        end,
                                        fade_in: 0.0,
                                        fade_out: 0.0,
                                    });
                                    self.selected_clip = Some(self.clips.len() - 1);
                                    self.status.clear();
                                } else {
                                    self.status = "Mark Out musi byc > Mark In.".to_string();
                                }
                            } else {
                                self.status = "Ustaw Mark In i Mark Out.".to_string();
                            }
                        }
                        if ui.button("Podziel klip").clicked() {
                            if let Some(idx) = self.selected_clip {
                                if let Some(split) = split_clip_at(&mut self.clips, idx, self.playhead) {
                                    self.selected_clip = Some(split);
                                    self.status.clear();
                                } else {
                                    self.status = "Playhead musi byc w srodku klipu.".to_string();
                                }
                            } else {
                                self.status = "Wybierz klip z timeline.".to_string();
                            }
                        }
                        if ui.button("Usun klip").clicked() {
                            if let Some(idx) = self.selected_clip {
                                if idx < self.clips.len() {
                                    self.clips.remove(idx);
                                    self.selected_clip = None;
                                }
                            }
                        }
                    });
                });
            });

        // Panel boczny: Narzedzia
        egui::SidePanel::left("tools_panel")
            .resizable(true)
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.heading("Edytor Video");
                ui.separator();

                ui.label("Plik wejsciowy:");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.input_path);
                    if ui.button("...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            self.input_path = path.display().to_string();
                            self.prepare_media_assets(ctx);
                        }
                    }
                });

                ui.label("Plik wyjsciowy:");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.output_path);
                    if ui.button("...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().save_file() {
                            self.output_path = path.display().to_string();
                        }
                    }
                });

                ui.separator();
                ui.label("Dlugosc (s):");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.duration).clamp_range(0.0..=86400.0));
                });
                ui.horizontal(|ui| {
                    if ui.button("Auto (ffprobe)").clicked() {
                        self.prepare_media_assets(ctx);
                    }
                    if ui.button("Utworz caly klip").clicked() {
                        if self.duration > 0.0 {
                            self.clips.clear();
                            self.clips.push(Clip {
                                start: 0.0,
                                end: self.duration,
                                fade_in: 0.0,
                                fade_out: 0.0,
                            });
                            self.selected_clip = Some(0);
                        } else {
                            self.status = "Ustaw dlugosc zanim utworzysz klip.".to_string();
                        }
                    }
                });
                
                ui.separator();
                ui.label("Narzedzia:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tool, Tool::Hand, "Reka");
                    ui.selectable_value(&mut self.tool, Tool::Scissors, "Nozyczki");
                });
                ui.checkbox(&mut self.live_drag_preview, "Live preview");
                ui.checkbox(&mut self.ripple_delete, "Ripple Delete (Auto-przesuwanie)");

                ui.separator();
                if ui.button("RENDERUJ FILM").clicked() {
                    match render_video(&self.input_path, &self.output_path, &self.clips) {
                        Ok(()) => self.status = "Render zakonczony.".to_string(),
                        Err(err) => self.status = format!("Blad: {err:#}"),
                    }
                }
                
                if !self.status.is_empty() {
                    ui.separator();
                    ui.label(&self.status);
                }
            });

        // Central Panel: Podglad (zajmuje reszte miejsca) + Sterowanie Playback
        egui::CentralPanel::default().show(ctx, |ui| {
            let available_size = ui.available_size();
            let controls_height = 40.0;
            let video_height = (available_size.y - controls_height).max(100.0);
            
            // Obszar wideo
            let video_rect_size = egui::vec2(available_size.x, video_height);
            let (rect, _) = ui.allocate_exact_size(video_rect_size, egui::Sense::hover());
            
            // Rysujemy czarne tlo
            ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);
            
            if let Some(texture) = &self.preview_texture {
                // Obliczamy aspekt wideo zeby narysowac je z zachowaniem proporcji na srodku
                let video_aspect = if self.video_height > 0 {
                    self.video_width as f32 / self.video_height as f32
                } else {
                    16.0 / 9.0
                };
                
                // Fit rect inside available rect maintaining aspect ratio
                let mut draw_width = rect.width();
                let mut draw_height = rect.width() / video_aspect;
                
                if draw_height > rect.height() {
                    draw_height = rect.height();
                    draw_width = draw_height * video_aspect;
                }
                
                let draw_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(draw_width, draw_height));

                // Software Fade Logic
                let mut alpha = 1.0;
                if let Some(clip) = self.clips.iter().find(|c| self.playhead >= c.start && self.playhead < c.end) {
                        let rel = self.playhead - clip.start;
                        if rel < clip.fade_in {
                            alpha = rel / clip.fade_in.max(0.001);
                        }
                        let end_rel = clip.end - self.playhead;
                        if end_rel < clip.fade_out {
                            alpha = alpha.min(end_rel / clip.fade_out.max(0.001));
                        }
                }
                let alpha = alpha.clamp(0.0, 1.0);
                let tint = egui::Color32::from_white_alpha((alpha * 255.0) as u8);

                let image = egui::Image::new(SizedTexture::new(texture.id(), draw_rect.size())).tint(tint);
                egui::Image::paint_at(&image, ui, draw_rect);
            } else {
                 ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Brak podgladu",
                    egui::TextStyle::Heading.resolve(ui.style()),
                    egui::Color32::GRAY,
                );
            }
            
            // Pasek kontrolny playera pod wideo
            ui.allocate_ui(egui::vec2(available_size.x, controls_height), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.horizontal(|ui| {
                        // <<
                        if ui.button("⏮").clicked() {
                            self.playhead = 0.0;
                            self.stop_playback();
                            user_seeked = true;
                        }
                        // Stop
                        if ui.add_enabled(self.is_playing, egui::Button::new("⏹")).clicked() {
                            self.stop_playback();
                        }
                        // Play
                        if ui.add_enabled(!self.is_playing, egui::Button::new("▶")).clicked() {
                            if self.duration > 0.0 {
                                self.is_playing = true;
                                self.last_tick = Some(Instant::now());
                                if let Err(err) = self.start_playback() {
                                    self.status = format!("Blad odtwarzania: {err:#}");
                                    self.is_playing = false;
                                }
                            }
                        }
                        // >>
                        if ui.button("⏭").clicked() {
                            self.playhead = self.duration.max(0.0);
                            self.stop_playback();
                            user_seeked = true;
                        }
                    });
                });
            });
        });

        if user_seeked {
            if self.is_playing {
                let _ = self.start_playback();
            } else if !self.dragging_playhead {
                self.maybe_update_preview(ctx);
            }
        }

        if self.was_dragging_playhead && !self.dragging_playhead && !self.is_playing {
            self.maybe_update_preview(ctx);
        }
        if self.dragging_playhead && !self.is_playing && self.live_drag_preview {
            self.maybe_update_preview_drag(ctx);
            ctx.request_repaint();
        }
        self.was_dragging_playhead = self.dragging_playhead;
    }
}

fn render_video(input_path: &str, output_path: &str, clips: &[Clip]) -> Result<()> {
    let input_path = Path::new(input_path);
    let output_path = Path::new(output_path);

    if clips.is_empty() {
        return Err(anyhow!("Brak fragmentow do zlozenia."));
    }
    if !input_path.exists() {
        return Err(anyhow!("Nie znaleziono pliku wejsciowego."));
    }

    let temp_dir = create_temp_dir().context("Nie mozna utworzyc katalogu tymczasowego")?;
    let mut segment_paths = Vec::with_capacity(clips.len());

    for (idx, clip) in clips.iter().enumerate() {
        let segment_path = temp_dir.join(format!("segment_{idx}.mp4"));
        let start = format!("{:.3}", clip.start);
        let end = format!("{:.3}", clip.end);
        let (vf, af) = build_fade_filters(clip);
        let mut args = vec![
            "-y",
            "-ss",
            start.as_str(),
            "-to",
            end.as_str(),
            "-i",
            input_path
                .to_str()
                .ok_or_else(|| anyhow!("Niepoprawna sciezka wejsciowa"))?,
        ];
        if let Some(filter) = &vf {
            args.push("-vf");
            args.push(filter);
        }
        if let Some(filter) = &af {
            args.push("-af");
            args.push(filter);
        }
        args.extend([
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "18",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            segment_path
                .to_str()
                .ok_or_else(|| anyhow!("Niepoprawna sciezka segmentu"))?,
        ]);
        run_ffmpeg(&args)
        .with_context(|| format!("Nie udalo sie wyciac segmentu {idx}"))?;
        segment_paths.push(segment_path);
    }

    let list_path = temp_dir.join("concat_list.txt");
    let mut list_contents = String::new();
    for path in &segment_paths {
        let escaped = path
            .to_str()
            .ok_or_else(|| anyhow!("Niepoprawna sciezka segmentu"))?
            .replace('\'', "\\'");
        list_contents.push_str(&format!("file '{}'\n", escaped));
    }
    fs::write(&list_path, list_contents).context("Nie mozna zapisac listy segmentow")?;

    run_ffmpeg(&[
        "-y",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
        list_path
            .to_str()
            .ok_or_else(|| anyhow!("Niepoprawna sciezka listy"))?,
        "-c",
        "copy",
        output_path
            .to_str()
            .ok_or_else(|| anyhow!("Niepoprawna sciezka wyjsciowa"))?,
    ])
    .context("Nie udalo sie polaczyc segmentow")?;

    Ok(())
}

fn draw_timeline(ui: &mut egui::Ui, app: &mut VideoEditorApp) -> bool {
    let desired_height = 160.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), desired_height),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(30));

    if app.duration <= 0.0 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Brak dlugosci materialu",
            egui::TextStyle::Body.resolve(ui.style()),
            egui::Color32::from_gray(160),
        );
        return false;
    }

    let video_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 8.0, rect.top() + 8.0),
        egui::pos2(rect.right() - 8.0, rect.center().y - 4.0),
    );
    let audio_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 8.0, rect.center().y + 4.0),
        egui::pos2(rect.right() - 8.0, rect.bottom() - 8.0),
    );

    painter.rect_filled(video_rect, 4.0, egui::Color32::from_gray(40));
    painter.rect_filled(audio_rect, 4.0, egui::Color32::from_gray(35));

    let left = rect.left() + 8.0;
    let right = rect.right() - 8.0;
    let width = (right - left).max(1.0);
    let min_zoom = width / app.duration.max(0.01);
    if app.timeline_zoom <= 0.0 {
        app.timeline_zoom = min_zoom;
    }
    let max_zoom = 800.0;
    app.timeline_zoom = app.timeline_zoom.clamp(min_zoom, max_zoom);
    let window = width / app.timeline_zoom;
    app.timeline_offset = clamp_offset(app.timeline_offset, app.duration, window);

    if response.hovered() {
        let (scroll_y, scroll_x, modifiers) = ui.ctx().input(|i| {
            (
                i.smooth_scroll_delta.y,
                i.smooth_scroll_delta.x,
                i.modifiers,
            )
        });
        if scroll_y.abs() > 0.0 {
            let zoom_factor = if scroll_y > 0.0 { 1.1 } else { 0.9 };
            let mouse_x = ui.ctx().pointer_latest_pos().map(|p| p.x).unwrap_or(left);
            let t_at_mouse =
                app.timeline_offset + ((mouse_x - left) / app.timeline_zoom).clamp(0.0, window);
            app.timeline_zoom = (app.timeline_zoom * zoom_factor).clamp(min_zoom, max_zoom);
            let new_window = width / app.timeline_zoom;
            app.timeline_offset =
                (t_at_mouse - (mouse_x - left) / app.timeline_zoom).clamp(0.0, app.duration - new_window);
        } else if modifiers.shift && scroll_x.abs() > 0.0 {
            let delta = -scroll_x / app.timeline_zoom;
            app.timeline_offset = clamp_offset(app.timeline_offset + delta, app.duration, window);
        }
    }

    if let Some(texture) = &app.waveform_texture {
        let u0 = (app.timeline_offset / app.duration).clamp(0.0, 1.0);
        let u1 = ((app.timeline_offset + window) / app.duration).clamp(0.0, 1.0);
        painter.image(
            texture.id(),
            audio_rect,
            egui::Rect::from_min_max(egui::pos2(u0, 0.0), egui::pos2(u1, 1.0)),
            egui::Color32::WHITE,
        );
    } else {
        painter.text(
            audio_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Brak waveform",
            egui::TextStyle::Body.resolve(ui.style()),
            egui::Color32::from_gray(140),
        );
    }

    if !app.thumb_textures.is_empty() && app.duration > 0.0 {
        let chunk = app.duration / app.thumb_textures.len().max(1) as f32;
        let thumb_w = app.timeline_zoom * chunk;
        for (idx, texture) in app.thumb_textures.iter().enumerate() {
            let t = app.thumb_times[idx];
            let x0 = left + (t - chunk * 0.5 - app.timeline_offset) * app.timeline_zoom;
            let x1 = x0 + thumb_w;
            if x1 < video_rect.left() || x0 > video_rect.right() {
                continue;
            }
            let thumb_rect = egui::Rect::from_min_max(
                egui::pos2(x0.max(video_rect.left()), video_rect.top()),
                egui::pos2(x1.min(video_rect.right()), video_rect.bottom()),
            );
            painter.image(
                texture.id(),
                thumb_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    } else {
        painter.text(
            video_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Brak miniatur",
            egui::TextStyle::Body.resolve(ui.style()),
            egui::Color32::from_gray(140),
        );
    }

    let hover_pos = ui.ctx().pointer_latest_pos();
    let mut hover_fade: Option<FadeDrag> = None;
    let handle_size = 20.0;

    let mut remove_clip_idx = None;

    for (idx, clip) in app.clips.iter().enumerate() {
        let start_x = left + (clip.start - app.timeline_offset) * app.timeline_zoom;
        let end_x = left + (clip.end - app.timeline_offset) * app.timeline_zoom;
        let video_clip_rect = egui::Rect::from_min_max(
            egui::pos2(start_x, video_rect.top()),
            egui::pos2(end_x, video_rect.bottom()),
        );
        let audio_clip_rect = egui::Rect::from_min_max(
            egui::pos2(start_x, audio_rect.top()),
            egui::pos2(end_x, audio_rect.bottom()),
        );

        // Interaction & Context Menu
        let interact_rect = video_clip_rect.union(audio_clip_rect);
        let response = ui.interact(interact_rect, ui.id().with("clip_interact").with(idx), egui::Sense::click());
        
        if response.clicked() {
            app.selected_clip = Some(idx);
        }
        response.context_menu(|ui| {
            if ui.button("Usun").clicked() {
                remove_clip_idx = Some(idx);
                ui.close_menu();
            }
            ui.label(if app.ripple_delete { "(Ripple On)" } else { "(Ripple Off)" });
        });

        let color = if Some(idx) == app.selected_clip {
            egui::Color32::from_rgb(80, 170, 255)
        } else {
            egui::Color32::from_rgb(70, 120, 90)
        };
        painter.rect_stroke(video_clip_rect, 4.0, egui::Stroke::new(2.0, color));
        painter.rect_stroke(audio_clip_rect, 4.0, egui::Stroke::new(2.0, color));

        let fade_in_w = (clip.fade_in * app.timeline_zoom).max(0.0);
        let fade_out_w = (clip.fade_out * app.timeline_zoom).max(0.0);
        if fade_in_w > 0.0 {
            let rect_in_v = egui::Rect::from_min_max(
                egui::pos2(video_clip_rect.left(), video_clip_rect.top()),
                egui::pos2((video_clip_rect.left() + fade_in_w).min(video_clip_rect.right()), video_clip_rect.bottom()),
            );
            let rect_in_a = egui::Rect::from_min_max(
                egui::pos2(audio_clip_rect.left(), audio_clip_rect.top()),
                egui::pos2((audio_clip_rect.left() + fade_in_w).min(audio_clip_rect.right()), audio_clip_rect.bottom()),
            );
            let fill = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 28);
            let tri_v = vec![
                egui::pos2(rect_in_v.left(), rect_in_v.bottom()),
                egui::pos2(rect_in_v.left(), rect_in_v.top()),
                egui::pos2(rect_in_v.right(), rect_in_v.top()),
            ];
            let tri_a = vec![
                egui::pos2(rect_in_a.left(), rect_in_a.bottom()),
                egui::pos2(rect_in_a.left(), rect_in_a.top()),
                egui::pos2(rect_in_a.right(), rect_in_a.top()),
            ];
            painter.add(egui::Shape::convex_polygon(tri_v, fill, egui::Stroke::NONE));
            painter.add(egui::Shape::convex_polygon(tri_a, fill, egui::Stroke::NONE));
        }
        if fade_out_w > 0.0 {
            let rect_out_v = egui::Rect::from_min_max(
                egui::pos2((video_clip_rect.right() - fade_out_w).max(video_clip_rect.left()), video_clip_rect.top()),
                egui::pos2(video_clip_rect.right(), video_clip_rect.bottom()),
            );
            let rect_out_a = egui::Rect::from_min_max(
                egui::pos2((audio_clip_rect.right() - fade_out_w).max(audio_clip_rect.left()), audio_clip_rect.top()),
                egui::pos2(audio_clip_rect.right(), audio_clip_rect.bottom()),
            );
            let fill = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 28);
            let tri_v = vec![
                egui::pos2(rect_out_v.right(), rect_out_v.bottom()),
                egui::pos2(rect_out_v.right(), rect_out_v.top()),
                egui::pos2(rect_out_v.left(), rect_out_v.top()),
            ];
            let tri_a = vec![
                egui::pos2(rect_out_a.right(), rect_out_a.bottom()),
                egui::pos2(rect_out_a.right(), rect_out_a.top()),
                egui::pos2(rect_out_a.left(), rect_out_a.top()),
            ];
            painter.add(egui::Shape::convex_polygon(tri_v, fill, egui::Stroke::NONE));
            painter.add(egui::Shape::convex_polygon(tri_a, fill, egui::Stroke::NONE));
        }

        let handle_in_v = egui::pos2(
            video_clip_rect.left() + fade_in_w,
            video_clip_rect.top(),
        );
        let handle_out_v = egui::pos2(
            video_clip_rect.right() - fade_out_w,
            video_clip_rect.top(),
        );
        let handle_in_a = egui::pos2(
            audio_clip_rect.left() + fade_in_w,
            audio_clip_rect.top(),
        );
        let handle_out_a = egui::pos2(
            audio_clip_rect.right() - fade_out_w,
            audio_clip_rect.top(),
        );
        let handle_hit_v_in = egui::Rect::from_center_size(handle_in_v, egui::vec2(handle_size, handle_size));
        let handle_hit_v_out = egui::Rect::from_center_size(handle_out_v, egui::vec2(handle_size, handle_size));
        let handle_hit_a_in = egui::Rect::from_center_size(handle_in_a, egui::vec2(handle_size, handle_size));
        let handle_hit_a_out = egui::Rect::from_center_size(handle_out_a, egui::vec2(handle_size, handle_size));

        if let Some(pos) = hover_pos {
            if handle_hit_v_in.contains(pos) {
                hover_fade = Some(FadeDrag { clip_idx: idx, kind: FadeKind::In });
            } else if handle_hit_v_out.contains(pos) {
                hover_fade = Some(FadeDrag { clip_idx: idx, kind: FadeKind::Out });
            } else if handle_hit_a_in.contains(pos) {
                hover_fade = Some(FadeDrag { clip_idx: idx, kind: FadeKind::In });
            } else if handle_hit_a_out.contains(pos) {
                hover_fade = Some(FadeDrag { clip_idx: idx, kind: FadeKind::Out });
            }
        }

        let dot = egui::Color32::from_gray(230);
        painter.circle_filled(handle_in_v, handle_size * 0.25, dot);
        painter.circle_filled(handle_out_v, handle_size * 0.25, dot);
        painter.circle_filled(handle_in_a, handle_size * 0.25, dot);
        painter.circle_filled(handle_out_a, handle_size * 0.25, dot);
    }

    let play_x = left + (app.playhead - app.timeline_offset) * app.timeline_zoom;
    let hover_hit = hover_pos
        .map(|pos| rect.contains(pos) && (pos.x - play_x).abs() <= 10.0)
        .unwrap_or(false);
    if let Some(fade) = hover_fade.or(app.dragging_fade) {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::None);
        if let Some(pos) = ui.ctx().pointer_latest_pos() {
            let size = 12.0;
            // Rysujemy wlasny kursor (trojkat symbolizujacy narastanie/zanikanie)
            let points = match fade.kind {
                FadeKind::In => vec![
                    pos + egui::vec2(-size, size),
                    pos + egui::vec2(-size, -size),
                    pos + egui::vec2(size, -size),
                ],
                FadeKind::Out => vec![
                    pos + egui::vec2(size, size),
                    pos + egui::vec2(size, -size),
                    pos + egui::vec2(-size, -size),
                ],
            };
            // Cien pod kursorem dla lepszej widocznosci
            painter.add(egui::Shape::convex_polygon(
                points.iter().map(|p| *p + egui::vec2(1.0, 1.0)).collect(),
                egui::Color32::from_black_alpha(100),
                egui::Stroke::NONE,
            ));
            // Wlasciwy kursor
            painter.add(egui::Shape::convex_polygon(
                points,
                egui::Color32::WHITE,
                egui::Stroke::new(1.5, egui::Color32::BLACK),
            ));
        }
    } else if app.tool == Tool::Scissors && response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Crosshair);
    } else if hover_hit || app.dragging_playhead {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::ResizeHorizontal);
    } else if response.hovered() && app.tool == Tool::Hand {
        ui.output_mut(|o| {
            o.cursor_icon = if app.dragging_timeline {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            };
        });
    }
    painter.line_segment(
        [
            egui::pos2(play_x, rect.top() + 4.0),
            egui::pos2(play_x, rect.bottom() - 4.0),
        ],
        egui::Stroke::new(
            if hover_hit || app.dragging_playhead { 3.0 } else { 2.0 },
            egui::Color32::from_rgb(240, 80, 80),
        ),
    );

    let mut changed = false;
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let hit = (pos.x - play_x).abs() <= 10.0;
            if let Some(fade_drag) = hover_fade {
                app.dragging_fade = Some(fade_drag);
                app.dragging_playhead = false;
                app.dragging_timeline = false;
            } else if app.tool == Tool::Hand && hit {
                app.dragging_playhead = true;
                app.dragging_timeline = false;
            } else if app.tool == Tool::Hand {
                app.dragging_timeline = true;
                app.dragging_playhead = false;
            } else {
                app.dragging_playhead = false;
                app.dragging_timeline = false;
            }
        }
    }
    if response.drag_stopped() {
        app.dragging_playhead = false;
        app.dragging_timeline = false;
        app.dragging_fade = None;
    }

    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let mut selected = None;
            for (idx, clip) in app.clips.iter().enumerate() {
                let start_x = left + (clip.start - app.timeline_offset) * app.timeline_zoom;
                let end_x = left + (clip.end - app.timeline_offset) * app.timeline_zoom;
                if pos.x >= start_x && pos.x <= end_x {
                    selected = Some(idx);
                    break;
                }
            }
            let t = app.timeline_offset + ((pos.x - left) / app.timeline_zoom).clamp(0.0, window);
            if let Some(fade_drag) = app.dragging_fade {
                if let Some(clip) = app.clips.get_mut(fade_drag.clip_idx) {
                    let duration = (clip.end - clip.start).max(0.0);
                    let t = t.clamp(clip.start, clip.end);
                    match fade_drag.kind {
                        FadeKind::In => {
                            let max = (duration - clip.fade_out).max(0.0);
                            clip.fade_in = (t - clip.start).max(0.0).min(max);
                        }
                        FadeKind::Out => {
                            let max = (duration - clip.fade_in).max(0.0);
                            clip.fade_out = (clip.end - t).max(0.0).min(max);
                        }
                    }
                    app.selected_clip = Some(fade_drag.clip_idx);
                    changed = true;
                }
            } else if response.clicked() {
                if app.tool == Tool::Scissors {
                    if hover_fade.is_some() {
                        app.selected_clip = selected;
                        changed = true;
                        return changed;
                    }
                    let by_time = app
                        .clips
                        .iter()
                        .position(|clip| t > clip.start && t < clip.end);
                    if let Some(idx) = selected.or(by_time) {
                        if let Some(split) = split_clip_at(&mut app.clips, idx, t) {
                            app.selected_clip = Some(split);
                            app.playhead = t;
                            app.status.clear();
                            changed = true;
                        } else {
                            app.status = "Nie mozna uciac na granicy klipu.".to_string();
                        }
                    } else {
                        app.status = "Brak klipu pod kursorem.".to_string();
                    }
                } else {
                    app.selected_clip = selected;
                    app.playhead = snap_time(t, app.timeline_zoom);
                    changed = true;
                }
            } else if app.dragging_playhead {
                app.playhead = snap_time(t, app.timeline_zoom);
                changed = true;
            } else if app.dragging_timeline && app.tool == Tool::Hand {
                let delta = ui.ctx().input(|i| i.pointer.delta()).x;
                if delta.abs() > 0.0 {
                    app.timeline_offset =
                        clamp_offset(app.timeline_offset - delta / app.timeline_zoom, app.duration, window);
                    changed = true;
                }
            }
        }
    }

    if app.is_playing {
        let margin = window * 0.1;
        if app.playhead < app.timeline_offset + margin {
            app.timeline_offset = clamp_offset(app.playhead - margin, app.duration, window);
        } else if app.playhead > app.timeline_offset + window - margin {
            app.timeline_offset =
                clamp_offset(app.playhead - (window - margin), app.duration, window);
        }
    }

    changed
}

fn split_clip_at(clips: &mut Vec<Clip>, idx: usize, t: f32) -> Option<usize> {
    let clip = clips.get(idx)?;
    if t <= clip.start || t >= clip.end {
        return None;
    }
    let right = Clip {
        start: t,
        end: clip.end,
        fade_in: 0.0,
        fade_out: clip.fade_out,
    };
    clips[idx].end = t;
    clips[idx].fade_out = 0.0;
    clips.insert(idx + 1, right);
    Some(idx + 1)
}

fn build_fade_filters(clip: &Clip) -> (Option<String>, Option<String>) {
    let duration = (clip.end - clip.start).max(0.0);
    if duration <= 0.0 {
        return (None, None);
    }
    let mut fade_in = clip.fade_in.max(0.0);
    let mut fade_out = clip.fade_out.max(0.0);
    if fade_in + fade_out > duration {
        let scale = duration / (fade_in + fade_out).max(0.001);
        fade_in *= scale;
        fade_out *= scale;
    }

    let mut vf_parts = Vec::new();
    let mut af_parts = Vec::new();
    if fade_in > 0.0 {
        vf_parts.push(format!("fade=t=in:st=0:d={:.3}", fade_in));
        af_parts.push(format!("afade=t=in:st=0:d={:.3}", fade_in));
    }
    if fade_out > 0.0 {
        let start = (duration - fade_out).max(0.0);
        vf_parts.push(format!("fade=t=out:st={:.3}:d={:.3}", start, fade_out));
        af_parts.push(format!("afade=t=out:st={:.3}:d={:.3}", start, fade_out));
    }

    let vf = if vf_parts.is_empty() {
        None
    } else {
        Some(vf_parts.join(","))
    };
    let af = if af_parts.is_empty() {
        None
    } else {
        Some(af_parts.join(","))
    };
    (vf, af)
}

fn generate_frame_memory(input: &str, time: f32, width: u32, height: i32) -> Result<Vec<u8>> {
    let width_str = if width == 0 { "-1".to_string() } else { width.to_string() };
    let height_str = if height == 0 { "-1".to_string() } else { height.to_string() };

    let output = Command::new("ffmpeg")
        .args([
            "-y",
            "-ss",
            &format!("{:.3}", time.max(0.0)),
            "-i",
            input,
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={width_str}:{height_str}"),
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "-",
        ])
        .output()
        .context("Nie mozna uruchomic ffmpeg dla frame memory")?;

    if !output.status.success() {
        return Err(anyhow!("ffmpeg frame error: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(output.stdout)
}

fn clamp_offset(offset: f32, duration: f32, window: f32) -> f32 {
    if duration <= window {
        0.0
    } else {
        offset.clamp(0.0, duration - window)
    }
}

fn snap_time(time: f32, _zoom: f32) -> f32 {
    time
}

fn run_ffmpeg(args: &[&str]) -> Result<()> {
    let output = Command::new("ffmpeg")
        .args(args)
        .output()
        .context("Nie mozna uruchomic ffmpeg (sprawdz PATH)")?;
    if !output.status.success() {
        return Err(anyhow!(
            "ffmpeg zwrocil blad: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn create_temp_dir() -> Result<PathBuf> {
    let base = std::env::temp_dir();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = base.join(format!("rust_editor_video_{nonce}"));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn get_video_info_ffprobe(path: &str) -> Result<(f32, u32, u32, f32)> {
    if path.trim().is_empty() {
        return Err(anyhow!("Brak pliku wejsciowego"));
    }
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "format=duration:stream=width,height,r_frame_rate",
            "-of",
            "default=noprint_wrappers=1:nokey=0",
            path,
        ])
        .output()
        .context("Nie mozna uruchomic ffprobe (sprawdz PATH)")?;
    if !output.status.success() {
        return Err(anyhow!(
            "ffprobe zwrocil blad: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut duration = None;
    let mut width = None;
    let mut height = None;
    let mut fps = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("duration=") {
            duration = value.trim().parse::<f32>().ok();
        } else if let Some(value) = line.strip_prefix("width=") {
            width = value.trim().parse::<u32>().ok();
        } else if let Some(value) = line.strip_prefix("height=") {
            height = value.trim().parse::<u32>().ok();
        } else if let Some(value) = line.strip_prefix("r_frame_rate=") {
            fps = parse_fps(value.trim());
        }
    }
    let duration = duration.context("Nie mozna odczytac dlugosci")?;
    let width = width.context("Nie mozna odczytac szerokosci")?;
    let height = height.context("Nie mozna odczytac wysokosci")?;
    let fps = fps.unwrap_or(30.0);
    Ok((duration, width, height, fps))
}

impl VideoEditorApp {
    fn save_project_as(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Rust Video Editor Project", &["rev", "json"])
            .save_file() 
        {
            let data = ProjectData {
                input_path: self.input_path.clone(),
                clips: self.clips.clone(),
                duration: self.duration,
                video_width: self.video_width,
                video_height: self.video_height,
                video_fps: self.video_fps,
            };
            match serde_json::to_string_pretty(&data) {
                Ok(json) => {
                    if let Err(e) = fs::write(path, json) {
                        self.status = format!("Blad zapisu projektu: {e}");
                    } else {
                        self.status = "Projekt zapisany.".to_string();
                    }
                }
                Err(e) => {
                    self.status = format!("Blad serializacji: {e}");
                }
            }
        }
    }

    fn load_project_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Rust Video Editor Project", &["rev", "json"])
            .pick_file() 
        {
            if let Ok(content) = fs::read_to_string(&path) {
                match serde_json::from_str::<ProjectData>(&content) {
                    Ok(data) => {
                        self.input_path = data.input_path;
                        self.clips = data.clips;
                        self.duration = data.duration;
                        self.video_width = data.video_width;
                        self.video_height = data.video_height;
                        self.video_fps = data.video_fps;
                        
                        // Reset stanu UI
                        self.playhead = 0.0;
                        self.selected_clip = None;
                        self.stop_playback();
                        
                        // Przywrocenie zasobow (podglady, waveform)
                        if !self.input_path.is_empty() {
                            // Tutaj musimy byc ostrozni, bo prepare_media_assets resetuje clips.
                            // Ale w mojej implementacji prepare_media_assets resetuje clips TYLKO jesli byly puste.
                            // Sprawdzmy to.
                            // W aktualnym kodzie: if self.clips.is_empty() ...
                            // Zatem jesli wczytamy clips, to prepare_media_assets ich nie usunie.
                            self.prepare_media_assets(ctx);
                        }
                        self.status = "Projekt wczytany.".to_string();
                    }
                    Err(e) => {
                        self.status = format!("Blad parsowania projektu: {e}");
                    }
                }
            } else {
                self.status = "Blad odczytu pliku projektu.".to_string();
            }
        }
    }

    fn build_playback_filters(&self, start_time: f32) -> (Option<String>, Option<String>) {
        let mut vf_list = Vec::new();
        let mut af_list = Vec::new();
        
        for clip in &self.clips {
             // Fade In
             if clip.fade_in > 0.0 {
                 let rel_start = clip.start - start_time;
                 // Jesli playback startuje po poczatku fade'a, nie mozemy zaaplikowac filtra z ujemnym czasem.
                 // Pomijamy efekt w tym przypadku (bedzie hard cut), co zapobiega crashowi ffmpeg.
                 if rel_start >= 0.0 {
                     vf_list.push(format!("fade=t=in:st={:.3}:d={:.3}", rel_start, clip.fade_in));
                     af_list.push(format!("afade=t=in:st={:.3}:d={:.3}", rel_start, clip.fade_in));
                 }
             }
             
             // Fade Out
             if clip.fade_out > 0.0 {
                 let fade_out_start = clip.end - clip.fade_out;
                 let rel_out = fade_out_start - start_time;
                 if rel_out >= 0.0 {
                     vf_list.push(format!("fade=t=out:st={:.3}:d={:.3}", rel_out, clip.fade_out));
                     af_list.push(format!("afade=t=out:st={:.3}:d={:.3}", rel_out, clip.fade_out));
                 }
             }
        }
        
        let vf = if vf_list.is_empty() { None } else { Some(vf_list.join(",")) };
        let af = if af_list.is_empty() { None } else { Some(af_list.join(",")) };
        (vf, af)
    }

    fn prepare_media_assets(&mut self, ctx: &egui::Context) {
        match get_video_info_ffprobe(&self.input_path) {
            Ok((duration, width, height, fps)) => {
                self.duration = duration.max(0.0);
                self.video_width = width;
                self.video_height = height;
                self.video_fps = fps;
                self.playhead = 0.0;
                self.last_preview_playhead = -1.0;
                self.stop_playback();
                self.mark_in = None;
                self.mark_out = None;
                if self.clips.is_empty() && self.duration > 0.0 {
                    self.clips.push(Clip {
                        start: 0.0,
                        end: self.duration,
                        fade_in: 0.0,
                        fade_out: 0.0,
                    });
                    self.selected_clip = Some(0);
                } else {
                    self.selected_clip = None;
                }
                self.timeline_zoom = 0.0;
                self.timeline_offset = 0.0;
                self.status.clear();
                if let Err(err) = self.ensure_temp_dir() {
                    self.status = format!("Blad temp: {err:#}");
                    return;
                }
                if let Err(err) = self.build_waveform(ctx) {
                    self.status = format!("Blad waveform: {err:#}");
                }
                if let Err(err) = self.build_thumbnails(ctx, 8) {
                    self.status = format!("Blad miniatur: {err:#}");
                }
                self.maybe_update_preview(ctx);
            }
            Err(err) => {
                self.status = format!("Blad ffprobe: {err:#}");
            }
        }
    }

    fn ensure_temp_dir(&mut self) -> Result<()> {
        if self.temp_dir.is_none() {
            self.temp_dir = Some(create_temp_dir()?);
        }
        Ok(())
    }

    fn maybe_update_preview(&mut self, ctx: &egui::Context) {
        if self.input_path.trim().is_empty() || self.duration <= 0.0 {
            return;
        }
        if self.is_playing {
            return;
        }
        let now = Instant::now();
        if let Some(last) = self.last_preview_time {
            if now.duration_since(last).as_millis() < 150 {
                return;
            }
        }
        if (self.playhead - self.last_preview_playhead).abs() < 0.05 {
            return;
        }
        if let Err(err) = self.build_preview(ctx) {
            self.status = format!("Blad podgladu: {err:#}");
        } else {
            self.last_preview_time = Some(now);
            self.last_preview_playhead = self.playhead;
        }
    }

    fn maybe_update_preview_drag(&mut self, ctx: &egui::Context) {
        if self.input_path.trim().is_empty() || self.duration <= 0.0 {
            return;
        }
        let now = Instant::now();
        if let Some(last) = self.last_drag_preview_time {
            if now.duration_since(last).as_millis() < 140 {
                return;
            }
        }
        if (self.playhead - self.last_drag_preview_playhead).abs() < 0.12 {
            return;
        }
        if let Err(err) = self.build_preview_scaled(ctx, 320) {
            self.status = format!("Blad podgladu: {err:#}");
        } else {
            self.last_drag_preview_time = Some(now);
            self.last_drag_preview_playhead = self.playhead;
        }
    }

    fn build_waveform(&mut self, ctx: &egui::Context) -> Result<()> {
        self.ensure_temp_dir()?;
        let temp_dir = self
            .temp_dir
            .as_ref()
            .ok_or_else(|| anyhow!("Brak katalogu temp"))?;
        let wave_path = temp_dir.join("waveform.png");
        generate_waveform(&self.input_path, &wave_path)?;
        let texture = load_texture_from_path(ctx, &wave_path, "waveform")?;
        self.waveform_texture = Some(texture);
        Ok(())
    }

    fn build_preview(&mut self, ctx: &egui::Context) -> Result<()> {
        // Nie potrzebujemy juz pliku tymczasowego do preview
        // Generujemy PNG w pamieci (scale=640:-1)
        let data = generate_frame_memory(&self.input_path, self.playhead, 640, 0)?;
        let texture = load_texture_from_memory(ctx, &data, "preview")?;
        self.preview_texture = Some(texture);
        Ok(())
    }

    fn build_preview_scaled(&mut self, ctx: &egui::Context, max_width: u32) -> Result<()> {
        let data = generate_frame_memory(&self.input_path, self.playhead, max_width, 0)?;
        let texture = load_texture_from_memory(ctx, &data, "preview_drag")?;
        self.preview_texture = Some(texture);
        Ok(())
    }

    fn build_thumbnails(&mut self, ctx: &egui::Context, count: usize) -> Result<()> {
        // Miniatury tez robimy w pamieci, bez zasmiecania dysku
        self.thumb_textures.clear();
        self.thumb_times.clear();
        if self.duration <= 0.0 || count == 0 {
            return Ok(());
        }
        for i in 0..count {
            let t = (i as f32 + 0.5) * (self.duration / count as f32);
            // scale=200:-1
            let data = generate_frame_memory(&self.input_path, t, 200, 0)?;
            let texture = load_texture_from_memory(ctx, &data, &format!("thumb_{i}"))?;
            self.thumb_textures.push(texture);
            self.thumb_times.push(t);
        }
        Ok(())
    }
    fn start_playback(&mut self) -> Result<()> {
        let was_playing = self.is_playing;
        self.stop_playback();
        if was_playing {
            self.is_playing = true;
            self.last_tick = Some(Instant::now());
        }
        if self.input_path.trim().is_empty() || self.duration <= 0.0 {
            return Err(anyhow!("Brak pliku lub dlugosci"));
        }
        self.start_audio_playback()?;
        self.start_video_playback()?;
        Ok(())
    }

    fn start_audio_playback(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("Brak urzadzenia audio"))?;
        let config = device.default_output_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        self.audio_sample_rate = sample_rate;
        self.audio_channels = channels;
        self.audio_samples_played.store(0, Ordering::Relaxed);
        if let Ok(mut q) = self.audio_buffer.lock() {
            q.clear();
        }

        let stop = Arc::new(AtomicBool::new(false));
        let buffer = Arc::clone(&self.audio_buffer);
        let input = self.input_path.clone();
        let start_time = self.playhead.max(0.0);
        
        // Generujemy filtry audio dla playbacku
        let (_, af_opt) = self.build_playback_filters(start_time);

        let stop_thread = Arc::clone(&stop);
        let buffer_thread = Arc::clone(&buffer);
        let audio_thread = thread::spawn(move || {
            let mut cmd = Command::new("ffmpeg");
            cmd.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-ss",
                &format!("{:.3}", start_time),
                "-i",
                &input,
                "-vn",
            ]);
            
            if let Some(filter) = &af_opt {
                cmd.args(["-af", filter]);
            }
            
            cmd.args([
                "-ac",
                &channels.to_string(),
                "-ar",
                &sample_rate.to_string(),
                "-f",
                "s16le",
                "-",
            ]);
            
            let mut child = match cmd
                .stdout(Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(_) => return,
            };

            let mut stdout = match child.stdout.take() {
                Some(out) => out,
                None => return,
            };

            let mut raw = vec![0u8; 4096];
            let max_samples = (sample_rate as usize * channels as usize * 4).max(1);
            while !stop_thread.load(Ordering::Relaxed) {
                let read = match stdout.read(&mut raw) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let mut samples = Vec::with_capacity(read / 2);
                for chunk in raw[..read].chunks_exact(2) {
                    samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
                }
                
                loop {
                    if stop_thread.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Ok(mut q) = buffer_thread.lock() {
                        if q.len() >= max_samples {
                            // Bufor pelny (backpressure). 
                            // Nie usuwamy starych probek (bo to spowoduje przeskoki),
                            // tylko opozniamy czytanie nowych.
                            // W tym czasie watek audio (cpal) oprozni troche kolejke.
                            drop(q);
                            thread::sleep(std::time::Duration::from_millis(5));
                            continue;
                        }
                        q.extend(samples.clone());
                        break;
                    } else {
                        break;
                    }
                }
            }
            let _ = child.kill();
        });

        let samples_played = Arc::clone(&self.audio_samples_played);
        let buffer_cb = Arc::clone(&self.audio_buffer);
        let err_fn = |err| eprintln!("Audio error: {err}");
        let stream = match config.sample_format() {
            cpal::SampleFormat::I16 => {
                let config = config.into();
                device.build_output_stream(
                    &config,
                    move |data: &mut [i16], _| {
                        let mut filled = 0;
                        if let Ok(mut q) = buffer_cb.lock() {
                            for sample in data.iter_mut() {
                                if let Some(v) = q.pop_front() {
                                    *sample = v;
                                } else {
                                    *sample = 0;
                                }
                                filled += 1;
                            }
                        } else {
                            for sample in data.iter_mut() {
                                *sample = 0;
                                filled += 1;
                            }
                        }
                        samples_played.fetch_add(filled as u64, Ordering::Relaxed);
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::F32 => {
                let config = config.into();
                device.build_output_stream(
                    &config,
                    move |data: &mut [f32], _| {
                        let mut filled = 0;
                        if let Ok(mut q) = buffer_cb.lock() {
                            for sample in data.iter_mut() {
                                if let Some(v) = q.pop_front() {
                                    *sample = v as f32 / 32768.0;
                                } else {
                                    *sample = 0.0;
                                }
                                filled += 1;
                            }
                        } else {
                            for sample in data.iter_mut() {
                                *sample = 0.0;
                                filled += 1;
                            }
                        }
                        samples_played.fetch_add(filled as u64, Ordering::Relaxed);
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                let config = config.into();
                device.build_output_stream(
                    &config,
                    move |data: &mut [u16], _| {
                        let mut filled = 0;
                        if let Ok(mut q) = buffer_cb.lock() {
                            for sample in data.iter_mut() {
                                if let Some(v) = q.pop_front() {
                                    *sample = (v as i32 + 32768).clamp(0, 65535) as u16;
                                } else {
                                    *sample = 32768;
                                }
                                filled += 1;
                            }
                        } else {
                            for sample in data.iter_mut() {
                                *sample = 32768;
                                filled += 1;
                            }
                        }
                        samples_played.fetch_add(filled as u64, Ordering::Relaxed);
                    },
                    err_fn,
                    None,
                )?
            }
            _ => {
                return Err(anyhow!("Nieobslugiwany format audio z urzadzenia"));
            }
        };
        stream.play()?;

        self.audio_stream = Some(stream);
        self.audio_stop = Some(stop);
        self.audio_thread = Some(audio_thread);
        Ok(())
    }

    fn start_video_playback(&mut self) -> Result<()> {
        let (width, height) = scaled_preview_size(self.video_width, self.video_height, 640);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let frames = Arc::clone(&self.playback_frames);
        let input = self.input_path.clone();
        let start_time = self.playhead.max(0.0);
        let fps = self.video_fps.max(1.0);
        let audio_clock = Arc::clone(&self.audio_samples_played);
        let sample_rate = self.audio_sample_rate.max(1);
        let channels = self.audio_channels.max(1);
        
        // Pobieramy filtry video
        let (vf_opt, _) = self.build_playback_filters(start_time);

        let handle = thread::spawn(move || {
            // Laczymy scale z filtrami fade
            let scale_str = format!("scale={width}:{height}");
            let vf_string = if let Some(fade) = &vf_opt {
                format!("{},{}", scale_str, fade)
            } else {
                scale_str
            };

            let mut child = match Command::new("ffmpeg")
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-ss",
                    &format!("{:.3}", start_time),
                    "-i",
                    &input,
                    "-vf",
                    &vf_string,
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgba",
                    "-",
                ])
                .stdout(Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(_) => return,
            };

            let mut stdout = match child.stdout.take() {
                Some(out) => out,
                None => return,
            };

            let frame_size = width as usize * height as usize * 4;
            let mut buffer = vec![0u8; frame_size];
            let mut frame_idx = (start_time * fps).floor() as u64;
            while !stop_thread.load(Ordering::Relaxed) {
                if let Err(_) = stdout.read_exact(&mut buffer) {
                    break;
                }
                
                let target_video_time = frame_idx as f32 / fps;
                
                // --- Frame Dropping Logic ---
                let played_samples = audio_clock.load(Ordering::Relaxed);
                let current_audio_time = played_samples as f32 / (sample_rate as f32 * channels as f32);
                let early_diff = target_video_time - current_audio_time;
                
                // Jesli jestesmy spoznieni wiecej niz 50ms (0.05s) wzgledem audio,
                // to pomijamy renderowanie tej klatki (drop), zeby nadgonic czas.
                if early_diff < -0.05 {
                    frame_idx += 1;
                    continue;
                }
                // -----------------------------
                
                loop {
                    if stop_thread.load(Ordering::Relaxed) {
                        break;
                    }

                    let played_samples = audio_clock.load(Ordering::Relaxed);
                    let current_audio_time = played_samples as f32 / (sample_rate as f32 * channels as f32);
                    
                    let diff = target_video_time - current_audio_time;
                    
                    if diff <= 0.005 {
                        break;
                    }
                    
                    let sleep_dur = diff.min(0.020); 
                    let sleep_dur = if sleep_dur > 0.002 { sleep_dur - 0.002 } else { 0.0 };
                    
                    if sleep_dur > 0.0 {
                         thread::sleep(std::time::Duration::from_secs_f32(sleep_dur));
                    }
                }
                
                if stop_thread.load(Ordering::Relaxed) {
                    break;
                }

                let image =
                    egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &buffer);
                if let Ok(mut slot) = frames.lock() {
                    *slot = Some(image);
                }
                frame_idx += 1;
            }
            let _ = child.kill();
        });
        self.playback_stop = Some(stop);
        self.playback_thread = Some(handle);
        Ok(())
    }

    fn stop_playback(&mut self) {
        if let Some(stop) = &self.playback_stop {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(stop) = &self.audio_stop {
            stop.store(true, Ordering::Relaxed);
        }
        self.audio_stream = None;
        if let Some(handle) = self.playback_thread.take() {
            thread::spawn(move || {
                let _ = handle.join();
            });
        }
        if let Some(handle) = self.audio_thread.take() {
            thread::spawn(move || {
                let _ = handle.join();
            });
        }
        self.playback_stop = None;
        self.audio_stop = None;
        self.is_playing = false;
        self.last_tick = None;
    }

    fn take_latest_frame(&mut self) -> Option<egui::ColorImage> {
        let mut slot = self.playback_frames.lock().ok()?;
        slot.take()
    }
}

fn generate_waveform(input: &str, output: &Path) -> Result<()> {
    run_ffmpeg(&[
        "-y",
        "-i",
        input,
        "-filter_complex",
        "aformat=channel_layouts=mono,showwavespic=s=1000x120:colors=white",
        "-frames:v",
        "1",
        output
            .to_str()
            .ok_or_else(|| anyhow!("Niepoprawna sciezka waveform"))?,
    ])
}

fn parse_fps(value: &str) -> Option<f32> {
    if let Some((num, den)) = value.split_once('/') {
        let n: f32 = num.parse().ok()?;
        let d: f32 = den.parse().ok()?;
        if d != 0.0 {
            return Some(n / d);
        }
        return None;
    }
    value.parse::<f32>().ok()
}

fn load_texture_from_path(
    ctx: &egui::Context,
    path: &Path,
    name: &str,
) -> Result<egui::TextureHandle> {
    let img = image::open(path).context("Nie mozna otworzyc obrazu")?;
    let size = [img.width() as usize, img.height() as usize];
    let rgba = img.to_rgba8();
    let pixels = rgba.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Ok(ctx.load_texture(
        name,
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}

fn load_texture_from_memory(
    ctx: &egui::Context,
    data: &[u8],
    name: &str,
) -> Result<egui::TextureHandle> {
    let img = image::load_from_memory(data).context("Nie mozna odkodowac obrazu z pamieci")?;
    let size = [img.width() as usize, img.height() as usize];
    let rgba = img.to_rgba8();
    let pixels = rgba.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Ok(ctx.load_texture(
        name,
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}

fn scaled_preview_size(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (max_width, max_width * 9 / 16);
    }
    if width <= max_width {
        return (width, height);
    }
    let new_width = max_width;
    let new_height = ((height as f32) * (new_width as f32) / (width as f32)).round() as u32;
    (new_width, new_height.max(1))
}

impl Default for VideoEditorApp {
    fn default() -> Self {
        Self {
            input_path: String::new(),
            output_path: String::new(),
            clips: Vec::new(),
            duration: 0.0,
            video_width: 0,
            video_height: 0,
            video_fps: 30.0,
            playhead: 0.0,
            mark_in: None,
            mark_out: None,
            selected_clip: None,
            preview_texture: None,
            waveform_texture: None,
            thumb_textures: Vec::new(),
            thumb_times: Vec::new(),
            temp_dir: None,
            last_preview_time: None,
            last_preview_playhead: -1.0,
            is_playing: false,
            last_tick: None,
            playback_thread: None,
            playback_stop: None,
            playback_frames: Arc::new(Mutex::new(None)),
            audio_thread: None,
            audio_stop: None,
            audio_stream: None,
            audio_buffer: Arc::new(Mutex::new(VecDeque::new())),
            audio_samples_played: Arc::new(AtomicU64::new(0)),
            audio_sample_rate: 48000,
            audio_channels: 2,
            dragging_playhead: false,
            was_dragging_playhead: false,
            timeline_zoom: 0.0,
            timeline_offset: 0.0,
            last_drag_preview_time: None,
            last_drag_preview_playhead: -1.0,
            live_drag_preview: true,
            tool: Tool::Hand,
            dragging_timeline: false,
            dragging_fade: None,
            ripple_delete: false,
            status: String::new(),
        }
    }
}
