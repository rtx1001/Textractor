import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Camera, Check, Copy, Save, ScanText, X } from "lucide-react";
import type { CaptureResult } from "../types";

export function ResultPopup() {
  const [result, setResult] = useState<CaptureResult | null>(null);
  const [copied, setCopied] = useState<"text" | "image" | "save" | null>(null);
  useEffect(() => {
    const refresh = () => invoke<CaptureResult | null>("get_last_result").then(value => value && setResult(value));
    void refresh();
    const unlisten = listen<CaptureResult>("capture-result", e => setResult(e.payload));
    const unfocus = getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) void refresh();
      else void invoke("close_result");
    });
    return () => { void unlisten.then(fn => fn()); void unfocus.then(fn => fn()); };
  }, []);
  const acknowledge = (kind: "text" | "image" | "save") => { setCopied(kind); window.setTimeout(() => setCopied(null), 1300); };
  return <main className="result-popup">
    <header><span><ScanText size={16} /> Extracted text</span><button onClick={() => invoke("close_result")}><X size={16} /></button></header>
    <div className={result?.text ? "result-text" : result?.error ? "result-text error-result" : "result-text empty"}>{result?.text || result?.error || "No text was detected in this area."}</div>
    <footer>
      <button disabled={!result?.text} onClick={() => invoke("copy_text", { text: result?.text }).then(() => acknowledge("text"))}>{copied === "text" ? <Check size={16} /> : <Copy size={16} />} {copied === "text" ? "Copied" : "Copy text"}</button>
      <button data-tooltip="Copy captured area as an image" onClick={() => invoke("copy_last_image").then(() => acknowledge("image"))}>{copied === "image" ? <Check size={16} /> : <Camera size={16} />} {copied === "image" ? "Copied" : "Copy image"}</button>
      <button data-tooltip="Save captured area as a PNG" onClick={() => invoke<string | null>("save_last_image").then(path => path && acknowledge("save"))}>{copied === "save" ? <Check size={16} /> : <Save size={16} />} {copied === "save" ? "Saved" : "Save PNG"}</button>
    </footer>
  </main>;
}
