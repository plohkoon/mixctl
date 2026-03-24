import { useCallback, useEffect, useRef, useState } from "react";
import { ChannelsApi, MixerApi, SystemApi } from "../../lib/api";
import { useMixerStore } from "../../lib/stores/mixer-store";
import VolumeFader from "./VolumeFader";
import MuteButton from "./MuteButton";
import type { OutputInfo } from "../../lib/types";

interface OutputHeaderProps {
  output: OutputInfo;
  index: number;
}

export default function OutputHeader({ output, index }: OutputHeaderProps) {
  const [localVolume, setLocalVolume] = useState(output.volume);
  const [deviceDropdownOpen, setDeviceDropdownOpen] = useState(false);
  const [isHovered, setIsHovered] = useState(false);
  const [isDragOver, setIsDragOver] = useState(false);
  const headerRef = useRef<HTMLDivElement>(null);
  const volumeDragging = useRef(false);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const playbackDevices = useMixerStore((s) => s.playbackDevices);
  const picking = useMixerStore((s) => s.picking);
  const setPicking = useMixerStore((s) => s.setPicking);

  useEffect(() => {
    if (!volumeDragging.current) setLocalVolume(output.volume);
  }, [output.volume]);

  // Close dropdown on click outside or Escape
  useEffect(() => {
    if (!deviceDropdownOpen) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (
        dropdownRef.current && !dropdownRef.current.contains(e.target as Node) &&
        buttonRef.current && !buttonRef.current.contains(e.target as Node)
      ) {
        setDeviceDropdownOpen(false);
      }
    };
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") setDeviceDropdownOpen(false);
    };
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleEscape);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [deviceDropdownOpen]);

  const handleChange = useCallback((vol: number) => {
    volumeDragging.current = true;
    setLocalVolume(vol);
    MixerApi.setOutputVolume(output.id, vol);
  }, [output.id]);

  const handleChangeEnd = useCallback((vol: number) => {
    volumeDragging.current = false;
    setLocalVolume(vol);
    MixerApi.setOutputVolume(output.id, vol);
  }, [output.id]);

  const handleMute = useCallback(() => {
    MixerApi.setOutputMute(output.id, !output.muted);
  }, [output.id, output.muted]);

  const handleSelectDevice = useCallback((deviceName: string) => {
    MixerApi.setOutputTarget(output.id, deviceName);
    setDeviceDropdownOpen(false);
  }, [output.id]);

  // Drag-and-drop for reordering
  const handleDragStart = useCallback((e: React.DragEvent) => {
    e.dataTransfer.setData("application/x-mixctl-output", JSON.stringify({ id: output.id, index }));
    e.dataTransfer.effectAllowed = "move";
  }, [output.id, index]);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    if (
      e.dataTransfer.types.includes("application/x-mixctl-output") ||
      e.dataTransfer.types.includes("application/x-mixctl-playback")
    ) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      setIsDragOver(true);
    }
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);

    // Handle playback device assignment
    if (e.dataTransfer.types.includes("application/x-mixctl-playback")) {
      try {
        const data = JSON.parse(e.dataTransfer.getData("application/x-mixctl-playback"));
        if (data.deviceName) {
          MixerApi.setOutputTarget(output.id, data.deviceName);
        }
      } catch { /* ignore */ }
      return;
    }

    // Handle output reorder
    if (e.dataTransfer.types.includes("application/x-mixctl-output")) {
      try {
        const data = JSON.parse(e.dataTransfer.getData("application/x-mixctl-output"));
        if (data.id !== undefined && data.id !== output.id) {
          ChannelsApi.moveOutput(data.id, index);
        }
      } catch { /* ignore */ }
    }
  }, [output.id, index]);

  const handlePickingClick = useCallback(() => {
    if (picking?.type === "playback" && picking.data.deviceName) {
      MixerApi.setOutputTarget(output.id, picking.data.deviceName);
      setPicking(null);
    }
  }, [picking, output.id, setPicking]);

  const isPlaybackPicking = picking?.type === "playback";

  const deviceDisplayName = output.target_device
    ? playbackDevices.find((d) => d.device_name === output.target_device)?.name ?? "Unknown"
    : "None";

  const showActions = isHovered || deviceDropdownOpen;

  return (
    <div
      ref={headerRef}
      className="flex flex-col gap-1 px-3 py-1.5 rounded-t-lg overflow-hidden relative"
      style={{
        background: `${output.color}0a`,
        borderBottom: `2px solid var(--border-strong)`,
        borderLeft: isDragOver ? "2px solid var(--accent-warning)" : "none",
        outline: isPlaybackPicking ? "2px solid var(--accent-warning)" : "none",
        outlineOffset: "-2px",
        minWidth: 0,
        cursor: isPlaybackPicking ? "pointer" : "grab",
      }}
      draggable
      onDragStart={handleDragStart}
      onDragOver={handleDragOver}
      onDragLeave={() => setIsDragOver(false)}
      onDrop={handleDrop}
      onClick={handlePickingClick}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      {/* Name row: name + device badge + action buttons */}
      <div className="flex items-center gap-1 min-w-0">
        <div className="flex flex-col min-w-0 flex-1">
          <span className="text-[13px] font-semibold truncate" style={{ color: output.color }}>
            {output.name}
          </span>
          <span className="text-[9px] truncate" style={{ color: "var(--text-muted)" }}>
            {"\u2192"} {deviceDisplayName}
          </span>
        </div>

        {/* Action buttons — visible on hover */}
        <div className="flex items-center gap-0.5 shrink-0 relative" style={{ opacity: showActions ? 1 : 0, transition: "opacity 120ms", pointerEvents: showActions ? "auto" : "none" }}>
          {/* Device selector */}
          <button
            ref={buttonRef}
            onClick={(e) => { e.stopPropagation(); setDeviceDropdownOpen((v) => !v); }}
            className="w-5 h-5 flex items-center justify-center rounded opacity-60 hover:opacity-100 transition-opacity cursor-pointer"
            title="Target device"
          >
            <svg width="11" height="11" viewBox="0 0 16 16" fill="var(--text-secondary)">
              <path d="M14 7h-1.5a.5.5 0 0 0-.5.5v1a.5.5 0 0 0 .5.5H14a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1zM2 7H.5a.5.5 0 0 0-.5.5v1a.5.5 0 0 0 .5.5H2V7zm9-2H5a2 2 0 0 0-2 2v2a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2zM6 10a1.5 1.5 0 1 1 0-3 1.5 1.5 0 0 1 0 3zm4 0a1.5 1.5 0 1 1 0-3 1.5 1.5 0 0 1 0 3z" />
            </svg>
          </button>
          {/* Wrench — settings popover */}
          <button
            onClick={(e) => { e.stopPropagation(); SystemApi.openChannelEditor("output", output.id); }}
            className="w-5 h-5 flex items-center justify-center rounded opacity-60 hover:opacity-100 transition-opacity cursor-pointer"
            title="Edit channel"
          >
            <svg width="11" height="11" viewBox="0 0 16 16" fill="var(--text-secondary)">
              <path d="M6.5 1a.5.5 0 0 0-.5.5v1.02a4.98 4.98 0 0 0-1.79.74L3.15 2.2a.5.5 0 0 0-.7.7l1.06 1.06A4.98 4.98 0 0 0 2.77 5.75H1.5a.5.5 0 0 0 0 1h1.27a4.98 4.98 0 0 0 .74 1.79L2.45 9.6a.5.5 0 0 0 .7.7l1.06-1.06c.52.4 1.12.68 1.79.84v1.42a.5.5 0 0 0 1 0v-1.42a4.98 4.98 0 0 0 1.79-.84l1.06 1.06a.5.5 0 0 0 .7-.7L9.49 8.54c.4-.52.68-1.12.84-1.79H11.5a.5.5 0 0 0 0-1h-1.17a4.98 4.98 0 0 0-.74-1.79L10.65 2.9a.5.5 0 0 0-.7-.7L8.89 3.26A4.98 4.98 0 0 0 7.1 2.52V1.5a.5.5 0 0 0-.5-.5zM7 5a2 2 0 1 1 0 4 2 2 0 0 1 0-4z" />
            </svg>
          </button>
          {/* DSP waveform */}
          <button
            onClick={(e) => { e.stopPropagation(); SystemApi.openDialog("dsp"); }}
            className="w-5 h-5 flex items-center justify-center rounded opacity-60 hover:opacity-100 transition-opacity cursor-pointer"
            title="Output DSP"
          >
            <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="var(--text-secondary)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M1 8h2l1.5-4L7 12l2-8 1.5 4H13" />
            </svg>
          </button>
        </div>

        {/* Device dropdown */}
        {deviceDropdownOpen && (
          <div
            ref={dropdownRef}
            className="absolute top-full right-2 mt-1 z-50 min-w-[160px] max-h-[200px] overflow-y-auto rounded-md py-1"
            style={{ background: "var(--bg-surface-3)", border: "1px solid var(--border-strong)", boxShadow: "0 4px 12px rgba(0,0,0,0.3)" }}
          >
            <button
              onClick={() => handleSelectDevice("")}
              className="w-full text-left px-3 py-1.5 text-[11px] cursor-pointer"
              style={{ color: !output.target_device ? "var(--text-primary)" : "var(--text-secondary)", background: !output.target_device ? "var(--bg-surface-2)" : "transparent" }}
              onMouseEnter={(e) => { if (output.target_device) e.currentTarget.style.background = "var(--bg-surface-2)"; }}
              onMouseLeave={(e) => { if (output.target_device) e.currentTarget.style.background = "transparent"; }}
            >
              None
            </button>
            {playbackDevices.map((device) => {
              const isSelected = output.target_device === device.device_name;
              return (
                <button
                  key={device.pw_node_id}
                  onClick={() => handleSelectDevice(device.device_name)}
                  className="w-full text-left px-3 py-1.5 text-[11px] cursor-pointer truncate"
                  style={{ color: isSelected ? "var(--text-primary)" : "var(--text-secondary)", background: isSelected ? "var(--bg-surface-2)" : "transparent" }}
                  onMouseEnter={(e) => { if (!isSelected) e.currentTarget.style.background = "var(--bg-surface-2)"; }}
                  onMouseLeave={(e) => { if (!isSelected) e.currentTarget.style.background = "transparent"; }}
                >
                  {device.name}
                </button>
              );
            })}
          </div>
        )}
      </div>

      {/* Master volume */}
      <div className="flex items-center gap-1.5">
        <VolumeFader value={localVolume} color={output.color} onChange={handleChange} onChangeEnd={handleChangeEnd} />
        <span className="w-8 text-right shrink-0 tabular-nums" style={{ fontFamily: "var(--font-mono)", fontSize: 11, color: "var(--text-secondary)" }}>
          {localVolume}
        </span>
        <MuteButton muted={output.muted} size={24} onClick={handleMute} />
      </div>

    </div>
  );
}
