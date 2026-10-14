use std::{borrow::Cow, io::Cursor, sync::{Mutex, atomic::{AtomicU64, Ordering}}};
mod translation;

use arboard::{Clipboard, ImageData};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use image::{DynamicImage, ImageFormat, RgbaImage};
use serde::{Deserialize, Serialize};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, Position, Size,
};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use windows::{
    Graphics::Imaging::BitmapDecoder,
    Media::Ocr::{OcrEngine, OcrResult},
    Storage::Streams::{DataWriter, InMemoryRandomAccessStream},
    Win32::Foundation::POINT,
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    Win32::UI::WindowsAndMessaging::GetCursorPos,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppConfig {
    hotkey: String,
    launch_at_startup: bool,
    copy_text_automatically: bool,
    #[serde(default = "default_tool_color")]
    tool_color: String,
    #[serde(default = "default_tool_alpha")]
    tool_alpha: f64,
    #[serde(default = "default_tool_thickness")]
    tool_thickness: f64,
    #[serde(default = "default_tool_text_size")]
    tool_text_size: f64,
    #[serde(default = "default_tool_arrow_size")]
    tool_arrow_size: f64,
    #[serde(default = "default_ocr_language")]
    ocr_language: String,
    #[serde(default = "default_translation_language")]
    translation_language: String,
    #[serde(default = "default_translation_source")]
    translation_source: String,
    #[serde(default = "default_extracted_text_size")]
    extracted_text_size: f64,
}

fn default_tool_color() -> String { "#F2BE8D".into() }
fn default_tool_alpha() -> f64 { 1.0 }
fn default_tool_thickness() -> f64 { 3.0 }
fn default_tool_text_size() -> f64 { 20.0 }
fn default_tool_arrow_size() -> f64 { 14.0 }
fn default_ocr_language() -> String { "en".into() }
fn default_translation_language() -> String { "en".into() }
fn default_translation_source() -> String { "auto".into() }
fn default_extracted_text_size() -> f64 { 12.0 }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Shift+2".into(),
            launch_at_startup: false,
            copy_text_automatically: false,
            tool_color: default_tool_color(),
            tool_alpha: default_tool_alpha(),
            tool_thickness: default_tool_thickness(),
            tool_text_size: default_tool_text_size(),
            tool_arrow_size: default_tool_arrow_size(),
            ocr_language: default_ocr_language(),
            translation_language: default_translation_language(),
            translation_source: default_translation_source(),
            extracted_text_size: default_extracted_text_size(),
        }
    }
}

#[derive(Default)]
struct AppState {
    capture_generation: AtomicU64,
    config: Mutex<AppConfig>,
    last_image: Mutex<Option<RgbaImage>>,
    last_result: Mutex<Option<CaptureResult>>,
    tray_capture_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    pending_screen: Mutex<Option<ScreenSnapshot>>,
}

