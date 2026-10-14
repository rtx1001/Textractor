use std::{io::Write, path::PathBuf, process::{Command, Stdio}, sync::Mutex};
use std::os::windows::process::CommandExt;
use serde_json::{json, Value};

static JOB: Mutex<()> = Mutex::new(());
const NO_WINDOW: u32 = 0x08000000;

fn root() -> Result<PathBuf, String> {
    Ok(dirs::data_local_dir().ok_or("Local application storage unavailable")?.join("Textractor/translation"))
}

fn scripts(path: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    std::fs::write(path.join("translation.py"), include_str!("translation.py")).map_err(|e| e.to_string())?;
    std::fs::write(path.join("vc-runtime.zip"), include_bytes!("../vc-runtime.zip")).map_err(|e| e.to_string())?;
    std::fs::write(path.join("VC-RUNTIME-NOTICE.txt"), include_str!("../VC-RUNTIME-NOTICE.txt")).map_err(|e| e.to_string())?;
    std::fs::write(path.join("bootstrap.ps1"), include_str!("translation_bootstrap.ps1")).map_err(|e| e.to_string())
}

fn powershell() -> Result<Command, String> {
    let windows = std::env::var_os("WINDIR").ok_or("Windows directory unavailable")?;
    let mut command = Command::new(PathBuf::from(windows).join("System32/WindowsPowerShell/v1.0/powershell.exe"));
    command.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass"]);
    command.creation_flags(NO_WINDOW);
    Ok(command)
}

fn check(output: std::process::Output) -> Result<Vec<u8>, String> {
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).chars().take(1200).collect());
    }
    Ok(output.stdout)
}

fn run(path: &std::path::Path, kind: &str, request: Value) -> Result<Value, String> {
    let mut command = Command::new(path.join("runtime/python.exe"));
    command.args(["-X", "utf8"]).arg(path.join("translation.py")).arg(kind).arg(path);
    command.creation_flags(NO_WINDOW).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| format!("Translation engine unavailable: {e}"))?;
    if let Some(mut input) = child.stdin.take() {
        input.write_all(request.to_string().as_bytes()).map_err(|e| e.to_string())?;
    }
    let data = check(child.wait_with_output().map_err(|e| e.to_string())?)?;
    serde_json::from_slice(&data).map_err(|e| format!("Invalid translation response: {e}"))
}

fn status(path: &std::path::Path) -> Value {
    let index: Vec<Value> = std::fs::read(path.join("index.json")).ok().and_then(|data| serde_json::from_slice(&data).ok()).unwrap_or_default();
    let packs: Vec<Value> = index.iter().filter_map(|item| {
        let source = item["from_code"].as_str()?;
        let target = item["to_code"].as_str()?;
        if !source.chars().all(|c| c.is_ascii_lowercase()) || !target.chars().all(|c| c.is_ascii_lowercase()) { return None; }
        let id = format!("{source}_{target}");
        Some(json!({"id": id, "source": source, "target": target,
            "label": format!("{} → {}", item["from_name"].as_str()?, item["to_name"].as_str()?),
            "installed": path.join("packs").join(&id).join("ready").exists()}))
    }).collect();
    json!({"ready": runtime_ready(path), "packs": packs})
}

fn runtime_ready(path: &std::path::Path) -> bool {
    std::fs::read_to_string(path.join("runtime-ready")).is_ok_and(|value| value.trim() == "2")
        && path.join("runtime/msvcp140.dll").exists()
        && path.join("runtime/python.exe").exists()
}

#[tauri::command]
pub async fn translation_packs(refresh: bool) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = JOB.lock().map_err(|e| e.to_string())?;
        let path = root()?;
        scripts(&path)?;
        if refresh {
            // Fixed URL and destination passed as arguments, never interpolated as shell code.
            let mut command = powershell()?;
            command.arg("-File").arg(path.join("bootstrap.ps1")).arg(&path).arg("-CatalogueOnly");
            check(command.output().map_err(|e| e.to_string())?)?;
            let data = std::fs::read(path.join("index.json.download")).map_err(|e| e.to_string())?;
            let _: Vec<Value> = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
            std::fs::write(path.join("index.json"), data).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(path.join("index.json.download"));
        }
        Ok(status(&path))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn manage_translation_pack(source: String, target: String, remove: bool) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = JOB.lock().map_err(|e| e.to_string())?;
        let path = root()?;
        scripts(&path)?;
        if !remove && !runtime_ready(&path) {
            let mut command = powershell()?;
            command.arg("-File").arg(path.join("bootstrap.ps1")).arg(&path);
            check(command.output().map_err(|e| e.to_string())?)?;
        }
        if !remove && source.chars().all(|c| c.is_ascii_lowercase()) && target.chars().all(|c| c.is_ascii_lowercase()) && path.join("packs").join(format!("{source}_{target}")).join("ready").exists() { return Ok(status(&path)); }
        run(&path, if remove { "remove" } else { "install" }, json!({"source": source, "target": target}))
    }).await.map_err(|e| e.to_string())?
}

pub async fn translate(text: String, source: String, target: String) -> Result<Value, String> {
    let normalize = |code: String| match code.as_str() { "zh-CN" => "zh".into(), "zh-TW" => "zt".into(), _ => code };
    let source = normalize(source);
    let target = normalize(target);
    if source == target && source != "auto" { return Ok(json!({"text":text,"source":source,"target":target})); }
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = JOB.lock().map_err(|e| e.to_string())?;
        let path = root()?;
        if !runtime_ready(&path) { return Err("Install a translation pack (or click Repair engine for an installed pack) in Settings → Translate first. The shared engine downloads once.".into()); }
        scripts(&path)?;
        run(&path, "translate", json!({"text": text, "source": source, "target": target}))
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_language_does_not_require_a_translation_runtime() {
        let result = tauri::async_runtime::block_on(translate("原文 12345".into(), "zh-CN".into(), "zh".into())).unwrap();
        assert_eq!(result["text"], "原文 12345");
        assert_eq!(result["source"], "zh");
    }

    #[test]
    fn app_private_engine_returns_unicode_json_without_browser() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/translation-runtime");
        if !fixture.join("runtime-ready").exists() { return; }
        let result = run(&fixture, "translate", json!({"source":"ja", "target":"en", "text":"日本語の言語検出をテストしています。1234567890"})).unwrap();
        assert_eq!(result["source"], "ja");
        assert!(result["text"].as_str().unwrap().contains("1234567890"));
        let snapshot = status(&fixture);
        assert_eq!(snapshot["ready"], true);
        assert!(snapshot["packs"].as_array().unwrap().iter().any(|p| p["id"] == "ja_en" && p["installed"] == true));
    }
}
