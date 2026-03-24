import { useCallback, useEffect, useRef, useState } from "react";
import type { EqBandInfo } from "../../lib/types";

// ---------------------------------------------------------------------------
// Biquad math (ported from crates/core/src/lib.rs)
// ---------------------------------------------------------------------------

const SAMPLE_RATE = 48000;
const CURVE_POINTS = 150;
const LOG_MIN = Math.log10(20);
const LOG_MAX = Math.log10(20000);
const LOG_RANGE = LOG_MAX - LOG_MIN;
const DB_RANGE = 24;

const BAND_COLORS = [
  "#e74c3c", "#e67e22", "#f1c40f", "#2ecc71",
  "#1abc9c", "#3498db", "#9b59b6", "#e91e63",
];

function eqBandCoeffs(
  bandType: string,
  freq: number,
  gainDb: number,
  q: number,
  sr: number
): [number, number, number, number, number] {
  const a = Math.pow(10, gainDb / 40);
  const w0 = (2 * Math.PI * freq) / sr;
  const sinW0 = Math.sin(w0);
  const cosW0 = Math.cos(w0);
  const alpha = sinW0 / (2 * q);

  if (bandType === "low_shelf") {
    const twoSqrtAAlpha = 2 * Math.sqrt(a) * alpha;
    const a0 = a + 1 + (a - 1) * cosW0 + twoSqrtAAlpha;
    return [
      (a * (a + 1 - (a - 1) * cosW0 + twoSqrtAAlpha)) / a0,
      (2 * a * (a - 1 - (a + 1) * cosW0)) / a0,
      (a * (a + 1 - (a - 1) * cosW0 - twoSqrtAAlpha)) / a0,
      (-2 * (a - 1 + (a + 1) * cosW0)) / a0,
      (a + 1 + (a - 1) * cosW0 - twoSqrtAAlpha) / a0,
    ];
  }

  if (bandType === "high_shelf") {
    const twoSqrtAAlpha = 2 * Math.sqrt(a) * alpha;
    const a0 = a + 1 - (a - 1) * cosW0 + twoSqrtAAlpha;
    return [
      (a * (a + 1 + (a - 1) * cosW0 + twoSqrtAAlpha)) / a0,
      (-2 * a * (a - 1 + (a + 1) * cosW0)) / a0,
      (a * (a + 1 + (a - 1) * cosW0 - twoSqrtAAlpha)) / a0,
      (2 * (a - 1 - (a + 1) * cosW0)) / a0,
      (a + 1 - (a - 1) * cosW0 - twoSqrtAAlpha) / a0,
    ];
  }

  // Peaking (default)
  const a0 = 1 + alpha / a;
  return [
    (1 + alpha * a) / a0,
    (-2 * cosW0) / a0,
    (1 - alpha * a) / a0,
    (-2 * cosW0) / a0,
    (1 - alpha / a) / a0,
  ];
}

function computeSingleBandCurve(band: EqBandInfo): number[] {
  const result: number[] = [];
  for (let i = 0; i < CURVE_POINTS; i++) {
    const t = i / (CURVE_POINTS - 1);
    const freq = Math.pow(10, LOG_MIN + t * LOG_RANGE);
    const w = (2 * Math.PI * freq) / SAMPLE_RATE;
    const sinW = Math.sin(w);
    const cosW = Math.cos(w);
    const sin2W = Math.sin(2 * w);
    const cos2W = Math.cos(2 * w);

    const [b0, b1, b2, a1, a2] = eqBandCoeffs(
      band.band_type, band.frequency, band.gain_db, band.q, SAMPLE_RATE
    );
    const numRe = b0 + b1 * cosW + b2 * cos2W;
    const numIm = -(b1 * sinW + b2 * sin2W);
    const denRe = 1 + a1 * cosW + a2 * cos2W;
    const denIm = -(a1 * sinW + a2 * sin2W);
    const numSq = numRe * numRe + numIm * numIm;
    const denSq = denRe * denRe + denIm * denIm;
    const db = denSq > 1e-20 ? 10 * Math.log10(numSq / denSq) : 0;
    result.push(Math.max(-30, Math.min(30, db)));
  }
  return result;
}

function computeCombinedCurve(bands: EqBandInfo[]): number[] {
  const result = new Array(CURVE_POINTS).fill(0);
  for (const band of bands) {
    if (band.band_type === "bypass") continue;
    const single = computeSingleBandCurve(band);
    for (let i = 0; i < CURVE_POINTS; i++) {
      result[i] += single[i];
    }
  }
  return result.map((v) => Math.max(-30, Math.min(30, v)));
}

function freqToX(freq: number, width: number): number {
  return ((Math.log10(freq) - LOG_MIN) / LOG_RANGE) * width;
}

