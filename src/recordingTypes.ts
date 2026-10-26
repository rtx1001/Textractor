export type RecordingOptions = { fps: number; cursor: boolean; system_audio: boolean; microphone: boolean };
export type RecordingStatus = { installed: boolean; options: RecordingOptions; phase: "idle" | "recording" | "paused" | "ready"; seconds: number; error: string | null; recovery_folder?: string | null };
export const EMPTY_RECORDING: RecordingStatus = { installed: false, options: { fps: 30, cursor: true, system_audio: false, microphone: false }, phase: "idle", seconds: 0, error: null };
