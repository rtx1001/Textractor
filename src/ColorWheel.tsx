import { useState } from "react";

export function hsvToHex(h: number, s: number, v: number): string {
  const c = v * s, x = c * (1 - Math.abs((h / 60) % 2 - 1)), m = v - c;
  const rgb = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return "#" + rgb.map(n => Math.round((n + m) * 255).toString(16).padStart(2, "0")).join("").toUpperCase();
}

function hexToHsv(hex: string) {
  const [r, g, b] = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255);
  const v = Math.max(r, g, b), delta = v - Math.min(r, g, b);
  const h = delta === 0 ? 0 : v === r ? ((g - b) / delta + 6) % 6 * 60 : v === g ? ((b - r) / delta + 2) * 60 : ((r - g) / delta + 4) * 60;
  return { h, s: v === 0 ? 0 : delta / v, v };
}

export function ColorWheel({ color, onChange, alpha, onAlphaChange }: { color: string; onChange: (color: string) => void; alpha: number; onAlphaChange: (alpha: number) => void }) {
  const [hsv, setHsv] = useState(() => hexToHsv(color));
  const change = (value: typeof hsv) => { setHsv(value); onChange(hsvToHex(value.h, value.s, value.v)); };
  const choose = (event: React.PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left - rect.width / 2, y = event.clientY - rect.top - rect.height / 2;
    change({ ...hsv, h: (Math.atan2(x, -y) * 180 / Math.PI + 360) % 360, s: Math.min(1, Math.hypot(x, y) / (rect.width / 2)) });
  };
  const angle = hsv.h * Math.PI / 180;
  return <div className="color-popover" role="dialog" aria-label="Tool color">
    <div className="color-wheel" role="slider" tabIndex={0} data-tooltip="Click or drag to choose hue and saturation. Arrow keys change hue; sliders below control brightness and opacity." aria-label="Color hue and saturation" aria-valuemin={0} aria-valuemax={360} aria-valuenow={Math.round(hsv.h)} onPointerDown={e => { e.currentTarget.setPointerCapture(e.pointerId); choose(e); }} onPointerMove={e => { if (e.currentTarget.hasPointerCapture(e.pointerId)) choose(e); }} onKeyDown={e => { if (e.key.startsWith("Arrow")) { e.preventDefault(); change({ ...hsv, h: (hsv.h + (e.key === "ArrowLeft" || e.key === "ArrowDown" ? 355 : 5)) % 360 }); } }}>
      <div className="wheel-darkness" style={{ opacity: 1 - hsv.v }} />
      <i style={{ left: `${50 + Math.sin(angle) * hsv.s * 50}%`, top: `${50 - Math.cos(angle) * hsv.s * 50}%`, background: color }} />
    </div>
    <input type="range" aria-label="Color brightness" data-tooltip="Change color brightness, from dark to bright." min={0} max={100} value={Math.round(hsv.v * 100)} onChange={e => change({ ...hsv, v: Number(e.target.value) / 100 })} />
    <input className="alpha-slider" type="range" aria-label="Color opacity" data-tooltip={`Opacity: ${Math.round(alpha * 100)}%. Lower values make annotations more transparent.`} min={0} max={100} value={Math.round(alpha * 100)} onChange={e => onAlphaChange(Number(e.target.value) / 100)} />
    <div className="color-value"><i style={{ background: color }} /><span>{color}</span></div>
  </div>;
}
