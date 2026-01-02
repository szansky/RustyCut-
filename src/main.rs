use anyhow::{anyhow, Result};

mod types;
mod ffmpeg;
// mod i18n; 
mod utils; 
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::egui;
use egui::load::SizedTexture;
use serde::{Deserialize, Serialize};
use std::collections::{VecDeque, HashMap};
use std::io::Read;
use std::fs;
use crate::types::*;
use crate::ffmpeg::*;
use crate::utils::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, atomic::AtomicU64,
};
use std::thread;
use std::sync::mpsc;

fn load_icon() -> egui::IconData {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::load_from_memory(include_bytes!("../icon.png"))
            .expect("Failed to open icon")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    
    egui::IconData {
        rgba: icon_rgba,
        width: icon_width,
        height: icon_height,
    }
}

fn main() -> Result<()> {
    let mut options = eframe::NativeOptions::default();
    // Set icon
    options.viewport.icon = Some(Arc::new(load_icon()));
    
    if let Err(err) = eframe::run_native(
        "RustyCut",
        options,
        Box::new(|_cc| Box::new(VideoEditorApp::default())),
    ) {
        return Err(anyhow!(err.to_string()));
    }
    Ok(())
}


#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
enum HwAccelMode {
    #[default]
    None,
    Auto,
    Cuda, // NVIDIA
    Vaapi, // Intel/AMD (Linux)
    VideoToolbox, // MacOS
}

impl std::fmt::Display for HwAccelMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HwAccelMode::None => write!(f, "None (CPU)"),
            HwAccelMode::Auto => write!(f, "Auto"),
            HwAccelMode::Cuda => write!(f, "CUDA (NVIDIA)"),
            HwAccelMode::Vaapi => write!(f, "VAAPI (Linux)"),
            HwAccelMode::VideoToolbox => write!(f, "VideoToolbox (Mac)"),
        }
    }
}


#[allow(dead_code)]
struct TextResources {
    // Menu
    file_menu: String,
    new_project: String,
    open_project: String,
    save_project: String,
    // Timeline
    timeline_label: String,
    mark_in: String,
    mark_out: String,
    add_clip: String,
    split_clip: String,
    remove_clip: String,
    // Tools
    editor_title: String,
    input_file: String,
    output_file: String,
    duration_label: String,
    auto_ffprobe: String,
    create_full_clip: String,
    tools_label: String,
    tool_hand: String,
    tool_scissors: String,
    live_preview: String,
    ripple_delete: String,
    render_button: String,
    // Status
    status_ready: String,
    status_render_done: String,
    status_new_project: String,
    status_project_loaded: String,
    status_project_saved: String,
    // Errors
    err_mark_out_greater: String,
    err_set_marks: String,
    err_playhead_inside: String,
    err_select_clip: String,
    err_set_duration: String,
    err_clip_boundary: String,
    err_no_clip_cursor: String,
    // Loading
    loading_change_lang: String,
    // Settings
    settings_title: String,
    language_label: String,
    // Generic
    no_preview: String,
    no_duration: String,
}


impl TextResources {
    fn new(lang: Language) -> Self {
        match lang {
            Language::En => Self {
                file_menu: "File".to_owned(),
                new_project: "New Project".to_owned(),
                open_project: "Open Project...".to_owned(),
                save_project: "Save Project As...".to_owned(),
                timeline_label: "Timeline:".to_owned(),
                mark_in: "Mark In".to_owned(),
                mark_out: "Mark Out".to_owned(),
                add_clip: "Add Clip".to_owned(),
                split_clip: "Split Clip".to_owned(),
                remove_clip: "Remove Clip".to_owned(),
                editor_title: "Video Editor".to_owned(),
                input_file: "Input File:".to_owned(),
                output_file: "Output File:".to_owned(),
                duration_label: "Duration (s):".to_owned(),
                auto_ffprobe: "Auto (ffprobe)".to_owned(),
                create_full_clip: "Create Full Clip".to_owned(),
                tools_label: "Tools:".to_owned(),
                tool_hand: "Hand".to_owned(),
                tool_scissors: "Blade".to_owned(),
                live_preview: "Live Preview".to_owned(),
                ripple_delete: "Ripple Delete".to_owned(),
                render_button: "RENDER VIDEO".to_owned(),
                status_ready: "Ready.".to_owned(),
                status_render_done: "Render finished.".to_owned(),
                status_new_project: "New project created.".to_owned(),
                status_project_loaded: "Project loaded.".to_owned(),
                status_project_saved: "Project saved.".to_owned(),
                err_mark_out_greater: "Mark Out must be > Mark In.".to_owned(),
                err_set_marks: "Set Mark In and Mark Out first.".to_owned(),
                err_playhead_inside: "Playhead must be inside a clip.".to_owned(),
                err_select_clip: "Select a clip from the timeline.".to_owned(),
                err_set_duration: "Set duration before creating a clip.".to_owned(),
                err_clip_boundary: "Cannot split on clip boundary.".to_owned(),
                err_no_clip_cursor: "No clip under cursor.".to_owned(),
                loading_change_lang: "Changing language...".to_owned(),
                settings_title: "Settings".to_owned(),
                language_label: "Language".to_owned(),
                no_preview: "No preview".to_owned(),
                no_duration: "No material duration".to_owned(),
            },
            Language::Pl => Self {
                file_menu: "Plik".to_owned(),
                new_project: "Nowy projekt".to_owned(),
                open_project: "Otwórz projekt...".to_owned(),
                save_project: "Zapisz projekt...".to_owned(),
                timeline_label: "Oś czasu:".to_owned(),
                mark_in: "Mark In".to_owned(),
                mark_out: "Mark Out".to_owned(),
                add_clip: "Dodaj klip".to_owned(),
                split_clip: "Podziel klip".to_owned(),
                remove_clip: "Usuń klip".to_owned(),
                editor_title: "Edytor Wideo".to_owned(),
                input_file: "Plik wejściowy:".to_owned(),
                output_file: "Plik wyjściowy:".to_owned(),
                duration_label: "Długość (s):".to_owned(),
                auto_ffprobe: "Auto (ffprobe)".to_owned(),
                create_full_clip: "Utwórz cały klip".to_owned(),
                tools_label: "Narzędzia:".to_owned(),
                tool_hand: "Ręka".to_owned(),
                tool_scissors: "Nożyczki".to_owned(),
                live_preview: "Podgląd live".to_owned(),
                ripple_delete: "Ripple Delete (Auto-przesuwanie)".to_owned(),
                render_button: "RENDERUJ FILM".to_owned(),
                status_ready: "Gotowy.".to_owned(),
                status_render_done: "Render zakończony.".to_owned(),
                status_new_project: "Nowy projekt utworzony.".to_owned(),
                status_project_loaded: "Projekt wczytany.".to_owned(),
                status_project_saved: "Projekt zapisany.".to_owned(),
                err_mark_out_greater: "Mark Out musi być > Mark In.".to_owned(),
                err_set_marks: "Ustaw najpierw Mark In i Mark Out.".to_owned(),
                err_playhead_inside: "Głowica musi być wewnątrz klipu.".to_owned(),
                err_select_clip: "Wybierz klip z osi czasu.".to_owned(),
                err_set_duration: "Ustaw długość zanim utworzysz klip.".to_owned(),
                err_clip_boundary: "Nie można dzielić na granicy klipu.".to_owned(),
                err_no_clip_cursor: "Brak klipu pod kursorem.".to_owned(),
                loading_change_lang: "Zmieniam język...".to_owned(),
                settings_title: "Ustawienia".to_owned(),
                language_label: "Język".to_owned(),
                no_preview: "Brak podglądu".to_owned(),
                no_duration: "Brak długości materiału".to_owned(),
            }
        }
    }
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
    selected_track: TrackType,
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
    last_drag_preview_playhead: f32,
    live_drag_preview: bool,
    tool: Tool,
    dragging_timeline: bool,
    dragging_fade: Option<FadeDrag>,
    dragging_clip: Option<usize>,      // NEW: Index of clip being dragged
    drag_clip_offset: f32,             // NEW: Offset from clip start to mouse
    ripple_delete: bool,
    show_settings: bool,
    language: Language,
    text: TextResources,
    
