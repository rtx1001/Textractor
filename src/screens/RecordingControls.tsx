import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Minimize2, Save, Video, X } from "lucide-react";
import { EMPTY_RECORDING, type RecordingStatus } from "../recordingTypes";
type Api = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export function RecordingControls({ api = invoke }: { api?: Api } = {}) {
  const [status, setStatus] = useState(EMPTY_RECORDING), [busy, setBusy] = useState(false), [error, setError] = useState("");
  useEffect(() => {
    let active = true, fetching = false;
    const refresh = async () => { if (fetching) return; fetching = true; try { const value = await api<RecordingStatus>("recording_status"); if (active) setStatus(value); } catch (e) { if (active) setError(String(e)); } finally { fetching = false; } };
    void refresh(); const timer = window.setInterval(() => void refresh(), 500);
    const escape = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); void api("hide_recording_controls"); } };
    window.addEventListener("keydown", escape);
    return () => { active = false; window.clearInterval(timer); window.removeEventListener("keydown", escape); };
  }, [api]);
  const act = async (command: string) => {
    setBusy(true); setError("");
    try { if (command === "save_recording" && status.phase !== "ready") await api("stop_recording"); await api(command); setStatus(await api<RecordingStatus>("recording_status")); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const time = `${Math.floor(status.seconds / 60).toString().padStart(2, "0")}:${(status.seconds % 60).toString().padStart(2, "0")}`;
  return <main className="recording-controls" onPointerDown={e => { if (e.button === 0 && !(e.target as Element).closest("button")) void api("drag_recording_controls"); }}>
    <div className="recording-controls-row"><Video size={19} /><span className={`recording-dot ${status.phase === "recording" ? "live" : ""}`} /><strong>{time}</strong><span className="recording-phase">{busy ? "Working…" : status.phase === "ready" ? "Ready to save" : status.phase === "paused" ? "Paused" : "Recording"}</span>
      <button aria-label="Save recording" data-tooltip="Stop recording and save as MP4. Canceling the save dialog keeps the video available. Existing filenames are numbered _000, _001…" disabled={busy} onClick={() => void act("save_recording")}><Save size={18} /></button>
      <button aria-label="Hide recording controls" data-tooltip="Hide this toolbar. Recording continues; click the Textractor tray icon to restore controls." onClick={() => void api("hide_recording_controls")}><Minimize2 size={17} /></button>
      <button aria-label="Cancel recording" data-tooltip="Stop and close without saving. Unsaved segments remain in the local recovery folder." disabled={busy} onClick={() => void act("cancel_recording")}><X size={18} /></button>
    </div><p className="recording-message" role="status" data-tooltip={error || status.error || (status.phase === "ready" ? status.recovery_folder || "" : "Drag to move. Esc hides controls. Pause/resume is available in the tray menu.")}>{error || status.error || (status.phase === "ready" ? "Save MP4 or cancel this recording" : "Drag to move · pause/resume in tray menu")}</p>
  </main>;
}
