import { createWorker, OEM, PSM, type Worker, type Word } from "tesseract.js";
import { installedModels, OCR_LANGUAGES } from "./ocrLanguages";

let active: Promise<Worker> | null = null;
let activeLanguages = "";
let currentLanguage = "";
let idleTimer: number | undefined;
const asset = (path: string) => new URL(`/ocr/${path}`, document.baseURI).href;

function visualText(words: Word[], fallback: string): string {
  const sorted = words.filter(w => w.text.trim() && w.confidence > 15).sort((a, b) => (a.bbox.y0 + a.bbox.y1) / 2 - (b.bbox.y0 + b.bbox.y1) / 2);
  const rows: Word[][] = [];
  for (const word of sorted) {
    const row = rows[rows.length - 1];
    const center = (word.bbox.y0 + word.bbox.y1) / 2;
    if (row && Math.abs(center - (row[0].bbox.y0 + row[0].bbox.y1) / 2) <= Math.max(word.bbox.y1 - word.bbox.y0, row[0].bbox.y1 - row[0].bbox.y0) * 0.6) row.push(word);
    else rows.push([word]);
  }
  if (!rows.length) return fallback.trim();
  const minX = Math.min(...sorted.map(w => w.bbox.x0));
  const charWidth = Math.max(2, sorted.reduce((sum, w) => sum + (w.bbox.x1 - w.bbox.x0) / Math.max(1, [...w.text].length), 0) / sorted.length);
  return rows.map(row => {
    row.sort((a, b) => a.bbox.x0 - b.bbox.x0);
    let text = "", previous: Word | null = null;
    for (const word of row) {
      const gap = previous ? word.bbox.x0 - previous.bbox.x1 : word.bbox.x0 - minX;
      const japanese = previous && /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Thai}]/u.test(previous.text + word.text);
      const spaces = previous ? (gap > charWidth * 2.5 ? Math.max(2, Math.round((word.bbox.x0 - minX) / charWidth) - [...text].length) : japanese || /^[.,!?;:%)\]}]/.test(word.text) ? 0 : 1) : Math.max(0, Math.round(gap / charWidth));
      text += " ".repeat(Math.min(300, spaces)) + word.text;
      previous = word;
    }
    return text.trimEnd();
  }).join("\n");
}

export async function multilingualOcr(image: string, language: string): Promise<string> {
  window.clearTimeout(idleTimer);
  const selected = OCR_LANGUAGES.find(item => item.value === language);
  if (!selected) throw new Error("Unsupported OCR language.");
  const installed = await installedModels();
  const codes = language === "auto" ? installed : [selected.code];
  if (language !== "auto" && !installed.includes(selected.code)) throw new Error("Install this OCR language in Settings → OCR first.");
  if (!codes.length) return "";
  const languages = codes.join("+");
  if (active && activeLanguages !== languages) { await (await active).terminate(); active = null; }
  if (!active) {
    activeLanguages = languages;
    currentLanguage = codes[0];
    active = createWorker(codes[0], OEM.LSTM_ONLY, { workerPath: asset("worker.min.js"), corePath: asset("core"), langPath: asset("lang"), gzip: false, cacheMethod: "readOnly", cachePath: "textractor-models", workerBlobURL: false });
    active.catch(() => { active = null; });
  }
  const worker = await active;
  const source = new Image(); source.src = image; await source.decode();
  const scale = Math.min(3, 4500 / Math.max(source.width, source.height));
  const canvas = document.createElement("canvas"); canvas.width = Math.round(source.width * scale); canvas.height = Math.round(source.height * scale);
  canvas.getContext("2d")!.drawImage(source, 0, 0, canvas.width, canvas.height);
  try {
    let best = { text: "", score: -1 };
    // Comparing separate recognizers avoids a Latin model stripping accents
    // when several installed languages are loaded as one combined model.
    for (const code of codes) {
      if (currentLanguage !== code) { await worker.reinitialize(code, OEM.LSTM_ONLY); currentLanguage = code; }
      await worker.setParameters({ tessedit_pageseg_mode: PSM.SINGLE_BLOCK, preserve_interword_spaces: "1", user_defined_dpi: "300" });
      const { data } = await worker.recognize(canvas, {}, { text: true, blocks: true });
      const words = (data.blocks || []).flatMap(block => block.paragraphs.flatMap(paragraph => paragraph.lines.flatMap(line => line.words)));
      const text = code === "ara" ? (data.text || "").trim() : visualText(words, data.text || "");
      const nativeScript = code === "vie" ? /[đĐăĂơƠưƯ\u1ea0-\u1eff]/.test(text) : code === "jpn" ? /[\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Han}]/u.test(text) : false;
      const score = data.confidence + (nativeScript ? 18 : 0);
      if (text && score > best.score) best = { text, score };
    }
    return best.text;
  } finally {
    idleTimer = window.setTimeout(() => { const idle = active; active = null; if (idle) void idle.then(value => value.terminate()); }, 60000);
  }
}

export function containsNonEnglishScript(text: string): boolean {
  return /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\p{Script=Cyrillic}\p{Script=Arabic}\p{Script=Devanagari}\p{Script=Thai}\u00c0-\u024f\u1e00-\u1eff]/u.test(text);
}
