import { useRef } from "react";
import type { Annotation, Point } from "./captureAnnotations";

export type TextEntry = { point: Point; value: string; width: number; minHeight: number; color: string; alpha: number; fontSize: number };
const PADDING = 8;

export function textLayout(value: string, width: number, maxHeight: number, preferredSize: number) {
  const context = document.createElement("canvas").getContext("2d")!;
  const wrap = (size: number) => {
    context.font = `600 ${size}px "Segoe UI", sans-serif`;
    const lines: string[] = [];
    for (const paragraph of value.split("\n")) {
      let line = "";
      for (const char of paragraph) {
        if (line && context.measureText(line + char).width > width) { lines.push(line); line = ""; }
        line += char;
      }
      lines.push(line);
    }
    return lines;
  };
  let fontSize = preferredSize;
  let lines = wrap(fontSize);
  while (fontSize > 0.5 && lines.length * fontSize * 1.25 > maxHeight) {
    fontSize = Math.max(0.5, fontSize - 0.5);
    lines = wrap(fontSize);
  }
  return { lines, fontSize, height: lines.length * fontSize * 1.25 };
}

export function textAnnotation(entry: TextEntry, regionHeight: number): Annotation {
  const layout = textLayout(entry.value, Math.max(1, entry.width - 18), Math.max(1, regionHeight - entry.point.y - 20), entry.fontSize);
  return { tool: "text", points: [{ x: entry.point.x + PADDING + 1, y: entry.point.y + PADDING + 1 }], text: entry.value, lines: layout.lines, fontSize: layout.fontSize, color: entry.color, alpha: entry.alpha };
}

export function TextAnnotationEditor({ entry, regionWidth, regionHeight, onChange }: { entry: TextEntry; regionWidth: number; regionHeight: number; onChange: (entry: TextEntry) => void }) {
  const gesture = useRef<{ mode: "move" | "resize"; x: number; y: number; entry: TextEntry } | null>(null);
  const layout = textLayout(entry.value, Math.max(1, entry.width - 18), Math.max(1, regionHeight - entry.point.y - 20), entry.fontSize);
  const height = Math.min(regionHeight - entry.point.y, Math.max(entry.minHeight, layout.height + 20));
  const begin = (event: React.PointerEvent<HTMLDivElement>, mode: "move" | "resize") => {
    event.preventDefault(); event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    gesture.current = { mode, x: event.clientX, y: event.clientY, entry: mode === "resize" ? { ...entry, minHeight: height } : entry };
  };
  const move = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    const initial = gesture.current;
    if (!initial) return;
    const dx = event.clientX - initial.x, dy = event.clientY - initial.y;
    if (initial.mode === "move") {
      onChange({ ...entry, point: { x: Math.max(0, Math.min(regionWidth - entry.width, initial.entry.point.x + dx)), y: Math.max(0, Math.min(regionHeight - height, initial.entry.point.y + dy)) } });
    } else {
      onChange({ ...entry, width: Math.max(Math.min(40, regionWidth - entry.point.x), Math.min(regionWidth - entry.point.x, initial.entry.width + dx)), minHeight: Math.max(Math.min(40, regionHeight - entry.point.y), Math.min(regionHeight - entry.point.y, initial.entry.minHeight + dy)) });
    }
  };
  return <div className="annotation-editor" style={{ left: entry.point.x, top: entry.point.y, width: entry.width, height }} onPointerDown={e => e.stopPropagation()} onPointerMove={move} onPointerUp={e => { e.stopPropagation(); gesture.current = null; }} onPointerCancel={() => { gesture.current = null; }}>
    <textarea aria-label="Annotation text" placeholder="Type text…" autoFocus value={entry.value} spellCheck={false} style={{ fontSize: layout.fontSize, color: entry.color, opacity: entry.alpha }} onChange={e => onChange({ ...entry, value: e.target.value })} />
    {(["top", "right", "bottom", "left"] as const).map(edge => <div key={edge} className={`text-move-edge ${edge}`} data-tooltip="Drag the border to move this text field." onPointerDown={e => begin(e, "move")} />)}
    <div className="text-resize-handle" role="button" aria-label="Resize text field" data-tooltip="Drag this bottom-right handle to resize the typing area." onPointerDown={e => begin(e, "resize")} />
  </div>;
}
