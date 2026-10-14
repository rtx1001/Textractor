import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown, Search } from "lucide-react";

export type SelectOption = { value: string; label: string };
export function CustomSelect({ label, value, options, onChange, disabled = false, tooltip }: { label: string; value: string; options: SelectOption[]; onChange: (value: string) => void; disabled?: boolean; tooltip?: string }) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null), popup = useRef<HTMLDivElement>(null), search = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false), [query, setQuery] = useState(""), [active, setActive] = useState(0);
  const [position, setPosition] = useState({ left: 0, top: 0, width: 180, height: 180 });
  const filtered = options.filter(option => option.label.toLocaleLowerCase().includes(query.toLocaleLowerCase()));
  const close = (focus = false) => { setOpen(false); if (focus) trigger.current?.focus(); };
  const choose = (next: string) => { onChange(next); close(true); };
  useLayoutEffect(() => {
    if (!open || !trigger.current) return;
    const rect = trigger.current.getBoundingClientRect(), below = innerHeight - rect.bottom - 10, above = rect.top - 10;
    const height = Math.min(238, 46 + Math.max(1, filtered.length) * 30, Math.max(below, above));
    setPosition({ left: Math.max(8, Math.min(rect.left, innerWidth - rect.width - 8)), top: Math.max(8, below >= height ? rect.bottom + 5 : rect.top - height - 5), width: rect.width, height });
    search.current?.focus();
  }, [open, filtered.length]);
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => { if (!trigger.current?.contains(event.target as Node) && !popup.current?.contains(event.target as Node)) close(); };
    const dismiss = () => close();
    document.addEventListener("pointerdown", outside); window.addEventListener("resize", dismiss);
    return () => { document.removeEventListener("pointerdown", outside); window.removeEventListener("resize", dismiss); };
  }, [open]);
  useEffect(() => { if (disabled) setOpen(false); }, [disabled]);
  useLayoutEffect(() => { if (open) document.getElementById(`${id}-option-${active}`)?.scrollIntoView({ block: "nearest" }); }, [active, open, query, position.height]);
  const show = () => { setQuery(""); setActive(Math.max(0, options.findIndex(option => option.value === value))); setOpen(true); };
  const keys = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(true); }
    else if (event.key === "Tab") close(true);
    else if (event.key === "ArrowDown" || event.key === "ArrowUp") { event.preventDefault(); if (!open) show(); else setActive(index => Math.max(0, Math.min(filtered.length - 1, index + (event.key === "ArrowDown" ? 1 : -1)))); }
    else if (event.key === "Enter" && open) { event.preventDefault(); if (filtered[active]) choose(filtered[active].value); }
  };
  return <>
    <button ref={trigger} type="button" role="combobox" className="custom-select-trigger" aria-label={label} aria-expanded={open} aria-controls={`${id}-list`} aria-haspopup="listbox" disabled={disabled} data-tooltip={open ? undefined : tooltip} onKeyDown={keys} onClick={() => open ? close() : show()}><span>{options.find(option => option.value === value)?.label || value}</span><ChevronDown size={14} /></button>
    {open && createPortal(<div ref={popup} className="custom-select-popup" style={{ left: position.left, top: position.top, width: position.width, height: position.height }} onKeyDown={keys}>
      <div className="custom-select-search"><Search size={13} /><input ref={search} type="search" aria-label={`Search ${label}`} autoComplete="off" placeholder="Find language…" value={query} onChange={event => { setQuery(event.target.value); setActive(0); }} aria-controls={`${id}-list`} aria-activedescendant={filtered[active] ? `${id}-option-${active}` : undefined} /></div>
      <div className="custom-select-options" role="listbox" id={`${id}-list`} aria-label={label}>{filtered.map((option, index) => <button type="button" role="option" tabIndex={-1} id={`${id}-option-${index}`} key={option.value} aria-selected={option.value === value} className={index === active ? "highlighted" : ""} onPointerMove={() => setActive(index)} onClick={() => choose(option.value)}><span>{option.label}</span>{option.value === value && <Check size={13} />}</button>)}{!filtered.length && <div className="custom-select-empty">No matches</div>}</div>
    </div>, document.body)}
  </>;
}
