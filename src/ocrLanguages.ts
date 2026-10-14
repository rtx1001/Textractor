export const OCR_LANGUAGES = [
  { value: "auto", code: "", label: "Automatic · installed languages", bundled: true },
  { value: "en", code: "eng", label: "English · Windows OCR", bundled: true },
  { value: "en-local", code: "eng", label: "English · offline pack", bundled: false },
  { value: "vi", code: "vie", label: "Vietnamese", bundled: false },
  { value: "ja", code: "jpn", label: "Japanese", bundled: false },
  { value: "zh-CN", code: "chi_sim", label: "Chinese (Simplified)", bundled: false },
  { value: "zh-TW", code: "chi_tra", label: "Chinese (Traditional)", bundled: false },
  { value: "ko", code: "kor", label: "Korean", bundled: false },
  { value: "fr", code: "fra", label: "French", bundled: false },
  { value: "de", code: "deu", label: "German", bundled: false },
  { value: "es", code: "spa", label: "Spanish", bundled: false },
  { value: "pt", code: "por", label: "Portuguese", bundled: false },
  { value: "ru", code: "rus", label: "Russian", bundled: false },
  { value: "ar", code: "ara", label: "Arabic", bundled: false },
  { value: "hi", code: "hin", label: "Hindi", bundled: false },
  { value: "th", code: "tha", label: "Thai", bundled: false },
];

async function modelStore(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("keyval-store", 1);
    request.onupgradeneeded = () => request.result.createObjectStore("keyval");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

export async function installedModels(): Promise<string[]> {
  const db = await modelStore();
  try { return await new Promise((resolve, reject) => {
    const request = db.transaction("keyval").objectStore("keyval").getAllKeys();
    request.onsuccess = () => resolve(request.result.map(String).filter(key => key.startsWith("textractor-models/") && key.endsWith(".traineddata")).map(key => key.slice("textractor-models/".length, -".traineddata".length))); request.onerror = () => reject(request.error);
  }); } finally { db.close(); }
}

export async function removeModel(code: string): Promise<void> {
  const db = await modelStore();
  try { await new Promise<void>((resolve, reject) => {
    const transaction = db.transaction("keyval", "readwrite"); transaction.objectStore("keyval").delete(`textractor-models/${code}.traineddata`);
    transaction.oncomplete = () => resolve(); transaction.onerror = () => reject(transaction.error);
  }); } finally { db.close(); }
}

export async function optionalModel(code: string, install = false): Promise<Uint8Array> {
  if (!OCR_LANGUAGES.some(item => item.code === code)) throw new Error("Unsupported OCR language pack.");
  const db = await modelStore();
  try {
    const existing = await new Promise<Uint8Array | undefined>((resolve, reject) => {
      const request = db.transaction("keyval").objectStore("keyval").get(`textractor-models/${code}.traineddata`);
      request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error);
    });
    if (existing) return existing;
    if (!install) throw new Error("This OCR language is not installed. Install it in Settings → OCR first.");
    let response: Response;
    try { response = await fetch(`https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/main/${code}.traineddata`, { signal: AbortSignal.timeout(90000) }); }
    catch { throw new Error("This language needs a one-time model download. Connect to the internet and try again."); }
    if (!response.ok) throw new Error(`Could not download the language model (${response.status}).`);
    const data = new Uint8Array(await response.arrayBuffer());
    if (data.byteLength < 100000) throw new Error("The language model download is incomplete.");
    await new Promise<void>((resolve, reject) => {
      const transaction = db.transaction("keyval", "readwrite"); transaction.objectStore("keyval").put(data, `textractor-models/${code}.traineddata`);
      transaction.oncomplete = () => resolve(); transaction.onerror = () => reject(transaction.error);
    });
    return data;
  } finally { db.close(); }
}
