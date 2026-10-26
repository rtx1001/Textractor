use std::{fs::{self, File, OpenOptions}, io, path::{Path, PathBuf}};
use std::{collections::BTreeMap, sync::Mutex};
static LOCATIONS: Mutex<()> = Mutex::new(());
fn explorer_selection(path: &Path) -> std::ffi::OsString {
    let mut argument = std::ffi::OsString::from("/select,");
    argument.push(path); argument
}
// Explorer receives a single OS argument, not a shell command. Failure to open
// the folder must never turn a completed save into a failed/retryable save.
pub fn reveal(path: &Path) {
    use std::os::windows::process::CommandExt;
    let Ok(path) = std::path::absolute(path) else { return; };
    if !path.is_file() { return; }
    if let Err(error) = std::process::Command::new("explorer.exe")
        .arg(explorer_selection(&path)).creation_flags(0x08000000).spawn() {
        eprintln!("Saved file, but could not open Explorer: {error}");
    }
}
fn locations_path() -> Option<PathBuf> { Some(dirs::config_dir()?.join("Textractor/save-locations.json")) }
fn read_locations() -> BTreeMap<String, PathBuf> { locations_path().and_then(|p| fs::read(p).ok()).and_then(|v| serde_json::from_slice(&v).ok()).unwrap_or_default() }
pub fn dialog_default(kind: &str, fallback: &str) -> PathBuf {
    let chosen = read_locations().get(kind).cloned().filter(|p| p.parent().is_some_and(|p| p.is_dir())).unwrap_or_else(|| fallback.into());
    if !chosen.exists() { return chosen; }
    (0..1_000_000).map(|n| numbered(&chosen, n)).find(|p| !p.exists()).unwrap_or(chosen)
}
pub fn remember(kind: &str, path: &Path) {
    let Ok(_guard) = LOCATIONS.lock() else { return; };
    let Some(file) = locations_path() else { return; };
    let mut locations = read_locations(); locations.insert(kind.into(), path.into());
    if let Some(parent) = file.parent() { let _ = fs::create_dir_all(parent); }
    if let Ok(data) = serde_json::to_vec(&locations) { let _ = fs::write(file, data); }
}

fn numbered(path: &Path, number: u32) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let base = stem.rsplit_once('_').filter(|(_, digits)| digits.len() >= 3 && digits.bytes().all(|b| b.is_ascii_digit())).map(|(base, _)| base).unwrap_or(&stem);
    let mut name = format!("{base}_{number:03}");
    if let Some(extension) = path.extension() { name.push('.'); name.push_str(&extension.to_string_lossy()); }
    path.with_file_name(name)
}

// create_new reserves the name atomically: another save cannot overwrite it.
pub fn write_unique<T>(path: &Path, write: impl FnOnce(&mut File) -> Result<T, String>) -> Result<PathBuf, String> {
    let mut candidate = path.to_path_buf();
    let mut index = 0;
    let mut file = loop {
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(file) => break file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && index < 1_000_000 => { candidate = numbered(path, index); index += 1; },
            Err(e) => return Err(e.to_string()),
        }
    };
    let result = write(&mut file).and_then(|_| file.sync_all().map_err(|e| e.to_string()));
    drop(file);
    if let Err(e) = result { let _ = fs::remove_file(&candidate); return Err(e); }
    Ok(candidate)
}
pub fn copy_unique(source: &Path, destination: &Path) -> Result<PathBuf, String> {
    write_unique(destination, |output| { let mut input = File::open(source).map_err(|e| e.to_string())?; io::copy(&mut input, output).map_err(|e| e.to_string()) })
}
#[cfg(test)] mod tests {
    use super::*;
    use std::io::Write;
    #[test] fn explorer_path_is_one_literal_unicode_argument() {
        let path = Path::new(r"C:\Saved captures\Việt, 日本 & video_000.mp4");
        assert_eq!(explorer_selection(path), std::ffi::OsString::from(r"/select,C:\Saved captures\Việt, 日本 & video_000.mp4"));
    }
    #[test] fn duplicates_start_at_zero_and_never_overwrite() {
        let folder = std::env::temp_dir().join(format!("textractor-save-test-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("capture.png");
        for expected in ["capture.png", "capture_000.png", "capture_001.png"] {
            let result = write_unique(&path, |f| f.write_all(b"keep").map_err(|e| e.to_string())).unwrap();
            assert_eq!(result.file_name().unwrap(), expected);
        }
        let result = write_unique(&folder.join("capture_000.png"), |f| f.write_all(b"new").map_err(|e| e.to_string())).unwrap();
        assert_eq!(result.file_name().unwrap(), "capture_002.png");
        assert_eq!(fs::read(&path).unwrap(), b"keep");
        for name in ["capture.png", "capture_000.png", "capture_001.png", "capture_002.png"] { fs::remove_file(folder.join(name)).unwrap(); }
        fs::remove_dir(folder).unwrap();
    }
    #[test] fn numbering_extends_past_three_digits() { assert_eq!(numbered(Path::new("video.mp4"), 1000), Path::new("video_1000.mp4")); }
}
