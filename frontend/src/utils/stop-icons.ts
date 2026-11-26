import type { Map as MaplibreMap } from "maplibre-gl";
import { eggRadius } from "@/utils/vehicle-icons";

/** Body radius at icon size 1. The style JSON files scale it with `icon-size`. */
const BODY_RADIUS = 10;
const POINT_EXTRA_RADIUS = 5;
const SHAPE_STEPS = 90;

/** Icon name that the style JSON files use for stops that have a bearing. */
const ICON_NAME = "stop-arrow";
const FILL = "#000";
const OUTLINE = "#fff";
const OUTLINE_WIDTH = 3;

/**
 * Draws a dot with a point toward the top (north). The style rotates it by the
 * stop bearing. The canvas center is the dot center, so the rotation keeps the
 * dot on the stop position.
 */
function drawStopArrow(pixelRatio: number): ImageData {
  const radius = BODY_RADIUS * pixelRatio;
  const extra = POINT_EXTRA_RADIUS * pixelRatio;
  const pad = (OUTLINE_WIDTH + 1) * pixelRatio;
  const size = Math.ceil(2 * (radius + extra + pad));
  const center = size / 2;
  const pointAngle = -Math.PI / 2;

  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d")!;

  ctx.beginPath();
  for (let i = 0; i <= SHAPE_STEPS; i++) {
    const angle = pointAngle + (i / SHAPE_STEPS) * 2 * Math.PI;
    const r = eggRadius(angle, radius, radius, pointAngle, extra);
    const x = center + r * Math.cos(angle);
    const y = center + r * Math.sin(angle);
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  }
  ctx.closePath();
  // Stroke first and fill after, so that only the outer half of the outline shows.
  ctx.lineJoin = "round";
  ctx.lineWidth = 2 * OUTLINE_WIDTH * pixelRatio;
  ctx.strokeStyle = OUTLINE;
  ctx.stroke();
  ctx.fillStyle = FILL;
  ctx.fill();

  return ctx.getImageData(0, 0, size, size);
}

export function ensureStopArrowIcons(map: MaplibreMap) {
  const pixelRatio = window.devicePixelRatio || 1;
  if (map.hasImage(ICON_NAME)) return;
  map.addImage(ICON_NAME, drawStopArrow(pixelRatio), { pixelRatio });
}
