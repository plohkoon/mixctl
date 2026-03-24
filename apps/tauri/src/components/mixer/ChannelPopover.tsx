import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ChannelsApi } from "../../lib/api";

const COLOR_PALETTE = [
  "#6c8cff", "#f87171", "#4ade80", "#fbbf24", "#c084fc",
  "#22d3ee", "#fb923c", "#f472b6", "#a3e635", "#e2e8f0",
];

interface ChannelPopoverProps {
  id: number;
  name: string;
  color: string;
  mode: "input" | "output";
  anchorRef: React.RefObject<HTMLElement | null>;
  onClose: () => void;
}

export default function ChannelPopover({
  id,
  name,
  color,
  mode,
  anchorRef,
  onClose,
}: ChannelPopoverProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [localName, setLocalName] = useState(name);
  const [localColor, setLocalColor] = useState(color);
  const [pos, setPos] = useState<{ top: number; left: number }>({ top: 0, left: 0 });

  // Calculate position from anchor element
  useEffect(() => {
    if (anchorRef.current) {
      const rect = anchorRef.current.getBoundingClientRect();
      // Position below for inputs (row labels), to the right for outputs (column headers)
      if (mode === "input") {
        setPos({
          top: rect.bottom + 4,
          left: Math.max(4, rect.left),
        });
      } else {
        setPos({
          top: rect.bottom + 4,
          left: Math.max(4, rect.left),
        });
      }
    }
  }, [anchorRef, mode]);

  // Close on click outside
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onClose();
      }
    };
    const timer = setTimeout(() => document.addEventListener("mousedown", handler), 0);
    return () => { clearTimeout(timer); document.removeEventListener("mousedown", handler); };
  }, [onClose]);

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onClose]);

  const commitName = useCallback(() => {
    const trimmed = localName.trim();
    if (trimmed && trimmed !== name) {
      if (mode === "input") ChannelsApi.setInputName(id, trimmed);
      else ChannelsApi.setOutputName(id, trimmed);
    }
  }, [id, localName, name, mode]);

  const handleColorSelect = useCallback((c: string) => {
    setLocalColor(c);
    if (mode === "input") ChannelsApi.setInputColor(id, c);
    else ChannelsApi.setOutputColor(id, c);
  }, [id, mode]);

  const handleHexChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    setLocalColor(val);
    if (/^#[0-9a-fA-F]{6}$/.test(val)) {
      if (mode === "input") ChannelsApi.setInputColor(id, val);
      else ChannelsApi.setOutputColor(id, val);
    }
  }, [id, mode]);

  const handleDelete = useCallback(() => {
    if (mode === "input") ChannelsApi.removeInput(id);
    else ChannelsApi.removeOutput(id);
    onClose();
  }, [id, mode, onClose]);

  return createPortal(
    <div
      ref={ref}
      className="fixed z-[9999] flex flex-col gap-3 p-3"
      style={{
        background: "var(--bg-surface-3)",
        border: "1px solid var(--border-strong)",
        borderRadius: 8,
        top: pos.top,
        left: pos.left,
        minWidth: 220,
        boxShadow: "0 4px 16px rgba(0,0,0,0.5)",
      }}
    >
      {/* Name input */}
      <input
        type="text"
        value={localName}
        onChange={(e) => setLocalName(e.target.value)}
        onBlur={commitName}
        onKeyDown={(e) => { if (e.key === "Enter") { commitName(); (e.target as HTMLInputElement).blur(); } }}
        autoFocus
        className="px-2 py-1 text-xs rounded"
        style={{ background: "var(--bg-surface-1)", border: "1px solid var(--border-default)", color: "var(--text-primary)", outline: "none" }}
      />

      {/* Color swatches */}
      <div className="flex flex-wrap gap-1.5">
        {COLOR_PALETTE.map((c) => (
          <button
            key={c}
            onClick={() => handleColorSelect(c)}
            className="w-5 h-5 rounded-full cursor-pointer"
            style={{
              background: c,
              border: localColor === c ? "2px solid var(--text-primary)" : "2px solid transparent",
              transform: localColor === c ? "scale(1.2)" : "scale(1)",
              transition: "transform 100ms",
            }}
            title={c}
          />
        ))}
      </div>

      {/* Hex input */}
      <input
        type="text"
        value={localColor}
        onChange={handleHexChange}
        className="px-2 py-1 text-xs rounded"
        style={{ background: "var(--bg-surface-1)", border: "1px solid var(--border-default)", color: "var(--text-primary)", outline: "none", width: 90, fontFamily: "var(--font-mono)" }}
        placeholder="#000000"
      />

      {/* Delete */}
      <button
        onClick={handleDelete}
        className="px-3 py-1.5 text-[12px] font-medium rounded cursor-pointer"
        style={{ background: "var(--accent-danger)", color: "#fff", border: "none" }}
      >
        Delete {mode === "input" ? "Input" : "Output"}
      </button>
    </div>,
    document.body
  );
}
