import { useCallback, useState } from "react";
import { ChannelsApi, MixerApi, SystemApi } from "../../lib/api";
import { useMixerStore } from "../../lib/stores/mixer-store";

interface InputLabelProps {
  inputId: number;
  name: string;
  color: string;
  index: number;
}

export default function InputLabel({ inputId, name, color, index }: InputLabelProps) {
  const picking = useMixerStore((s) => s.picking);
  const setPicking = useMixerStore((s) => s.setPicking);
  const [isDragOver, setIsDragOver] = useState(false);
  const [isHovered, setIsHovered] = useState(false);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    const types = e.dataTransfer.types;
    if (
      types.includes("application/x-mixctl-stream") ||
      types.includes("application/x-mixctl-input") ||
      types.includes("application/x-mixctl-capture") ||
      types.includes("application/x-mixctl-rule")
    ) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      setIsDragOver(true);
    }
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragOver(false);

      if (e.dataTransfer.types.includes("application/x-mixctl-stream")) {
        try {
          const data = JSON.parse(e.dataTransfer.getData("application/x-mixctl-stream"));
          if (data.pwNodeId) MixerApi.assignStream(data.pwNodeId, inputId, true);
        } catch { /* ignore */ }
        return;
      }

      if (e.dataTransfer.types.includes("application/x-mixctl-capture")) {
        try {
          const data = JSON.parse(e.dataTransfer.getData("application/x-mixctl-capture"));
          if (data.deviceName) SystemApi.bindCaptureToInput(inputId, data.deviceName);
        } catch { /* ignore */ }
        return;
      }

      if (e.dataTransfer.types.includes("application/x-mixctl-rule")) {
        try {
          const data = JSON.parse(e.dataTransfer.getData("application/x-mixctl-rule"));
          if (data.appName) SystemApi.setAppRule(data.appName, inputId);
        } catch { /* ignore */ }
        return;
      }

      if (e.dataTransfer.types.includes("application/x-mixctl-input")) {
        try {
          const data = JSON.parse(e.dataTransfer.getData("application/x-mixctl-input"));
          if (data.id !== undefined && data.id !== inputId) ChannelsApi.moveInput(data.id, index);
        } catch { /* ignore */ }
      }
    },
    [inputId, index]
  );

  const handleClick = useCallback(async () => {
    if (!picking) return;
    if (picking.type === "stream" && picking.data.pwNodeId) {
      MixerApi.assignStream(picking.data.pwNodeId, inputId, true);
    } else if (picking.type === "capture" && picking.data.deviceName) {
      SystemApi.bindCaptureToInput(inputId, picking.data.deviceName);
    } else if (picking.type === "rule" && picking.data.appName) {
      SystemApi.setAppRule(picking.data.appName, inputId);
    }
    setPicking(null);
  }, [picking, inputId, setPicking]);

  const handleDragStart = useCallback(
    (e: React.DragEvent) => {
      e.dataTransfer.setData("application/x-mixctl-input", JSON.stringify({ id: inputId, index }));
      e.dataTransfer.effectAllowed = "move";
    },
    [inputId, index]
  );

  const isPickingTarget = picking !== null && picking.type !== "playback";
  const showControls = isHovered;

  return (
    <div
      className="relative flex items-center gap-1 px-2 h-full transition-colors duration-[120ms]"
      style={{
        borderLeft: `3px solid ${color}`,
        background: isDragOver ? "var(--accent-warning-muted)" : "transparent",
        outline: isPickingTarget ? "2px solid var(--accent-warning)" : "none",
        outlineOffset: "-2px",
        cursor: isPickingTarget ? "pointer" : undefined,
      }}
      onDragOver={handleDragOver}
      onDragLeave={() => setIsDragOver(false)}
      onDrop={handleDrop}
      onClick={handleClick}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      draggable
      onDragStart={handleDragStart}
    >
      {/* Drag handle — visible on hover */}
      <span
        className="text-[10px] shrink-0 select-none"
        style={{ cursor: "grab", color: showControls ? "var(--text-muted)" : "transparent", transition: "color 120ms", lineHeight: 1 }}
      >
        {"\u2807"}
      </span>

      {/* Color dot */}
      <span className="w-2 h-2 rounded-full shrink-0" style={{ background: color }} />

      {/* Name — full width normally, truncated when controls visible */}
      <span
        className="text-xs font-medium truncate"
        style={{ flex: 1, minWidth: 0 }}
      >
        {name}
      </span>

      {/* Action buttons — only visible on hover */}
      {showControls && (
        <div className="flex items-center gap-0.5 shrink-0">
          <button
            onClick={(e) => { e.stopPropagation(); SystemApi.openChannelEditor("input", inputId); }}
            className="w-4 h-4 flex items-center justify-center rounded cursor-pointer opacity-50 hover:opacity-100"
            style={{ background: "transparent", border: "none", color: "var(--text-secondary)" }}
            title="Edit channel"
          >
            <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
              <path d="M6.5 1a.5.5 0 0 0-.5.5v1.02a4.98 4.98 0 0 0-1.79.74L3.15 2.2a.5.5 0 0 0-.7.7l1.06 1.06A4.98 4.98 0 0 0 2.77 5.75H1.5a.5.5 0 0 0 0 1h1.27a4.98 4.98 0 0 0 .74 1.79L2.45 9.6a.5.5 0 0 0 .7.7l1.06-1.06c.52.4 1.12.68 1.79.84v1.42a.5.5 0 0 0 1 0v-1.42a4.98 4.98 0 0 0 1.79-.84l1.06 1.06a.5.5 0 0 0 .7-.7L9.49 8.54c.4-.52.68-1.12.84-1.79H11.5a.5.5 0 0 0 0-1h-1.17a4.98 4.98 0 0 0-.74-1.79L10.65 2.9a.5.5 0 0 0-.7-.7L8.89 3.26A4.98 4.98 0 0 0 7.1 2.52V1.5a.5.5 0 0 0-.5-.5zM7 5a2 2 0 1 1 0 4 2 2 0 0 1 0-4z" />
            </svg>
          </button>
          <button
            onClick={(e) => { e.stopPropagation(); SystemApi.openDialog("dsp"); }}
            className="w-4 h-4 flex items-center justify-center rounded cursor-pointer opacity-50 hover:opacity-100"
            style={{ background: "transparent", border: "none", color: "var(--text-secondary)" }}
            title="DSP"
          >
            <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M1 8h2l1.5-4L7 12l2-8 1.5 4H13" />
            </svg>
          </button>
        </div>
      )}

    </div>
  );
}
