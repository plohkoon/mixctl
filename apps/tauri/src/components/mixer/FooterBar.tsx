import { useCallback, useEffect, useRef, useState } from "react";
import { useMixerStore } from "../../lib/stores/mixer-store";
import { MixerApi, SystemApi } from "../../lib/api";
import type {
  CaptureDeviceInfo,
  PlaybackDeviceInfo,
} from "../../lib/types";

// ---------------------------------------------------------------------------
// FooterBar
// ---------------------------------------------------------------------------

export default function FooterBar({
  onToggleExpand,
}: {
  onToggleExpand: () => void;
}) {
  const streams = useMixerStore((s) => s.streams);
  const inputs = useMixerStore((s) => s.inputs);
  const outputs = useMixerStore((s) => s.outputs);
  const captureDevices = useMixerStore((s) => s.captureDevices);
  const playbackDevices = useMixerStore((s) => s.playbackDevices);
  const rules = useMixerStore((s) => s.rules);
  const picking = useMixerStore((s) => s.picking);
  const setPicking = useMixerStore((s) => s.setPicking);
  const refreshRules = useMixerStore((s) => s.refreshRules);
  const refreshCaptureDevices = useMixerStore((s) => s.refreshCaptureDevices);

  const [draggingType, setDraggingType] = useState<string | null>(null);

  // Hidden devices — persisted in localStorage
  const [hiddenDevices, setHiddenDevices] = useState<Set<string>>(() => {
    try {
      const stored = localStorage.getItem("mixctl-hidden-devices");
      return stored ? new Set(JSON.parse(stored)) : new Set();
    } catch { return new Set(); }
  });
  const hideDevice = useCallback((deviceName: string) => {
    setHiddenDevices((prev) => {
      const next = new Set(prev);
      next.add(deviceName);
      localStorage.setItem("mixctl-hidden-devices", JSON.stringify([...next]));
      return next;
    });
  }, []);

  const visibleStreams = streams.filter(
    (s) =>
      !s.app_name.includes("mixctl.") && !s.app_name.startsWith("output."),
  );

  const showUnbindZone =
    picking?.type === "capture" ||
    picking?.type === "playback" ||
    draggingType === "capture" ||
    draggingType === "playback";

  // --- Toggle picking helpers ------------------------------------------------

  const toggleStreamPicking = useCallback(
    (pwNodeId: number, appName: string) => {
      const id = `stream-${pwNodeId}`;
      if (picking?.id === id) {
        setPicking(null);
      } else {
        setPicking({
          type: "stream",
          id,
          data: { pwNodeId, appName },
        });
      }
    },
    [picking, setPicking],
  );

  const toggleCapturePicking = useCallback(
    (device: CaptureDeviceInfo) => {
      const id = `capture-${device.pw_node_id}`;
      if (picking?.id === id) {
        setPicking(null);
      } else {
        setPicking({
          type: "capture",
          id,
          data: {
            pwNodeId: device.pw_node_id,
            deviceName: device.device_name,
            name: device.name,
            isAdded: device.is_added,
            inputId: device.input_id,
          },
        });
      }
    },
    [picking, setPicking],
  );

  const togglePlaybackPicking = useCallback(
    (device: PlaybackDeviceInfo) => {
      const id = `playback-${device.pw_node_id}`;
      if (picking?.id === id) {
        setPicking(null);
      } else {
        setPicking({
          type: "playback",
          id,
          data: {
            pwNodeId: device.pw_node_id,
            deviceName: device.device_name,
            name: device.name,
          },
        });
      }
    },
    [picking, setPicking],
  );

  const toggleRulePicking = useCallback(
    (appName: string) => {
      const id = `rule-${appName}`;
      if (picking?.id === id) {
        setPicking(null);
      } else {
        setPicking({
          type: "rule",
          id,
          data: { appName },
        });
      }
    },
    [picking, setPicking],
  );

  // --- Unbind handlers -------------------------------------------------------

  const handleUnbindDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      const captureRaw = e.dataTransfer.getData(
        "application/x-mixctl-capture",
      );
      const playbackRaw = e.dataTransfer.getData(
        "application/x-mixctl-playback",
      );

      if (captureRaw) {
        const data = JSON.parse(captureRaw) as {
          inputId?: number;
        };
        if (data.inputId && data.inputId > 0) {
          await SystemApi.removeCaptureInput(data.inputId);
        }
        refreshCaptureDevices();
      } else if (playbackRaw) {
        const data = JSON.parse(playbackRaw) as {
          deviceName: string;
        };
        const output = outputs.find(
          (o) => o.target_device === data.deviceName,
        );
        if (output) {
          await MixerApi.setOutputTarget(output.id, "");
        }
      }
    },
    [outputs, refreshCaptureDevices],
  );

  const handleUnbindClick = useCallback(async () => {
    if (!picking) return;
    if (picking.type === "capture" && picking.data.inputId && picking.data.inputId > 0) {
      await SystemApi.removeCaptureInput(picking.data.inputId);
      refreshCaptureDevices();
    } else if (picking.type === "playback" && picking.data.deviceName) {
      const output = outputs.find(
        (o) => o.target_device === picking.data.deviceName,
      );
      if (output) {
        await MixerApi.setOutputTarget(output.id, "");
      }
    }
    setPicking(null);
  }, [picking, outputs, setPicking, refreshCaptureDevices]);

  // --- Drag start callbacks that set local draggingType ----------------------

  const handleDeviceDragStart = useCallback(
    (type: "capture" | "playback") => {
      setDraggingType(type);
    },
    [],
  );

  const handleDragEnd = useCallback(() => {
    setDraggingType(null);
  }, []);

  return (
    <div
      className="relative shrink-0"
      style={{
        background: "var(--bg-surface-0)",
        borderTop: "1px solid var(--border-default)",
      }}
    >
      {/* Unbind zone */}
      {showUnbindZone && (
        <UnbindZone
          onDrop={handleUnbindDrop}
          onClick={handleUnbindClick}
        />
      )}

      {/* Pill row */}
      <div className="flex items-center gap-1.5 px-6 py-2 overflow-x-auto">
        {/* Expand button */}
        <ExpandButton onClick={onToggleExpand} />

        {/* Stream pills */}
        {visibleStreams.map((stream) => {
          const input = inputs.find((i) => i.id === stream.input_id);
          const id = `stream-${stream.pw_node_id}`;
          return (
            <StreamPill
              key={id}
              pwNodeId={stream.pw_node_id}
              appName={stream.app_name}
              inputName={input?.name ?? "?"}
              inputColor={input?.color ?? "#888"}
              isPicked={picking?.id === id}
              onPick={() =>
                toggleStreamPicking(stream.pw_node_id, stream.app_name)
              }
            />
          );
        })}

        {/* Capture device pills — hide unbound devices the user dismissed */}
        {captureDevices
          .filter((d) => d.input_id > 0 || !hiddenDevices.has(d.device_name))
          .map((device) => {
          const boundInput = inputs.find((i) => i.id === device.input_id);
          const id = `capture-${device.pw_node_id}`;
          return (
            <CapturePill
              key={id}
              device={device}
              boundInputName={boundInput?.name}
              boundInputColor={boundInput?.color}
              isPicked={picking?.id === id}
              onPick={() => toggleCapturePicking(device)}
              onDragStart={() => handleDeviceDragStart("capture")}
              onDragEnd={handleDragEnd}
              onHide={device.input_id === 0 ? () => hideDevice(device.device_name) : undefined}
            />
          );
        })}

        {/* Playback device pills — hide unbound devices the user dismissed */}
        {playbackDevices
          .filter((d) => outputs.some((o) => o.target_device === d.device_name) || !hiddenDevices.has(d.device_name))
          .map((device) => {
          const boundOutput = outputs.find(
            (o) => o.target_device === device.device_name,
          );
          const id = `playback-${device.pw_node_id}`;
          return (
            <SpeakerPill
              key={id}
              device={device}
              boundOutputName={boundOutput?.name}
              boundOutputColor={boundOutput?.color}
              isPicked={picking?.id === id}
              onPick={() => togglePlaybackPicking(device)}
              onDragStart={() => handleDeviceDragStart("playback")}
              onDragEnd={handleDragEnd}
              onHide={!boundOutput ? () => hideDevice(device.device_name) : undefined}
            />
          );
        })}

        {/* Rule pills — hidden when a stream with matching app_name exists */}
        {rules
          .filter(
            (rule) =>
              !visibleStreams.some((s) => s.app_name === rule.app_name),
          )
          .map((rule) => {
            const input = inputs.find((i) => i.id === rule.input_id);
            const id = `rule-${rule.app_name}`;
            return (
              <RulePill
                key={id}
                appName={rule.app_name}
                inputName={input?.name ?? "?"}
                isPicked={picking?.id === id}
                onPick={() => toggleRulePicking(rule.app_name)}
                onDelete={async () => {
                  await SystemApi.removeAppRule(rule.app_name);
                  refreshRules();
                }}
              />
            );
          })}

        {/* Add rule pill */}
        <AddRulePill inputs={inputs} onAdded={refreshRules} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Expand Button
// ---------------------------------------------------------------------------

function ExpandButton({ onClick }: { onClick: () => void }) {
  return (
    <span
      onClick={onClick}
      className="shrink-0 cursor-pointer opacity-30 hover:opacity-70 transition-opacity"
      title="Expand"
    >
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="var(--text-secondary)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <polyline points="4 10 8 6 12 10" />
      </svg>
    </span>
  );
}

// ---------------------------------------------------------------------------
// Unbind Zone
// ---------------------------------------------------------------------------

function UnbindZone({
  onDrop,
  onClick,
}: {
  onDrop: (e: React.DragEvent) => void;
  onClick: () => void;
}) {
  const [dragOver, setDragOver] = useState(false);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    setDragOver(true);
  }, []);

  const handleDragLeave = useCallback(() => {
    setDragOver(false);
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      setDragOver(false);
      onDrop(e);
    },
    [onDrop],
  );

  return (
    <div className="flex justify-center px-6 py-1.5">
      <div
        onClick={onClick}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        className="flex items-center gap-1.5 px-4 py-1 rounded-full text-[11px] cursor-pointer shrink-0 transition-all duration-[120ms]"
        style={{
          background: dragOver
            ? "var(--accent-danger)"
            : "color-mix(in srgb, var(--accent-danger) 20%, transparent)",
          border: `1px solid var(--accent-danger)`,
          color: dragOver ? "white" : "var(--accent-danger)",
        }}
      >
        <span style={{ fontSize: 11, lineHeight: 1 }}>{"\uD83D\uDDD1"}</span>
        <span>Unbind</span>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Stream Pill
// ---------------------------------------------------------------------------

interface StreamPillProps {
  pwNodeId: number;
  appName: string;
  inputName: string;
  inputColor: string;
  isPicked: boolean;
  onPick: () => void;
}

function StreamPill({
  pwNodeId,
  appName,
  inputName,
  inputColor,
  isPicked,
  onPick,
}: StreamPillProps) {
  const [isDragging, setIsDragging] = useState(false);

  const handleDragStart = useCallback(
    (e: React.DragEvent) => {
      e.dataTransfer.setData(
        "application/x-mixctl-stream",
        JSON.stringify({ pwNodeId, appName }),
      );
      e.dataTransfer.effectAllowed = "move";
      setIsDragging(true);
    },
    [pwNodeId, appName],
  );

  return (
    <div
      draggable
      onDragStart={handleDragStart}
      onDragEnd={() => setIsDragging(false)}
      onClick={onPick}
      className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] cursor-pointer shrink-0 transition-all duration-[120ms]"
      style={{
        background: isPicked
          ? "var(--accent-warning-muted)"
          : isDragging
            ? "var(--bg-surface-3)"
            : "var(--bg-surface-1)",
        border: isPicked
          ? "1px solid var(--accent-warning)"
          : "1px solid var(--border-default)",
        opacity: isDragging ? 0.5 : 1,
        color: "var(--text-secondary)",
      }}
      title={`${appName} \u2192 ${inputName}`}
    >
      <span style={{ fontSize: 11, lineHeight: 1 }}>{"\uD83D\uDD0A"}</span>
      <span className="truncate max-w-24">{appName}</span>
      <span style={{ color: "var(--text-muted)" }}>{"\u2192"}</span>
      <span className="truncate max-w-16" style={{ color: inputColor }}>
        {inputName}
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Capture Pill
// ---------------------------------------------------------------------------

interface CapturePillProps {
  device: CaptureDeviceInfo;
  boundInputName: string | undefined;
  boundInputColor: string | undefined;
  isPicked: boolean;
  onPick: () => void;
  onDragStart: () => void;
  onDragEnd: () => void;
  onHide?: () => void;
}

function CapturePill({
  device,
  boundInputName,
  boundInputColor,
  isPicked,
  onPick,
  onDragStart,
  onDragEnd,
  onHide,
}: CapturePillProps) {
  const [isDragging, setIsDragging] = useState(false);
  const isBound = !!boundInputName;

  const handleDragStart = useCallback(
    (e: React.DragEvent) => {
      e.dataTransfer.setData(
        "application/x-mixctl-capture",
        JSON.stringify({
          pwNodeId: device.pw_node_id,
          deviceName: device.device_name,
          name: device.name,
          isAdded: device.is_added,
          inputId: device.input_id,
        }),
      );
      e.dataTransfer.effectAllowed = "move";
      setIsDragging(true);
      onDragStart();
    },
    [device, onDragStart],
  );

  const handleDragEnd = useCallback(() => {
    setIsDragging(false);
    onDragEnd();
  }, [onDragEnd]);

  return (
    <div
      draggable
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onClick={onPick}
      className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] cursor-pointer shrink-0 transition-all duration-[120ms]"
      style={{
        background: isPicked
          ? "var(--accent-warning-muted)"
          : isDragging
            ? "var(--bg-surface-3)"
            : "var(--bg-surface-1)",
        border: isPicked
          ? "1px solid var(--accent-warning)"
          : `1px solid ${isBound ? "var(--accent-success)" : "var(--border-default)"}`,
        opacity: isDragging ? 0.5 : 1,
        color: "var(--text-secondary)",
      }}
      title={`${device.name} \u2192 ${boundInputName ?? "unbound"}`}
    >
      <span
        style={{
          color: "var(--accent-success)",
          fontSize: 11,
          lineHeight: 1,
        }}
      >
        {"\uD83C\uDFA4"}
      </span>
      <span className="truncate max-w-24">{device.name}</span>
      <span style={{ color: "var(--text-muted)" }}>{"\u2192"}</span>
      <span
        className="truncate max-w-16"
        style={{ color: boundInputColor ?? "var(--text-muted)" }}
      >
        {boundInputName ?? "unbound"}
      </span>
      {onHide && (
        <span
          onClick={(e) => { e.stopPropagation(); onHide(); }}
          className="ml-0.5 opacity-40 hover:opacity-100 cursor-pointer"
          style={{ fontSize: 10, lineHeight: 1, color: "var(--text-muted)" }}
          title="Hide this device"
        >
          {"\u2715"}
        </span>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Speaker Pill (Playback Device)
// ---------------------------------------------------------------------------

interface SpeakerPillProps {
  device: PlaybackDeviceInfo;
  boundOutputName: string | undefined;
  boundOutputColor: string | undefined;
  isPicked: boolean;
  onPick: () => void;
  onDragStart: () => void;
  onDragEnd: () => void;
  onHide?: () => void;
}

function SpeakerPill({
  device,
  boundOutputName,
  boundOutputColor,
  isPicked,
  onPick,
  onDragStart,
  onDragEnd,
  onHide,
}: SpeakerPillProps) {
  const [isDragging, setIsDragging] = useState(false);
  const isBound = !!boundOutputName;

  const handleDragStart = useCallback(
    (e: React.DragEvent) => {
      e.dataTransfer.setData(
        "application/x-mixctl-playback",
        JSON.stringify({
          pwNodeId: device.pw_node_id,
          deviceName: device.device_name,
          name: device.name,
        }),
      );
      e.dataTransfer.effectAllowed = "move";
      setIsDragging(true);
      onDragStart();
    },
    [device, onDragStart],
  );

  const handleDragEnd = useCallback(() => {
    setIsDragging(false);
    onDragEnd();
  }, [onDragEnd]);

  return (
    <div
      draggable
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onClick={onPick}
      className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] cursor-pointer shrink-0 transition-all duration-[120ms]"
      style={{
        background: isPicked
          ? "var(--accent-warning-muted)"
          : isDragging
            ? "var(--bg-surface-3)"
            : "var(--bg-surface-1)",
        border: isPicked
          ? "1px solid var(--accent-warning)"
          : `1px solid ${isBound ? "var(--accent-info, #3b82f6)" : "var(--border-default)"}`,
        opacity: isDragging ? 0.5 : 1,
        color: "var(--text-secondary)",
      }}
      title={`${device.name} \u2192 ${boundOutputName ?? "unbound"}`}
    >
      <span
        style={{
          color: "var(--accent-info, #3b82f6)",
          fontSize: 11,
          lineHeight: 1,
        }}
      >
        {"\uD83D\uDD08"}
      </span>
      <span
        className="truncate max-w-16"
        style={{ color: boundOutputColor ?? "var(--text-muted)" }}
      >
        {boundOutputName ?? "unbound"}
      </span>
      <span style={{ color: "var(--text-muted)" }}>{"\u2192"}</span>
      <span className="truncate max-w-24">{device.name}</span>
      {onHide && (
        <span
          onClick={(e) => { e.stopPropagation(); onHide(); }}
          className="ml-0.5 opacity-40 hover:opacity-100 cursor-pointer"
          style={{ fontSize: 10, lineHeight: 1, color: "var(--text-muted)" }}
          title="Hide this device"
        >
          {"\u2715"}
        </span>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Rule Pill
// ---------------------------------------------------------------------------

interface RulePillProps {
  appName: string;
  inputName: string;
  isPicked: boolean;
  onPick: () => void;
  onDelete: () => void;
}

function RulePill({
  appName,
  inputName,
  isPicked,
  onPick,
  onDelete,
}: RulePillProps) {
  const [isDragging, setIsDragging] = useState(false);

  const handleDragStart = useCallback(
    (e: React.DragEvent) => {
      e.dataTransfer.setData(
        "application/x-mixctl-rule",
        JSON.stringify({ appName }),
      );
      e.dataTransfer.effectAllowed = "move";
      setIsDragging(true);
    },
    [appName],
  );

  return (
    <div
      draggable
      onDragStart={handleDragStart}
      onDragEnd={() => setIsDragging(false)}
      onClick={onPick}
      className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] cursor-pointer shrink-0 transition-all duration-[120ms]"
      style={{
        background: isPicked
          ? "var(--accent-warning-muted)"
          : isDragging
            ? "var(--bg-surface-3)"
            : "var(--bg-surface-1)",
        border: isPicked
          ? "1px solid var(--accent-warning)"
          : "1px solid var(--border-default)",
        opacity: isDragging ? 0.5 : 1,
        color: "var(--text-muted)",
      }}
      title={`Rule: ${appName} \u2192 ${inputName}`}
    >
      <span style={{ fontSize: 11, lineHeight: 1 }}>{"\uD83D\uDCCB"}</span>
      <span className="truncate max-w-24">{appName}</span>
      <span>{"\u2192"}</span>
      <span className="truncate max-w-16">{inputName}</span>
      <button
        onClick={(e) => {
          e.stopPropagation();
          onDelete();
        }}
        className="ml-0.5 w-4 h-4 flex items-center justify-center rounded-full opacity-50 hover:opacity-100 transition-opacity cursor-pointer"
        style={{
          color: "var(--text-muted)",
          fontSize: 10,
        }}
        title="Remove rule"
      >
        {"\u00D7"}
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Add Rule Pill
// ---------------------------------------------------------------------------

interface AddRulePillProps {
  inputs: { id: number; name: string }[];
  onAdded: () => void;
}

function AddRulePill({ inputs, onAdded }: AddRulePillProps) {
  const [formOpen, setFormOpen] = useState(false);
  const [appName, setAppName] = useState("");
  const [selectedInputId, setSelectedInputId] = useState<number>(
    inputs[0]?.id ?? 0,
  );
  const formRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (inputs.length > 0 && selectedInputId === 0) {
      setSelectedInputId(inputs[0].id);
    }
  }, [inputs, selectedInputId]);

  useEffect(() => {
    if (!formOpen) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (formRef.current && !formRef.current.contains(e.target as Node)) {
        setFormOpen(false);
      }
    };

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") setFormOpen(false);
    };

    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleEscape);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [formOpen]);

  const handleSubmit = useCallback(async () => {
    const trimmed = appName.trim();
    if (!trimmed || !selectedInputId) return;
    await SystemApi.setAppRule(trimmed, selectedInputId);
    setAppName("");
    setFormOpen(false);
    onAdded();
  }, [appName, selectedInputId, onAdded]);

  if (!formOpen) {
    return (
      <div
        onClick={() => setFormOpen(true)}
        className="flex items-center gap-1 px-2.5 py-1 rounded-full text-[11px] cursor-pointer shrink-0 transition-all duration-[120ms]"
        style={{
          background: "transparent",
          border: "1px dashed var(--border-default)",
          color: "var(--text-muted)",
        }}
      >
        [+ rule]
      </div>
    );
  }

  return (
    <div
      ref={formRef}
      className="flex items-center gap-1.5 px-2 py-1 rounded-lg text-[11px] shrink-0"
      style={{
        background: "var(--bg-surface-2)",
        border: "1px solid var(--border-default)",
      }}
    >
      <input
        type="text"
        value={appName}
        onChange={(e) => setAppName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") handleSubmit();
        }}
        placeholder="app name"
        autoFocus
        className="w-24 px-1.5 py-0.5 rounded text-[11px] outline-none"
        style={{
          background: "var(--bg-surface-0)",
          border: "1px solid var(--border-default)",
          color: "var(--text-primary)",
        }}
      />
      <select
        value={selectedInputId}
        onChange={(e) => setSelectedInputId(Number(e.target.value))}
        className="px-1 py-0.5 rounded text-[11px] outline-none cursor-pointer"
        style={{
          background: "var(--bg-surface-0)",
          border: "1px solid var(--border-default)",
          color: "var(--text-primary)",
        }}
      >
        {inputs.map((input) => (
          <option key={input.id} value={input.id}>
            {input.name}
          </option>
        ))}
      </select>
      <button
        onClick={handleSubmit}
        className="px-2 py-0.5 rounded text-[11px] cursor-pointer transition-opacity hover:opacity-80"
        style={{
          background: "var(--accent-primary)",
          color: "var(--text-on-accent)",
          border: "none",
        }}
      >
        Add
      </button>
    </div>
  );
}
