import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

export function TooltipLayer() {
  const id = useId();
  const [tip, setTip] = useState<{ text: string; target: HTMLElement } | null>(null);
  const bubble = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: 8, top: 8 });
  useEffect(() => {
    let timer: number | undefined;
    const hide = () => { clearTimeout(timer); setTip(null); };
    const blockNativeMenu = (event: Event) => event.preventDefault();
    const show = (event: Event) => {
      const target = (event.target as Element).closest<HTMLElement>("[data-tooltip]"), text = target?.dataset.tooltip;
      if (!target || !text) { hide(); return; }
      if (event.type === "focusin" && !target.matches(":focus-visible")) { hide(); return; }
      clearTimeout(timer); timer = window.setTimeout(() => { if (target.isConnected && target.dataset.tooltip === text && target.getAttribute("aria-expanded") !== "true") setTip({ text, target }); }, event.type === "focusin" ? 0 : 400);
    };
    document.addEventListener("pointerover", show); document.addEventListener("focusin", show); document.addEventListener("pointerout", hide); document.addEventListener("focusout", hide); document.addEventListener("pointerdown", hide); document.addEventListener("keydown", hide); window.addEventListener("resize", hide);
    window.addEventListener("blur", hide); document.addEventListener("visibilitychange", hide); document.addEventListener("scroll", hide, true); document.addEventListener("contextmenu", blockNativeMenu);
    return () => { hide(); document.removeEventListener("pointerover", show); document.removeEventListener("focusin", show); document.removeEventListener("pointerout", hide); document.removeEventListener("focusout", hide); document.removeEventListener("pointerdown", hide); document.removeEventListener("keydown", hide); window.removeEventListener("resize", hide); window.removeEventListener("blur", hide); document.removeEventListener("visibilitychange", hide); document.removeEventListener("scroll", hide, true); document.removeEventListener("contextmenu", blockNativeMenu); };
  }, []);
  useLayoutEffect(() => {
    if (!tip || !bubble.current) return;
    const rect = tip.target.getBoundingClientRect(), size = bubble.current.getBoundingClientRect();
    setPosition({ left: Math.max(8, Math.min(rect.left + rect.width / 2 - size.width / 2, innerWidth - size.width - 8)), top: rect.top > size.height + 12 ? rect.top - size.height - 7 : Math.min(innerHeight - size.height - 8, rect.bottom + 7) });
    const old = tip.target.getAttribute("aria-describedby"); tip.target.setAttribute("aria-describedby", `${old || ""} ${id}`.trim());
    return () => { if (old) tip.target.setAttribute("aria-describedby", old); else tip.target.removeAttribute("aria-describedby"); };
  }, [tip, id]);
  return tip && createPortal(<div ref={bubble} id={id} role="tooltip" className="app-tooltip" style={position}>{tip.text}</div>, document.body);
}
