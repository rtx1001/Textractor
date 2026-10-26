import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { createPortal } from "react-dom";
import { Camera, Check, Copy, Keyboard, Power, RotateCcw, ScanText, X } from "lucide-react";
import type { AppConfig } from "../types";
import { installedModels, optionalModel, removeModel, OCR_LANGUAGES } from "../ocrLanguages";
import { TranslationSettings } from "./TranslationSettings";
import { CustomSelect } from "../CustomSelect";
import { ColorWheel } from "../ColorWheel";
import { listen } from "@tauri-apps/api/event";
import { RecorderSettings } from "./RecorderSettings";

const DEFAULTS: AppConfig = { hotkey: "Ctrl+Shift+2", launch_at_startup: false, copy_text_automatically: false, tool_color: "#F2BE8D", tool_alpha: 1, tool_thickness: 3, tool_text_size: 20, tool_arrow_size: 14, ocr_language: "en", translation_language: "en", extracted_text_size: 12 };
type Api = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export function Settings({ api = invoke }: { api?: Api } = {}) {
  const [config, setConfig] = useState(DEFAULTS);
  const [tab, setTab] = useState<"general" | "tools" | "languages" | "translate" | "recorder">("general");
  const [recording, setRecording] = useState(false);
  const [status, setStatus] = useState("Settings save automatically");
  const [error, setError] = useState("");
  const [ready, setReady] = useState(false);
  const [installed, setInstalled] = useState<string[]>([]);
  const [installing, setInstalling] = useState(false);
  const [colorOpen, setColorOpen] = useState(false);
  const [colorPosition, setColorPosition] = useState({ left: 16, top: 60 });
  const colorButton = useRef<HTMLButtonElement>(null);
  const colorPopup = useRef<HTMLDivElement>(null);
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const revision = useRef(0);
  const recordingRef = useRef(false);

  useEffect(() => { setColorOpen(false); }, [tab]);
  useEffect(() => {
    if (api !== invoke) return;
    const unlisten = listen("settings-tab", event => { if (event.payload === "recorder") setTab("recorder"); });
    return () => { void unlisten.then(fn => fn()); };
  }, [api]);
  useEffect(() => {
    if (!colorOpen) return;
    const outside = (event: PointerEvent) => { if (!colorButton.current?.contains(event.target as Node) && !colorPopup.current?.contains(event.target as Node)) setColorOpen(false); };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") { setColorOpen(false); colorButton.current?.focus(); } };
    document.addEventListener("pointerdown", outside); document.addEventListener("keydown", escape);
    return () => { document.removeEventListener("pointerdown", outside); document.removeEventListener("keydown", escape); };
  }, [colorOpen]);

  useEffect(() => {
    void api<AppConfig>("get_config").then(value => { if (value) setConfig({ ...DEFAULTS, ...value }); setReady(true); }).catch(e => setError(String(e)));
    void installedModels().then(setInstalled).catch(e => setError(String(e)));
  }, [api]);

  useEffect(() => {
    if (api !== invoke) return;
    const unlisten = getCurrentWindow().onFocusChanged(({ payload }) => {
      if (!payload) return;
      void queue.current.then(async () => {
        const id = revision.current;
        const latest = await api<AppConfig>("get_config");
        if (id === revision.current) setConfig({ ...DEFAULTS, ...latest });
        setInstalled(await installedModels());
      }).catch(e => setError(String(e)));
    });
    return () => { void unlisten.then(fn => fn()); };
  }, [api]);

  const save = (next: AppConfig, toolsOnly = false) => {
    const id = ++revision.current;
    setError(""); setConfig(next); setStatus("Saving…");
    queue.current = queue.current.catch(() => {}).then(async () => {
      if (toolsOnly) await api("save_tool_config", { color: next.tool_color, alpha: next.tool_alpha, thickness: next.tool_thickness, textSize: next.tool_text_size, arrowSize: next.tool_arrow_size, extractedTextSize: next.extracted_text_size });
      else await api("save_config", { config: next });
    }).then(() => { if (id === revision.current) setStatus("Saved"); }).catch(async e => {
      if (id === revision.current) { setError(String(e)); setStatus("Could not save"); const saved = await api<AppConfig>("get_config").catch(() => null); if (saved) setConfig({ ...DEFAULTS, ...saved }); }
    });
  };

  const recordKey = (event: React.KeyboardEvent) => {
    if (!recordingRef.current) return;
    event.preventDefault(); event.stopPropagation();
    if (event.key === "Escape") { recordingRef.current = false; setRecording(false); return; }
    if (["Control", "Shift", "Alt", "Meta"].includes(event.key)) return;
    const mainKey = event.code === "PrintScreen" || event.key === "PrintScreen" ? "PrintScreen" : event.key.length === 1 ? event.key.toUpperCase() : event.key;
    const keys = [event.ctrlKey && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift", event.metaKey && "Super", mainKey].filter(Boolean);
    recordingRef.current = false; setRecording(false);
    save({ ...config, hotkey: keys.join("+") });
  };
  const capture = async () => { await queue.current; void api("start_capture").catch(e => setError(String(e))); };
  const selectedLanguage = OCR_LANGUAGES.find(item => item.value === config.ocr_language);
  const managePack = async (remove = false) => {
    if (!selectedLanguage?.code) return;
    setInstalling(true); setError(""); setStatus(remove ? "Removing language…" : "Downloading language…");
    try { if (remove) await removeModel(selectedLanguage.code); else await optionalModel(selectedLanguage.code, true); setInstalled(await installedModels()); setStatus(remove ? "Language removed" : "Language installed · ready offline"); }
    catch (e) { setError(String(e)); } finally { setInstalling(false); }
  };
  const descriptions = { tool_thickness: "Width of lines and shape outlines. You can also change it with the mouse wheel while drawing.", tool_text_size: "Starting font size for text annotations. Mouse wheel adjusts it while typing.", tool_arrow_size: "Arrow head size; its shaft thickness scales with it. Mouse wheel changes this while drawing arrows.", extracted_text_size: "Font size used to display OCR results and translations. This does not change the captured image." };
  const numberOption = (key: "tool_thickness" | "tool_text_size" | "tool_arrow_size" | "extracted_text_size", label: string, min: number, max: number) => <label className="compact-field"><span>{label}</span><div><input type="number" aria-label={label} data-tooltip={descriptions[key]} min={min} max={max} value={config[key] ?? DEFAULTS[key]} onChange={e => { if (e.target.value !== "") save({ ...config, [key]: Math.max(min, Math.min(max, Number(e.target.value))) }, true); }} /><small>px</small></div></label>;

  return <main className="settings-shell compact-settings" onKeyDown={recordKey} onKeyUp={recordKey} tabIndex={-1}>
    <header className="compact-brand" onPointerDown={event => { if (api === invoke && event.button === 0 && !(event.target as Element).closest("button")) void api("drag_settings"); }}><div className="brand-mark"><ScanText size={21} /></div><div><h1>Textractor</h1><span className="compact-version">v0.4.0 · Tray app</span></div><button className="primary capture-now" data-tooltip="Freeze the screen and select a region. Extract text, translate, draw, copy, or save it." disabled={!ready} onClick={() => void capture()}><Camera size={15} /> Capture</button><button className="icon-control settings-close" aria-label="Close settings" data-tooltip="Hide settings. Textractor keeps running in the system tray." onClick={() => void api("hide_settings")}><X size={16} /></button></header>
    <nav className="settings-tabs" role="tablist" aria-label="Settings categories"><button role="tab" id="general-tab" data-tooltip="Capture shortcut, automatic text copy, and Windows startup." aria-controls="general-panel" aria-selected={tab === "general"} onClick={() => setTab("general")}>General</button><button role="tab" id="tools-tab" data-tooltip="Default annotation color, opacity, stroke size, and displayed text size." aria-controls="tools-panel" aria-selected={tab === "tools"} onClick={() => setTab("tools")}>Tools</button><button role="tab" id="languages-tab" data-tooltip="Choose and install languages for recognizing text in screenshots." aria-controls="languages-panel" aria-selected={tab === "languages"} onClick={() => setTab("languages")}>OCR</button><button role="tab" id="translate-tab" data-tooltip="Choose translation languages and install only the packs needed for them." aria-controls="translate-panel" aria-selected={tab === "translate"} onClick={() => setTab("translate")}>Translate</button><button role="tab" id="recorder-tab" aria-controls="recorder-panel" aria-selected={tab === "recorder"} data-tooltip="Screen recording setup, frame rate, cursor, and MP4 recording. FFmpeg downloads only when you approve." onClick={() => setTab("recorder")}>Recorder</button></nav>
    <div className="settings-content">
      {tab === "general" ? <section id="general-panel" role="tabpanel" aria-labelledby="general-tab" className="compact-general">
        <div className="compact-row"><Keyboard size={17} /><span>Capture shortcut</span><button aria-label="Change capture shortcut" data-tooltip="Click and press your shortcut. It works from any window while Textractor runs. Escape cancels recording." disabled={!ready} className={`hotkey ${recording ? "recording" : ""}`} onClick={() => { recordingRef.current = true; setRecording(true); }}>{recording ? "Press keys…" : config.hotkey}</button></div>
        <div className="compact-row"><Copy size={17} /><span>Copy text automatically</span><button role="switch" aria-label="Copy text automatically" data-tooltip="Copy recognized text to the clipboard as soon as extraction finishes." aria-checked={config.copy_text_automatically} disabled={!ready} className={`switch ${config.copy_text_automatically ? "on" : ""}`} onClick={() => save({ ...config, copy_text_automatically: !config.copy_text_automatically })}><i /></button></div>
        <div className="compact-row"><Power size={17} /><span>Start with Windows</span><button role="switch" aria-label="Start with Windows" data-tooltip="Launch quietly in the tray when you sign in. Keep the portable EXE in a stable location." aria-checked={config.launch_at_startup} disabled={!ready} className={`switch ${config.launch_at_startup ? "on" : ""}`} onClick={() => save({ ...config, launch_at_startup: !config.launch_at_startup })}><i /></button></div>
        <p className="compact-hint">Closing this window keeps Textractor in the tray.</p>
      </section> : tab === "recorder" ? <RecorderSettings api={api} /> : tab === "translate" ? <TranslationSettings api={api} config={config} save={save} report={(message, failed = false) => { setError(failed ? message : ""); setStatus(failed ? "Translation failed" : message); }} /> : tab === "languages" ? <section id="languages-panel" role="tabpanel" aria-labelledby="languages-tab" className="compact-languages">
        <div className="compact-field"><span>Text recognition</span><CustomSelect label="OCR language" tooltip="Language used to read text from your selection. Automatic compares installed packs. Recognition packs are separate from translation packs." value={config.ocr_language || "auto"} disabled={installing} onChange={value => save({ ...config, ocr_language: value })} options={OCR_LANGUAGES.map(item => ({ value: item.value, label: item.label + (item.bundled ? "" : installed.includes(item.code) ? " · installed" : " · not installed") }))} /></div>
        <div className="language-pack"><span>{selectedLanguage?.bundled ? selectedLanguage.value === "en" ? "Windows OCR · ready" : `${installed.length} installed packs` : installed.includes(selectedLanguage?.code || "") ? "Installed · works offline" : "Pack required · download once"}</span>{selectedLanguage && !selectedLanguage.bundled && <button data-tooltip={installed.includes(selectedLanguage.code) ? "Remove this OCR pack from local storage. You can download it again later." : "Download this recognition language once. Screenshots stay on your PC."} disabled={installing} onClick={() => void managePack(installed.includes(selectedLanguage.code))}>{installing ? "Working…" : installed.includes(selectedLanguage.code) ? "Remove" : "Install"}</button>}</div>
        <p className="compact-hint">OCR packs download once, then work offline. Translation packs are managed separately in Translate.</p>
      </section> : <section id="tools-panel" role="tabpanel" aria-labelledby="tools-tab" className="compact-tools">
        <div className="compact-field"><span>Tool color</span><div><button ref={colorButton} className="settings-color-button" aria-label="Tool color" aria-expanded={colorOpen} data-tooltip="Choose the default drawing and annotation text color using the color wheel." style={{ background: config.tool_color }} onClick={event => { const rect = event.currentTarget.getBoundingClientRect(); setColorPosition({ left: Math.max(8, Math.min(rect.left, innerWidth - 180)), top: Math.max(8, Math.min(rect.bottom + 5, innerHeight - 274)) }); setColorOpen(open => !open); }} /><small>{config.tool_color}</small></div></div>
        <label className="compact-field opacity-field"><span>Opacity <small>{Math.round((config.tool_alpha ?? 1) * 100)}%</small></span><input aria-label="Tool opacity" data-tooltip="Transparency of new annotations: 0% is invisible, 100% is solid." type="range" min={0} max={100} value={Math.round((config.tool_alpha ?? 1) * 100)} onChange={e => save({ ...config, tool_alpha: Number(e.target.value) / 100 }, true)} /></label>
        {numberOption("tool_thickness", "Stroke width", 1, 64)}
        {numberOption("tool_text_size", "Text size", 8, 128)}
        {numberOption("tool_arrow_size", "Arrow size", 4, 96)}
        {numberOption("extracted_text_size", "Extracted text size", 8, 24)}
      </section>}
    </div>
    <footer className={error ? "compact-status has-error" : "compact-status"} role="status" data-tooltip={error || status}>{error || <><Check size={12} />{status}</>}{tab === "tools" && <button className="footer-reset" data-tooltip="Restore default annotation color, opacity, stroke, arrow, and text sizes." aria-label="Reset tools" onClick={() => save({ ...config, tool_color: DEFAULTS.tool_color, tool_alpha: DEFAULTS.tool_alpha, tool_thickness: DEFAULTS.tool_thickness, tool_text_size: DEFAULTS.tool_text_size, tool_arrow_size: DEFAULTS.tool_arrow_size, extracted_text_size: 12 }, true)}><RotateCcw size={13} /></button>}</footer>
    {colorOpen && createPortal(<div ref={colorPopup} className="settings-color-popover" style={colorPosition}><ColorWheel color={config.tool_color || "#F2BE8D"} onChange={color => save({ ...config, tool_color: color }, true)} alpha={config.tool_alpha ?? 1} onAlphaChange={alpha => save({ ...config, tool_alpha: alpha }, true)} /></div>, document.body)}
  </main>;
}