    // Media Library
    media_library: Vec<MediaAsset>,
    media_thumbs: HashMap<usize, egui::TextureHandle>, // ID -> Texture

    language_switch_start: Option<Instant>,
    status: String,
    // Async Preview
    preview_rx: mpsc::Receiver<(f32, Vec<u8>)>,
    preview_tx: mpsc::Sender<(f32, Vec<u8>)>,
    preview_busy: Arc<AtomicBool>,
    // Frame Cache (LRU) - klucz: timestamp w ms, wartość: PNG bytes
    #[allow(dead_code)]
    frame_cache: HashMap<i64, Vec<u8>>,
    #[allow(dead_code)]
    frame_cache_max_size: usize,
    
    // Video Sync
    waiting_for_video_ready: bool,
    video_ready_signal: Arc<AtomicBool>,
    playback_start_playhead: f32, // Position when playback started
    
    // Settings
    hw_accel_mode: HwAccelMode,
}





impl eframe::App for VideoEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok((_time, data)) = self.preview_rx.try_recv() {
             // Hack: musimy zaladowac teksture w glownym watku (tutaj), bo ctx jest dostepny
             // Ale load_texture wymaga Context. OK.
             if let Ok(texture) = load_texture_from_memory(ctx, &data, "preview_async") {
                 self.preview_texture = Some(texture);
                 // self.last_preview_time = Some(Instant::now()); // Opcjonalne
                 // self.last_preview_playhead = time; // Ważne dla logiki
             }
        }

        let mut user_seeked = false;

        // Skroty klawiszowe
        if ctx.input(|i| i.key_pressed(egui::Key::A)) {
            self.tool = Tool::Hand;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::B)) {
            self.tool = Tool::Scissors;
        }
        // Delete / Backspace - usuwa zaznaczony klip
        if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            if let Some(idx) = self.selected_clip {
                if idx < self.clips.len() {
                    if self.ripple_delete {
                        // Ripple Delete - przesun pozostale klipy
                        let duration = self.clips[idx].end - self.clips[idx].start;
                        self.clips.remove(idx);
                        for clip in self.clips.iter_mut().skip(idx) {
                            clip.start -= duration;
                            clip.end -= duration;
                        }
                    } else {
                        self.clips.remove(idx);
                    }
                    self.selected_clip = None;
                    self.status = "Klip usuniety.".to_string();
                }
            }
        }

        // Logika Fake Loading przy zmianie jezyka
        if let Some(start_time) = self.language_switch_start {
            let duration = start_time.elapsed();
            if duration.as_secs_f32() > 1.0 {
                 // Koniec ladowania, faktyczna zmiana juz sie odbywa w momencie klikniecia? 
                 // Nie, uzytkownik chce aby ladowanie bylo PO kliknieciu.
                 // Wiec tutaj finalizujemy zmiane text resources.
                 // Ale zaraz, musimy wiedziec na jaki jezyk zmienic.
                 // Uproszczenie: flaga language jest ustawiana PO loading screenie? 
                 // Lepiej: przyciski ustawiaja TARGET language, a tutaj go aplikujemy.
                 // Ale w prostym modelu: przyciski ustawiaja language_switch_start.
                 // A tutaj zmieniamy na *przeciwny* czy wybrany? 
                 // Zrobmy tak: przyciski ustawiaja `self.language` od razu, ALE self.text dopiero tutaj.
                 // Wtedy loading screen bedzie mial stary tekst. 
                 // OK, przyciski ustawiaja `switch_start` i `pending_language` (ale nie mam takiego pola).
                 // Hack: Przyciski zmieniaja `self.language` od razu, a `self.text` updatujemy tutaj.
                 // Wtedy loading text bedzie juz w nowym jezyku? Nie, `self.text` jest stary.
                 // OK:
                 self.text = TextResources::new(self.language);
                 self.language_switch_start = None;
            } else {
                 // Wyswietlenie Modala Ladowania
                 egui::CentralPanel::default().show(ctx, |ui| {
                     ui.vertical_centered(|ui| {
                         ui.add_space(ui.available_height() / 2.0 - 50.0);
                         ui.heading(&self.text.loading_change_lang);
                         ui.add(egui::Spinner::new().size(40.0));
                     });
                 });
                 ctx.request_repaint(); // Wymus odswiezanie
                 return; // Zatrzymaj rysowanie reszty UI
            }
        }

        // VIDEO SYNC: Check if waiting for video buffer
        if self.waiting_for_video_ready {
            if self.video_ready_signal.load(Ordering::Relaxed) {
                // Video is ready! Start Audio and Time
                self.waiting_for_video_ready = false;
                if let Err(e) = self.start_audio_playback() {
                    self.status = format!("Audio error: {}", e);
                }
                self.is_playing = true;
                self.last_tick = Some(Instant::now());
            } else {
                // Buffering... request repaint to poll
                // Check if thread died?
                if let Some(handle) = &self.playback_thread {
                    if handle.is_finished() {
                        self.waiting_for_video_ready = false;
                        self.is_playing = false;
                        self.status = "Błąd: Wątek wideo zakończył pracę przed startem.".to_string();
                        return;
                    }
                }
                ctx.request_repaint();
            }
        }

        if self.is_playing && !self.waiting_for_video_ready {
            let now = Instant::now();
            let dt = if let Some(last) = self.last_tick {
                now.duration_since(last).as_secs_f32()
            } else {
                0.0
            };
            self.last_tick = Some(now);
            
            if !self.clips.is_empty() {
                let new_playhead = if self.audio_stream.is_some() {
                     // AUDIO MASTER SYNC
                     let played = self.audio_samples_played.load(Ordering::Relaxed) as f32;
                     let rate = self.audio_sample_rate.max(1) as f32;
                     let channels = self.audio_channels.max(1) as f32;
                     let audio_time = played / (rate * channels);
                     self.playback_start_playhead + audio_time
                } else {
                     // Fallback to strict timer if no audio
                     self.playhead + dt
                };
                
                // Find the last clip end (effective duration)
                let effective_end = self.clips.iter()
                    .filter(|c| c.video_enabled || c.audio_enabled)
                    .map(|c| c.end)
                    .fold(0.0f32, |a, b| a.max(b));
                
                // Check if new playhead is in a gap (not inside any clip)
                let in_clip = self.clips.iter()
                    .filter(|c| c.video_enabled || c.audio_enabled)
                    .any(|c| new_playhead >= c.start && new_playhead < c.end);
                
                if !in_clip && new_playhead < effective_end {
                    // Start playback is just linear, gaps are black/silent.
                    // Do NOT skip gaps.
                }
                
                self.playhead = new_playhead;
                
                if self.playhead >= effective_end {
                    self.playhead = effective_end;
                    self.stop_playback();
                }
            } else if self.clips.is_empty() {
                self.stop_playback();
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
                let file_label = self.text.file_menu.clone();
                let new_proj = self.text.new_project.clone();
                let open_proj = self.text.open_project.clone();
                let save_proj = self.text.save_project.clone();
                
                ui.menu_button(&file_label, |ui| {
                    if ui.button(&new_proj).clicked() {
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
                        self.status = self.text.status_new_project.clone();
                        ui.close_menu();
                    }
                    if ui.button(&open_proj).clicked() {
                        self.load_project_dialog(ctx);
                        ui.close_menu();
                    }
                    if ui.button(&save_proj).clicked() {
                        self.save_project_as();
                        ui.close_menu();
                    }
                });

                // Przelacznik Settings
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⚙").clicked() {
                        self.show_settings = !self.show_settings;
                    }
                });
            });
        });

        // Okno Ustawien
        if self.show_settings {
            let title = self.text.settings_title.clone();
            let label_lang = self.text.language_label.clone();
            
            egui::Window::new(title)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.screen_rect().center())
                .open(&mut self.show_settings)
                .show(ctx, |ui| {
                    ui.label(label_lang);
                    ui.horizontal(|ui| {
                         if ui.button("🇵🇱 PL").clicked() {
                             if self.language != Language::Pl {
                                 self.language = Language::Pl;
                                 self.language_switch_start = Some(Instant::now());
                             }
                         }
                         if ui.button("🇺🇸 EN").clicked() {
                             if self.language != Language::En {
                                 self.language = Language::En;
                                 self.language_switch_start = Some(Instant::now());
                             }
                         }
                    });
                     
                     ui.add_space(10.0);
                     ui.label("Hardware Acceleration:");
                     egui::ComboBox::from_id_source("hw_accel")
                        .selected_text(self.hw_accel_mode.to_string())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.hw_accel_mode, HwAccelMode::None, "None (CPU)");
                            ui.selectable_value(&mut self.hw_accel_mode, HwAccelMode::Auto, "Auto");
                            ui.selectable_value(&mut self.hw_accel_mode, HwAccelMode::Cuda, "CUDA (NVIDIA)");
                            ui.selectable_value(&mut self.hw_accel_mode, HwAccelMode::Vaapi, "VAAPI (Linux)");
                            ui.selectable_value(&mut self.hw_accel_mode, HwAccelMode::VideoToolbox, "VideoToolbox (Mac)");
                        });
                });
        }

        // Panel dolny: Timeline
        egui::TopBottomPanel::bottom("timeline_panel")
            .resizable(true)
            .min_height(150.0)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.label(&self.text.timeline_label);
                    if draw_timeline(ui, self) {
                        user_seeked = true;
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(format!("Playhead: {:.2}s", self.playhead));
                        if ui.button(&self.text.mark_in).clicked() {
                            self.mark_in = Some(self.playhead);
                        }
                        if ui.button(&self.text.mark_out).clicked() {
                            self.mark_out = Some(self.playhead);
                        }
                        if ui.button(&self.text.add_clip).clicked() {
                            if let (Some(start), Some(end)) = (self.mark_in, self.mark_out) {
                                if end > start {
                                    self.clips.push(Clip {
                                        asset_id: None,
                                        start,
                                        end,
                                        fade_in: 0.0,
                                        fade_out: 0.0,
                                        linked: true,
                                        video_enabled: true,
                                        audio_enabled: true,
                                    });
                                    self.selected_clip = Some(self.clips.len() - 1);
                                    self.status.clear();
                                } else {
                                    self.status = self.text.err_mark_out_greater.clone();
                                }
                            } else {
                                self.status = self.text.err_set_marks.clone();
                            }
                        }
                        if ui.button(&self.text.split_clip).clicked() {
                            if let Some(idx) = self.selected_clip {
                                if let Some(split) = split_clip_at(&mut self.clips, idx, self.playhead) {
                                    self.selected_clip = Some(split);
                                    self.status.clear();
                                } else {
                                    self.status = self.text.err_playhead_inside.clone();
                                }
                            } else {
                                self.status = self.text.err_select_clip.clone();
                            }
                        }
                        if ui.button(&self.text.remove_clip).clicked() {
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
                ui.heading(&self.text.editor_title);
                ui.separator();

                ui.collapsing("Project Settings", |ui| {
                     ui.label(&self.text.input_file);
                     ui.horizontal(|ui| {
                         ui.text_edit_singleline(&mut self.input_path);
                         if ui.button("...").clicked() {
                             if let Some(path) = rfd::FileDialog::new().pick_file() {
                                 self.input_path = path.display().to_string();
                                 self.prepare_media_assets(ctx);
                             }
                         }
                     });

                     ui.label(&self.text.output_file);
                     ui.horizontal(|ui| {
                         ui.text_edit_singleline(&mut self.output_path);
                         if ui.button("...").clicked() {
                             if let Some(path) = rfd::FileDialog::new().save_file() {
                                 self.output_path = path.display().to_string();
                             }
                         }
                     });
                });

                ui.separator();
                ui.heading("Media Library");
                if ui.button("📂 Import Media").clicked() {
                    if let Some(paths) = rfd::FileDialog::new().pick_files() {
                        for path in paths {
                            let path_str = path.display().to_string();
                            // Detect type using ffprobe logic or extension
                            // For MVP, we use ffprobe. 
                            if let Ok((dur, w, h, _fps)) = get_video_info_ffprobe(&path_str) {
                                let kind = if w == 0 && h == 0 {
                                    MediaType::Audio 
                                } else if dur < 0.1 && (path_str.ends_with(".png") || path_str.ends_with(".jpg")) {
                                    MediaType::Image
                                } else {
                                    MediaType::Video
                                };
                                
                                let id = self.media_library.len() + 1; // 0 reserved? No, let's start at 0? 
                                // Actually asset_id is Option<usize>. 
                                // Let's use simple indexing for now, but safer to use ID.
                                // Current logic: self.media_library index.
                                let asset = MediaAsset {
                                    id, 
                                    path: path_str.clone(),
                                    name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                                    kind,
                                    duration: if kind == MediaType::Image { 5.0 } else { dur },
                                };
                                self.media_library.push(asset);
                            }
                        }
                    }
                }
                
                ui.add_space(5.0);
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    let mut added_clip = None;
                    for (idx, asset) in self.media_library.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let icon = match asset.kind {
                                MediaType::Video => "🎬",
                                MediaType::Audio => "🔊",
                                MediaType::Image => "🖼️",
                            };
                            ui.label(icon);
                            ui.label(egui::RichText::new(&asset.name).strong());
                            if ui.button("➕").on_hover_text("Add to Timeline").clicked() {
                                added_clip = Some(idx);
                            }
                        });
                        ui.label(format!("Dur: {:.2}s", asset.duration));
                        ui.separator();
                    }
                    
                    if let Some(idx) = added_clip {
                        let asset = &self.media_library[idx];
                        self.clips.push(Clip {
                            start: self.playhead,
                            end: self.playhead + asset.duration,
                            fade_in: 0.0,
                            fade_out: 0.0,
                            linked: asset.kind == MediaType::Video,
                            video_enabled: asset.kind != MediaType::Audio,
                            audio_enabled: asset.kind != MediaType::Image,
                            asset_id: Some(idx), // Using index as ID for MVP
                        });
                        self.selected_clip = Some(self.clips.len() - 1);
                        self.status = format!("Added clip: {}", asset.name);
                    }
                });

                ui.separator();
                ui.label(&self.text.duration_label);
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.duration).clamp_range(0.0..=86400.0));
                });
                ui.horizontal(|ui| {
                    if ui.button(&self.text.auto_ffprobe).clicked() {
                        self.prepare_media_assets(ctx);
                    }
                    if ui.button(&self.text.create_full_clip).clicked() {
                        if self.duration > 0.0 {
                            self.clips.clear();
                            self.clips.push(Clip {
                                asset_id: None,
                                start: 0.0,
                                end: self.duration,
                                fade_in: 0.0,
                                fade_out: 0.0,
                                linked: true,
                                video_enabled: true,
                                audio_enabled: true,
                            });
                            self.selected_clip = Some(0);
                        } else {
                            self.status = self.text.err_set_duration.clone();
                        }
                    }
                });
                
                ui.separator();
                ui.label(&self.text.tools_label);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tool, Tool::Hand, &self.text.tool_hand);
                    ui.selectable_value(&mut self.tool, Tool::Scissors, &self.text.tool_scissors);
                });
                ui.checkbox(&mut self.live_drag_preview, &self.text.live_preview);
                ui.checkbox(&mut self.ripple_delete, &self.text.ripple_delete);

                ui.separator();
                if ui.button(&self.text.render_button).clicked() {
                    match render_video(&self.input_path, &self.output_path, &self.clips, &self.media_library) {
                        Ok(()) => self.status = self.text.status_render_done.clone(),
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

                // Check if playhead is inside any video clip
                let current_clip = self.clips.iter().find(|c| 
                    self.playhead >= c.start && self.playhead < c.end && c.video_enabled
                );

                if let Some(clip) = current_clip {
                    // Software Fade Logic
                    let mut alpha = 1.0;
                    let rel = self.playhead - clip.start;
                    if rel < clip.fade_in {
                        alpha = rel / clip.fade_in.max(0.001);
                    }
                    let end_rel = clip.end - self.playhead;
                    if end_rel < clip.fade_out {
                        alpha = alpha.min(end_rel / clip.fade_out.max(0.001));
                    }
                    
                    let alpha = alpha.clamp(0.0, 1.0);
                    let tint = egui::Color32::from_white_alpha((alpha * 255.0) as u8);

                    let image = egui::Image::new(SizedTexture::new(texture.id(), draw_rect.size())).tint(tint);
                    egui::Image::paint_at(&image, ui, draw_rect);
                } else {
                    // No clip at playhead position -> Draw NOTHING (Black background remains)
                    // Optionally draw logo or placeholder
                }
            } else {
                 ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &self.text.no_preview,
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



fn draw_timeline(ui: &mut egui::Ui, app: &mut VideoEditorApp) -> bool {
    let desired_height = 160.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), desired_height),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(30));

    // Parametry Layoutu
    let ruler_height = 24.0;
    let left = rect.left() + 8.0;
    let right = rect.right() - 8.0;
    let width = (right - left).max(1.0);

    if app.duration <= 0.0 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &app.text.no_duration,
            egui::TextStyle::Body.resolve(ui.style()),
            egui::Color32::from_gray(160),
        );
        return false;
    }

    // Ruler (Linijka)
    let ruler_rect = egui::Rect::from_min_max(
        egui::pos2(left, rect.top()),
        egui::pos2(right, rect.top() + ruler_height),
    );
    // Klipy (przesunięte w dół)
    let video_rect = egui::Rect::from_min_max(
        egui::pos2(left, ruler_rect.bottom() + 4.0),
        egui::pos2(right, ruler_rect.bottom() + 4.0 + (rect.height() - ruler_height - 8.0) * 0.5),
    );
    let audio_rect = egui::Rect::from_min_max(
        egui::pos2(left, video_rect.bottom() + 2.0),
        egui::pos2(right, rect.bottom() - 2.0),
    );

    painter.rect_filled(ruler_rect, 0.0, egui::Color32::from_gray(25));
    painter.rect_filled(video_rect, 4.0, egui::Color32::from_gray(40));
    painter.rect_filled(audio_rect, 4.0, egui::Color32::from_gray(35));

    // Zoom i Offset Logic
    let min_zoom = width / app.duration.max(0.01);
    if app.timeline_zoom <= 0.0 {
        app.timeline_zoom = min_zoom;
    }
    let max_zoom = 800.0;
    app.timeline_zoom = app.timeline_zoom.clamp(min_zoom, max_zoom);
    let window = width / app.timeline_zoom;
    app.timeline_offset = clamp_offset(app.timeline_offset, app.duration, window);

    // Rysowanie Podziałki (Ticks)
    let step = if window < 10.0 { 1.0 } 
               else if window < 60.0 { 5.0 }
               else if window < 300.0 { 30.0 }
               else { 60.0 };
    
    let start_t = (app.timeline_offset / step).floor() * step;
    let end_t = app.timeline_offset + window;
    let mut t = start_t;
    
    while t <= end_t {
         let x = left + (t - app.timeline_offset) * app.timeline_zoom;
         if x >= left && x <= right {
             painter.line_segment(
                [egui::pos2(x, ruler_rect.bottom()), egui::pos2(x, ruler_rect.bottom() - 5.0)],
                egui::Stroke::new(1.0, egui::Color32::GRAY),
             );
             if t >= 0.0 {
                 let ts = t as u32;
                 let text = format!("{:02}:{:02}", ts / 60, ts % 60);
                 painter.text(
                    egui::pos2(x + 2.0, ruler_rect.bottom() - 10.0),
                    egui::Align2::LEFT_CENTER,
                    text,
                    egui::TextStyle::Small.resolve(ui.style()),
                    egui::Color32::GRAY,
                 );
             }
         }
         t += step;
    }



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

    if let Some(_texture) = &app.waveform_texture {
        // Waveform is now drawn per-clip below
    } else {
        painter.text(
            audio_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Brak waveform",
            egui::TextStyle::Body.resolve(ui.style()),
            egui::Color32::from_gray(140),
        );
    }

    if app.thumb_textures.is_empty() {
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

    let mut remove_clip_idx: Option<(usize, TrackType)> = None;
    let mut toggle_link_idx: Option<usize> = None;

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

        // Separate interactions for video and audio when unlinked
        let clip_sense = if app.tool == Tool::Hand {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::click()
        };
        let video_resp = ui.interact(video_clip_rect, ui.id().with("clip_video").with(idx), clip_sense);
        let audio_resp = ui.interact(audio_clip_rect, ui.id().with("clip_audio").with(idx), clip_sense);

        // Get click position for cutting
        let click_pos = video_resp.interact_pointer_pos().or(audio_resp.interact_pointer_pos());

        // Drag Start - begin clip dragging
        if (video_resp.drag_started() || audio_resp.drag_started()) && app.tool == Tool::Hand {
            if let Some(pos) = click_pos {
                let t = app.timeline_offset + ((pos.x - left) / app.timeline_zoom);
                app.dragging_clip = Some(idx);
                app.drag_clip_offset = t - clip.start;
                app.selected_clip = Some(idx);
                app.selected_track = if clip.linked { TrackType::Both } else if video_resp.drag_started() { TrackType::Video } else { TrackType::Audio };
            }
        }

        // Dragging clip - update position (store for after loop)
        if (video_resp.dragged() || audio_resp.dragged()) && app.dragging_clip == Some(idx) {
            if let Some(pos) = click_pos {
                let t = app.timeline_offset + ((pos.x - left) / app.timeline_zoom);
                let new_start = (t - app.drag_clip_offset).max(0.0);
                // Store the move request as status (will process after loop)
                app.status = format!("MOVE:{}:{}", idx, new_start);
            }
        }

        // Drag stopped on this clip
        if video_resp.drag_stopped() || audio_resp.drag_stopped() {
            app.dragging_clip = None;
        }

        // Selection logic OR Cutting logic (only on click, not drag)
        if video_resp.clicked() || audio_resp.clicked() {
            if app.tool == Tool::Scissors {
                // Blade Tool - cut the clip at mouse position
                if let Some(pos) = click_pos {
                    let t = app.timeline_offset + ((pos.x - left) / app.timeline_zoom).clamp(0.0, window);
                    if t > clip.start && t < clip.end {
                        // We need to defer the cut to after the loop to avoid borrow issues
                        // Store info for later
                        // For now, we'll use a workaround - store cut request
                        app.status = format!("CUT:{}:{}", idx, t);
                    }
                }
            } else {
                // Normal selection
                if video_resp.clicked() {
                    app.selected_clip = Some(idx);
                    app.selected_track = if clip.linked { TrackType::Both } else { TrackType::Video };
                }
                if audio_resp.clicked() {
                    app.selected_clip = Some(idx);
                    app.selected_track = if clip.linked { TrackType::Both } else { TrackType::Audio };
                }
            }
        }

        // Context Menu for VIDEO track
        if clip.video_enabled {
            video_resp.context_menu(|ui| {
                if clip.linked {
                    if ui.button("🔗 Unlink (Rozłącz)").clicked() {
                        toggle_link_idx = Some(idx);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(&app.text.remove_clip).clicked() {
                        remove_clip_idx = Some((idx, TrackType::Both));
                        ui.close_menu();
                    }
                } else {
                    if ui.button("🔗 Link (Połącz)").clicked() {
                        toggle_link_idx = Some(idx);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("🎬 Usuń Video").clicked() {
                        remove_clip_idx = Some((idx, TrackType::Video));
                        ui.close_menu();
                    }
                    if clip.audio_enabled {
                        if ui.button("🔊 Usuń Audio").clicked() {
                            remove_clip_idx = Some((idx, TrackType::Audio));
                            ui.close_menu();
                        }
                    }
                }
                ui.separator();
                ui.label(if app.ripple_delete { format!("({} On)", app.text.ripple_delete) } else { format!("({} Off)", app.text.ripple_delete) });
            });
        }

        // Context Menu for AUDIO track
        if clip.audio_enabled {
            audio_resp.context_menu(|ui| {
                if clip.linked {
                    if ui.button("🔗 Unlink (Rozłącz)").clicked() {
                        toggle_link_idx = Some(idx);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(&app.text.remove_clip).clicked() {
                        remove_clip_idx = Some((idx, TrackType::Both));
                        ui.close_menu();
                    }
                } else {
                    if ui.button("🔗 Link (Połącz)").clicked() {
                        toggle_link_idx = Some(idx);
                        ui.close_menu();
                    }
                    ui.separator();
                    if clip.video_enabled {
                        if ui.button("🎬 Usuń Video").clicked() {
                            remove_clip_idx = Some((idx, TrackType::Video));
                            ui.close_menu();
                        }
                    }
                    if ui.button("🔊 Usuń Audio").clicked() {
                        remove_clip_idx = Some((idx, TrackType::Audio));
                        ui.close_menu();
                    }
                }
                ui.separator();
                ui.label(if app.ripple_delete { format!("({} On)", app.text.ripple_delete) } else { format!("({} Off)", app.text.ripple_delete) });
            });
        }

        // Visual styling based on selection and link status
        let is_selected = Some(idx) == app.selected_clip;
        let video_selected = is_selected && (app.selected_track == TrackType::Both || app.selected_track == TrackType::Video);
        let audio_selected = is_selected && (app.selected_track == TrackType::Both || app.selected_track == TrackType::Audio);

        // Video track colors
        let video_color = if !clip.video_enabled {
            egui::Color32::from_gray(60)
        } else if video_selected {
            egui::Color32::from_rgb(80, 170, 255)
        } else {
            egui::Color32::from_rgb(70, 120, 90)
        };

        // Audio track colors
        let audio_color = if !clip.audio_enabled {
            egui::Color32::from_gray(60)
        } else if audio_selected {
            egui::Color32::from_rgb(80, 170, 255)
        } else {
            egui::Color32::from_rgb(70, 120, 90)
        };

        // Draw thumbnails INSIDE clip bounds (video track)
        if clip.video_enabled && !app.thumb_textures.is_empty() && app.duration > 0.0 {
            let chunk = app.duration / app.thumb_textures.len().max(1) as f32;
            let thumb_w = app.timeline_zoom * chunk;
            for (tidx, texture) in app.thumb_textures.iter().enumerate() {
                let t = app.thumb_times[tidx];
                // Check if this thumbnail overlaps with the clip
                let thumb_start = t - chunk * 0.5;
                let thumb_end = t + chunk * 0.5;
                if thumb_end < clip.start || thumb_start > clip.end {
                    continue;
                }
                let x0 = left + (thumb_start - app.timeline_offset) * app.timeline_zoom;
                let x1 = x0 + thumb_w;
                // Clip the thumbnail to the clip bounds
                let clip_x0 = left + (clip.start - app.timeline_offset) * app.timeline_zoom;
                let clip_x1 = left + (clip.end - app.timeline_offset) * app.timeline_zoom;
                let draw_x0 = x0.max(clip_x0).max(video_rect.left());
                let draw_x1 = x1.min(clip_x1).min(video_rect.right());
                if draw_x1 <= draw_x0 {
                    continue;
                }
                // Calculate UV coordinates for partial thumbnail
                let u0 = ((draw_x0 - x0) / thumb_w).clamp(0.0, 1.0);
                let u1 = ((draw_x1 - x0) / thumb_w).clamp(0.0, 1.0);
                let thumb_rect = egui::Rect::from_min_max(
                    egui::pos2(draw_x0, video_rect.top()),
                    egui::pos2(draw_x1, video_rect.bottom()),
                );
                painter.image(
                    texture.id(),
                    thumb_rect,
                    egui::Rect::from_min_max(egui::pos2(u0, 0.0), egui::pos2(u1, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        }

        // Draw waveform INSIDE clip bounds (audio track)
        if clip.audio_enabled {
            if let Some(texture) = &app.waveform_texture {
                // Calculate UV coordinates based on clip position in the original video
                let u0 = (clip.start / app.duration).clamp(0.0, 1.0);
                let u1 = (clip.end / app.duration).clamp(0.0, 1.0);
                painter.image(
                    texture.id(),
                    audio_clip_rect,
                    egui::Rect::from_min_max(egui::pos2(u0, 0.0), egui::pos2(u1, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        }

        // Draw clip rectangles
        if clip.video_enabled {
            painter.rect_stroke(video_clip_rect, 4.0, egui::Stroke::new(2.0, video_color));
        } else {
            // Disabled track - dim overlay
            painter.rect_filled(video_clip_rect, 4.0, egui::Color32::from_rgba_unmultiplied(50, 50, 50, 150));
        }

        if clip.audio_enabled {
            painter.rect_stroke(audio_clip_rect, 4.0, egui::Stroke::new(2.0, audio_color));
        } else {
            painter.rect_filled(audio_clip_rect, 4.0, egui::Color32::from_rgba_unmultiplied(50, 50, 50, 150));
        }

        // Link indicator (line connecting video and audio when linked)
        if clip.linked && clip.video_enabled && clip.audio_enabled {
            let link_x = start_x + 10.0;
            painter.line_segment(
                [egui::pos2(link_x, video_clip_rect.bottom()), egui::pos2(link_x, audio_clip_rect.top())],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(200, 200, 200)),
            );
            // Small chain icon
            painter.circle_filled(egui::pos2(link_x, (video_clip_rect.bottom() + audio_clip_rect.top()) / 2.0), 4.0, egui::Color32::from_rgb(200, 200, 200));
        }

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

    // Toggle Link/Unlink
    if let Some(idx) = toggle_link_idx {
        if let Some(clip) = app.clips.get_mut(idx) {
            clip.linked = !clip.linked;
        }
    }

    // Handle clip removal
    if let Some((idx, track_type)) = remove_clip_idx {
        match track_type {
            TrackType::Both => {
                // Remove entire clip (Ripple Delete if enabled)
                if app.ripple_delete {
                    if let Some(clip) = app.clips.get(idx) {
                        let duration = clip.end - clip.start;
                        app.clips.remove(idx);
                        for other in app.clips.iter_mut().skip(idx) {
                            other.start -= duration;
                            other.end -= duration;
                        }
                    }
                } else {
                    app.clips.remove(idx);
                }
                app.selected_clip = None;
            }
            TrackType::Video => {
                // Disable video track only
                if let Some(clip) = app.clips.get_mut(idx) {
                    clip.video_enabled = false;
                    // If both are now disabled, remove the clip entirely
                    if !clip.video_enabled && !clip.audio_enabled {
                        app.clips.remove(idx);
                        app.selected_clip = None;
                    }
                }
            }
            TrackType::Audio => {
                // Disable audio track only
                if let Some(clip) = app.clips.get_mut(idx) {
                    clip.audio_enabled = false;
                    // If both are now disabled, remove the clip entirely
                    if !clip.video_enabled && !clip.audio_enabled {
                        app.clips.remove(idx);
                        app.selected_clip = None;
                    }
                }
            }
        }
    }

    // Handle clip MOVE (live dragging)
    if app.status.starts_with("MOVE:") {
        let parts: Vec<&str> = app.status.split(':').collect();
        if parts.len() == 3 {
            if let (Ok(idx), Ok(new_start)) = (parts[1].parse::<usize>(), parts[2].parse::<f32>()) {
                if let Some(clip) = app.clips.get_mut(idx) {
                    let clip_duration = clip.end - clip.start;
                    clip.start = new_start;
                    clip.end = new_start + clip_duration;
                }
            }
        }
        app.status.clear();
    }

    // Handle Blade Tool cuts (deferred from inside the loop)
    if app.status.starts_with("CUT:") {
        let parts: Vec<&str> = app.status.split(':').collect();
        if parts.len() == 3 {
            if let (Ok(idx), Ok(t)) = (parts[1].parse::<usize>(), parts[2].parse::<f32>()) {
                if let Some(split_idx) = split_clip_at(&mut app.clips, idx, t) {
                    app.selected_clip = Some(split_idx);
                    app.playhead = t;
                    app.status.clear();
                } else {
                    app.status = "Nie mozna uciac na granicy klipu.".to_string();
                }
            } else {
                app.status.clear();
            }
        } else {
            app.status.clear();
        }
    }

    let play_x = left + (app.playhead - app.timeline_offset) * app.timeline_zoom;
    let hover_hit = hover_pos
        .map(|pos| rect.contains(pos) && (pos.x - play_x).abs() <= 10.0)
        .unwrap_or(false);
    if let Some(fade) = hover_fade.or(app.dragging_fade) {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::None);
        if let Some(pos) = ui.ctx().pointer_latest_pos() {
            let size = 12.0;
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
            painter.add(egui::Shape::convex_polygon(
                points.iter().map(|p| *p + egui::vec2(1.0, 1.0)).collect(),
                egui::Color32::from_black_alpha(100),
                egui::Stroke::NONE,
            ));
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

    // Hover Guide Line
    if response.hovered() && app.tool == Tool::Scissors {
        if let Some(pos) = hover_pos {
             painter.line_segment(
                [egui::pos2(pos.x, ruler_rect.bottom()), egui::pos2(pos.x, rect.bottom())],
                egui::Stroke::new(1.0, egui::Color32::from_white_alpha(200)),
            );
        }
    }

    // Playhead Drawing
    painter.line_segment(
        [
            egui::pos2(play_x, rect.top() + ruler_height),
            egui::pos2(play_x, rect.bottom()),
        ],
        egui::Stroke::new(
            if hover_hit || app.dragging_playhead { 3.0 } else { 2.0 },
            egui::Color32::RED,
        ),
    );
    // Triangle Head in Ruler
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(play_x - 6.0, ruler_rect.top() + ruler_height * 0.4),
            egui::pos2(play_x + 6.0, ruler_rect.top() + ruler_height * 0.4),
            egui::pos2(play_x, ruler_rect.bottom()),
        ],
        egui::Color32::RED,
        egui::Stroke::NONE,
    ));

    let mut changed = false;
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let hit = (pos.x - play_x).abs() <= 10.0;
            // Sprawdzenie czy kliknieto w Ruler
            let in_ruler = ruler_rect.contains(pos);

            if let Some(fade_drag) = hover_fade {
                app.dragging_fade = Some(fade_drag);
            } else if in_ruler || (app.tool == Tool::Hand && hit) {
                // Dragging in Ruler OR grabbing playhead with Hand
                app.dragging_playhead = true;
            } else if app.tool == Tool::Hand {
                app.dragging_timeline = true;
            }
        }
    }
    if response.drag_stopped() {
        app.dragging_playhead = false;
        app.dragging_timeline = false;
        app.dragging_fade = None;
        app.dragging_clip = None;
    }

    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let in_ruler = ruler_rect.contains(pos);
            
            // Logic: Selection (check clips under cursor)
            let mut selected = None;
            // Only check clips if NOT in ruler
            if !in_ruler {
                for (idx, clip) in app.clips.iter().enumerate() {
                    let start_x = left + (clip.start - app.timeline_offset) * app.timeline_zoom;
                    let end_x = left + (clip.end - app.timeline_offset) * app.timeline_zoom;
                    if pos.x >= start_x && pos.x <= end_x {
                        selected = Some(idx);
                        break;
                    }
                }
            }

            let t = app.timeline_offset + ((pos.x - left) / app.timeline_zoom).clamp(0.0, window);

            if let Some(fade_drag) = app.dragging_fade {
                 if let Some(clip) = app.clips.get_mut(fade_drag.clip_idx) {
                    let duration = (clip.end - clip.start).max(0.0);
                    let t_local = t.clamp(clip.start, clip.end);
                    match fade_drag.kind {
                        FadeKind::In => {
                            let max = (duration - clip.fade_out).max(0.0);
                            clip.fade_in = (t_local - clip.start).max(0.0).min(max);
                        }
                        FadeKind::Out => {
                            let max = (duration - clip.fade_in).max(0.0);
                            clip.fade_out = (clip.end - t_local).max(0.0).min(max);
                        }
                    }
                    app.selected_clip = Some(fade_drag.clip_idx);
                    changed = true;
                }
            } else if let Some(drag_idx) = app.dragging_clip {
                // Clip dragging - move the clip in time
                if let Some(clip) = app.clips.get_mut(drag_idx) {
                    let clip_duration = clip.end - clip.start;
                    let new_start = (t - app.drag_clip_offset).max(0.0);
                    clip.start = new_start;
                    clip.end = new_start + clip_duration;
                    changed = true;
                }
            } else if app.dragging_playhead || (in_ruler && (response.clicked() || response.dragged())) {
                // Scrubbing via Ruler or Playhead Drag
                app.playhead = snap_time(t, app.timeline_zoom);
                app.dragging_playhead = true;
                changed = true;
            } else if response.clicked() {
                if app.tool == Tool::Scissors {
                     // Cut logic
                     let by_time = app.clips.iter().position(|clip| t > clip.start && t < clip.end);
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
                    // Regular Selection
                    app.selected_clip = selected;
                    changed = true;
                }
            } else if app.dragging_timeline && app.tool == Tool::Hand {
                let delta = ui.ctx().input(|i| i.pointer.delta()).x;
                if delta.abs() > 0.0 {
                     app.timeline_offset = clamp_offset(app.timeline_offset - delta / app.timeline_zoom, app.duration, window);
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
        asset_id: clip.asset_id,
        start: t,
        end: clip.end,
        fade_in: 0.0,
        fade_out: clip.fade_out,
        linked: clip.linked,
        video_enabled: clip.video_enabled,
        audio_enabled: clip.audio_enabled,
    };
    clips[idx].end = t;
    clips[idx].fade_out = 0.0;
    clips.insert(idx + 1, right);
    Some(idx + 1)
}




impl VideoEditorApp {
    fn save_project_as(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Rust Video Editor Project", &["rev", "json"])
            .save_file() 
        {
            let data = ProjectData {
                input_path: self.input_path.clone(),
                output_path: self.output_path.clone(),
                playhead: self.playhead,
                clips: self.clips.clone(),
                media_library: self.media_library.clone(),
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
                        self.output_path = data.output_path;
                        self.clips = data.clips;
                        self.media_library = data.media_library;
                        self.duration = data.duration;
                        self.video_width = data.video_width;
                        self.video_height = data.video_height;
                        self.video_fps = data.video_fps;
                        self.playhead = data.playhead;
                        
                        // Reset stanu UI
                        self.selected_clip = None;
                        self.stop_playback();
                        
                        // Przywrocenie zasobow (podglady, waveform)
                        if !self.input_path.is_empty() {
                            self.prepare_media_assets(ctx);
                        }
                        
                        // Regeneracja miniatur biblioteki
                        self.media_thumbs.clear();
                        for (idx, asset) in self.media_library.iter().enumerate() {
                             let mut thumb = None;
                             let path = Path::new(&asset.path);
                             if asset.kind == MediaType::Image {
                                 if let Ok(t) = load_texture_from_path(ctx, path, &format!("thumb_{}", idx)) {
                                     thumb = Some(t);
                                 }
                             } else {
                                 // Video thumb
                                 if let Ok(data) = generate_frame_memory(&asset.path, asset.duration * 0.1, 128, 0) { 
                                     if let Ok(t) = load_texture_from_memory(ctx, &data, &format!("thumb_{}", idx)) {
                                         thumb = Some(t);
                                     }
                                 }
                             }
                             if let Some(t) = thumb {
                                 self.media_thumbs.insert(idx, t); 
                             }
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
                        asset_id: None,
                        start: 0.0,
                        end: self.duration,
                        fade_in: 0.0,
                        fade_out: 0.0,
                        linked: true,
                        video_enabled: true,
                        audio_enabled: true,
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

    fn maybe_update_preview_drag(&mut self, _ctx: &egui::Context) {
        if self.input_path.trim().is_empty() || self.duration <= 0.0 {
            return;
        }

        // Jesli watek pracuje, nie robimy nic (drop frame) - to zapewnia plynnosc UI
        if self.preview_busy.load(Ordering::Relaxed) {
             return;
        }

        // Jesli pozycja zmienila sie nieznacznie, tez ignorujemy
        if (self.playhead - self.last_drag_preview_playhead).abs() < 0.1 {
            return;
        }
        
        self.last_drag_preview_playhead = self.playhead;
        
        let busy = self.preview_busy.clone();
        let tx = self.preview_tx.clone();
        // Resolve source!
        // We need access to clips... but we are moving 'self' fields into closure.
        // Complex. Thread needs the path.
        // We can resolve BEFORE spawning.
        let (input, time) = self.resolve_clip_source(self.playhead);
        
        // Ustawiamy flage busy
        busy.store(true, Ordering::Relaxed);
        
        // Spawn watku
        thread::spawn(move || {
            // Low-Res Proxy: 320px szerokosci dla szybkosci
            if let Ok(data) = generate_frame_memory(&input, time, 320, 0) {
                let _ = tx.send((time, data));
            }
            // Zwalniamy flage
            busy.store(false, Ordering::Relaxed);
        });
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

    fn resolve_clip_source(&self, time: f32) -> (String, f32) {
        for (_idx, clip) in self.clips.iter().enumerate() {
            if clip.video_enabled && time >= clip.start && time < clip.end {
                let local_time = time - clip.start;
                // Fade in/out logic might be here but for source we just need path
                if let Some(asset_id) = clip.asset_id {
                    // Find asset in library (by index for MVP, assuming valid)
                     if let Some(asset) = self.media_library.get(asset_id) {
                         if asset.kind == MediaType::Video || asset.kind == MediaType::Image {
                             return (asset.path.clone(), local_time);
                         }
                     }
                }
                // Fallback to input_path if no asset_id (legacy clip)
                if clip.asset_id.is_none() {
                     return (self.input_path.clone(), time); // Main video uses global time? No, main video clip usually 0..duration.
                }
            }
        }
        // If no clip found, return input_path and time? Or empty?
        // Default behavior: show input_path at time.
        (self.input_path.clone(), time)
    }

    fn build_preview(&mut self, ctx: &egui::Context) -> Result<()> {
        let (path, local_time) = self.resolve_clip_source(self.playhead);
        if path.is_empty() { return Ok(()); }
        
        let data = generate_frame_memory(&path, local_time, 640, 0)?;
        let texture = load_texture_from_memory(ctx, &data, "preview")?;
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
        
        // Wstępne załadowanie pierwszej ramki (instant preview)
        let (width, height) = scaled_preview_size(self.video_width, self.video_height, 640);
        let (start_input, start_time) = self.resolve_clip_source(self.playhead);
        if let Ok(frame_data) = generate_frame_memory(&start_input, start_time, width, height as i32) {
            if let Ok(image) = image::load_from_memory(&frame_data) {
                let rgba = image.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba.into_raw());
                // Aktualizuj preview - ramka będzie widoczna od razu
                if let Ok(mut frames) = self.playback_frames.lock() {
                    *frames = Some(color_image);
                }
            }
        }
        
        // Initialize start position for audio sync
        self.playback_start_playhead = self.playhead;

        // VIDEO SYNC: Start video thread, but wait for signal before starting audio and time
        self.waiting_for_video_ready = true;
        self.video_ready_signal.store(false, Ordering::Relaxed);
        
        self.start_video_playback()?;
        // Audio will be started in update() when video is ready
        Ok(())
    }

    fn start_audio_playback(&mut self) -> Result<()> {
        // Early exit if no valid input
        if self.input_path.is_empty() && self.media_library.is_empty() {
            return Ok(());
        }
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

        // Collect valid audio intervals for masking
        // (start, end)
        let mut audio_intervals = Vec::new();
        for clip in &self.clips {
             if clip.audio_enabled {
                 audio_intervals.push((clip.start, clip.end));
             }
        }
        let audio_intervals = Arc::new(audio_intervals);
        let playback_start_playhead_cp = self.playback_start_playhead;

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
                        
                        // Masking logic
                        let intervals = audio_intervals.clone();
                        let start_ph = playback_start_playhead_cp;
                        // We need calculate time for each sample? Expensive. 
                        // Calculate for block? 
                        // Let's do imprecise block masking or sample precise.
                        // Sample precise is better.
                        let current_played = samples_played.load(Ordering::Relaxed);
                        for (i, sample) in data.iter_mut().enumerate() {
                            let time = start_ph + (current_played + i as u64) as f32 / (sample_rate as f32 * channels as f32);
                            let mut valid = false;
                            for (s, e) in intervals.iter() {
                                if time >= *s && time < *e {
                                    valid = true;
                                    break;
                                }
                            }
                            if !valid {
                                *sample = 0;
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

                        // Masking logic
                        let intervals = audio_intervals.clone();
                        let start_ph = playback_start_playhead_cp;
                        let current_played = samples_played.load(Ordering::Relaxed);
                        for (i, sample) in data.iter_mut().enumerate() {
                            let time = start_ph + (current_played + i as u64) as f32 / (sample_rate as f32 * channels as f32);
                            let mut valid = false;
                            for (s, e) in intervals.iter() {
                                if time >= *s && time < *e {
                                    valid = true;
                                    break;
                                }
                            }
                            if !valid {
                                *sample = 0.0;
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

                        // Masking logic
                        let intervals = audio_intervals.clone();
                        let start_ph = playback_start_playhead_cp;
                        let current_played = samples_played.load(Ordering::Relaxed);
                        for (i, sample) in data.iter_mut().enumerate() {
                            let time = start_ph + (current_played + i as u64) as f32 / (sample_rate as f32 * channels as f32);
                            let mut valid = false;
                            for (s, e) in intervals.iter() {
                                if time >= *s && time < *e {
                                    valid = true;
                                    break;
                                }
                            }
                            if !valid {
                                *sample = 32768;
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
        // Early exit if no valid input
        if self.input_path.is_empty() && self.media_library.is_empty() {
            return Ok(());
        }
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
        let ready_signal = Arc::clone(&self.video_ready_signal); // VIDEO SYNC
        let hw_accel = self.hw_accel_mode; // Capture for thread
        
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

            let start_time_str = format!("{:.3}", start_time);
            
            let mut cmd = Command::new("ffmpeg");
            cmd.args(["-hide_banner", "-loglevel", "error"]);
            
            match hw_accel {
                HwAccelMode::Auto => { cmd.args(["-hwaccel", "auto"]); },
                HwAccelMode::Cuda => { cmd.args(["-hwaccel", "cuda"]); },
                HwAccelMode::Vaapi => { cmd.args(["-hwaccel", "vaapi"]); },
                HwAccelMode::VideoToolbox => { cmd.args(["-hwaccel", "videotoolbox"]); },
                HwAccelMode::None => {}, 
            }
            
            let mut child = match cmd
                .args([
                    "-ss", &start_time_str,
                    "-i", &input,
                    "-vf", &vf_string,
                    "-f", "rawvideo",
                    "-pix_fmt", "rgba",
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
                if let Err(e) = stdout.read_exact(&mut buffer) {
                    println!("FFmpeg read error (Unexpected EOF?): {}", e);
                    break;
                }
                
                // --- Frame Dropping Logic ---
                let video_timestamp = frame_idx as f32 / fps;
                let target_video_rel = video_timestamp - start_time; // Time relative to playback start
                let played_samples = audio_clock.load(Ordering::Relaxed);
                let current_audio_time = played_samples as f32 / (sample_rate as f32 * channels as f32);
                let early_diff = target_video_rel - current_audio_time;
                
                // Jesli jestesmy spoznieni więcej niż 50ms I wideo juz ruszylo (signal=true)
                if ready_signal.load(Ordering::Relaxed) && early_diff < -0.05 {
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
                    
                    let diff = target_video_rel - current_audio_time;
                    
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
                
                // Signal readiness AFTER frame is pushed
                if !ready_signal.load(Ordering::Relaxed) {
                    ready_signal.store(true, Ordering::Relaxed);
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



impl Default for VideoEditorApp {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
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
            selected_track: TrackType::Both,
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
            last_drag_preview_playhead: -1.0,
            live_drag_preview: true,
            tool: Tool::Hand,
            dragging_timeline: false,
            dragging_fade: None,
            dragging_clip: None,
            drag_clip_offset: 0.0,

            ripple_delete: false,
            show_settings: false,
            language: Language::En,
            text: TextResources::new(Language::En),
            
            media_library: Vec::new(),
            media_thumbs: HashMap::new(),
            language_switch_start: None,
            status: String::new(),
            preview_rx: rx,
            preview_tx: tx,
            preview_busy: Arc::new(AtomicBool::new(false)),
            frame_cache: HashMap::new(),
            frame_cache_max_size: 100,  // Max 100 frames in cache (~50MB for 720p)
            
            waiting_for_video_ready: false,
            video_ready_signal: Arc::new(AtomicBool::new(false)),
            playback_start_playhead: 0.0,
            
            hw_accel_mode: HwAccelMode::None,
        }
    }
}