#[derive(Clone)]
struct ScreenSnapshot {
    image: RgbaImage,
    x: i32,
    y: i32,
    data_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selection {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    scale_factor: f64,
}

#[derive(Clone, Serialize)]
struct CaptureResult {
    text: String,
    error: Option<String>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn config_path() -> std::path::PathBuf {
    let base = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
    base.join("Textractor").join("config.json")
}

fn read_config() -> AppConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

fn show_settings(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn cursor_position() -> (i32, i32) {
    let mut point = POINT::default();
    unsafe { let _ = GetCursorPos(&mut point); }
    (point.x, point.y)
}

fn show_capture(app: &tauri::AppHandle) -> Result<(), String> {
    app.state::<AppState>().capture_generation.fetch_add(1, Ordering::SeqCst);
    let window = app.get_webview_window("capture").ok_or("Capture window is unavailable")?;
    window.hide().map_err(|e| e.to_string())?;
    if let Some(result) = app.get_webview_window("result") { let _ = result.hide(); }
    if let Some(region) = app.get_webview_window("region") { let _ = region.hide(); }
    let (cx, cy) = cursor_position();
    // Freeze the screen before showing our transparent overlay. Hardware video
    // surfaces can disappear from desktop duplication after overlay composition.
    let snapshot = capture_monitor_at(cx, cy)?;
    *app.state::<AppState>().pending_screen.lock().map_err(|e| e.to_string())? = Some(snapshot);
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let monitor = monitors.iter().find(|m| {
        let p = m.position(); let s = m.size();
        cx >= p.x && cy >= p.y && cx < p.x + s.width as i32 && cy < p.y + s.height as i32
    }).or_else(|| monitors.first()).ok_or("No monitor was found")?;
    window.set_fullscreen(false).map_err(|e| e.to_string())?;
    window.set_position(Position::Physical(*monitor.position())).map_err(|e| e.to_string())?;
    window.set_size(Size::Physical(*monitor.size())).map_err(|e| e.to_string())?;
    window.set_resizable(false).map_err(|e| e.to_string())?;
    window.set_ignore_cursor_events(false).map_err(|e| e.to_string())?;
    window.set_fullscreen(true).map_err(|e| e.to_string())?;
    app.emit_to("capture", "capture-reset", ()).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_config(app: tauri::AppHandle, state: tauri::State<AppState>) -> AppConfig {
    let config = state.config.lock().expect("config lock").clone();
    if std::env::args().any(|arg| arg == "--portable-smoke") {
        println!("{}", serde_json::json!({"version":env!("CARGO_PKG_VERSION"), "embedded_ui_loaded":true, "webview2_created":true}));
        app.exit(0);
    }
    config
}

#[tauri::command]
fn save_config(app: tauri::AppHandle, state: tauri::State<AppState>, mut config: AppConfig) -> Result<(), String> {
    // Settings may have loaded before the editor saved its tool preferences.
    // Keep the latest tool values when saving shortcut/startup preferences.
    {
        let current = state.config.lock().map_err(|e| e.to_string())?;
        config.tool_color = current.tool_color.clone();
        config.tool_alpha = current.tool_alpha;
        config.tool_thickness = current.tool_thickness;
        config.tool_text_size = current.tool_text_size;
        config.tool_arrow_size = current.tool_arrow_size;
        config.extracted_text_size = current.extracted_text_size;
    }
    app.global_shortcut().unregister_all().map_err(|e| e.to_string())?;
    if let Err(error) = app.global_shortcut().register(config.hotkey.as_str()) {
        let old = state.config.lock().expect("config lock").hotkey.clone();
        let _ = app.global_shortcut().register(old.as_str());
        return Err(format!("That shortcut is unavailable. Windows or another app may already be using it ({error})"));
    }
    let autostart = app.autolaunch();
    let autostart_result = if config.launch_at_startup { autostart.enable() } else { autostart.disable() };
    if let Err(error) = autostart_result {
        app.global_shortcut().unregister_all().ok();
        let old = state.config.lock().expect("config lock").hotkey.clone();
        let _ = app.global_shortcut().register(old.as_str());
        return Err(format!("Windows startup setting could not be updated: {error}"));
    }
    write_config(&config)?;
    if let Some(item) = state.tray_capture_item.lock().map_err(|e| e.to_string())?.as_ref() {
        item.set_accelerator(Some(config.hotkey.as_str())).map_err(|e| format!("Shortcut was saved, but the tray menu could not be updated: {e}"))?;
    }
    *state.config.lock().expect("config lock") = config;
    Ok(())
}

#[tauri::command]
fn get_last_result(state: tauri::State<AppState>) -> Option<CaptureResult> {
    state.last_result.lock().expect("result lock").clone()
}

#[tauri::command]
fn save_tool_config(state: tauri::State<AppState>, color: String, alpha: f64, thickness: f64, text_size: f64, arrow_size: f64, extracted_text_size: Option<f64>) -> Result<(), String> {
    if color.len() != 7 || !color.starts_with('#') || !color[1..].chars().all(|c| c.is_ascii_hexdigit()) || ![alpha, thickness, text_size, arrow_size].iter().all(|n| n.is_finite()) {
        return Err("Invalid tool settings".into());
    }
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    let mut updated = config.clone();
    updated.tool_color = color;
    updated.tool_alpha = alpha.clamp(0.0, 1.0);
    updated.tool_thickness = thickness.clamp(1.0, 64.0);
    updated.tool_text_size = text_size.clamp(8.0, 128.0);
    updated.tool_arrow_size = arrow_size.clamp(4.0, 96.0);
    if let Some(size) = extracted_text_size { if !size.is_finite() { return Err("Invalid text size".into()); } updated.extracted_text_size = size.clamp(8.0, 24.0); }
    write_config(&updated)?;
    *config = updated;
    Ok(())
}

#[tauri::command]
fn get_capture_background(state: tauri::State<AppState>) -> Option<String> {
    state.pending_screen.lock().ok()?.as_ref().map(|snapshot| snapshot.data_url.clone())
}

#[tauri::command]
fn start_capture(app: tauri::AppHandle) -> Result<(), String> { show_capture(&app) }

#[tauri::command]
fn hide_settings(app: tauri::AppHandle) -> Result<(), String> { app.get_webview_window("main").ok_or("Settings window unavailable")?.hide().map_err(|e| e.to_string()) }

#[tauri::command]
fn drag_settings(app: tauri::AppHandle) -> Result<(), String> { app.get_webview_window("main").ok_or("Settings window unavailable")?.start_dragging().map_err(|e| e.to_string()) }

#[tauri::command]
fn cancel_capture(app: tauri::AppHandle) {
    app.state::<AppState>().capture_generation.fetch_add(1, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("capture") { let _ = window.hide(); }
}

#[tauri::command]
fn close_result(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("result") { let _ = window.hide(); }
    if let Some(window) = app.get_webview_window("region") { let _ = window.hide(); }
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    Clipboard::new().and_then(|mut c| c.set_text(text)).map_err(|e| e.to_string())
}

#[tauri::command]
fn accept_ocr_text(state: tauri::State<AppState>, text: String) -> Result<CaptureResult, String> {
    let mut result = state.last_result.lock().map_err(|e| e.to_string())?.clone().ok_or("No selected region")?;
    result.text = correct_numeric_text(&text);
    result.error = None;
    if state.config.lock().map_err(|e| e.to_string())?.copy_text_automatically && !result.text.is_empty() { copy_text(result.text.clone())?; }
    *state.last_result.lock().map_err(|e| e.to_string())? = Some(result.clone());
    Ok(result)
}

#[tauri::command]
async fn translate_text(state: tauri::State<'_, AppState>, text: String) -> Result<serde_json::Value, String> {
    let (source, target) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        (config.translation_source.clone(), config.translation_language.clone())
    };
    translation::translate(text, source, target).await
}

#[tauri::command]
fn copy_last_image(state: tauri::State<AppState>) -> Result<(), String> {
    let image = state.last_image.lock().map_err(|e| e.to_string())?.clone().ok_or("No captured image")?;
    let data = ImageData { width: image.width() as usize, height: image.height() as usize, bytes: Cow::Owned(image.into_raw()) };
    Clipboard::new().and_then(|mut c| c.set_image(data)).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_last_image(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    let image = state.last_image.lock().map_err(|e| e.to_string())?.clone().ok_or("No captured image")?;
    tauri::async_runtime::spawn_blocking(move || {
        let selected = app.dialog().file()
            .set_title("Save captured image")
            .set_file_name("textractor-capture.png")
            .add_filter("PNG image", &["png"])
            .blocking_save_file();
        let Some(selected) = selected else { return Ok(None); };
        let mut path = selected.into_path().map_err(|e| e.to_string())?;
        if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("png")) { path.set_extension("png"); }
        DynamicImage::ImageRgba8(image).save_with_format(&path, ImageFormat::Png).map_err(|e| e.to_string())?;
        Ok(Some(path.to_string_lossy().into_owned()))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn prepare_capture(app: tauri::AppHandle, state: tauri::State<'_, AppState>, selection: Selection) -> Result<String, String> {
    let capture_window = app.get_webview_window("capture").ok_or("Capture window unavailable")?;
    let origin = capture_window.outer_position().map_err(|e| e.to_string())?;
    let abs_x = origin.x + (selection.left * selection.scale_factor).round() as i32;
    let abs_y = origin.y + (selection.top * selection.scale_factor).round() as i32;
    let width = (selection.width * selection.scale_factor).round().max(1.0) as u32;
    let height = (selection.height * selection.scale_factor).round().max(1.0) as u32;

    let captured = {
        let snapshot = state.pending_screen.lock().map_err(|e| e.to_string())?;
        snapshot.as_ref().and_then(|screen| crop_snapshot(screen, abs_x, abs_y, width, height)).ok_or("The selected area is outside the captured screen")?
    };
    let data_url = format!("data:image/png;base64,{}", BASE64.encode(encode_png(&captured)?));
    *state.last_result.lock().map_err(|e| e.to_string())? = Some(CaptureResult {
        text: String::new(), error: None, x: abs_x, y: abs_y, width: captured.width(), height: captured.height(),
    });
    *state.last_image.lock().map_err(|e| e.to_string())? = Some(captured.clone());
    Ok(data_url)
}

#[tauri::command]
fn update_capture_image(state: tauri::State<AppState>, data_url: String) -> Result<(), String> {
    let encoded = data_url.strip_prefix("data:image/png;base64,").ok_or("Expected a PNG image")?;
    if encoded.len() > 100_000_000 { return Err("Captured image is too large".into()); }
    let bytes = BASE64.decode(encoded).map_err(|e| e.to_string())?;
    let image = image::load_from_memory_with_format(&bytes, ImageFormat::Png).map_err(|e| e.to_string())?.to_rgba8();
    let mut current = state.last_image.lock().map_err(|e| e.to_string())?;
    let previous = current.as_ref().ok_or("No captured image")?;
    if image.dimensions() != previous.dimensions() { return Err("Image dimensions do not match the selection".into()); }
    *current = Some(image);
    Ok(())
}

#[tauri::command]
async fn extract_capture_text(state: tauri::State<'_, AppState>) -> Result<CaptureResult, String> {
    let generation = state.capture_generation.load(Ordering::SeqCst);
    let mut result = state.last_result.lock().map_err(|e| e.to_string())?.clone().ok_or("No selected region")?;
    let ocr_image = state.last_image.lock().map_err(|e| e.to_string())?.clone().ok_or("No captured image")?;
    let ocr = tauri::async_runtime::spawn_blocking(move || recognize_image(&ocr_image)).await;
    let (text, error) = match ocr {
        Ok(Ok(text)) => (text, None),
        Ok(Err(error)) => (String::new(), Some(error)),
        Err(error) => (String::new(), Some(format!("OCR task failed: {error}"))),
    };
    if state.capture_generation.load(Ordering::SeqCst) != generation { return Err("Capture was cancelled".into()); }
    let config = state.config.lock().map_err(|e| e.to_string())?.clone();
    if config.copy_text_automatically && !text.is_empty() { let _ = copy_text(text.clone()); }

    result.text = text;
    result.error = error;
    *state.last_result.lock().map_err(|e| e.to_string())? = Some(result.clone());
    Ok(result)
}

fn recognize_image(image: &RgbaImage) -> Result<String, String> {
    let mut candidates = Vec::new();
    let original = recognize_text(&encode_png(image)?)?;
    if !original.trim().is_empty() { candidates.push(original); }

    // Screen text is often small, antialiased, outlined, or composited over a
    // moving background. Compare color and high-contrast enlarged passes rather
    // than accepting the first non-empty (but potentially inaccurate) result.
    let max_dimension = image.width().max(image.height());
    let desired_factor = if image.height() <= 500 { 3 } else { 2 };
    let factor = desired_factor.min((7000 / max_dimension.max(1)).max(1));
    let resized = image::imageops::resize(
        image,
        image.width() * factor,
        image.height() * factor,
        image::imageops::FilterType::Lanczos3,
    );
    let enlarged = recognize_text(&encode_png(&resized)?)?;
    if !enlarged.trim().is_empty() { candidates.push(enlarged); }

    let grayscale = DynamicImage::ImageRgba8(resized).grayscale().to_rgba8();
    let enhanced = image::imageops::contrast(&grayscale, 30.0);
    let contrasted = recognize_text(&encode_png(&enhanced)?)?;
    if !contrasted.trim().is_empty() { candidates.push(contrasted); }

    Ok(correct_numeric_text(&candidates.into_iter().max_by_key(|text| ocr_quality_score(text)).unwrap_or_default()))
}

fn correct_numeric_text(text: &str) -> String {
    // Only repair tokens with strong numeric context; never rewrite ordinary
    // words, filenames, identifiers, or hexadecimal colors.
    text.split_inclusive(char::is_whitespace).map(|part| {
        let token = part.trim_end_matches(char::is_whitespace);
        let whitespace = &part[token.len()..];
        let (prefix, number) = if token.starts_with('v') || token.starts_with('V') { (&token[..1], &token[1..]) } else { ("", token) };
        let digits = number.chars().filter(char::is_ascii_digit).count();
        let numeric_chars = number.chars().all(|c| c.is_ascii_digit() || "lI|oOS.,:/+-()%".contains(c) || (c == 'B' && digits >= 3 && number.starts_with(|c: char| c.is_ascii_digit())));
        let version = !prefix.is_empty() && number.contains('.') && digits > 0;
        let delimited_number = prefix.is_empty() && number.contains(['/', ':', '.']) && digits >= 2;
        let mostly_digits = prefix.is_empty() && digits >= 2 && digits * 2 >= number.chars().count();
        if numeric_chars && (version || delimited_number || mostly_digits) {
            let repaired: String = number.chars().map(|c| match c { 'l' | 'I' | '|' => '1', 'o' | 'O' => '0', 'S' => '5', 'B' => '8', _ => c }).collect();
            format!("{prefix}{repaired}{whitespace}")
        } else { part.to_string() }
    }).collect()
}

fn ocr_quality_score(text: &str) -> i32 {
    let alphanumeric = text.chars().filter(|c| c.is_alphanumeric()).count() as i32;
    let words = text.split_whitespace().filter(|word| word.chars().any(|c| c.is_alphanumeric())).count() as i32;
    let suspicious = text.chars().filter(|c| !c.is_alphanumeric() && !c.is_whitespace() && !".,!?;:'\"-–—()[]/%+&@#".contains(*c)).count() as i32;
    alphanumeric * 3 + words * 8 - suspicious * 6
}

fn capture_monitor_at(x: i32, y: i32) -> Result<ScreenSnapshot, String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let monitor = monitors.into_iter().find(|m| {
        let (Ok(mx), Ok(my), Ok(mw), Ok(mh)) = (m.x(), m.y(), m.width(), m.height()) else { return false; };
        x >= mx && y >= my && x < mx + mw as i32 && y < my + mh as i32
    }).ok_or("The monitor under the pointer could not be captured")?;
    let monitor_x = monitor.x().map_err(|e| e.to_string())?;
    let monitor_y = monitor.y().map_err(|e| e.to_string())?;
    let image = monitor.capture_image().map_err(|e| e.to_string())?;
    let png = encode_png(&image)?;
    Ok(ScreenSnapshot {
        image,
        x: monitor_x,
        y: monitor_y,
        data_url: format!("data:image/png;base64,{}", BASE64.encode(png)),
    })
}

fn crop_snapshot(snapshot: &ScreenSnapshot, x: i32, y: i32, width: u32, height: u32) -> Option<RgbaImage> {
    let local_x = x.checked_sub(snapshot.x)?.max(0) as u32;
    let local_y = y.checked_sub(snapshot.y)?.max(0) as u32;
    let crop_w = width.min(snapshot.image.width().checked_sub(local_x)?);
    let crop_h = height.min(snapshot.image.height().checked_sub(local_y)?);
    if crop_w == 0 || crop_h == 0 { return None; }
    Some(image::imageops::crop_imm(&snapshot.image, local_x, local_y, crop_w, crop_h).to_image())
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image.clone()).write_to(&mut bytes, ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(bytes.into_inner())
}

fn recognize_text(png: &[u8]) -> Result<String, String> {
    // Tokio's blocking worker threads are not COM/WinRT apartments by default.
    // Windows OCR needs an initialized MTA or every async WinRT operation fails.
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
        .map_err(|e| format!("Could not initialize Windows OCR: {e}"))?;
    let outcome = (|| {
        let stream = InMemoryRandomAccessStream::new().map_err(|e| format!("Could not create OCR image stream: {e}"))?;
        let writer = DataWriter::CreateDataWriter(&stream).map_err(|e| format!("Could not write OCR image: {e}"))?;
        writer.WriteBytes(png).map_err(|e| format!("Could not write OCR image bytes: {e}"))?;
        writer.StoreAsync().and_then(|op| op.get()).map_err(|e| format!("Could not store OCR image: {e}"))?;
        writer.FlushAsync().and_then(|op| op.get()).map_err(|e| format!("Could not flush OCR image: {e}"))?;
        writer.DetachStream().map_err(|e| format!("Could not finalize OCR image: {e}"))?;
        stream.Seek(0).map_err(|e| format!("Could not rewind OCR image: {e}"))?;
        let decoder = BitmapDecoder::CreateAsync(&stream).and_then(|op| op.get()).map_err(|e| format!("Could not decode captured image: {e}"))?;
        let bitmap = decoder.GetSoftwareBitmapAsync().and_then(|op| op.get()).map_err(|e| format!("Could not prepare image for OCR: {e}"))?;
        let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| "Windows OCR is unavailable on this PC. In Settings → OCR, select English · offline pack (or your language), then Install.".to_string())?;
        let result = engine.RecognizeAsync(&bitmap).and_then(|op| op.get()).map_err(|e| format!("Windows OCR could not read this image: {e}"))?;
        format_ocr_layout(&result)
    })();
    unsafe { RoUninitialize(); }
    outcome
}

fn format_ocr_layout(result: &OcrResult) -> Result<String, String> {
    let lines = result.Lines().map_err(|e| format!("Could not retrieve OCR lines: {e}"))?;
    let line_count = lines.Size().map_err(|e| e.to_string())?;
    if line_count == 0 { return Ok(String::new()); }

    let mut all_words = Vec::new();
    let mut min_x = f32::MAX;
    let mut char_width_total = 0.0f32;
    let mut char_width_count = 0u32;
    for line_index in 0..line_count {
        let line = lines.GetAt(line_index).map_err(|e| e.to_string())?;
        let words = line.Words().map_err(|e| e.to_string())?;
        let word_count = words.Size().map_err(|e| e.to_string())?;
        for word_index in 0..word_count {
            let word = words.GetAt(word_index).map_err(|e| e.to_string())?;
            let text = word.Text().map_err(|e| e.to_string())?.to_string();
            let rect = word.BoundingRect().map_err(|e| e.to_string())?;
            if !text.is_empty() {
                min_x = min_x.min(rect.X);
                char_width_total += rect.Width / text.chars().count().max(1) as f32;
                char_width_count += 1;
                all_words.push((text, rect));
            }
        }
    }
    if all_words.is_empty() {
        return result.Text().map(|text| text.to_string()).map_err(|e| format!("Could not retrieve OCR text: {e}"));
    }

    // Windows OCR reports reading order by column. Reconstruct visual rows
    // from word positions instead, so dates and sizes stay beside filenames.
    all_words.sort_by(|a, b| (a.1.Y + a.1.Height / 2.0).total_cmp(&(b.1.Y + b.1.Height / 2.0)));
    let mut rows: Vec<(f32, f32, Vec<(String, windows::Foundation::Rect)>)> = Vec::new();
    for word in all_words {
        let rect = word.1;
        let center = rect.Y + rect.Height / 2.0;
        if let Some(row) = rows.last_mut().filter(|row| {
            let row_center = (row.0 + row.1) / 2.0;
            (center - row_center).abs() <= rect.Height.max(row.1 - row.0) * 0.6
        }) {
            row.0 = row.0.min(rect.Y);
            row.1 = row.1.max(rect.Y + rect.Height);
            row.2.push(word);
        } else {
            rows.push((rect.Y, rect.Y + rect.Height, vec![word]));
        }
    }
    let mut char_width = (char_width_total / char_width_count.max(1) as f32).max(2.0);
    let line_height = (rows.iter().map(|row| row.1 - row.0).sum::<f32>() / rows.len() as f32).max(4.0);
    let mut row_gaps: Vec<f32> = rows.windows(2).map(|pair| pair[1].0 - pair[0].0).collect();
    row_gaps.sort_by(f32::total_cmp);
    let row_pitch = row_gaps.get(row_gaps.len() / 2).copied().unwrap_or(line_height * 1.5).max(line_height * 1.25);
    // Group nearby words into cells, preserving punctuation within each cell.
    let mut layout = Vec::new();
    for (top, bottom, mut words) in rows {
        words.sort_by(|a, b| a.1.X.total_cmp(&b.1.X));
        let mut cells: Vec<(f32, f32, String)> = Vec::new();
        for (text, rect) in words {
            if let Some(cell) = cells.last_mut().filter(|cell| rect.X - cell.1 < char_width * 2.5) {
                let gap = rect.X - cell.1;
                let closes_previous = text.chars().all(|c| ".,!?;:%)]}’”\"".contains(c));
                if gap >= char_width * 0.4 && !closes_previous { cell.2.push(' '); }
                cell.2.push_str(&text);
                cell.1 = rect.X + rect.Width;
            } else {
                cells.push((rect.X, rect.X + rect.Width, text));
            }
        }
        // A proportional-font filename can occupy more monospace columns.
        // Reserve enough room before the next cell rather than wrapping it.
        for pair in cells.windows(2) {
            char_width = char_width.min((pair[1].0 - pair[0].0) / (pair[0].2.chars().count() + 2) as f32);
        }
        layout.push((top, bottom, cells));
    }
    let char_width = char_width.max(1.0);
    let mut output = String::new();
    let mut previous_top: Option<f32> = None;
    for (top, _, cells) in layout {
        if let Some(previous) = previous_top {
            let blank_lines = ((((top - previous).max(0.0) / row_pitch).round() as usize).saturating_sub(1)).min(3);
            output.push('\n');
            output.extend(std::iter::repeat_n('\n', blank_lines));
        }
        let mut column = 0usize;
        for (left, _, text) in cells {
            let target = (((left - min_x) / char_width).round() as usize).min(1000);
            let spaces = target.saturating_sub(column).max(if column == 0 { 0 } else { 2 });
            output.extend(std::iter::repeat_n(' ', spaces));
            column += spaces + text.chars().count();
            output.push_str(&text);
        }
        previous_top = Some(top);
    }
    Ok(output)
}


fn tray_icon() -> Image<'static> {
    Image::from_bytes(include_bytes!("../icons/tray-icon.png")).expect("valid embedded tray icon")
}

pub fn run() {
    if std::env::args().any(|arg| arg == "--portable-check") {
        let webview = tauri::webview_version();
        let windows_ocr = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok();
        let ocr_available = windows_ocr && OcrEngine::TryCreateFromUserProfileLanguages().is_ok();
        if windows_ocr { unsafe { RoUninitialize(); } }
        println!("{}", serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"), "architecture": "x86_64",
            "webview2": webview.ok(), "windows_ocr_available": ocr_available,
            "optional_english_ocr": true, "bundled_language_models": false,
            "executable": std::env::current_exe().ok(), "config_path": config_path(),
            "requires_node_or_system_python": false
        }));
        return;
    }
    if tauri::webview_version().is_err() {
        startup_message("Textractor needs Microsoft Edge WebView2 Runtime to display its interface.\n\nInstall the Evergreen Runtime from Microsoft's official site, then start Textractor again. No Textractor installer is needed.\n\nhttps://developer.microsoft.com/microsoft-edge/webview2/\n\nThis Windows 10/11 build is for 64-bit Intel/AMD PCs.");
        return;
    }
    let outcome = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::Builder::new().arg("--hidden").app_name("Textractor").build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed { let _ = show_capture(app); }
        }).build())
        .manage(AppState {
            capture_generation: AtomicU64::new(0),
            config: Mutex::new(read_config()),
            last_image: Mutex::new(None),
            last_result: Mutex::new(None),
            tray_capture_item: Mutex::new(None),
            pending_screen: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![get_config, save_config, save_tool_config, get_last_result, get_capture_background, start_capture, hide_settings, drag_settings, cancel_capture, prepare_capture, update_capture_image, extract_capture_text, accept_ocr_text, translate_text, translation::translation_packs, translation::manage_translation_pack, close_result, copy_text, copy_last_image, save_last_image])
        .setup(|app| {
            let smoke = std::env::args().any(|arg| arg == "--portable-smoke");
            let configured_hotkey = app.state::<AppState>().config.lock().expect("config lock").hotkey.clone();
            let capture = MenuItem::with_id(app, "capture", "Capture text", true, Some(configured_hotkey.as_str()))?;
            let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Textractor", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&capture, &settings, &quit])?;
            let handle = app.handle().clone();
            TrayIconBuilder::new().icon(tray_icon()).tooltip("Textractor").menu(&menu)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "capture" => { let _ = show_capture(app); },
                    "settings" => show_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(move |_tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event { show_settings(&handle); }
                }).build(app)?;
            *app.state::<AppState>().tray_capture_item.lock().expect("tray item lock") = Some(capture);
            let hotkey = app.state::<AppState>().config.lock().expect("config lock").hotkey.clone();
            if let Err(error) = if smoke { Ok(()) } else { app.global_shortcut().register(hotkey.as_str()) } {
                startup_message(&format!("The capture shortcut {hotkey} is unavailable on this PC ({error}).\n\nTextractor will keep running. Open Settings from the tray and choose another shortcut."));
            }
            let launch_at_startup = app.state::<AppState>().config.lock().expect("config lock").launch_at_startup;
            let startup_result = if smoke { Ok(()) } else if launch_at_startup { app.autolaunch().enable() } else { app.autolaunch().disable() };
            if let Err(error) = startup_result { startup_message(&format!("The Windows startup setting could not be applied ({error}).\n\nTextractor will still run normally when started manually.")); }
            if std::env::args().any(|arg| arg == "--hidden") {
                if let Some(main) = app.get_webview_window("main") { let _ = main.hide(); }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event { api.prevent_close(); let _ = window.hide(); }
            }
        })
        .run(tauri::generate_context!());
    if let Err(error) = outcome { startup_message(&format!("Textractor could not start:\n\n{error}\n\nPlease check that WebView2 is installed and your capture shortcut is not already in use.")); }
}

