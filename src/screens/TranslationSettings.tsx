import { useEffect, useState } from "react";
import { Check, Download, LoaderCircle, RefreshCw } from "lucide-react";
import type { AppConfig, TranslationStatus } from "../types";
import { CustomSelect } from "../CustomSelect";
import { normalizeLanguage, translationPlan } from "../translationPlan";

type Api = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export function TranslationSettings({ api, config, save, report }: { api: Api; config: AppConfig; save: (config: AppConfig) => void; report: (message: string, error?: boolean) => void }) {
  const [status, setStatus] = useState<TranslationStatus>({ ready: false, packs: [] });
  const [working, setWorking] = useState(false), [loading, setLoading] = useState(true);
  useEffect(() => {
    let mounted = true;
    void (async () => {
      try { let value = await api<TranslationStatus>("translation_packs", { refresh: false }); if (!value?.packs.length) value = await api<TranslationStatus>("translation_packs", { refresh: true }); if (mounted && value) setStatus(value); }
      catch (e) { if (mounted) report(String(e), true); }
      finally { if (mounted) setLoading(false); }
    })();
    return () => { mounted = false; };
  }, [api]);
  const refresh = async () => {
    setLoading(true); report("Refreshing languages…");
    try { setStatus(await api<TranslationStatus>("translation_packs", { refresh: true })); report("Language list updated"); }
    catch (e) { report(String(e), true); } finally { setLoading(false); }
  };
  const source = normalizeLanguage(config.translation_source || "auto"), target = normalizeLanguage(config.translation_language || "en");
  const plan = translationPlan(status.packs, source, target), missing = plan.required.filter(pack => !pack.installed);
  const repair = !status.ready && plan.required.length > 0 && !missing.length;
  const install = async () => {
    const needed = missing.length ? missing : repair ? plan.required.slice(0, 1) : [];
    if (!needed.length) return;
    setWorking(true);
    try {
      for (let index = 0; index < needed.length; index++) {
        const pack = needed[index];
        report(`${repair ? "Repairing engine" : `Installing ${pack.label}`}…${needed.length > 1 ? ` (${index + 1}/${needed.length})` : ""}`);
        setStatus(await api<TranslationStatus>("manage_translation_pack", { source: pack.source, target: pack.target, remove: false }));
      }
      report("Ready for offline translation");
    } catch (e) { report(String(e), true); } finally { setWorking(false); }
  };
  const languages = new Map<string, string>([["en", "English"], ["vi", "Vietnamese"], ["ja", "Japanese"], ["ko", "Korean"], ["zh", "Chinese (Simplified)"], ["zt", "Chinese (Traditional)"], ["fr", "French"], ["de", "German"], ["es", "Spanish"], ["pt", "Portuguese"], ["ru", "Russian"], ["ar", "Arabic"], ["hi", "Hindi"], ["th", "Thai"]]);
  for (const item of status.packs) { const [from, to] = item.label.split(" → "); languages.set(item.source, from); languages.set(item.target, to); }
  const options = [...languages].sort((a, b) => a[1].localeCompare(b[1])).map(([value, label]) => ({ value, label }));
  const ready = plan.kind === "identity" || (plan.required.length > 0 && !missing.length && status.ready);
  return <section id="translate-panel" role="tabpanel" aria-labelledby="translate-tab" className="compact-languages translation-settings">
    <div className="translation-pair">
      <div className="compact-field"><span>Translate from</span><CustomSelect label="Translation source language" value={source} options={[{ value: "auto", label: "Detect automatically" }, ...options]} disabled={working} onChange={value => save({ ...config, translation_source: value })} tooltip="Language of captured text. Automatic detection uses installed packs; select a source to install its missing packs." /></div>
      <div className="compact-field"><span>Translate to</span><CustomSelect label="Translation target language" value={target} options={options} disabled={working} onChange={value => save({ ...config, translation_language: value })} tooltip="Language to translate into. Only packs needed for your selected source and target are downloaded." /></div>
    </div>
    <div className="translation-readiness"><span>{loading ? "Checking language packs…" : ready ? <><Check size={14} />{plan.kind === "identity" ? "Same language · no pack needed" : "Ready offline"}</> : plan.message}</span><button className="icon-control" aria-label="Refresh language list" data-tooltip="Refresh available languages. Downloads only a small catalogue, not any models." disabled={working || loading} onClick={() => void refresh()}><RefreshCw size={14} /></button></div>
    {(missing.length > 0 || repair) && <button className="install-language-button" disabled={working || loading} data-tooltip={`${repair ? "Add private engine libraries without deleting your packs." : `Download ${missing.map(pack => pack.label).join(" and ")} for this pair.`} ${!status.ready ? "The shared engine (~150 MB) is set up once." : ""} Text is never sent online.`} onClick={() => void install()}>{working ? <LoaderCircle size={15} className="spinning" /> : <Download size={15} />}{working ? "Installing…" : repair ? "Repair translation engine" : "Install language pack"}</button>}
    <p className="compact-hint">{source === "auto" ? "Automatic detection uses installed source languages. Choose a source above to add its missing packs." : plan.kind === "bridge" ? "This pair translates through English. Install downloads only the missing legs." : "Packs are checked for your chosen pair. Once installed, translation works offline."}</p>
    <p className="translation-privacy">Text stays on your PC.{!status.ready && plan.kind !== "identity" ? " First install adds a ~150 MB engine." : ""}</p>
  </section>;
}
