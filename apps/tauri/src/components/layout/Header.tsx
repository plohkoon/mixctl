import { useMixerStore } from "../../lib/stores/mixer-store";
import { MixerApi, SystemApi } from "../../lib/api";

export default function Header() {
  const audioConnected = useMixerStore((s) => s.audioConnected);
  const beacnConnected = useMixerStore((s) => s.beacnConnected);
  const inputs = useMixerStore((s) => s.inputs);
  const outputs = useMixerStore((s) => s.outputs);
  const defaultInputId = useMixerStore((s) => s.defaultInputId);
  const defaultOutputId = useMixerStore((s) => s.defaultOutputId);

  return (
    <div
      className="h-12 flex items-center px-4 gap-3 shrink-0"
      style={{
        background: "var(--bg-surface-1)",
        borderBottom: "1px solid var(--border-default)",
      }}
    >
      <span className="text-base font-bold tracking-tight">MixCtl</span>

      {/* Status */}
      <div className="flex items-center gap-1.5">
        <span
          className="w-2 h-2 rounded-full"
          style={{
            background: audioConnected ? "var(--accent-success)" : "var(--accent-danger)",
            boxShadow: audioConnected ? "0 0 6px var(--accent-success)" : "none",
          }}
        />
        <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>
          {audioConnected ? "Connected" : "Disconnected"}
        </span>
      </div>

      {/* Separator */}
      <div className="w-px h-5" style={{ background: "var(--border-default)" }} />

      {/* Default input */}
      <div className="flex items-center gap-1.5">
        <span className="text-[10px] uppercase font-medium" style={{ color: "var(--text-muted)", letterSpacing: "0.04em" }}>
          Default In
        </span>
        <select
          className="text-[11px] px-1.5 py-0.5 rounded cursor-pointer"
          style={{ background: "var(--bg-surface-2)", border: "1px solid var(--border-default)", color: "var(--text-secondary)", maxWidth: 120 }}
          value={defaultInputId}
          onChange={(e) => MixerApi.setDefaultInput(Number(e.target.value))}
        >
          {inputs.map((inp) => (
            <option key={inp.id} value={inp.id}>{inp.name}</option>
          ))}
        </select>
      </div>

      {/* Default output */}
      <div className="flex items-center gap-1.5">
        <span className="text-[10px] uppercase font-medium" style={{ color: "var(--text-muted)", letterSpacing: "0.04em" }}>
          Default Out
        </span>
        <select
          className="text-[11px] px-1.5 py-0.5 rounded cursor-pointer"
          style={{ background: "var(--bg-surface-2)", border: "1px solid var(--border-default)", color: "var(--text-secondary)", maxWidth: 120 }}
          value={defaultOutputId}
          onChange={(e) => MixerApi.setDefaultOutput(Number(e.target.value))}
        >
          {outputs.map((out) => (
            <option key={out.id} value={out.id}>{out.name}</option>
          ))}
        </select>
      </div>

      <div className="flex-1" />

      {/* Dialog buttons */}
      <HeaderButton label="DSP" onClick={() => SystemApi.openDialog("dsp")} />
      {beacnConnected && (
        <HeaderButton label="Beacn" onClick={() => SystemApi.openDialog("beacn")} />
      )}
    </div>
  );
}

function HeaderButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className="px-3 py-1.5 text-[12px] font-medium rounded-md cursor-pointer transition-all duration-[120ms]"
      style={{ background: "var(--bg-surface-2)", border: "1px solid var(--border-default)", color: "var(--text-secondary)" }}
      onMouseEnter={(e) => { e.currentTarget.style.background = "var(--bg-surface-3)"; e.currentTarget.style.color = "var(--text-primary)"; }}
      onMouseLeave={(e) => { e.currentTarget.style.background = "var(--bg-surface-2)"; e.currentTarget.style.color = "var(--text-secondary)"; }}
    >
      {label}
    </button>
  );
}
