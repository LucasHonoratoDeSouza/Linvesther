"use client";

import { useEffect, useRef } from "react";

/** Direct port of linvesther-clean-canvas-v10.html's drawing logic —
 * same bezier control points, same phase timing, same easing. Ported
 * to a React component (canvas ref + requestAnimationFrame in a
 * cleanup-safe effect) rather than the original's inline <script>, but
 * the actual animation math is unchanged. */

type Point = { x: number; y: number };
type Curve = [Point, Point, Point, Point];

const SIZE = 180;
const WIDTH = 18;
const R = WIDTH / 2;
const COLOR = "#d8ff91";
const CYCLE_SECONDS = 5.6;

const C: Point = { x: 96, y: 92 };
const leftMain: Curve = [C, { x: 80, y: 92 }, { x: 59, y: 91 }, { x: 36, y: 90 }];
const rightMain: Curve = [C, { x: 111, y: 84 }, { x: 130, y: 68 }, { x: 146, y: 62 }];
const topA: Curve = [C, { x: 82, y: 82 }, { x: 70, y: 62 }, { x: 57, y: 51 }];
const topB: Curve = [{ x: 57, y: 51 }, { x: 50, y: 45 }, { x: 43, y: 43 }, { x: 36, y: 44 }];
const bottomA: Curve = [C, { x: 82, y: 102 }, { x: 70, y: 121 }, { x: 57, y: 131 }];
const bottomB: Curve = [{ x: 57, y: 131 }, { x: 50, y: 137 }, { x: 43, y: 139 }, { x: 35, y: 136 }];

const clamp = (v: number) => Math.max(0, Math.min(1, v));
const ease = (v: number) => {
  v = clamp(v);
  return v * v * (3 - 2 * v);
};
const span = (t: number, a: number, b: number) => ease((t - a) / (b - a));

function bezierPoint(p0: Point, p1: Point, p2: Point, p3: Point, t: number): Point {
  const mt = 1 - t;
  return {
    x: mt * mt * mt * p0.x + 3 * mt * mt * t * p1.x + 3 * mt * t * t * p2.x + t * t * t * p3.x,
    y: mt * mt * mt * p0.y + 3 * mt * mt * t * p1.y + 3 * mt * t * t * p2.y + t * t * t * p3.y,
  };
}

function drawCircle(ctx: CanvasRenderingContext2D, p: Point, r: number = R) {
  ctx.beginPath();
  ctx.arc(p.x, p.y, r, 0, Math.PI * 2);
  ctx.fill();
}

function drawBezierSegment(ctx: CanvasRenderingContext2D, curve: Curve, progress: number, options: { lineCap?: CanvasLineCap; roundEnd?: boolean } = {}): Point {
  progress = clamp(progress);
  const [p0, p1, p2, p3] = curve;
  if (progress <= 0) return p0;

  const steps = Math.max(3, Math.ceil(56 * progress));
  ctx.save();
  ctx.lineCap = options.lineCap || "round";
  ctx.beginPath();
  ctx.moveTo(p0.x, p0.y);

  let end = p0;
  for (let i = 1; i <= steps; i++) {
    const t = (progress * i) / steps;
    end = bezierPoint(p0, p1, p2, p3, t);
    ctx.lineTo(end.x, end.y);
  }
  ctx.stroke();
  if (options.roundEnd) drawCircle(ctx, end, R);
  ctx.restore();
  return end;
}

function drawCompound(ctx: CanvasRenderingContext2D, curveA: Curve, curveB: Curve, progress: number, options: { lineCap?: CanvasLineCap; roundEnd?: boolean } = {}): Point {
  progress = clamp(progress);
  if (progress <= 0) return curveA[0];
  if (progress < 0.55) {
    return drawBezierSegment(ctx, curveA, progress / 0.55, options);
  }
  drawBezierSegment(ctx, curveA, 1, options);
  return drawBezierSegment(ctx, curveB, (progress - 0.55) / 0.45, options);
}

function render(ctx: CanvasRenderingContext2D, seconds: number) {
  ctx.clearRect(0, 0, SIZE, SIZE);
  ctx.strokeStyle = COLOR;
  ctx.fillStyle = COLOR;
  ctx.lineWidth = WIDTH;
  ctx.lineJoin = "round";

  const u = (seconds % CYCLE_SECONDS) / CYCLE_SECONDS;
  let trunk = 0;
  let branches = 0;

  if (u < 0.11) {
    // point only
  } else if (u < 0.32) {
    trunk = span(u, 0.11, 0.32);
  } else if (u < 0.5) {
    trunk = 1;
    branches = span(u, 0.32, 0.5);
  } else if (u < 0.65) {
    trunk = 1;
    branches = 1;
  } else if (u < 0.82) {
    trunk = 1;
    branches = 1 - span(u, 0.65, 0.82);
  } else if (u < 0.96) {
    trunk = 1 - span(u, 0.82, 0.96);
  }

  if (branches > 0) {
    drawCompound(ctx, topA, topB, branches, { lineCap: "butt", roundEnd: true });
    drawCompound(ctx, bottomA, bottomB, branches, { lineCap: "butt", roundEnd: true });
  }
  if (trunk > 0) {
    drawBezierSegment(ctx, leftMain, trunk, { lineCap: "round" });
    drawBezierSegment(ctx, rightMain, trunk, { lineCap: "round" });
  }
  drawCircle(ctx, C, R);
}

export function AnimatedMark({ size = 180 }: { size?: number }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d", { alpha: true });
    if (!ctx) return;

    const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

    function resize() {
      if (!canvas || !ctx) return;
      const box = canvas.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio || 1, 3);
      canvas.width = Math.round(box.width * dpr);
      canvas.height = Math.round(box.height * dpr);
      ctx.setTransform(canvas.width / SIZE, 0, 0, canvas.height / SIZE, 0, 0);
    }
    resize();
    window.addEventListener("resize", resize);

    if (reduced) {
      render(ctx, 3);
      return () => window.removeEventListener("resize", resize);
    }

    let raf = 0;
    const start = performance.now();
    function loop(now: number) {
      if (!ctx) return;
      render(ctx, (now - start) / 1000);
      raf = requestAnimationFrame(loop);
    }
    raf = requestAnimationFrame(loop);

    return () => {
      window.removeEventListener("resize", resize);
      cancelAnimationFrame(raf);
    };
  }, []);

  return <canvas ref={canvasRef} className="animated-mark" style={{ width: size, height: size, display: "block" }} aria-label="Linvesther animated logo" role="img" />;
}
