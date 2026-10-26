import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Download, LoaderCircle, Monitor, Scan } from "lucide-react";
import { CustomSelect } from "../CustomSelect";
import { EMPTY_RECORDING, type RecordingStatus, type RecordingOptions } from "../recordingTypes";
type Api = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export function RecorderSettings({ api = invoke }: { api?: Api }) {
  const [status, setStatus] = useState(EMPTY_RECORDING), [busy, setBusy] = useState(false), [error, setError] = useState("");
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; void api<RecordingStatus>("recording_status").then(s => { if (mounted.current) setStatus(s); }).catch(e => { if (mounted.current) setError(String(e)); }); return () => { mounted.current = false; }; }, [api]);
  const action = async (job: () => Promise<unknown>) => {
    setBusy(true); setError("");
    try { await job(); const s = await api<RecordingStatus>("recording_status"); if (mounted.current) setStatus(s); }
    catch (e) { if (mounted.current) setError(String(e)); }
    finally { if (mounted.current) setBusy(false); }
  };
  const change = (options: RecordingOptions) => void action(async () => { await api("save_recording_options", { options }); setStatus(s => ({ ...s, options })); });
  return <section id="recorder-panel" role="tabpanel" aria-labelledby="recorder-tab" className="recorder-settings">
    {!status.installed ? <>
      <p className="compact-hint">Recording needs FFmpeg (~115 MB), downloaded once from Gyan’s GitHub builds. Stored in AppData; no installer or admin access. Video stays local.</p>
      <button className="primary recorder-install" disabled={busy} data-tooltip="Download FFmpeg 9.0.2 (GPLv3) only with your approval. Its SHA-256 is verified before installation. No recording starts automatically." onClick={() => void action(() => api("install_recording_engine"))}>{busy ? <LoaderCircle size={15} className="spinning" /> : <Download size={15} />}{busy ? "Downloading and verifying…" : "Download FFmpeg"}</button>
      <p className="compact-hint">Optional · no download at startup. MP4 video with optional system sound and microphone.</p>
    </> : <>
      <div className="recorder-options"><div className="compact-field"><span>Frame rate</span><CustomSelect label="Recording frame rate" value={String(status.options.fps)} onChange={v => change({ ...status.options, fps: Number(v) })} options={[15, 30, 60].map(fps => ({ value: String(fps), label: `${fps} FPS` }))} disabled={busy || status.phase !== "idle"} tooltip="Higher frame rates use more CPU and storage. 30 FPS is recommended." /></div><label className="compact-field"><span>Show cursor</span><button role="switch" className={`switch ${status.options.cursor ? "on" : ""}`} aria-label="Record mouse cursor" aria-checked={status.options.cursor} data-tooltip="Include the mouse pointer in your video." disabled={busy || status.phase !== "idle"} onClick={() => change({ ...status.options, cursor: !status.options.cursor })}><i /></button></label></div>
      <div className="recorder-audio">{(["system_audio", "microphone"] as const).map(key => <div key={key}><span>{key === "system_audio" ? "System sound" : "Microphone"}</span><button role="switch" className={`switch ${status.options[key] ? "on" : ""}`} aria-label={key === "system_audio" ? "Record system sound" : "Record microphone"} aria-checked={!!status.options[key]} data-tooltip={key === "system_audio" ? "Record the Windows default playback device using WASAPI loopback. No Stereo Mix driver needed." : "Record the Windows default microphone. Requires Windows microphone access permission. Disabled by default."} disabled={busy || status.phase !== "idle"} onClick={() => change({ ...status.options, [key]: !status.options[key] })}><i /></button></div>)}</div>
      <div className="recorder-actions"><button disabled={busy} data-tooltip="Select a region, then click the video icon in the capture toolbar to start recording." onClick={() => void action(() => api("start_capture"))}><Scan size={15} /> Select region</button><button disabled={busy || status.phase !== "idle"} data-tooltip="Start recording the monitor containing Settings. Settings and capture controls will be hidden." onClick={() => void action(() => api("start_recording", { fullScreen: true }))}><Monitor size={15} /> Full screen</button></div>
      <p className="compact-hint">Save stops recording. Pause/resume: tray menu.</p>
    </>}
    {error && <p className="recorder-error" role="alert">{error}</p>}
  </section>;
}
