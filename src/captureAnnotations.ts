export type Point = { x: number; y: number };
export type Tool = "line" | "freehand" | "rectangle" | "ellipse" | "arrow" | "text";
export type Annotation = { tool: Tool; points: Point[]; text?: string; color: string; alpha?: number; thickness?: number; fontSize?: number; lines?: string[]; arrowSize?: number };
export const INK = "#F2BE8D";
export const STROKE = 3;
export const TEXT_SIZE = 20;

export function arrowHead(start: Point, end: Point, headSize = 14): Point[] {
  const angle = Math.atan2(end.y - start.y, end.x - start.x);
  const size = headSize;
  return [
    { x: end.x - size * Math.cos(angle - Math.PI / 6), y: end.y - size * Math.sin(angle - Math.PI / 6) },
    end,
    { x: end.x - size * Math.cos(angle + Math.PI / 6), y: end.y - size * Math.sin(angle + Math.PI / 6) },
  ];
}

export function roundedArrow(start: Point, end: Point, size = 14) {
  const angle = Math.atan2(end.y - start.y, end.x - start.x);
  const ux = Math.cos(angle), uy = Math.sin(angle);
  const base = { x: end.x - size * ux, y: end.y - size * uy };
  const corners = [end, { x: base.x - size * 0.45 * uy, y: base.y + size * 0.45 * ux }, { x: base.x + size * 0.45 * uy, y: base.y - size * 0.45 * ux }];
  const rounded = corners.map((p, i) => {
    const previous = corners[(i + 2) % 3], next = corners[(i + 1) % 3];
    return { p, before: { x: p.x + (previous.x - p.x) * 0.12, y: p.y + (previous.y - p.y) * 0.12 }, after: { x: p.x + (next.x - p.x) * 0.12, y: p.y + (next.y - p.y) * 0.12 } };
  });
  let path = `M ${rounded[0].before.x} ${rounded[0].before.y}`;
  for (const corner of rounded) path += ` Q ${corner.p.x} ${corner.p.y} ${corner.after.x} ${corner.after.y} L ${rounded[(rounded.indexOf(corner) + 1) % 3].before.x} ${rounded[(rounded.indexOf(corner) + 1) % 3].before.y}`;
  return { base, path: path + " Z" };
}

export async function composeCapture(source: string, width: number, height: number, annotations: Annotation[]): Promise<string> {
  const image = new Image();
  image.src = source;
  await image.decode();
  const canvas = document.createElement("canvas");
  canvas.width = image.naturalWidth;
  canvas.height = image.naturalHeight;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Could not render captured image");
  ctx.drawImage(image, 0, 0);
  ctx.scale(canvas.width / width, canvas.height / height);
  ctx.strokeStyle = INK;
  ctx.fillStyle = INK;
  ctx.lineWidth = STROKE;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.font = `600 ${TEXT_SIZE}px "Segoe UI", sans-serif`;
  ctx.textBaseline = "top";
  for (const annotation of annotations) {
    ctx.strokeStyle = annotation.color;
    ctx.fillStyle = annotation.color;
    ctx.globalAlpha = annotation.alpha ?? 1;
    ctx.lineWidth = annotation.thickness ?? STROKE;
    const start = annotation.points[0];
    const end = annotation.points[annotation.points.length - 1];
    ctx.beginPath();
    if (annotation.tool === "text") {
      const fontSize = annotation.fontSize ?? TEXT_SIZE;
      ctx.font = `600 ${fontSize}px "Segoe UI", sans-serif`;
      (annotation.lines || (annotation.text || "").split("\n")).forEach((line, i) => ctx.fillText(line, start.x, start.y + i * fontSize * 1.25));
      continue;
    }
    if (annotation.tool === "rectangle") {
      ctx.rect(Math.min(start.x, end.x), Math.min(start.y, end.y), Math.abs(end.x - start.x), Math.abs(end.y - start.y));
    } else if (annotation.tool === "ellipse") {
      ctx.ellipse((start.x + end.x) / 2, (start.y + end.y) / 2, Math.abs(end.x - start.x) / 2, Math.abs(end.y - start.y) / 2, 0, 0, Math.PI * 2);
    } else {
      ctx.moveTo(start.x, start.y);
      const head = annotation.tool === "arrow" ? roundedArrow(start, end, annotation.arrowSize) : null;
      const points = annotation.tool === "freehand" ? annotation.points.slice(1) : [head?.base || end];
      points.forEach(point => ctx.lineTo(point.x, point.y));
      if (annotation.tool === "arrow") {
        ctx.stroke();
        ctx.fill(new Path2D(head!.path));
        continue;
      }
    }
    ctx.stroke();
  }
  return canvas.toDataURL("image/png");
}
