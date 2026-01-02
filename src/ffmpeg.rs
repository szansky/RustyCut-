// ffmpeg.rs - Wszystkie operacje FFmpeg
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::fs;

use crate::types::Clip;

/// Uruchamia FFmpeg z podanymi argumentami
pub fn run_ffmpeg(args: &[&str]) -> Result<()> {
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

/// Generuje pojedynczą ramkę z wideo (z hardware acceleration)
pub fn generate_frame_memory(input: &str, time: f32, width: u32, height: i32) -> Result<Vec<u8>> {
    let width_str = if width == 0 { "-1".to_string() } else { width.to_string() };
    let height_str = if height == 0 { "-1".to_string() } else { height.to_string() };
    let time_str = format!("{:.3}", time.max(0.0));
    let scale_str = format!("scale={width_str}:{height_str}");

    let output = Command::new("ffmpeg")
        .args([
            "-y",
            "-hwaccel", "auto",  // GPU acceleration
            "-ss", &time_str,
            "-i", input,
            "-frames:v", "1",
            "-vf", &scale_str,
            "-f", "image2pipe",
            "-vcodec", "png",
            "-",
        ])
        .output()
        .context("Nie mozna uruchomic ffmpeg dla frame memory")?;

    if !output.status.success() {
        return Err(anyhow!("ffmpeg frame error: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(output.stdout)
}

/// Pobiera informacje o wideo przez ffprobe
pub fn get_video_info_ffprobe(path: &str) -> Result<(f32, u32, u32, f32)> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height,duration,r_frame_rate",
            "-of", "csv=p=0",
            path,
        ])
        .output()
        .context("Nie mozna uruchomic ffprobe")?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split(',').collect();
    if parts.len() < 4 {
        return Err(anyhow!("Nieprawidlowy format ffprobe: {}", stdout));
    }
    
    let width: u32 = parts[0].parse().unwrap_or(1920);
    let height: u32 = parts[1].parse().unwrap_or(1080);
    let duration: f32 = parts[2].parse().unwrap_or(0.0);
    let fps = parse_fps(parts[3]).unwrap_or(30.0);
    
    Ok((duration, width, height, fps))
}

/// Parsuje FPS z formatu "30/1" lub "29.97"
pub fn parse_fps(value: &str) -> Option<f32> {
    if let Some((num, den)) = value.split_once('/') {
        let n: f32 = num.trim().parse().ok()?;
        let d: f32 = den.trim().parse().ok()?;
        if d > 0.0 {
            return Some(n / d);
        }
    }
    value.trim().parse().ok()
}

/// Generuje waveform z audio
pub fn generate_waveform(input: &str, output: &Path) -> Result<()> {
    run_ffmpeg(&[
        "-y",
        "-i", input,
        "-filter_complex", "showwavespic=s=2048x100:colors=white",
        "-frames:v", "1",
        output.to_str().unwrap_or("waveform.png"),
    ])
}

/// Tworzy katalog tymczasowy
pub fn create_temp_dir() -> Result<PathBuf> {
    let base = std::env::temp_dir();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = base.join(format!("rust_editor_video_{nonce}"));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Buduje filtry fade dla klipu
pub fn build_fade_filters(clip: &Clip) -> (Option<String>, Option<String>) {
    let duration = (clip.end - clip.start).max(0.0);
    let mut vf_parts = Vec::new();
    let mut af_parts = Vec::new();

    if clip.fade_in > 0.0 {
        vf_parts.push(format!("fade=t=in:st=0:d={:.2}", clip.fade_in));
        af_parts.push(format!("afade=t=in:st=0:d={:.2}", clip.fade_in));
    }
    if clip.fade_out > 0.0 {
        let out_start = (duration - clip.fade_out).max(0.0);
        vf_parts.push(format!("fade=t=out:st={:.2}:d={:.2}", out_start, clip.fade_out));
        af_parts.push(format!("afade=t=out:st={:.2}:d={:.2}", out_start, clip.fade_out));
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

/// Renderuje wideo na podstawie listy klipów
pub fn render_video(input_path: &str, output_path: &str, clips: &[Clip]) -> Result<()> {
    if clips.is_empty() {
        return Err(anyhow!("Brak klipow do renderowania"));
    }
    
    let temp_dir = create_temp_dir()?;
    let mut segment_paths: Vec<PathBuf> = Vec::new();

    for (i, clip) in clips.iter().enumerate() {
        if !clip.video_enabled && !clip.audio_enabled {
            continue;
        }
        
        let seg_path = temp_dir.join(format!("seg_{i:04}.mp4"));
        let duration = clip.end - clip.start;
        
        let (vf, af) = build_fade_filters(clip);
        
        let mut args: Vec<String> = vec![
            "-y".into(),
            "-hwaccel".into(), "auto".into(),
            "-ss".into(), format!("{:.3}", clip.start),
            "-t".into(), format!("{:.3}", duration),
            "-i".into(), input_path.into(),
        ];

        if let Some(vf_str) = vf {
            args.push("-vf".into());
            args.push(vf_str);
        }
        if let Some(af_str) = af {
            args.push("-af".into());
            args.push(af_str);
        }

        // Kodeki
        args.push("-c:v".into());
        args.push("libx264".into());
        args.push("-preset".into());
        args.push("fast".into());
        args.push("-crf".into());
        args.push("18".into());
        args.push("-c:a".into());
        args.push("aac".into());
        args.push("-b:a".into());
        args.push("192k".into());
        args.push(seg_path.to_string_lossy().into());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        run_ffmpeg(&args_refs)?;
        segment_paths.push(seg_path);
    }

    if segment_paths.is_empty() {
        return Err(anyhow!("Brak segmentow do polaczenia"));
    }

    // Concat lista
    let concat_list = temp_dir.join("concat.txt");
    let concat_content: String = segment_paths
        .iter()
        .map(|p| format!("file '{}'\n", p.to_string_lossy()))
        .collect();
    fs::write(&concat_list, concat_content)?;

    // Concat
    run_ffmpeg(&[
        "-y",
        "-f", "concat",
        "-safe", "0",
        "-i", concat_list.to_str().unwrap(),
        "-c", "copy",
        output_path,
    ])?;

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
    
    Ok(())
}
