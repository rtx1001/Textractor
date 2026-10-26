import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CaptureOverlay } from "./screens/CaptureOverlay";
import { ResultPopup } from "./screens/ResultPopup";
import { Settings } from "./screens/Settings";
import { RegionOutline } from "./screens/RegionOutline";
import { RecordingControls } from "./screens/RecordingControls";
import { TooltipLayer } from "./TooltipLayer";
import "./styles.css";

const label = getCurrentWindow().label;
const Screen = label === "recorder" ? RecordingControls : label === "capture" ? CaptureOverlay : label === "result" ? ResultPopup : label === "region" ? RegionOutline : Settings;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode><Screen /><TooltipLayer /></React.StrictMode>
);
