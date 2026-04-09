<template>
  <canvas ref="canvasRef" :width="size" :height="size" />
</template>

<script setup>
import { ref, onMounted, onUnmounted, watch } from "vue";

const props = defineProps({
  // Canvas render size in px
  size: {
    type: Number,
    default: 340,
  },
  // When true, animation snaps to final frame and stops
  loaded: {
    type: Boolean,
    default: false,
  },
});

const canvasRef = ref(null);
let rafId = null;

const BLUE_PATHS = [
  "M1072.1,936.38c-93.56,76.2-191.35,140.64-272.57,96.41-16.39-8.92-31.79-22.26-48.09-36.38-28.5-24.68-57.96-50.19-97.42-55.85-33.89-4.87-65.68,6.99-93.74,17.48l-7.44,2.77c-90.83,33.44-160.86,34.28-214.09,2.48-17.22-10.27-33.05-24.25-49.81-39.06-20.71-18.29-42.14-37.2-67.85-50.27-57.56-29.28-119.6-22.39-181.08-4.04v50.83c57.23-18.84,111.64-27.6,159.06-3.48,20.22,10.28,38.43,26.37,57.71,43.39,17.69,15.61,35.98,31.76,57.06,44.35,65.75,39.23,151.81,39.7,255.81,1.38l7.64-2.83c24.8-9.27,48.21-17.94,69.83-14.92,25.34,3.64,48.25,23.49,72.51,44.5,17.53,15.19,35.67,30.89,56.67,42.33,26.6,14.47,53.78,20.52,80.8,20.52,95.98,0,189.67-76.31,245.69-121.93,57.12-46.52,103.11-65.49,137.22-56.77v-49.73c-47.05-6.73-103.23,16.16-167.9,68.82Z",
  "M1057.45,743.24c-93.56,76.2-191.35,140.66-272.57,96.41-16.39-8.92-31.79-22.27-48.1-36.4-28.49-24.66-57.95-50.18-97.43-55.85-33.82-4.87-65.65,7.02-93.72,17.5l-7.44,2.77c-90.83,33.44-160.87,34.26-214.09,2.48-17.22-10.28-33.06-24.27-49.84-39.07-20.71-18.29-42.12-37.19-67.83-50.27-53-26.96-109.78-23.23-166.44-8.16v50.37c51.85-15.47,101.05-20.91,144.41,1.1,20.21,10.28,38.41,26.35,57.7,43.39,17.69,15.61,35.99,31.78,57.09,44.37,65.73,39.25,151.8,39.7,255.8,1.38l7.64-2.83c24.82-9.27,48.25-17.99,69.82-14.93,25.35,3.65,48.25,23.49,72.51,44.51,17.54,15.19,35.68,30.89,56.68,42.33,26.6,14.47,53.78,20.52,80.8,20.52,95.98,0,189.67-76.31,245.69-121.93,65.76-53.57,116.78-70.59,151.86-50.89v-52.5c-49.88-14.44-111.03,7.46-182.55,65.71Z",
];

const NODE_DEFS = [
  { id: 0, cx: 148.2, cy: 330.37 },
  { id: 1, cx: 408.48, cy: 578.7 },
  { id: 2, cx: 555.47, cy: 290.43 },
  { id: 3, cx: 862.79, cy: 635.95 },
  { id: 4, cx: 1133.58, cy: 330.22 },
];

const EDGE_DEFS = [
  [0, 2],
  [2, 1],
  [2, 3],
  [3, 4],
];

const SEQUENCE = [
  { type: "node", id: 2 },
  { type: "node", id: 0 },
  { type: "edge", id: 0 },
  { type: "node", id: 1 },
  { type: "edge", id: 1 },
  { type: "node", id: 4 },
  { type: "edge", id: 2 },
  { type: "node", id: 3 },
  { type: "edge", id: 3 },
];

const STEP_DUR = 20;
const HOLD = 50;
const TOTAL = SEQUENCE.length * STEP_DUR + HOLD;

function easeOut(t) {
  return 1 - Math.pow(1 - t, 3);
}
function easeInOut(t) {
  return t < 0.5 ? 2 * t * t : -1 + (4 - 2 * t) * t;
}