fn startup_message(message: &str) {
    use windows::{core::PCWSTR, Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK, MB_ICONERROR}};
    let message: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = "Textractor".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { MessageBoxW(None, PCWSTR(message.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONERROR); }
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, correct_numeric_text, recognize_image, recognize_text};
    use std::str::FromStr;
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

    #[test]
    fn print_screen_shortcut_is_valid() {
        let shortcut = Shortcut::from_str("Ctrl+Shift+PrintScreen").expect("valid Print Screen shortcut");
        assert_eq!(shortcut.key, Code::PrintScreen);
        assert!(shortcut.mods.contains(Modifiers::CONTROL));
        assert!(shortcut.mods.contains(Modifiers::SHIFT));
    }

    #[test]
    fn numeric_correction_preserves_words_identifiers_and_layout() {
        let text = "Portable v0.l.ll   2026/l0/08  O4:2O\n177O88 KB  177B88 KB  #F2BE8D  CUSA03173\nhello  B2B  B52  O2  SOlO  libc++.dll";
        let fixed = correct_numeric_text(text);
        assert_eq!(fixed, "Portable v0.1.11   2026/10/08  04:20\n177088 KB  177888 KB  #F2BE8D  CUSA03173\nhello  B2B  B52  O2  SOlO  libc++.dll");
    }

    #[test]
    fn old_settings_keep_shortcut_and_gain_tool_defaults() {
        let old = r#"{"hotkey":"Ctrl+Shift+PrintScreen","launch_at_startup":true,"copy_text_automatically":false}"#;
        let mut config: AppConfig = serde_json::from_str(old).expect("old settings remain readable");
        assert_eq!(config.hotkey, "Ctrl+Shift+PrintScreen");
        assert!(config.launch_at_startup);
        assert_eq!(config.tool_color, "#F2BE8D");
        assert_eq!(config.ocr_language, "en");
        assert_eq!(config.translation_language, "en");
        assert_eq!(config.extracted_text_size, 12.0);
        config.tool_alpha = 0.4;
        config.tool_thickness = 7.0;
        config.tool_arrow_size = 26.0;
        config.tool_text_size = 32.0;
        let saved = serde_json::to_string(&config).expect("serialize settings");
        let restored: AppConfig = serde_json::from_str(&saved).expect("restore saved settings");
        assert_eq!(restored.tool_alpha, 0.4);
        assert_eq!(restored.tool_thickness, 7.0);
        assert_eq!(restored.tool_arrow_size, 26.0);
        assert_eq!(restored.tool_text_size, 32.0);
        assert_eq!(restored.hotkey, "Ctrl+Shift+PrintScreen");
    }

    #[test]
    fn windows_ocr_reads_a_known_image() {
        let Ok(png) = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../testdata/ocr-sample.png")) else { return; };
        let text = recognize_text(&png).expect("Windows OCR should process the sample image");
        assert!(text.contains("TEXTRACTOR"), "unexpected OCR output: {text:?}");
        assert!(text.contains("4829"), "unexpected OCR output: {text:?}");
    }

    #[test]
    fn enhanced_ocr_reads_small_screen_text() {
        let Ok(png) = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../testdata/ocr-small-sample.png")) else { return; };
        let image = image::load_from_memory(&png).expect("valid sample PNG").to_rgba8();
        let text = recognize_image(&image).expect("enhanced OCR should process small screen text");
        assert!(text.to_lowercase().contains("screen"), "unexpected OCR output: {text:?}");
        assert!(text.contains("7391"), "unexpected OCR output: {text:?}");
    }

    #[test]
    fn explorer_columns_stay_on_the_same_visual_row() {
        let Ok(png) = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../testdata/ocr-explorer.png")) else { return; };
        let image = image::load_from_memory(&png).expect("valid Explorer screenshot").to_rgba8();
        let text = recognize_image(&image).expect("OCR should process Explorer columns");
        println!("Explorer layout:\n{text}");
        let first = text.lines().find(|line| line.contains("CUSA03173") && !line.contains("patch") && !line.contains("pkg"))
            .expect("first folder row");
        assert!(first.contains("2026/10/08") && first.contains("File folder"), "columns separated: {first:?}");
        let rows_with_dates = text.lines().filter(|line| line.contains("2026/")).count();
        assert_eq!(rows_with_dates, 10, "missing or duplicated visual rows: {text}");
        assert!(text.lines().count() <= 12, "column order introduced extra lines: {text}");
    }
}
