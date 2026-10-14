export type AppConfig = {
  hotkey: string;
  launch_at_startup: boolean;
  copy_text_automatically: boolean;
  tool_color?: string;
  tool_alpha?: number;
  tool_thickness?: number;
  tool_text_size?: number;
  tool_arrow_size?: number;
  ocr_language?: string;
  translation_language?: string;
  translation_source?: string;
  extracted_text_size?: number;
};

export type TranslationResult = { text: string; source: string; target: string };
export type TranslationPack = { id: string; source: string; target: string; label: string; installed: boolean };
export type TranslationStatus = { ready: boolean; packs: TranslationPack[] };

export type CaptureResult = {
  text: string;
  error: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
};