function dbToY(db: number, height: number): number {
  return height / 2 - (db / DB_RANGE) * (height / 2);
}

function xToFreq(x: number, width: number): number {
  return Math.pow(10, (x / width) * LOG_RANGE + LOG_MIN);
}

function yToDb(y: number, height: number): number {
  return -((y - height / 2) / (height / 2)) * DB_RANGE;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

interface EqCurveProps {
  bands: EqBandInfo[];
  selectedBand: number | null;
  onBandDrag: (bandIdx: number, freq: number, gainDb: number) => void;
  onBandDragEnd: (bandIdx: number, freq: number, gainDb: number) => void;
  onBandSelect: (bandIdx: number) => void;
  onBandQScroll: (bandIdx: number, delta: number) => void;
}

export default function EqCurve({
  bands,
  selectedBand,
  onBandDrag,
  onBandDragEnd,
  onBandSelect,
  onBandQScroll,
}: EqCurveProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 600, height: 200 });
  const [hoverPos, setHoverPos] = useState<{ x: number; y: number } | null>(null);
  const [hoverBand, setHoverBand] = useState<number | null>(null);
  const draggingBand = useRef<number | null>(null);

  // Resize observer
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width } = entry.contentRect;
        setSize({ width, height: Math.round(width / 3) });
      }
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  // Draw
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const { width, height } = size;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    ctx.scale(dpr, dpr);

    // Background
    ctx.fillStyle = "#141419";
    ctx.fillRect(0, 0, width, height);

    // Grid lines — dB
    const dbLines = [-18, -12, -6, 0, 6, 12, 18];
    for (const db of dbLines) {
      const y = dbToY(db, height);
      ctx.strokeStyle = db === 0 ? "#46464b" : "#23232a";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(width, y);
      ctx.stroke();
    }

    // Grid lines — frequency
    const freqLines = [50, 100, 200, 500, 1000, 2000, 5000, 10000];
    for (const freq of freqLines) {
      const x = freqToX(freq, width);
      ctx.strokeStyle = "#23232a";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, height);
      ctx.stroke();
    }

    // Per-band individual curves (subtle)
    for (let b = 0; b < bands.length; b++) {
      if (bands[b].band_type === "bypass") continue;
      const single = computeSingleBandCurve(bands[b]);
      ctx.strokeStyle = BAND_COLORS[b % BAND_COLORS.length] + "60";
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let i = 0; i < CURVE_POINTS; i++) {
        const t = i / (CURVE_POINTS - 1);
        const x = t * width;
        const y = dbToY(single[i], height);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
    }

    // Combined curve
    const combined = computeCombinedCurve(bands);

    // Filled region
    ctx.fillStyle = "rgba(0, 100, 140, 0.3)";
    ctx.beginPath();
    const zeroY = dbToY(0, height);
    ctx.moveTo(0, zeroY);
    for (let i = 0; i < CURVE_POINTS; i++) {
      const t = i / (CURVE_POINTS - 1);
      const x = t * width;
      const y = dbToY(combined[i], height);
      ctx.lineTo(x, y);
    }
    ctx.lineTo(width, zeroY);
    ctx.closePath();
    ctx.fill();

    // Curve line
    ctx.strokeStyle = "#00c8ff";
    ctx.lineWidth = 3;
    ctx.beginPath();
    for (let i = 0; i < CURVE_POINTS; i++) {
      const t = i / (CURVE_POINTS - 1);
      const x = t * width;
      const y = dbToY(combined[i], height);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    // Band markers
    for (let b = 0; b < bands.length; b++) {
      if (bands[b].band_type === "bypass") continue;
      const bx = freqToX(bands[b].frequency, width);
      const by = dbToY(bands[b].gain_db, height);
      const isSelected = selectedBand === b;
      const isHovered = hoverBand === b;

      ctx.fillStyle = BAND_COLORS[b % BAND_COLORS.length];
      ctx.beginPath();
      ctx.arc(bx, by, isSelected || isHovered ? 6 : 4, 0, Math.PI * 2);
      ctx.fill();

      if (isSelected) {
        ctx.strokeStyle = "#fff";
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(bx, by, 8, 0, Math.PI * 2);
        ctx.stroke();
      }
    }

    // Hover crosshair
    if (hoverPos && draggingBand.current === null && hoverBand === null) {
      const freq = xToFreq(hoverPos.x, width);
      const db = yToDb(hoverPos.y, height);

      ctx.strokeStyle = "#ffffff30";
      ctx.lineWidth = 1;
      ctx.setLineDash([4, 4]);
      ctx.beginPath();
      ctx.moveTo(hoverPos.x, 0);
      ctx.lineTo(hoverPos.x, height);
      ctx.moveTo(0, hoverPos.y);
      ctx.lineTo(width, hoverPos.y);
      ctx.stroke();
      ctx.setLineDash([]);

      // Readout
      const freqStr = freq >= 1000 ? `${(freq / 1000).toFixed(1)}kHz` : `${Math.round(freq)}Hz`;
      const dbStr = `${db >= 0 ? "+" : ""}${db.toFixed(1)}dB`;
      ctx.fillStyle = "#ffffffcc";
      ctx.font = "11px system-ui";
      ctx.fillText(`${freqStr} / ${dbStr}`, hoverPos.x + 8, hoverPos.y - 8);
    }

    // Hover band tooltip
    if (hoverBand !== null && hoverBand < bands.length) {
      const band = bands[hoverBand];
      const bx = freqToX(band.frequency, width);
      const by = dbToY(band.gain_db, height);
      const freqStr =
        band.frequency >= 1000
          ? `${(band.frequency / 1000).toFixed(1)}kHz`
          : `${Math.round(band.frequency)}Hz`;
      const text = `Band ${hoverBand + 1}: ${freqStr}, ${band.gain_db >= 0 ? "+" : ""}${band.gain_db.toFixed(1)}dB, Q ${band.q.toFixed(1)}`;

      ctx.fillStyle = "#000000cc";
      const metrics = ctx.measureText(text);
      const tx = Math.min(bx + 12, width - metrics.width - 8);
      const ty = Math.max(by - 12, 16);
      ctx.fillRect(tx - 4, ty - 12, metrics.width + 8, 16);
      ctx.fillStyle = "#ffffffdd";
      ctx.font = "11px system-ui";
      ctx.fillText(text, tx, ty);
    }

    // Axis labels
    ctx.fillStyle = "#666";
    ctx.font = "10px system-ui";
    // dB labels
    for (const db of [-18, -12, -6, 0, 6, 12, 18]) {
      const y = dbToY(db, height);
      const label = db === 0 ? "0dB" : `${db > 0 ? "+" : ""}${db}`;
      ctx.fillText(label, 2, y - 2);
    }
    // Freq labels
    const freqLabels = [
      [20, "20"], [100, "100"], [1000, "1k"], [10000, "10k"], [20000, "20k"],
    ] as const;
    for (const [freq, label] of freqLabels) {
      const x = freqToX(freq, width);
      ctx.fillText(label, x - 8, height - 2);
    }
  }, [bands, selectedBand, hoverPos, hoverBand, size]);

  // Mouse interaction
  const findBandAtPos = useCallback(
    (x: number, y: number): number | null => {
      for (let b = 0; b < bands.length; b++) {
        if (bands[b].band_type === "bypass") continue;
        const bx = freqToX(bands[b].frequency, size.width);
        const by = dbToY(bands[b].gain_db, size.height);
        const dist = Math.sqrt((x - bx) ** 2 + (y - by) ** 2);
        if (dist < 10) return b;
      }
      return null;
    },
    [bands, size]
  );

  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      const rect = canvasRef.current?.getBoundingClientRect();
      if (!rect) return;
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;

      if (draggingBand.current !== null) {
        const freq = Math.max(20, Math.min(20000, xToFreq(x, size.width)));
        const db = Math.max(-DB_RANGE, Math.min(DB_RANGE, yToDb(y, size.height)));
        onBandDrag(draggingBand.current, freq, db);
      } else {
        setHoverPos({ x, y });
        setHoverBand(findBandAtPos(x, y));
      }
    },
    [size, findBandAtPos, onBandDrag]
  );

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      const rect = canvasRef.current?.getBoundingClientRect();
      if (!rect) return;
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      const band = findBandAtPos(x, y);
      if (band !== null) {
        draggingBand.current = band;
        onBandSelect(band);
      }
    },
    [findBandAtPos, onBandSelect]
  );

  const handleMouseUp = useCallback(() => {
    if (draggingBand.current !== null) {
      const b = draggingBand.current;
      const band = bands[b];
      if (band) {
        onBandDragEnd(b, band.frequency, band.gain_db);
      }
      draggingBand.current = null;
    }
  }, [bands, onBandDragEnd]);

  const handleMouseLeave = useCallback(() => {
    setHoverPos(null);
    setHoverBand(null);
  }, []);

  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      e.preventDefault();
      if (selectedBand !== null) {
        const delta = e.deltaY < 0 ? 0.1 : -0.1;
        onBandQScroll(selectedBand, delta);
      }
    },
    [selectedBand, onBandQScroll]
  );

  return (
    <div ref={containerRef} className="w-full">
      <canvas
        ref={canvasRef}
        style={{ width: size.width, height: size.height, cursor: hoverBand !== null ? "grab" : "crosshair" }}
        onMouseMove={handleMouseMove}
        onMouseDown={handleMouseDown}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseLeave}
        onWheel={handleWheel}
      />
    </div>
  );
}