function edgeEndpoints(n1, n2, R) {
  const x1 = n1.cx,
    y1 = n1.cy;
  const x2 = n2.cx,
    y2 = n2.cy;
  const dx = x2 - x1,
    dy = y2 - y1;
  const dist = Math.sqrt(dx * dx + dy * dy);
  const ux = dx / dist,
    uy = dy / dist;
  return {
    x1: x1 + ux * R,
    y1: y1 + uy * R,
    x2: x2 - ux * R,
    y2: y2 - uy * R,
  };
}

function drawFrame(ctx, S, frameIndex) {
  const size = ctx.canvas.width;
  ctx.clearRect(0, 0, size, size);

  const R = 73.9;
  const STROKE = 18;
  const EDGE_W = 14;

  const f = frameIndex % TOTAL;
  const stepsDone = Math.min(SEQUENCE.length, Math.floor(f / STEP_DUR));

  const nodeP = {};
  const edgeP = {};

  for (let i = 0; i < stepsDone; i++) {
    const s = SEQUENCE[i];
    if (s.type === "node") nodeP[s.id] = 1;
    if (s.type === "edge") edgeP[s.id] = 1;
  }
  if (stepsDone < SEQUENCE.length) {
    const cur = SEQUENCE[stepsDone];
    const p = Math.min(1, (f % STEP_DUR) / (STEP_DUR * 0.8));
    if (cur.type === "node") nodeP[cur.id] = p;
    if (cur.type === "edge") edgeP[cur.id] = p;
  }

  // Edges
  ctx.save();
  ctx.scale(S, S);
  for (const [eid, prog] of Object.entries(edgeP)) {
    const [a, b] = EDGE_DEFS[eid];
    const ep = edgeEndpoints(NODE_DEFS[a], NODE_DEFS[b], R);
    const ex = ep.x1 + (ep.x2 - ep.x1) * easeInOut(prog);
    const ey = ep.y1 + (ep.y2 - ep.y1) * easeInOut(prog);
    ctx.strokeStyle = "#4daf4e";
    ctx.lineWidth = EDGE_W;
    ctx.lineCap = "round";
    ctx.globalAlpha = 1;
    ctx.beginPath();
    ctx.moveTo(ep.x1, ep.y1);
    ctx.lineTo(ex, ey);
    ctx.stroke();
  }
  ctx.restore();

  // Nodes (drawn on top so rings cover edge endpoints)
  ctx.save();
  ctx.scale(S, S);
  for (const [nid, prog] of Object.entries(nodeP)) {
    const n = NODE_DEFS[nid];
    const r = R * easeOut(prog);
    const isNew =
      stepsDone < SEQUENCE.length &&
      SEQUENCE[stepsDone].type === "node" &&
      SEQUENCE[stepsDone].id === n.id;
    // pulse ring
    if (isNew && prog < 1) {
      ctx.globalAlpha = (1 - prog) * 0.4;
      ctx.strokeStyle = "#4daf4e";
      ctx.lineWidth = STROKE * 0.6;
      ctx.beginPath();
      ctx.arc(n.cx, n.cy, r + 10 * (1 - prog), 0, Math.PI * 2);
      ctx.stroke();
    }
    // hollow ring
    ctx.globalAlpha = Math.min(1, prog * 1.5);
    ctx.strokeStyle = "#4daf4e";
    ctx.lineWidth = STROKE;
    ctx.beginPath();
    ctx.arc(n.cx, n.cy, r, 0, Math.PI * 2);
    ctx.stroke();
  }
  ctx.restore();

  // Blue waves
  ctx.save();
  ctx.scale(S, S);
  ctx.fillStyle = "#478fcc";
  for (const d of BLUE_PATHS) ctx.fill(new Path2D(d));
  ctx.restore();
}

onMounted(() => {
  const canvas = canvasRef.value;
  const ctx = canvas.getContext("2d");
  const S = props.size / 1280;
  let frame = 0;

  function loop() {
    if (props.loaded) {
      // Snap to final completed frame and stop
      drawFrame(ctx, S, TOTAL - HOLD);
      return;
    }
    drawFrame(ctx, S, frame++);
    rafId = requestAnimationFrame(loop);
  }

  loop();
});

// If `loaded` flips to true mid-animation, cancel the loop
watch(
  () => props.loaded,
  (val) => {
    if (val && rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
      const canvas = canvasRef.value;
      const ctx = canvas.getContext("2d");
      const S = props.size / 1280;
      drawFrame(ctx, S, TOTAL - HOLD);
    }
  }
);

onUnmounted(() => {
  if (rafId !== null) cancelAnimationFrame(rafId);
});
</script>
