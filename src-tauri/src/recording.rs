use std::{fs, io::Write, os::windows::process::CommandExt, path::{Path, PathBuf}, process::{Child, Command, Stdio}, sync::Mutex, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{Emitter, Manager, Position, PhysicalPosition};
use tauri_plugin_dialog::DialogExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows::Win32::{Foundation::HANDLE, System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JobObjectExtendedLimitInformation}};

const NO_WINDOW: u32 = 0x08000000;
static INSTALL: Mutex<()> = Mutex::new(());
#[derive(Clone, Serialize, Deserialize)]
pub struct Options { pub fps: u32, pub cursor: bool, #[serde(default)] pub system_audio: bool, #[serde(default)] pub microphone: bool }
impl Default for Options { fn default() -> Self { Self { fps: 30, cursor: true, system_audio: false, microphone: false } } }
#[derive(Clone, Copy, Debug)]
pub struct Area { pub x: i32, pub y: i32, pub width: u32, pub height: u32 }
#[derive(Default)]
pub struct RecordingState(pub Mutex<Recorder>);
#[derive(Default)]
pub struct Recorder {
    child: Option<Child>, area: Option<Area>, options: Options,
    folder: Option<PathBuf>, segments: usize, ready: Option<PathBuf>,
    elapsed: Duration, since: Option<Instant>, error: Option<String>,
    job: Option<OwnedHandle>,
    audio: Option<super::recording_audio::AudioGroup>, encoder: Option<PathBuf>,
}
impl Drop for Recorder { fn drop(&mut self) { let _ = self.finish_segment(); } }
pub fn root() -> Result<PathBuf, String> { Ok(dirs::data_local_dir().ok_or("Local storage unavailable")?.join("Textractor/recording")) }
fn binary() -> Result<PathBuf, String> { Ok(root()?.join("ffmpeg.exe")) }
fn options() -> Options { root().ok().and_then(|p| fs::read(p.join("options.json")).ok()).and_then(|v| serde_json::from_slice::<Options>(&v).ok()).filter(|o| [15, 30, 60].contains(&o.fps)).unwrap_or_default() }
fn command(exe: &Path) -> Command { let mut c = Command::new(exe); c.creation_flags(NO_WINDOW).stdout(Stdio::null()); c }
pub fn capture_args(area: Area, options: &Options) -> Vec<String> {
    ["-hide_banner".into(), "-loglevel".into(), "error".into(), "-y".into(),
        "-f".into(), "gdigrab".into(), "-framerate".into(), options.fps.to_string(),
        "-draw_mouse".into(), if options.cursor { "1" } else { "0" }.into(),
        "-offset_x".into(), area.x.to_string(), "-offset_y".into(), area.y.to_string(),
        "-video_size".into(), format!("{}x{}", area.width, area.height), "-i".into(), "desktop".into(),
        "-an".into(), "-vf".into(), "pad=ceil(iw/2)*2:ceil(ih/2)*2".into(),
        "-c:v".into(), "libx264".into(), "-preset".into(), "ultrafast".into(), "-crf".into(), "23".into(),
        "-pix_fmt".into(), "yuv420p".into(), "-r".into(), options.fps.to_string(), "-movflags".into(), "+faststart".into()].into()
}
impl Recorder {
    fn status(&mut self) -> Value {
        if let Some(audio) = &self.audio { let errors = audio.errors(); if !errors.is_empty() { self.error = Some(errors.join(" ")); } }
        if let Some(child) = &mut self.child {
            if let Ok(Some(exit)) = child.try_wait() {
                self.child = None;
                if let Some(audio) = self.audio.take() { let _ = audio.finish(); }
                if let Some(since) = self.since.take() { self.elapsed += since.elapsed(); }
                self.error = Some(format!("Recording stopped unexpectedly ({exit}). Your segments remain in {}", self.folder.as_ref().map(|p| p.display().to_string()).unwrap_or_default()));
            }
        }
        json!({"installed": binary().is_ok_and(|p| p.exists()), "options": options(),
            "phase": if self.child.is_some() { "recording" } else if self.ready.is_some() { "ready" } else if self.folder.is_some() { "paused" } else { "idle" },
            "seconds": (self.elapsed + self.since.map(|i| i.elapsed()).unwrap_or_default()).as_secs(),
            "error": self.error, "recovery_folder": self.folder })
    }
    fn begin_segment(&mut self) -> Result<(), String> {
        self.begin_segment_with(&binary()?)
    }
    fn begin_segment_with(&mut self, exe: &Path) -> Result<(), String> {
        if self.options.system_audio || self.options.microphone {
            self.audio = Some(super::recording_audio::AudioGroup::prepare(self.folder.as_ref().ok_or("No recording session")?, self.segments, self.options.system_audio, self.options.microphone)?);
        }
        let result = self.begin_segment_using(exe, capture_args(self.area.ok_or("No recording area")?, &self.options));
        if result.is_err() { if let Some(audio) = self.audio.take() { let _ = audio.finish(); } }
        result
    }
    fn begin_segment_using(&mut self, exe: &Path, args: Vec<String>) -> Result<(), String> {
        let folder = self.folder.as_ref().ok_or("No recording session")?;
        let output = folder.join(format!("segment-{}.video.mp4", self.segments));
        let log = fs::File::create(folder.join(format!("segment-{}.log", self.segments))).map_err(|e| e.to_string())?;
        let started = Instant::now();
        if let Some(audio) = &self.audio { audio.begin(); }
        let mut child = command(exe).args(args).arg(output).stdin(Stdio::piped()).stderr(log).spawn().map_err(|e| format!("Could not start FFmpeg: {e}"))?;
        // The OS kills FFmpeg if Textractor crashes or is terminated, so an
        // orphan process can never continue recording without the tray app.
        let attach = (|| -> Result<(), String> { unsafe {
            if self.job.is_none() {
                let handle = CreateJobObjectW(None, None).map_err(|e| e.to_string())?;
                let owned = OwnedHandle::from_raw_handle(handle.0);
                let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                SetInformationJobObject(handle, JobObjectExtendedLimitInformation, &limits as *const _ as _, std::mem::size_of_val(&limits) as u32).map_err(|e| e.to_string())?;
                self.job = Some(owned);
            }
            AssignProcessToJobObject(HANDLE(self.job.as_ref().unwrap().as_raw_handle()), HANDLE(child.as_raw_handle())).map_err(|e| e.to_string())?;
            Ok(())
        } })();
        if let Err(e) = attach { let _ = child.kill(); let _ = child.wait(); return Err(format!("Recording safety guard unavailable: {e}")); }
        std::thread::sleep(Duration::from_millis(600));
        if let Some(code) = child.try_wait().map_err(|e| e.to_string())? { return Err(format!("FFmpeg could not start ({code}). Check the recording log in {}", folder.display())); }
        self.encoder = Some(exe.to_path_buf()); self.child = Some(child); self.segments += 1; self.since = Some(started); self.error = None;
        Ok(())
    }
    fn finish_segment(&mut self) -> Result<(), String> {
        let audio = self.audio.take().map(|audio| audio.finish());
        if let Some(mut child) = self.child.take() {
            if let Some(since) = self.since.take() { self.elapsed += since.elapsed(); }
            if let Some(mut input) = child.stdin.take() { let _ = input.write_all(b"q\n"); }
            let deadline = Instant::now() + Duration::from_secs(12);
            loop {
                match child.try_wait() {
                    Ok(Some(exit)) => { if !exit.success() { return Err("FFmpeg could not finish the segment. Recording files are preserved in AppData.".into()); } break; },
                    Err(e) => { let _ = child.kill(); let _ = child.wait(); return Err(e.to_string()); },
                    _ if Instant::now() >= deadline => { let _ = child.kill(); let _ = child.wait(); return Err("FFmpeg timed out. Recording segments are preserved in AppData.".into()); },
                    _ => std::thread::sleep(Duration::from_millis(50)),
                }
            }
            let folder = self.folder.as_ref().ok_or("No recording folder")?;
            let input = folder.join(format!("segment-{}.video.mp4", self.segments - 1));
            let output = folder.join(format!("segment-{}.mp4", self.segments - 1));
            if let Some(audio) = audio {
                if !audio.errors.is_empty() { self.error = Some(audio.errors.join(" ")); }
                let result = command(self.encoder.as_ref().ok_or("FFmpeg unavailable")?).args(mux_args(&input, &audio.tracks, &output)).stderr(Stdio::piped()).output().map_err(|e| e.to_string())?;
                if !result.status.success() { return Err(format!("Could not add recorded audio: {}. Original tracks remain in {}", String::from_utf8_lossy(&result.stderr), folder.display())); }
                let _ = fs::remove_file(input); for track in audio.tracks { let _ = fs::remove_file(track.path); }
            } else { fs::rename(input, output).map_err(|e| e.to_string())?; }
        }
        Ok(())
    }
    fn finalize(&mut self) -> Result<(), String> {
        self.finalize_with(&binary()?)
    }
    fn finalize_with(&mut self, exe: &Path) -> Result<(), String> {
        self.finish_segment()?;
        if self.ready.is_some() { return Ok(()); }
        let folder = self.folder.as_ref().ok_or("No recording to stop")?;
        if self.segments == 0 { return Err("No video frames were recorded".into()); }
        let output = folder.join("recording.mp4");
        if self.segments == 1 { fs::copy(folder.join("segment-0.mp4"), &output).map_err(|e| e.to_string())?; }
        else {
            let list = (0..self.segments).map(|i| format!("file 'segment-{i}.mp4'\n")).collect::<String>();
            fs::write(folder.join("segments.txt"), list).map_err(|e| e.to_string())?;
            let result = command(exe).current_dir(folder).args(["-hide_banner", "-loglevel", "error", "-y", "-f", "concat", "-safe", "1", "-i", "segments.txt", "-c", "copy", "-movflags", "+faststart", "recording.mp4"]).stderr(Stdio::piped()).output().map_err(|e| e.to_string())?;
            if !result.status.success() { return Err(format!("Could not combine recording: {}", String::from_utf8_lossy(&result.stderr))); }
        }
        self.ready = Some(output); Ok(())
    }
}
pub fn mux_args(video: &Path, audio: &[super::recording_audio::Track], output: &Path) -> Vec<String> {
    let mut args = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-y".into(), "-i".into(), video.to_string_lossy().into_owned()];
    for track in audio { args.extend(["-f".into(), track.format.into(), "-ar".into(), track.rate.to_string(), "-ac".into(), track.channels.to_string(), "-i".into(), track.path.to_string_lossy().into_owned()]); }
    if audio.len() == 2 { args.extend(["-filter_complex".into(), "[1:a][2:a]amix=inputs=2:duration=longest:normalize=0,alimiter=limit=0.95:latency=1,apad[a]".into(), "-map".into(), "0:v:0".into(), "-map".into(), "[a]".into()]); }
    else { args.extend(["-map".into(), "0:v:0".into(), "-map".into(), "1:a:0".into(), "-af".into(), "apad".into()]); }
    args.extend(["-c:v".into(), "copy".into(), "-c:a".into(), "aac".into(), "-ar".into(), "48000".into(), "-ac".into(), "2".into(), "-b:a".into(), "192k".into(), "-shortest".into(), "-movflags".into(), "+faststart".into(), output.to_string_lossy().into_owned()]); args
}
fn indicate(app: &tauri::AppHandle, active: bool) {
    if !active { if let Some(w) = app.get_webview_window("region") { let _ = w.hide(); } }
    if let Some(tray) = app.tray_by_id("textractor") {
        let _ = tray.set_tooltip(Some(if active { "Textractor • Recording (click to show controls)" } else { "Textractor" }));
        if active {
            if let Ok(icon) = image::load_from_memory(include_bytes!("../icons/tray-icon.png")) {
                let mut pixels = icon.to_rgba8();
                for y in 20..32 { for x in 20..32 { if (x as i32 - 26).pow(2) + (y as i32 - 26).pow(2) <= 25 { pixels.put_pixel(x, y, image::Rgba([255, 121, 103, 255])); } } }
                let _ = tray.set_icon(Some(tauri::image::Image::new_owned(pixels.into_raw(), 32, 32)));
            }
        } else { let _ = tray.set_icon(Some(super::tray_icon())); }
    }
    let _ = app.emit("recording-changed", ());
}
fn show_outline(app: &tauri::AppHandle, area: Area) {
    if let Some(w) = app.get_webview_window("region") {
        let _ = w.set_content_protected(true);
        let _ = w.set_ignore_cursor_events(true);
        let _ = w.set_position(Position::Physical(PhysicalPosition::new(area.x, area.y)));
        let _ = w.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(area.width, area.height)));
        // A genuinely hollow native window, not a fullscreen transparent layer:
        // only the thin edge participates in desktop composition/capture.
        if let Ok(hwnd) = w.hwnd() { unsafe {
            use windows::Win32::{Foundation::HWND, Graphics::Gdi::{CreateRectRgn, CombineRgn, DeleteObject, SetWindowRgn, HGDIOBJ, RGN_DIFF}};
            let outer = CreateRectRgn(0, 0, area.width as i32, area.height as i32);
            let inner = CreateRectRgn(2, 2, area.width as i32 - 2, area.height as i32 - 2);
            if !outer.0.is_null() && !inner.0.is_null() {
                CombineRgn(Some(outer), Some(outer), Some(inner), RGN_DIFF);
                if SetWindowRgn(HWND(hwnd.0), Some(outer), true) == 0 { let _ = DeleteObject(HGDIOBJ(outer.0)); }
            } else if !outer.0.is_null() { let _ = DeleteObject(HGDIOBJ(outer.0)); }
            if !inner.0.is_null() { let _ = DeleteObject(HGDIOBJ(inner.0)); }
        } }
        let _ = w.show();
    }
}
pub fn show_controls(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("recorder") { let _ = w.set_content_protected(true); let _ = w.show(); let _ = w.set_focus(); }
}
pub fn is_active(app: &tauri::AppHandle) -> bool { app.state::<RecordingState>().0.try_lock().map(|r| r.folder.is_some()).unwrap_or(true) }
#[tauri::command]
pub async fn recording_status(app: tauri::AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<RecordingState>().0.lock().map_err(|e| e.to_string()).map(|mut r| r.status())).await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn save_recording_options(options: Options) -> Result<(), String> {
    if ![15, 30, 60].contains(&options.fps) { return Err("Choose 15, 30, or 60 FPS".into()); }
    let path = root()?; fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    fs::write(path.join("options.json"), serde_json::to_vec(&options).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn install_recording_engine() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let _job = INSTALL.lock().map_err(|e| e.to_string())?;
        if binary()?.exists() { return Ok(()); }
        let path = root()?; fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        fs::write(path.join("install.ps1"), include_str!("recording_install.ps1")).map_err(|e| e.to_string())?;
        let windows = std::env::var_os("WINDIR").ok_or("Windows directory unavailable")?;
        let result = command(&PathBuf::from(windows).join("System32/WindowsPowerShell/v1.0/powershell.exe")).args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(path.join("install.ps1")).arg("-Destination").arg(&path).stderr(Stdio::piped()).output().map_err(|e| e.to_string())?;
        if !result.status.success() { return Err(format!("FFmpeg download failed. Retry when online. {}", String::from_utf8_lossy(&result.stderr).chars().take(500).collect::<String>())); }
        Ok(())
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn open_recording_settings(app: tauri::AppHandle) {
    super::cancel_capture(app.clone()); super::show_settings(&app); let _ = app.emit_to("main", "settings-tab", "recorder");
}
#[tauri::command]
pub async fn start_recording(app: tauri::AppHandle, full_screen: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !binary()?.exists() { return Err("Install FFmpeg in Settings → Recorder first".into()); }
        let state = app.state::<RecordingState>(); let mut r = state.0.lock().map_err(|e| e.to_string())?;
        if r.folder.is_some() { return Err("Save or discard the current recording first".into()); }
        let area = if full_screen {
            let window = app.get_webview_window("main").ok_or("Settings unavailable")?;
            let monitor = window.current_monitor().map_err(|e| e.to_string())?.ok_or("No monitor found")?;
            Area { x: monitor.position().x, y: monitor.position().y, width: monitor.size().width, height: monitor.size().height }
        } else {
            let state = app.state::<super::AppState>(); let selected = state.last_result.lock().map_err(|e| e.to_string())?.clone().ok_or("Select a recording region first")?;
            Area { x: selected.x, y: selected.y, width: selected.width, height: selected.height }
        };
        if area.width < 8 || area.height < 8 || area.width > 16384 || area.height > 16384 { return Err("Unsupported recording dimensions".into()); }
        let folder = root()?.join("captures").join(format!("capture-{}", SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis()));
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        *r = Recorder::default();
        r.folder = Some(folder); r.area = Some(area); r.options = options();
        super::cancel_capture(app.clone());
        if let Some(w) = app.get_webview_window("main") { let _ = w.hide(); }
        if let Some(w) = app.get_webview_window("recorder") {
            let reference = app.get_webview_window(if full_screen { "main" } else { "capture" });
            let monitor = reference.and_then(|w| w.current_monitor().ok().flatten());
            let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
            let width = (352.0 * scale).round() as i32;
            let height = (84.0 * scale).round() as i32;
            let gap = (8.0 * scale).round() as i32;
            let mut x = area.x.saturating_add((area.width as i32 - width) / 2);
            let mut y = area.y.saturating_add(area.height as i32).saturating_add(gap);
            if let Some(m) = monitor {
                x = x.clamp(m.position().x, (m.position().x + m.size().width as i32 - width).max(m.position().x));
                if y + height > m.position().y + m.size().height as i32 { y = area.y - height - gap; }
                y = y.clamp(m.position().y, (m.position().y + m.size().height as i32 - height).max(m.position().y));
            }
            let _ = w.set_position(Position::Physical(PhysicalPosition::new(x, y)));
        }
        if let Err(e) = r.begin_segment() {
            *r = Recorder::default();
            if full_screen { super::show_settings(&app); }
            else if let Some(w) = app.get_webview_window("capture") { let _ = w.show(); let _ = w.set_focus(); }
            return Err(e);
        }
        drop(r); indicate(&app, true); show_outline(&app, area); show_controls(&app); Ok(())
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn pause_recording(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<RecordingState>(); let mut r = state.0.lock().map_err(|e| e.to_string())?;
        if r.ready.is_some() || r.folder.is_none() { return Err("No active recording".into()); }
        if r.child.is_some() { r.finish_segment()?; } else { r.begin_segment()?; }
        drop(r); indicate(&app, true); Ok(())
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn stop_recording(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<RecordingState>().0.lock().map_err(|e| e.to_string())?.finalize()?;
        indicate(&app, false); show_controls(&app); Ok(())
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn save_recording(app: tauri::AppHandle) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = app.state::<RecordingState>().0.lock().map_err(|e| e.to_string())?.ready.clone().ok_or("Stop the recording before saving")?;
        if let Some(w) = app.get_webview_window("recorder") { let _ = w.hide(); }
        let result = (|| {
            let suggested = super::save_files::dialog_default("video", "textractor-recording.mp4");
            let mut dialog = app.dialog().file();
            if let Some(parent) = suggested.parent().filter(|p| p.is_dir()) { dialog = dialog.set_directory(parent); }
            let Some(file) = dialog.set_title("Save screen recording").set_file_name(suggested.file_name().unwrap_or_default().to_string_lossy()).add_filter("MP4 video", &["mp4"]).blocking_save_file() else { return Ok(false); };
            let mut path = file.into_path().map_err(|e| e.to_string())?;
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")) { path.set_extension("mp4"); }
            if let (Some(parent), Some(original_parent)) = (path.parent(), source.parent()) {
                let parent = parent.canonicalize().map_err(|e| e.to_string())?;
                let original_parent = original_parent.canonicalize().map_err(|e| e.to_string())?;
                if parent.starts_with(original_parent) { return Err("Choose a save location outside the temporary recording folder.".into()); }
            }
            let saved = super::save_files::copy_unique(&source, &path).map_err(|e| format!("Could not save video: {e}. Original kept at {}", source.display()))?;
            super::save_files::remember("video", &saved);
            super::save_files::reveal(&saved);
            Ok(true)
        })();
        if matches!(result, Ok(true)) {
            let state = app.state::<RecordingState>(); let mut r = state.0.lock().map_err(|e| e.to_string())?;
            // Only app-generated intermediates are removed after successful copy.
            if let Some(folder) = &r.folder {
                for i in 0..r.segments { let _ = fs::remove_file(folder.join(format!("segment-{i}.mp4"))); let _ = fs::remove_file(folder.join(format!("segment-{i}.log"))); }
                let _ = fs::remove_file(folder.join("segments.txt")); let _ = fs::remove_file(folder.join("recording.mp4")); let _ = fs::remove_dir(folder);
            }
            *r = Recorder::default();
        } else { show_controls(&app); }
        result
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn hide_recording_controls(app: tauri::AppHandle) { if let Some(w) = app.get_webview_window("recorder") { let _ = w.hide(); } }
#[tauri::command]
pub async fn cancel_recording(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<RecordingState>(); let mut r = state.0.lock().map_err(|e| e.to_string())?;
        let _ = r.finish_segment();
        // Close without exporting; preserve unsaved segments for recovery.
        *r = Recorder::default(); drop(r);
        hide_recording_controls(app.clone()); indicate(&app, false); Ok(())
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn drag_recording_controls(app: tauri::AppHandle) { if let Some(w) = app.get_webview_window("recorder") { let _ = w.start_dragging(); } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn args_preserve_negative_coordinates_and_odd_sizes() {
        let args = capture_args(Area { x: -1600, y: -200, width: 837, height: 415 }, &Options { fps: 60, cursor: false, ..Default::default() });
        let value = |key: &str| args[args.iter().position(|s| s == key).unwrap() + 1].clone();
        assert_eq!(value("-offset_x"), "-1600"); assert_eq!(value("-offset_y"), "-200");
        assert_eq!(value("-video_size"), "837x415"); assert_eq!(value("-draw_mouse"), "0");
        assert_eq!(value("-framerate"), "60"); assert!(value("-vf").starts_with("pad="));
    }
    #[test] fn invalid_fps_never_writes_preferences() { assert!(save_recording_options(Options { fps: 0, cursor: true, ..Default::default() }).is_err()); }
    #[test] fn older_options_do_not_enable_audio() { let o: Options = serde_json::from_str(r#"{"fps":30,"cursor":true}"#).unwrap(); assert!(!o.system_audio && !o.microphone); }
    fn test_video(desktop: bool) {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata");
        let exe = base.join("recording-runtime/ffmpeg.exe");
        if !exe.exists() { return; }
        let folder = base.join(format!("recording-test-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()));
        fs::create_dir_all(&folder).unwrap();
        let mut r = Recorder::default(); r.folder = Some(folder); r.area = Some(Area { x: 0, y: 0, width: 161, height: 121 });
        let args = || if desktop { capture_args(r.area.unwrap(), &r.options) } else {
            ["-hide_banner", "-loglevel", "error", "-y", "-re", "-f", "lavfi", "-i", "testsrc=size=161x121:rate=30", "-an", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"].map(String::from).to_vec()
        };
        let first_args = args(); let second_args = args();
        r.begin_segment_using(&exe, first_args).expect("start recording");
        std::thread::sleep(Duration::from_millis(300)); r.finish_segment().expect("pause");
        let paused = r.elapsed; std::thread::sleep(Duration::from_millis(120)); assert_eq!(r.elapsed, paused);
        r.begin_segment_using(&exe, second_args).expect("resume"); std::thread::sleep(Duration::from_millis(300)); r.finalize_with(&exe).expect("finalize segments");
        assert_eq!(r.segments, 2); assert!(r.child.is_none());
        let output = command(&exe).args(["-v", "error", "-i"]).arg(r.ready.as_ref().unwrap()).args(["-f", "null", "-"]).stderr(Stdio::piped()).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        // Verify system+microphone mixing and AAC export with deterministic tones,
        // without opening or recording a real microphone during automated tests.
        let folder = r.folder.as_ref().unwrap();
        let mut tracks = vec![];
        for (i, hz) in [440.0f32, 660.0].iter().enumerate() {
            let path = folder.join(format!("tone-{i}.f32")); let mut file = fs::File::create(&path).unwrap();
            for frame in 0..96_000 { let sample = (frame as f32 / 48_000.0 * hz * std::f32::consts::TAU).sin() * 0.2; for _ in 0..2 { file.write_all(&sample.to_le_bytes()).unwrap(); } }
            tracks.push(super::super::recording_audio::Track::float_stereo(path));
        }
        for count in [1, 2] {
            let output = folder.join(format!("with-{count}-audio.mp4"));
            let mixed = command(&exe).args(mux_args(r.ready.as_ref().unwrap(), &tracks[..count], &output)).stderr(Stdio::piped()).output().unwrap();
            assert!(mixed.status.success(), "{}", String::from_utf8_lossy(&mixed.stderr));
            let audio = command(&exe).args(["-v", "error", "-i"]).arg(output).args(["-map", "0:a:0", "-f", "null", "-"]).stderr(Stdio::piped()).output().unwrap();
            assert!(audio.status.success(), "AAC track must decode: {}", String::from_utf8_lossy(&audio.stderr));
        }
    }
    #[test] fn private_ffmpeg_encodes_pauses_resumes_and_decodes() { test_video(false); }
    #[test] #[ignore = "Requires an unlocked interactive Windows desktop"]
    fn private_ffmpeg_records_live_desktop() { test_video(true); }
}
