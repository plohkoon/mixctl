import { useMixerStore } from "../../lib/stores/mixer-store";

interface ExpandedFooterProps {
  onClose: () => void;
}

export default function ExpandedFooter({ onClose }: ExpandedFooterProps) {
  const streams = useMixerStore((s) => s.streams);
  const inputs = useMixerStore((s) => s.inputs);
  const outputs = useMixerStore((s) => s.outputs);
  const captureDevices = useMixerStore((s) => s.captureDevices);
  const playbackDevices = useMixerStore((s) => s.playbackDevices);
  const rules = useMixerStore((s) => s.rules);

  const visibleStreams = streams.filter(
    (s) => !s.app_name.includes("mixctl.") && !s.app_name.startsWith("output.")
  );

  return (
    <div
      className="absolute bottom-0 left-0 right-0 z-40 flex flex-col"
      style={{
        height: "60%",
        background: "var(--bg-surface-0)",
        borderTop: "2px solid var(--border-strong)",
        boxShadow: "0 -8px 32px rgba(0,0,0,0.4)",
      }}
    >
      {/* Header */}
      <div className="flex items-center px-6 py-2 shrink-0" style={{ borderBottom: "1px solid var(--border-default)" }}>
        <button
          onClick={onClose}
          className="w-6 h-6 flex items-center justify-center rounded cursor-pointer text-[14px]"
          style={{ color: "var(--text-muted)", background: "var(--bg-surface-2)", border: "1px solid var(--border-default)" }}
          title="Collapse"
        >
          {"\u02C7"}
        </button>
        <div className="flex-1" />
        <span className="text-[11px] font-semibold uppercase tracking-wide" style={{ color: "var(--text-muted)", letterSpacing: "0.04em" }}>
          Streams, Devices & Rules
        </span>
        <div className="flex-1" />
        <div className="w-6" />
      </div>

      {/* Three columns */}
      <div className="flex-1 flex gap-4 p-4 overflow-hidden min-h-0">
        {/* Streams column */}
        <div className="flex-1 flex flex-col gap-1 overflow-y-auto">
          <span className="text-[11px] font-semibold uppercase tracking-wide shrink-0 pb-1" style={{ color: "var(--text-muted)", letterSpacing: "0.04em" }}>
            Streams ({visibleStreams.length})
          </span>
          {visibleStreams.length === 0 && (
            <span className="text-[11px]" style={{ color: "var(--text-disabled)" }}>No active streams</span>
          )}
          {visibleStreams.map((stream) => {
            const input = inputs.find((i) => i.id === stream.input_id);
            return (
              <div
                key={stream.pw_node_id}
                className="flex items-center gap-2 px-3 py-1.5 rounded-md text-[11px]"
                style={{ background: "var(--bg-surface-1)", border: "1px solid var(--border-subtle)" }}
              >
                <span>{"\uD83D\uDD0A"}</span>
                <span className="truncate flex-1">{stream.app_name}</span>
                <span style={{ color: "var(--text-muted)" }}>{"\u2192"}</span>
                <span className="truncate" style={{ color: input?.color ?? "var(--text-muted)" }}>
                  {input?.name ?? "?"}
                </span>
              </div>
            );
          })}
        </div>

        {/* Devices column */}
        <div className="flex-1 flex flex-col gap-1 overflow-y-auto">
          <span className="text-[11px] font-semibold uppercase tracking-wide shrink-0 pb-1" style={{ color: "var(--text-muted)", letterSpacing: "0.04em" }}>
            Devices ({captureDevices.length + playbackDevices.length})
          </span>
          {captureDevices.length === 0 && playbackDevices.length === 0 && (
            <span className="text-[11px]" style={{ color: "var(--text-disabled)" }}>No devices</span>
          )}
          {captureDevices.map((device) => {
            const boundInput = inputs.find((i) => i.id === device.input_id);
            return (
              <div
                key={`cap-${device.pw_node_id}`}
                className="flex items-center gap-2 px-3 py-1.5 rounded-md text-[11px]"
                style={{ background: "var(--bg-surface-1)", border: `1px solid ${boundInput ? "var(--accent-success)" : "var(--border-subtle)"}40` }}
              >
                <span style={{ color: "var(--accent-success)" }}>{"\uD83C\uDFA4"}</span>
                <span className="truncate flex-1">{device.name}</span>
                <span style={{ color: "var(--text-muted)" }}>{"\u2192"}</span>
                <span className="truncate" style={{ color: boundInput?.color ?? "var(--text-muted)" }}>
                  {boundInput?.name ?? "unbound"}
                </span>
              </div>
            );
          })}
          {playbackDevices.map((device) => {
            const boundOutput = outputs.find((o) => o.target_device === device.device_name);
            return (
              <div
                key={`pb-${device.pw_node_id}`}
                className="flex items-center gap-2 px-3 py-1.5 rounded-md text-[11px]"
                style={{ background: "var(--bg-surface-1)", border: `1px solid ${boundOutput ? "var(--accent-primary)" : "var(--border-subtle)"}40` }}
              >
                <span style={{ color: "var(--accent-primary)" }}>{"\uD83D\uDD08"}</span>
                <span className="truncate flex-1">{device.name}</span>
                <span style={{ color: "var(--text-muted)" }}>{"\u2192"}</span>
                <span className="truncate" style={{ color: boundOutput?.color ?? "var(--text-muted)" }}>
                  {boundOutput?.name ?? "unbound"}
                </span>
              </div>
            );
          })}
        </div>

        {/* Rules column */}
        <div className="flex-1 flex flex-col gap-1 overflow-y-auto">
          <span className="text-[11px] font-semibold uppercase tracking-wide shrink-0 pb-1" style={{ color: "var(--text-muted)", letterSpacing: "0.04em" }}>
            Rules ({rules.length})
          </span>
          {rules.length === 0 && (
            <span className="text-[11px]" style={{ color: "var(--text-disabled)" }}>No rules</span>
          )}
          {rules.map((rule) => {
            const input = inputs.find((i) => i.id === rule.input_id);
            return (
              <div
                key={rule.app_name}
                className="flex items-center gap-2 px-3 py-1.5 rounded-md text-[11px]"
                style={{ background: "var(--bg-surface-1)", border: "1px solid var(--border-subtle)" }}
              >
                <span>{"\uD83D\uDCCB"}</span>
                <span className="truncate flex-1">{rule.app_name}</span>
                <span style={{ color: "var(--text-muted)" }}>{"\u2192"}</span>
                <span className="truncate" style={{ color: input?.color ?? "var(--text-muted)" }}>
                  {input?.name ?? "?"}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
