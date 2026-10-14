import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ArrowUpRight, Camera, Check, Circle, Copy, Languages, LoaderCircle, Minus, Pencil, Save, ScanText, Square, Type, Undo2, X } from "lucide-react";
import type { AppConfig, CaptureResult, TranslationResult } from "../types";
import { ColorWheel } from "../ColorWheel";
import { multilingualOcr, containsNonEnglishScript } from "../multilingualOcr";
import { TextAnnotationEditor, textAnnotation, textLayout, type TextEntry } from "../TextAnnotationEditor";
import { roundedArrow, composeCapture, INK, STROKE, TEXT_SIZE, type Annotation, type Point, type Tool } from "../captureAnnotations";

type Bounds = { left: number; top: number; width: number; height: number };
type Api = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
const tools = [
  { id: "line", label: "Straight line", Icon: Minus },
  { id: "freehand", label: "Freehand drawing", Icon: Pencil },
  { id: "rectangle", label: "Rectangle", Icon: Square },
  { id: "ellipse", label: "Circle / oval", Icon: Circle },
  { id: "arrow", label: "Arrow", Icon: ArrowUpRight },
  { id: "text", label: "Insert text", Icon: Type },
] as const;
const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(value, max));
const box = (a: Point, b: Point): Bounds => ({ left: Math.min(a.x, b.x), top: Math.min(a.y, b.y), width: Math.abs(b.x - a.x), height: Math.abs(b.y - a.y) });

function Mark({ annotation }: { annotation: Annotation }) {
  const { tool, points, text } = annotation;
  const a = points[0], b = points[points.length - 1];
  if (tool === "text") return <text x={a.x} y={a.y} fill={annotation.color} stroke="none" fontSize={annotation.fontSize ?? TEXT_SIZE} fontWeight={600} fontFamily="Segoe UI, sans-serif" dominantBaseline="text-before-edge">{(annotation.lines || (text || "").split("\n")).map((line, i) => <tspan key={i} x={a.x} dy={i ? (annotation.fontSize ?? TEXT_SIZE) * 1.25 : 0}>{line}</tspan>)}</text>;
  if (tool === "rectangle") return <rect x={Math.min(a.x, b.x)} y={Math.min(a.y, b.y)} width={Math.abs(b.x - a.x)} height={Math.abs(b.y - a.y)} />;
  if (tool === "ellipse") return <ellipse cx={(a.x + b.x) / 2} cy={(a.y + b.y) / 2} rx={Math.abs(b.x - a.x) / 2} ry={Math.abs(b.y - a.y) / 2} />;
  if (tool === "freehand") return <polyline points={points.map(p => `${p.x},${p.y}`).join(" ")} />;
  if (tool === "arrow") {
    const head = roundedArrow(a, b, annotation.arrowSize);
    return <><line x1={a.x} y1={a.y} x2={head.base.x} y2={head.base.y} /><path d={head.path} fill={annotation.color} stroke="none" /></>;
  }
  return <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} />;
}

