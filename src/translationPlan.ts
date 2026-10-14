import type { TranslationPack } from "./types";

export type TranslationPlan = { required: TranslationPack[]; kind: "direct" | "bridge" | "auto" | "identity" | "unavailable"; message: string };
export const normalizeLanguage = (code: string) => code === "zh-CN" ? "zh" : code === "zh-TW" ? "zt" : code;
export function translationPlan(packs: TranslationPack[], sourceValue: string, targetValue: string): TranslationPlan {
  const source = normalizeLanguage(sourceValue), target = normalizeLanguage(targetValue);
  const find = (from: string, to: string) => packs.find(pack => pack.source === from && pack.target === to);
  if (source === target) return { kind: "identity", required: [], message: "Same language · no translation pack needed" };
  if (source === "auto") {
    const base = target !== "en" ? find("en", target) : undefined;
    const installed = packs.filter(pack => pack.installed && pack.target === target);
    return { kind: "auto", required: base ? [base] : installed, message: base ? base.label : installed.length ? "Automatic · installed source languages ready" : "Choose a source to install its language pack" };
  }
  const direct = find(source, target), first = source !== "en" && target !== "en" ? find(source, "en") : undefined, second = first ? find("en", target) : undefined;
  if (direct?.installed) return { kind: "direct", required: [direct], message: direct.label };
  if (first?.installed && second?.installed) return { kind: "bridge", required: [first, second], message: "Via English · both packs installed" };
  if (direct) return { kind: "direct", required: [direct], message: direct.label };
  if (first && second) return { kind: "bridge", required: [first, second], message: "Via English · requires two directional packs" };
  return { kind: "unavailable", required: [], message: "No offline route available for this pair" };
}
