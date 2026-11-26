import type { Map as MaplibreMap } from "maplibre-gl";

const BEARING_STEP = 12;
const POINT_SPREAD = (50 * Math.PI) / 180;
const POINT_EXTRA_RADIUS = 6;
const BODY_HEIGHT = 32;
const BASE_BODY_WIDTH = 22;
const WIDTH_PER_LABEL_CHAR = 6.2;
const TEXT_SIZE = 14;
const SHAPE_STEPS = 90;

const COLORS: Record<string, string> = {
  red: "#ff0000",
  blue: "#0000ff",
};

function ellipseRadius(angle: number, semiW: number, semiH: number): number {
  return (semiW * semiH) / Math.hypot(semiW * Math.sin(angle), semiH * Math.cos(angle));
}

function normalizeAngle(a: number): number {
  a = a % (2 * Math.PI);
  if (a > Math.PI) a -= 2 * Math.PI;
  return a;
}

export function eggRadius(
  angle: number,
  semiW: number,
  semiH: number,
  pointAngle: number,
  extraR: number,
): number {
  const base = ellipseRadius(angle, semiW, semiH);
  const diff = normalizeAngle(angle - pointAngle);
  if (Math.abs(diff) <= POINT_SPREAD) {
    const t = 1 - Math.abs(diff) / POINT_SPREAD;
    return (1 - t) * base + t * (ellipseRadius(pointAngle, semiW, semiH) + t * t * extraR);
  }
  return base;
}

function createVehicleIcon(
  routeId: string,
  bearing: number | null,
  color: string,
  pixelRatio: number,
): ImageData {
  const dpr = pixelRatio;
  const shadowBlur = 3 * dpr;
  const pad = 1 * dpr;
  const extraR = POINT_EXTRA_RADIUS * dpr;
  const bodyWidth = (BASE_BODY_WIDTH + WIDTH_PER_LABEL_CHAR * routeId.length) * dpr;
  const bodyHeight = BODY_HEIGHT * dpr;
  const halfW = bodyWidth / 2;
  const halfH = bodyHeight / 2;

  const hasBearing = bearing !== null;
  const pointAngle = hasBearing ? ((bearing - 90) * Math.PI) / 180 : 0;

  let canvasW = bodyWidth + 2 * pad + 2 * shadowBlur;
  let canvasH = bodyHeight + 2 * pad + 2 * shadowBlur;
  if (hasBearing) {
    canvasW += 2 * extraR;
    canvasH += 2 * extraR;
  }

  const canvas = document.createElement("canvas");
  canvas.width = canvasW;
  canvas.height = canvasH;

  const cx = canvasW / 2;
  const cy = canvasH / 2;
  const ctx = canvas.getContext("2d")!;

  ctx.shadowColor = "rgba(0, 0, 0, 0.6)";
  ctx.shadowBlur = shadowBlur;
  ctx.fillStyle = COLORS[color] ?? "#ff0000";
  ctx.strokeStyle = "rgba(0, 0, 0, 1)";
  ctx.lineWidth = 0.5 * dpr;
  ctx.beginPath();
  if (hasBearing) {
    for (let i = 0; i <= SHAPE_STEPS; i++) {
      const a = pointAngle + (i / SHAPE_STEPS) * 2 * Math.PI;
      const r = eggRadius(a, halfW, halfH, pointAngle, extraR);
      const x = cx + r * Math.cos(a);
      const y = cy + r * Math.sin(a);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.closePath();
  } else {
    ctx.ellipse(cx, cy, halfW, halfH, 0, 0, 2 * Math.PI);
  }
  ctx.fill();
  ctx.stroke();

  ctx.fillStyle = "#FFFFFF";
  ctx.font = `800 ${TEXT_SIZE * dpr}px IosevkAllyP, sans-serif`;
  ctx.textBaseline = "middle";
  ctx.textAlign = "center";
  ctx.shadowColor = "rgba(0, 0, 0, 0.25)";
  ctx.shadowBlur = 2 * dpr;
  ctx.shadowOffsetX = 0;
  ctx.shadowOffsetY = 0;
  ctx.fillText(routeId, cx, cy);

  return ctx.getImageData(0, 0, canvasW, canvasH);
}

export function quantizeBearing(bearing: number | null): number | null {
  if (bearing === null) return null;
  let q = Math.round(bearing / BEARING_STEP) * BEARING_STEP;
  if (q >= 360) q = 0;
  if (q < 0) q += 360;
  return q;
}

export function vehicleIconName(routeId: string, color: string, qBearing: number | null): string {
  return `vehicle-${color}-r${encodeURIComponent(routeId)}-b${qBearing ?? "none"}`;
}

export type VehicleIconDescriptor = {
  routeId: string;
  color: string;
  qBearing: number | null;
};

export function ensureVehicleIcons(map: MaplibreMap, icons: VehicleIconDescriptor[]) {
  const pixelRatio = window.devicePixelRatio || 1;

  for (const icon of icons) {
    const name = vehicleIconName(icon.routeId, icon.color, icon.qBearing);
    if (!map.hasImage(name)) {
      const imageData = createVehicleIcon(icon.routeId, icon.qBearing, icon.color, pixelRatio);
      map.addImage(name, imageData, { pixelRatio });
    }
  }
}
