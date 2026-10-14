import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CaptureResult } from "../types";

export function RegionOutline() {
  const [result, setResult] = useState<CaptureResult | null>(null);
  useEffect(() => {
    const unlisten = listen<CaptureResult>("capture-result", e => setResult(e.payload));
    void invoke<CaptureResult | null>("get_last_result").then(setResult);
    return () => { void unlisten.then(fn => fn()); };
  }, []);
  return <div className="region-outline">{result && <span className="region-size">{result.width} × {result.height}</span>}</div>;
}
