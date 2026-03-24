import { useCallback, useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { ChannelsApi, MixerApi } from "../lib/api";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { InputInfo, OutputInfo } from "../lib/types";

const COLOR_PALETTE = [
  "#6c8cff", "#f87171", "#4ade80", "#fbbf24", "#c084fc",
  "#22d3ee", "#fb923c", "#f472b6", "#a3e635", "#e2e8f0",
];

type Mode = "input" | "output";

// ---------------------------------------------------------------------------
// HSV Color Picker
// ---------------------------------------------------------------------------

function hexToHsv(hex: string): [number, number, number] {
  const r = parseInt(hex.slice(1, 3), 16) / 255;
  const g = parseInt(hex.slice(3, 5), 16) / 255;
  const b = parseInt(hex.slice(5, 7), 16) / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  const d = max - min;
  let h = 0;
  if (d !== 0) {
    if (max === r) h = 60 * (((g - b) / d) % 6);
    else if (max === g) h = 60 * ((b - r) / d + 2);
    else h = 60 * ((r - g) / d + 4);
  }
  if (h < 0) h += 360;
  const s = max === 0 ? 0 : d / max;
  return [h, s, max];
}

function GradientPicker({ color, onChange }: { color: string; onChange: (c: string) => void }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [dragging, setDragging] = useState(false);

  // Draw the full hue×lightness gradient
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const w = canvas.width;
    const h = canvas.height;
    for (let x = 0; x < w; x++) {
      for (let y = 0; y < h; y++) {
        const hue = (x / w) * 360;
        const lightness = 1 - y / h;
        ctx.fillStyle = `hsl(${hue}, 100%, ${lightness * 100}%)`;
        ctx.fillRect(x, y, 1, 1);
      }
    }
  }, []);

  const pickColor = useCallback((clientX: number, clientY: number) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = Math.max(0, Math.min(canvas.width - 1, Math.round((clientX - rect.left) / rect.width * canvas.width)));
    const y = Math.max(0, Math.min(canvas.height - 1, Math.round((clientY - rect.top) / rect.height * canvas.height)));
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const pixel = ctx.getImageData(x, y, 1, 1).data;
    const hex = `#${pixel[0].toString(16).padStart(2, "0")}${pixel[1].toString(16).padStart(2, "0")}${pixel[2].toString(16).padStart(2, "0")}`;
    onChange(hex);
  }, [onChange]);

  useEffect(() => {
    if (!dragging) return;
    const onMove = (e: MouseEvent) => pickColor(e.clientX, e.clientY);
    const onUp = () => setDragging(false);
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => { window.removeEventListener("mousemove", onMove); window.removeEventListener("mouseup", onUp); };
  }, [dragging, pickColor]);

  // Find the dot position from current color
  const [dotPos, setDotPos] = useState<{ x: number; y: number } | null>(null);
  useEffect(() => {
    try {
      if (!/^#[0-9a-fA-F]{6}$/.test(color)) return;
      const [h, , ] = hexToHsv(color);
      const r = parseInt(color.slice(1, 3), 16);
      const g = parseInt(color.slice(3, 5), 16);
      const b = parseInt(color.slice(5, 7), 16);
      const max = Math.max(r, g, b) / 255;
      const min = Math.min(r, g, b) / 255;
      const l = (max + min) / 2;
      setDotPos({ x: h / 360, y: 1 - l });
    } catch { /* ignore */ }
  }, [color]);

  return (
    <div className="relative rounded overflow-hidden cursor-crosshair" style={{ width: "100%", height: 120 }}>
      <canvas
        ref={canvasRef}
        width={300}
        height={120}
        style={{ width: "100%", height: "100%", display: "block" }}
        onMouseDown={(e) => { setDragging(true); pickColor(e.clientX, e.clientY); }}
      />
      {dotPos && (
        <div
          className="absolute w-3 h-3 rounded-full pointer-events-none"
          style={{
            left: `${dotPos.x * 100}%`,
            top: `${dotPos.y * 100}%`,
            transform: "translate(-50%, -50%)",
            border: "2px solid white",
            boxShadow: "0 0 3px rgba(0,0,0,0.6)",
          }}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main Page
// ---------------------------------------------------------------------------

export default function ChannelEditorPage() {
  const [params] = useSearchParams();
  const editId = params.get("id") ? Number(params.get("id")) : null;
  const initialMode = (params.get("mode") as Mode) ?? "input";

  const [mode, setMode] = useState<Mode>(initialMode);
  const [name, setName] = useState("");
  const [color, setColor] = useState(COLOR_PALETTE[0]);
  const [loading, setLoading] = useState(!!editId);

  useEffect(() => {
    if (!editId) return;
    (async () => {
      try {
        const [inputs, outputs] = await Promise.all([MixerApi.listInputs(), MixerApi.listOutputs()]);
        const channel: InputInfo | OutputInfo | undefined =
          mode === "input" ? inputs.find((i) => i.id === editId) : outputs.find((o) => o.id === editId);
        if (channel) { setName(channel.name); setColor(channel.color); }
      } catch { /* ignore */ }
      setLoading(false);
    })();
  }, [editId, mode]);

  useEffect(() => {
    if (editId) return;
    (async () => {
      try {
        const [inputs, outputs] = await Promise.all([MixerApi.listInputs(), MixerApi.listOutputs()]);
        if (mode === "input") {
          setName(`Input ${inputs.length + 1}`);
          setColor(COLOR_PALETTE[inputs.length % COLOR_PALETTE.length]);
        } else {
          setName(`Output ${outputs.length + 1}`);
          setColor(COLOR_PALETTE[outputs.length % COLOR_PALETTE.length]);
        }
      } catch { /* ignore */ }
    })();
  }, [editId, mode]);

  const handleSubmit = useCallback(async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    if (editId) {
      if (mode === "input") { await ChannelsApi.setInputName(editId, trimmed); await ChannelsApi.setInputColor(editId, color); }
      else { await ChannelsApi.setOutputName(editId, trimmed); await ChannelsApi.setOutputColor(editId, color); }
    } else {
      if (mode === "input") await ChannelsApi.addInput(trimmed, color);
      else await ChannelsApi.addOutput(trimmed, color, 0);
    }
    await getCurrentWindow().close();
  }, [editId, mode, name, color]);

  const handleDelete = useCallback(async () => {
    if (!editId) return;
    if (mode === "input") await ChannelsApi.removeInput(editId);
    else await ChannelsApi.removeOutput(editId);
    await getCurrentWindow().close();
  }, [editId, mode]);

  if (loading) return null;
  const isEdit = !!editId;

  return (
    <div className="flex flex-col h-screen overflow-y-auto" style={{ background: "var(--bg-base)" }}>
      <div className="flex flex-col p-5 gap-4 flex-1">
        {/* Title */}
        <span className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
          {isEdit ? "Edit Channel" : "New Channel"}
        </span>

        {/* Mode tabs */}
        {!isEdit && (
          <div className="flex gap-1">
            {(["input", "output"] as const).map((m) => (
              <button
                key={m}
                onClick={() => setMode(m)}
                className="px-3 py-1.5 text-[12px] font-medium rounded cursor-pointer"
                style={{
                  background: mode === m ? "var(--accent-primary)" : "var(--bg-surface-2)",
                  color: mode === m ? "#fff" : "var(--text-secondary)",
                  border: "none",
                }}
              >
                {m === "input" ? "Input" : "Output"}
              </button>
            ))}
          </div>
        )}

        {/* Name */}
        <div className="flex flex-col gap-1">
          <label className="text-[11px] font-medium" style={{ color: "var(--text-muted)" }}>Name</label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleSubmit(); }}
            autoFocus
            className="px-3 py-1.5 text-[13px] rounded"
            style={{ background: "var(--bg-surface-2)", border: "1px solid var(--border-default)", color: "var(--text-primary)", outline: "none" }}
          />
        </div>

        {/* Color: swatches */}
        <div className="flex flex-col gap-1.5">
          <label className="text-[11px] font-medium" style={{ color: "var(--text-muted)" }}>Quick Colors</label>
          <div className="flex flex-wrap gap-2">
            {COLOR_PALETTE.map((c) => (
              <button
                key={c}
                onClick={() => setColor(c)}
                className="w-6 h-6 rounded-full cursor-pointer"
                style={{
                  background: c,
                  border: color.toLowerCase() === c.toLowerCase() ? "2px solid var(--text-primary)" : "2px solid transparent",
                  transform: color.toLowerCase() === c.toLowerCase() ? "scale(1.15)" : "scale(1)",
                  transition: "transform 100ms",
                }}
              />
            ))}
          </div>
        </div>

        {/* Color: gradient picker */}
        <div className="flex flex-col gap-1.5">
          <label className="text-[11px] font-medium" style={{ color: "var(--text-muted)" }}>Custom Color</label>
          <GradientPicker color={color} onChange={setColor} />
          <input
            type="text"
            value={color}
            onChange={(e) => setColor(e.target.value)}
            className="px-2 py-1 text-[11px] rounded"
            style={{ background: "var(--bg-surface-2)", border: "1px solid var(--border-default)", color: "var(--text-primary)", outline: "none", width: 90, fontFamily: "var(--font-mono)" }}
            placeholder="#000000"
          />
        </div>

        {/* Preview */}
        <div className="flex items-center gap-2 px-3 py-2 rounded" style={{ background: "var(--bg-surface-1)", border: "1px solid var(--border-subtle)" }}>
          <span className="w-3 h-3 rounded-full" style={{ background: color }} />
          <span className="text-xs font-medium" style={{ color: "var(--text-primary)", borderLeft: `3px solid ${color}`, paddingLeft: 8 }}>
            {name || "Unnamed"}
          </span>
        </div>

        <div className="flex-1" />
      </div>

      {/* Buttons — sticky at bottom */}
      <div className="flex gap-2 justify-end px-5 py-3 shrink-0" style={{ borderTop: "1px solid var(--border-default)", background: "var(--bg-surface-0)" }}>
        <button
          onClick={() => getCurrentWindow().close()}
          className="px-4 py-1.5 text-[12px] font-medium rounded cursor-pointer"
          style={{ background: "transparent", border: "1px solid var(--border-default)", color: "var(--text-secondary)" }}
        >
          Cancel
        </button>
        {isEdit && (
          <button
            onClick={handleDelete}
            className="px-4 py-1.5 text-[12px] font-medium rounded cursor-pointer"
            style={{ background: "var(--accent-danger)", color: "#fff", border: "none" }}
          >
            Delete
          </button>
        )}
        <button
          onClick={handleSubmit}
          disabled={!name.trim()}
          className="px-4 py-1.5 text-[12px] font-medium rounded cursor-pointer"
          style={{
            background: name.trim() ? "var(--accent-primary)" : "var(--bg-surface-2)",
            color: name.trim() ? "#fff" : "var(--text-disabled)",
            border: "none",
          }}
        >
          {isEdit ? "Update" : "Create"}
        </button>
      </div>
    </div>
  );
}