export function CaptureOverlay({ api = invoke, previewBackground }: { api?: Api; previewBackground?: string } = {}) {
  const [bounds, setBounds] = useState<Bounds | null>(null);
  const [selected, setSelected] = useState(false);
  const [background, setBackground] = useState<string | null>(previewBackground || null);
  const [source, setSource] = useState<string | null>(null);
  const [annotations, setAnnotations] = useState<Annotation[]>([]);
  const [draft, setDraft] = useState<Annotation | null>(null);
  const [tool, setTool] = useState<Tool | null>(null);
  const [color, setColor] = useState(INK);
  const [alpha, setAlpha] = useState(1);
  const [thickness, setThickness] = useState(STROKE);
  const [textSize, setTextSize] = useState(TEXT_SIZE);
  const [arrowSize, setArrowSize] = useState(14);
  const [ocrLanguage, setOcrLanguage] = useState("auto");
  const [extractedTextSize, setExtractedTextSize] = useState(12);
  const [preferencesLoaded, setPreferencesLoaded] = useState(false);
  const loadedPreferences = useRef("");
  const [sizePreview, setSizePreview] = useState<Point | null>(null);
  const [previewSize, setPreviewSize] = useState(STROKE);
  const [colorOpen, setColorOpen] = useState(false);
  const [textEntry, setTextEntry] = useState<TextEntry | null>(null);
  const textEntryRef = useRef<TextEntry | null>(null);
  const confirmedPointer = useRef<number | null>(null);
  const previewTimer = useRef<number | undefined>();
  const [result, setResult] = useState<CaptureResult | null>(null);
  const [translation, setTranslation] = useState<TranslationResult | null>(null);
  const [showTranslation, setShowTranslation] = useState(false);
  const [phase, setPhase] = useState("");
  const resultMarks = useRef("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [viewport, setViewport] = useState({ width: window.innerWidth, height: window.innerHeight });
  const [contentHeight, setContentHeight] = useState(0);
  const start = useRef<Point | null>(null);
  const session = useRef(0);
  const action = useRef(false);
  const contentMeasure = useRef<HTMLDivElement>(null);
  const closed = useRef(false);
  const changeText = (entry: TextEntry | null) => { textEntryRef.current = entry; setTextEntry(entry); };
  const commitText = (): Annotation | null => {
    const entry = textEntryRef.current;
    if (!entry || !bounds) return null;
    const mark = entry.value.trim() ? textAnnotation(entry, bounds.height) : null;
    if (mark) setAnnotations(items => [...items, mark]);
    changeText(null);
    return mark;
  };

  const reset = () => {
    session.current++;
    closed.current = false;
    start.current = null;
    action.current = false;
    setBounds(null); setSelected(false); setSource(null); setAnnotations([]); setDraft(null);
    setTool(null); setColorOpen(false); changeText(null); setSizePreview(null); setResult(null); setBusy(false); setError(null); setNotice(null);
    setTranslation(null); setShowTranslation(false); setPhase(""); resultMarks.current = "";
  };
  const close = () => {
    closed.current = true;
    session.current++;
    start.current = null;
    setBounds(null); setSelected(false); setAnnotations([]); setDraft(null); changeText(null); setSizePreview(null);
    setResult(null); setBackground(null); setTool(null); setColorOpen(false); setError(null); setNotice(null);
    setTranslation(null); setShowTranslation(false); setPhase(""); resultMarks.current = "";
    void api("cancel_capture");
  };
  const undo = () => {
    if (action.current) return;
    if (textEntry) changeText(null);
    else if (draft) setDraft(null);
    else setAnnotations(items => items.slice(0, -1));
  };

  useEffect(() => {
    let mounted = true;
    void api<AppConfig>("get_config").then(config => {
      if (!mounted) return;
      loadedPreferences.current = JSON.stringify({ color: config?.tool_color || INK, alpha: config?.tool_alpha ?? 1, thickness: config?.tool_thickness ?? STROKE, textSize: config?.tool_text_size ?? TEXT_SIZE, arrowSize: config?.tool_arrow_size ?? 14 });
      if (config) {
        setColor(config.tool_color || INK);
        setAlpha(config.tool_alpha ?? 1);
        setThickness(config.tool_thickness ?? STROKE);
        setTextSize(config.tool_text_size ?? TEXT_SIZE);
        setArrowSize(config.tool_arrow_size ?? 14);
        setOcrLanguage(config.ocr_language || "auto");
        setExtractedTextSize(config.extracted_text_size ?? 12);
      }
      setPreferencesLoaded(true);
    }).catch(e => setError(`Could not load tool settings: ${e}`));
    return () => { mounted = false; };
  }, [api]);

  useEffect(() => {
    if (!preferencesLoaded) return;
    const preferences = { color, alpha, thickness, textSize, arrowSize };
    const signature = JSON.stringify(preferences);
    if (signature === loadedPreferences.current) return;
    const timer = window.setTimeout(() => {
      void api("save_tool_config", preferences).then(() => { loadedPreferences.current = signature; }).catch(e => setError(`Could not save tool settings: ${e}`));
    }, 250);
    return () => window.clearTimeout(timer);
  }, [color, alpha, thickness, textSize, arrowSize, preferencesLoaded, api]);

  useEffect(() => {
    if (previewBackground) return;
    const load = async () => {
      const id = session.current;
      const [image, config] = await Promise.all([api<string | null>("get_capture_background"), api<AppConfig>("get_config")]);
      if (id === session.current && !closed.current) {
        if (config) {
          setOcrLanguage(config.ocr_language || "auto");
          setExtractedTextSize(config.extracted_text_size ?? 12);
          loadedPreferences.current = JSON.stringify({ color: config.tool_color || INK, alpha: config.tool_alpha ?? 1, thickness: config.tool_thickness ?? STROKE, textSize: config.tool_text_size ?? TEXT_SIZE, arrowSize: config.tool_arrow_size ?? 14 });
          setColor(config.tool_color || INK); setAlpha(config.tool_alpha ?? 1); setThickness(config.tool_thickness ?? STROKE); setTextSize(config.tool_text_size ?? TEXT_SIZE); setArrowSize(config.tool_arrow_size ?? 14);
        }
        setPreferencesLoaded(true);
        setBackground(image);
      }
    };
    void load();
    const unlisten = listen("capture-reset", () => { reset(); setPreferencesLoaded(false); setBackground(null); void load(); });
    return () => { void unlisten.then(fn => fn()); };
  }, [api, previewBackground]);

  useEffect(() => {
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault(); event.stopPropagation();
        if (tool || textEntryRef.current || draft || colorOpen) {
          commitText();
          if (draft) setAnnotations(items => [...items, draft]);
          setDraft(null); setTool(null); setColorOpen(false); setSizePreview(null);
        } else close();
      }
      else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z" && !(event.target instanceof HTMLTextAreaElement)) { event.preventDefault(); undo(); }
    };
    const resize = () => setViewport({ width: window.innerWidth, height: window.innerHeight });
    const wheel = (event: WheelEvent) => {
      if (!selected || !tool || busy || event.deltaY === 0 || (event.target as Element).closest(".capture-panel")) return;
      event.preventDefault();
      const direction = event.deltaY < 0 ? 1 : -1;
      let next = thickness;
      if (tool === "text") {
        const entry = textEntryRef.current;
        if (entry) {
          const editor = document.querySelector<HTMLDivElement>(".annotation-editor");
          if (!editor) return;
          const fieldHeight = editor?.getBoundingClientRect().height || entry.minHeight;
          const current = parseFloat(getComputedStyle(editor!.querySelector("textarea")!).fontSize) * window.devicePixelRatio;
          const requested = clamp(current + direction, Math.min(8, current), 128);
          const layout = textLayout(entry.value, Math.max(1, entry.width - 18), Math.max(1, fieldHeight - 20), requested / window.devicePixelRatio);
          next = layout.fontSize * window.devicePixelRatio;
          changeText({ ...entry, fontSize: layout.fontSize, minHeight: fieldHeight });
          setTextSize(clamp(next, 8, 128));
        } else { next = clamp(textSize + direction, 8, 128); setTextSize(next); }
      } else if (tool === "arrow") {
        next = clamp(arrowSize + direction, 4, 96);
        setArrowSize(next);
        if (draft) setDraft({ ...draft, arrowSize: next / window.devicePixelRatio, thickness: next / 5 / window.devicePixelRatio });
      } else {
        next = clamp(thickness + direction, 1, 64);
        setThickness(next);
        if (draft) setDraft({ ...draft, thickness: next / window.devicePixelRatio });
      }
      setPreviewSize(Math.round(next * 10) / 10);
      setSizePreview({ x: event.clientX, y: event.clientY });
      window.clearTimeout(previewTimer.current);
      previewTimer.current = window.setTimeout(() => setSizePreview(null), 1100);
    };
    const outsideText = (event: PointerEvent) => {
      confirmedPointer.current = null;
      if (textEntryRef.current && !(event.target as Element).closest(".annotation-editor")) {
        if (!(event.target as Element).closest(".capture-panel")) confirmedPointer.current = event.pointerId;
        commitText();
      }
    };
    window.addEventListener("keydown", keyboard, true);
    window.addEventListener("resize", resize);
    window.addEventListener("wheel", wheel, { passive: false });
    window.addEventListener("pointerdown", outsideText, true);
    return () => { window.removeEventListener("keydown", keyboard, true); window.removeEventListener("resize", resize); window.removeEventListener("wheel", wheel); window.removeEventListener("pointerdown", outsideText, true); };
  });

  const displayText = error || phase || (showTranslation && translation ? translation.text : result ? (result.text || result.error || "No text was detected in this area.") : "");
  const maxWidth = Math.max(1, viewport.width - 16);
  const textWidth = Math.max(...displayText.split("\n").map(line => line.length * 7.3 * extractedTextSize / 12 + 32), 0);
  const panelWidth = Math.min(maxWidth, Math.max(558, bounds?.width || 0, textWidth));
  const panelLeft = clamp(bounds?.left || 0, 8, viewport.width - panelWidth - 8);
  const toolbarHeight = viewport.width < 574 ? 90 : 46;
  const toolbarTop = clamp((bounds?.top || 0) + (bounds?.height || 0) + 8, 8, viewport.height - toolbarHeight - 8);
  const spaceBelow = viewport.height - toolbarTop - toolbarHeight - 8;
  const above = spaceBelow < Math.min(contentHeight || 150, 180) && toolbarTop - 8 > spaceBelow;
  const availableHeight = Math.max(0, above ? toolbarTop - 8 : spaceBelow);
  const resultHeight = displayText ? Math.min(contentHeight, availableHeight) : 0;

  useLayoutEffect(() => {
    if (contentMeasure.current) setContentHeight(Math.ceil(contentMeasure.current.getBoundingClientRect().height) + 2);
  }, [displayText, panelWidth, extractedTextSize, translation, showTranslation, phase, error]);

  const point = (event: React.PointerEvent): Point => ({ x: clamp(event.clientX, 0, viewport.width), y: clamp(event.clientY, 0, viewport.height) });
  const localPoint = (event: React.PointerEvent): Point => ({ x: clamp(event.clientX - bounds!.left, 0, bounds!.width), y: clamp(event.clientY - bounds!.top, 0, bounds!.height) });

  const selectStart = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    if (confirmedPointer.current === event.pointerId) { confirmedPointer.current = null; return; }
    if (event.button !== 0 || action.current || !background) return;
    if (selected && textEntry) return;
    if (selected) { close(); return; }
    event.currentTarget.setPointerCapture(event.pointerId);
    start.current = point(event);
    setBounds(box(start.current, start.current));
  };
  const selectMove = (event: React.PointerEvent<HTMLDivElement>) => {
    if (start.current && !selected) setBounds(box(start.current, point(event)));
  };
  const selectFinish = async (event: React.PointerEvent<HTMLDivElement>) => {
    if (!start.current || selected) return;
    const area = box(start.current, point(event));
    start.current = null;
    if (area.width < 8 || area.height < 8) { setBounds(null); return; }
    const id = session.current;
    action.current = true; setBusy(true); setBounds(area); setSelected(true);
    try {
      const image = await api<string>("prepare_capture", { selection: { ...area, scaleFactor: window.devicePixelRatio } });
      if (id === session.current && !closed.current) setSource(image);
    } catch (e) { if (id === session.current) setError(String(e)); }
    finally { if (id === session.current) { action.current = false; setBusy(false); } }
  };

  const drawStart = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation(); event.preventDefault();
    if (confirmedPointer.current === event.pointerId) { confirmedPointer.current = null; return; }
    if (textEntry) { commitText(); return; }
    if (action.current || event.button !== 0 || !tool) return;
    const p = localPoint(event);
    setColorOpen(false);
    if (tool === "text") {
      const origin = { x: clamp(p.x, 0, Math.max(0, bounds!.width - 40)), y: clamp(p.y, 0, Math.max(0, bounds!.height - 40)) };
      changeText({ point: origin, value: "", width: Math.min(240, bounds!.width - origin.x), minHeight: Math.min(42, bounds!.height - origin.y), color, alpha, fontSize: textSize / window.devicePixelRatio });
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    setDraft({ tool, points: [p, p], color, alpha, thickness: (tool === "arrow" ? arrowSize / 5 : thickness) / window.devicePixelRatio, arrowSize: arrowSize / window.devicePixelRatio });
  };
  const drawMove = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    if (!draft) return;
    const p = localPoint(event);
    setDraft(current => current && ({ ...current, points: current.tool === "freehand" ? [...current.points, p] : [current.points[0], p] }));
  };
  const drawFinish = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    if (!draft) return;
    const p = localPoint(event);
    const completed = { ...draft, points: draft.tool === "freehand" ? [...draft.points, p] : [draft.points[0], p] };
    if (Math.hypot(p.x - draft.points[0].x, p.y - draft.points[0].y) > 1 || draft.tool === "freehand") setAnnotations(items => [...items, completed]);
    setDraft(null);
  };
  const run = async (kind: "extract" | "translate" | "copy" | "image" | "save") => {
    if (action.current || !bounds || !source) return;
    const id = session.current;
    action.current = true; setBusy(true); setError(null); setTool(null); setColorOpen(false);
    try {
      if (kind === "copy") {
        await api("copy_text", { text: showTranslation && translation ? translation.text : result?.text || "" });
      } else {
        const mark = commitText();
        const marks = mark ? [...annotations, mark] : annotations;
        const image = await composeCapture(source, bounds.width, bounds.height, marks);
        if (id !== session.current || closed.current) return;
        await api("update_capture_image", { dataUrl: image });
        if (id !== session.current || closed.current) return;
        if (kind === "extract" || kind === "translate") {
          let value: CaptureResult;
          const signature = JSON.stringify(marks);
          setPhase("Extracting text…");
          if (kind === "translate" && result?.text && resultMarks.current === signature) {
            value = result;
          } else if (ocrLanguage === "en" || previewBackground) {
            value = await api<CaptureResult>("extract_capture_text");
          } else {
            const text = await multilingualOcr(image, ocrLanguage);
            if (id !== session.current || closed.current) return;
            if (ocrLanguage === "auto" && !containsNonEnglishScript(text)) {
              value = await api<CaptureResult>("extract_capture_text");
              if (!value.text && text) value = await api<CaptureResult>("accept_ocr_text", { text });
            } else value = await api<CaptureResult>("accept_ocr_text", { text });
          }
          if (id !== session.current || closed.current) return;
          setResult(value); resultMarks.current = signature; setTranslation(null); setShowTranslation(false);
          if (kind === "translate" && value.text) {
            setPhase("Translating offline…");
            const translated = await api<TranslationResult>("translate_text", { text: value.text });
            if (id !== session.current || closed.current) return;
            setTranslation(translated); setShowTranslation(true);
          }
        } else if (kind === "image") await api("copy_last_image");
        else {
          const saved = await api<string | null>("save_last_image");
          if (!saved) return;
        }
      }
      if (id === session.current && kind !== "extract" && kind !== "translate") { setNotice(kind); window.setTimeout(() => setNotice(null), 1600); }
    } catch (e) { if (id === session.current) setError(String(e)); }
    finally { if (id === session.current) { action.current = false; setBusy(false); setPhase(""); } }
  };

  return <div className={`capture-overlay editor-overlay ${bounds ? "has-region" : ""}`} style={{ backgroundImage: background ? `url(${background})` : undefined, backgroundColor: "#171310" }} onContextMenu={e => { e.preventDefault(); close(); }} onPointerDown={selectStart} onPointerMove={selectMove} onPointerUp={selectFinish}>
    {bounds && <>
      <div className="shade top" style={{ height: bounds.top }} />
      <div className="shade left" style={{ top: bounds.top, width: bounds.left, height: bounds.height }} />
      <div className="shade right" style={{ top: bounds.top, left: bounds.left + bounds.width, right: 0, height: bounds.height }} />
      <div className="shade bottom" style={{ top: bounds.top + bounds.height, bottom: 0 }} />
      <div className={`selection ${selected ? "selected" : ""} ${tool ? "drawing" : ""}`} style={bounds} onPointerDown={selected ? drawStart : undefined} onPointerMove={selected ? drawMove : undefined} onPointerUp={selected ? drawFinish : undefined} onPointerCancel={() => setDraft(null)}>
        <span className="region-size" style={bounds.top < 26 ? { top: 3 } : undefined}>{Math.round(bounds.width * window.devicePixelRatio)} × {Math.round(bounds.height * window.devicePixelRatio)}</span>
        {selected && <svg className="annotation-layer" width={bounds.width} height={bounds.height} viewBox={`0 0 ${bounds.width} ${bounds.height}`} fill="none" stroke={INK} strokeWidth={STROKE} strokeLinecap="round" strokeLinejoin="round">{annotations.map((annotation, i) => <g key={i} stroke={annotation.color} opacity={annotation.alpha ?? 1} strokeWidth={annotation.thickness ?? STROKE}><Mark annotation={annotation} /></g>)}{draft && <g stroke={draft.color} opacity={draft.alpha ?? 1} strokeWidth={draft.thickness ?? STROKE}><Mark annotation={draft} /></g>}</svg>}
        {textEntry && <TextAnnotationEditor entry={textEntry} regionWidth={bounds.width} regionHeight={bounds.height} onChange={changeText} />}
      </div>
      {selected && <section className={`capture-panel ${above ? "text-above" : ""}`} style={{ left: panelLeft, top: above ? toolbarTop - resultHeight : toolbarTop, width: panelWidth }} onPointerDown={e => e.stopPropagation()} onPointerMove={e => e.stopPropagation()} onPointerUp={e => e.stopPropagation()} onContextMenu={e => e.stopPropagation()}>
        <div className="capture-toolbar" role="toolbar" aria-label="Capture tools">
          <button data-tooltip="Read all text in the selected area. Keeps line breaks and spacing where possible." aria-label="Extract text" className="extract-tool" disabled={busy || !source} onClick={() => void run("extract")}>{busy ? <LoaderCircle className="spinning" size={19} /> : <ScanText size={19} />}</button>
          <i className="tool-divider" />
          {tools.map(({ id, label, Icon }) => <button key={id} data-tooltip={id === "text" ? "Click inside the region to type. Wheel changes font size; drag the border to move or the bottom-right handle to resize. Click outside to confirm." : `${label}: click and drag inside the region. ${id === "arrow" ? "Wheel changes arrow size and thickness." : "Wheel changes outline thickness."} Click again or press Esc to turn off.`} aria-label={label} aria-pressed={tool === id} className={tool === id ? "active" : ""} disabled={busy || !source} onClick={() => { commitText(); setTool(current => current === id ? null : id); setSizePreview(null); }}><Icon size={18} /></button>)}
          <button data-tooltip="Remove the most recent annotation. Keyboard shortcut: Ctrl+Z." aria-label="Undo" disabled={busy || (!annotations.length && !textEntry)} onClick={undo}><Undo2 size={18} /></button>
          <div className={`color-control ${toolbarTop < 260 ? "open-below" : ""}`}>
            <button data-tooltip={colorOpen ? undefined : "Choose annotation color, brightness, and transparency. The swatch shows the current color."} aria-label="Tool color" aria-expanded={colorOpen} disabled={busy} onClick={() => setColorOpen(open => !open)}><span className="color-swatch" style={{ background: color, opacity: alpha }} /></button>
            {colorOpen && <ColorWheel color={color} onChange={setColor} alpha={alpha} onAlphaChange={setAlpha} />}
          </div>
          <i className="tool-divider" />
          <button data-tooltip="Copy the text currently shown: original OCR text or its translation." aria-label="Copy text" disabled={busy || !result?.text} onClick={() => void run("copy")}>{notice === "copy" ? <Check size={18} /> : <Copy size={18} />}</button>
          <button data-tooltip="Extract if needed, then translate offline into your Settings target language. Install missing packs in Settings → Translate." aria-label="Translate text" aria-busy={phase === "Translating offline…"} onClick={() => void run("translate")}><Languages size={18} /></button>
          <button data-tooltip="Copy the selected image, including your annotations, to the clipboard." aria-label="Copy image" disabled={busy || !source} onClick={() => void run("image")}>{notice === "image" ? <Check size={18} /> : <Camera size={18} />}</button>
          <button data-tooltip="Save the selected image and its annotations as a PNG file." aria-label="Save PNG" disabled={busy || !source} onClick={() => void run("save")}>{notice === "save" ? <Check size={18} /> : <Save size={18} />}</button>
          <button data-tooltip="Close capture. Esc first turns off an active tool; press Esc again to close." aria-label="Close capture" onClick={close}><X size={18} /></button>
        </div>
        {displayText && <div className={`editor-result ${result?.error || error ? "error-result" : ""}`} style={{ fontSize: extractedTextSize, lineHeight: `${extractedTextSize * 1.55}px`, height: resultHeight, overflowY: contentHeight > resultHeight + 1 ? "auto" : "hidden" }} aria-label={showTranslation ? "Translated text" : "Extracted text"}>{translation && !phase && !error && <button className="translation-view-toggle" data-tooltip="Switch between the original recognized text and its translation." onClick={() => setShowTranslation(value => !value)}>{showTranslation ? "Original" : "Translation"}</button>}{displayText}</div>}
      </section>}
      {sizePreview && <div className="thickness-preview" style={{ left: clamp(sizePreview.x, Math.max(40, previewSize / 2 + 8), viewport.width - Math.max(40, previewSize / 2 + 8)), top: clamp(sizePreview.y, Math.max(40, previewSize / 2 + 28), viewport.height - Math.max(40, previewSize / 2 + 8)) }}><span style={{ bottom: previewSize / 2 + 8 }}>{previewSize} px</span><i style={{ width: previewSize, height: previewSize, background: color, opacity: alpha }} /></div>}
      <div ref={contentMeasure} className="editor-result result-measure" style={{ fontSize: extractedTextSize, lineHeight: `${extractedTextSize * 1.55}px`, width: panelWidth - 2 }} aria-hidden="true">{translation && !phase && !error && <span className="translation-view-toggle">{showTranslation ? "Original" : "Translation"}</span>}{displayText}</div>
    </>}
  </div>;
}
