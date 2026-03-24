import React, { useCallback, useEffect, useState } from "react";
import { DspApi, MixerApi } from "../lib/api";
import type {
  CompressorInfo,
  DeesserInfo,
  EqBandInfo,
  GateInfo,
  InputInfo,
  LimiterInfo,
  OutputInfo,
} from "../lib/types";
import EqCurve from "../components/shared/EqCurve";

const DEFAULT_EQ_BAND: EqBandInfo = {
  band_type: "peaking",
  frequency: 1000,
  gain_db: 0,
  q: 1,
};

const BAND_TYPES = ["low_shelf", "peaking", "high_shelf"];
const BAND_TYPE_LABELS = ["LS", "PK", "HS"];

type ChannelMode = "input" | "output";

interface ChannelOption {
  id: number;
  name: string;
  color: string;
  mode: ChannelMode;
}

export default function DspPage() {
  const [channels, setChannels] = useState<ChannelOption[]>([]);
  const [selectedIdx, setSelectedIdx] = useState(0);

  // EQ state (shared for input — future: output too)
  const [eqEnabled, setEqEnabled] = useState(false);
  const [eqBands, setEqBands] = useState<EqBandInfo[]>(
    Array(8).fill(null).map(() => ({ ...DEFAULT_EQ_BAND }))
  );
  const [selectedBand, setSelectedBand] = useState<number | null>(0);

  // Input-specific DSP
  const [gate, setGate] = useState<GateInfo>({
    enabled: false, threshold_db: -40, attack_ms: 1, release_ms: 50, hold_ms: 5,
  });
  const [deesser, setDeesser] = useState<DeesserInfo>({
    enabled: false, frequency: 6000, threshold_db: -20, ratio: 4,
  });

  // Output-specific DSP
  const [compressor, setCompressor] = useState<CompressorInfo>({
    enabled: false, threshold_db: -18, ratio: 4, attack_ms: 10, release_ms: 100, makeup_gain_db: 0, knee_db: 0,
  });
  const [limiter, setLimiter] = useState<LimiterInfo>({
    enabled: false, ceiling_db: -0.5, release_ms: 50,
  });

  const selected = channels[selectedIdx];
  const mode = selected?.mode ?? "input";
  const channelId = selected?.id ?? 0;

  // Load channel list
  useEffect(() => {
    Promise.all([MixerApi.listInputs(), MixerApi.listOutputs()]).then(
      ([inp, out]) => {
        const opts: ChannelOption[] = [
          ...inp.map((i: InputInfo) => ({ id: i.id, name: i.name, color: i.color, mode: "input" as ChannelMode })),
          ...out.map((o: OutputInfo) => ({ id: o.id, name: o.name, color: o.color, mode: "output" as ChannelMode })),
        ];
        setChannels(opts);
        if (opts.length > 0) loadDsp(opts[0]);
      }
    );
  }, []);

  const loadDsp = useCallback(async (ch: ChannelOption) => {
    if (ch.mode === "input") {
      const dsp = await DspApi.getInputDsp(ch.id);
      setEqEnabled(dsp.eqEnabled);
      const bands = [...dsp.eqBands];
      while (bands.length < 8) bands.push({ ...DEFAULT_EQ_BAND });
      setEqBands(bands);
      setGate(dsp.gate);
      setDeesser(dsp.deesser);
    } else {
      const dsp = await DspApi.getOutputDsp(ch.id);
      setCompressor(dsp.compressor);
      setLimiter(dsp.limiter);
      // Output EQ not yet supported — show flat
      setEqEnabled(false);
      setEqBands(Array(8).fill(null).map(() => ({ ...DEFAULT_EQ_BAND })));
    }
  }, []);

  // EQ interactions
  const handleBandDrag = useCallback(
    (bandIdx: number, freq: number, gainDb: number) => {
      setEqBands((prev) => {
        const next = [...prev];
        next[bandIdx] = { ...next[bandIdx], frequency: freq, gain_db: gainDb };
        return next;
      });
    }, []
  );

  const handleBandDragEnd = useCallback(
    (bandIdx: number, freq: number, gainDb: number) => {
      if (!channelId || mode !== "input") return;
      const band = eqBands[bandIdx];
      DspApi.setInputEqBand(channelId, bandIdx, band.band_type, freq, gainDb, band.q);
    }, [channelId, mode, eqBands]
  );

  const handleBandQScroll = useCallback(
    (bandIdx: number, delta: number) => {
      setEqBands((prev) => {
        const next = [...prev];
        const newQ = Math.max(0.1, Math.min(18, next[bandIdx].q + delta));
        next[bandIdx] = { ...next[bandIdx], q: newQ };
        if (channelId && mode === "input") {
          DspApi.setInputEqBand(channelId, bandIdx, next[bandIdx].band_type, next[bandIdx].frequency, next[bandIdx].gain_db, newQ);
        }
        return next;
      });
    }, [channelId, mode]
  );

  const updateBand = useCallback(
    (bandIdx: number, field: keyof EqBandInfo, value: number | string) => {
      setEqBands((prev) => {
        const next = [...prev];
        next[bandIdx] = { ...next[bandIdx], [field]: value };
        if (channelId && mode === "input") {
          const b = next[bandIdx];
          DspApi.setInputEqBand(channelId, bandIdx, b.band_type, b.frequency, b.gain_db, b.q);
        }
        return next;
      });
    }, [channelId, mode]
  );

  return (
    <div className="flex flex-col h-screen overflow-hidden" style={{ background: "var(--bg-base)" }}>
      {/* Top bar: channel selector */}
      <div className="flex items-center gap-3 px-4 py-2 shrink-0" style={{ background: "var(--bg-surface-1)", borderBottom: "1px solid var(--border-default)" }}>
        <span className="text-xs font-semibold uppercase tracking-wide" style={{ color: "var(--text-muted)" }}>Channel</span>
        <select
          className="px-2 py-1 text-[12px] font-medium rounded cursor-pointer"
          style={{
            background: "var(--bg-surface-2)",
            border: "1px solid var(--border-default)",
            color: selected ? selected.color : "var(--text-secondary)",
            minWidth: 160,
          }}
          value={selectedIdx}
          onChange={(e) => {
            const i = parseInt(e.target.value);
            setSelectedIdx(i);
            if (channels[i]) loadDsp(channels[i]);
          }}
        >
          {channels.map((ch, i) => (
            <option key={`${ch.mode}-${ch.id}`} value={i}>
              {ch.mode === "input" ? "\u25B6" : "\u25C0"} {ch.name} ({ch.mode === "input" ? "Input" : "Output"})
            </option>
          ))}
        </select>
        <div className="flex-1" />
        {mode === "input" && (
          <div className="flex items-center gap-2">
            <label className="flex items-center gap-1 text-[11px] cursor-pointer" style={{ color: "var(--text-secondary)" }}>
              <input type="checkbox" checked={eqEnabled} onChange={(e) => {
                setEqEnabled(e.target.checked);
                if (channelId) DspApi.setInputEqEnabled(channelId, e.target.checked);
              }} />
              EQ
            </label>
            <button
              onClick={() => { if (channelId) DspApi.resetInputEq(channelId).then(() => loadDsp(selected)); }}
              className="px-2 py-0.5 text-[10px] rounded cursor-pointer"
              style={{ background: "var(--bg-surface-2)", border: "1px solid var(--border-default)", color: "var(--text-muted)" }}
            >
              Reset
            </button>
          </div>
        )}
      </div>

      {/* Main content: EQ curve + band strips + DSP params */}
      <div className="flex-1 flex flex-col overflow-y-auto">
        {/* EQ Curve */}
        <div className="px-4 pt-3 pb-1">
          <EqCurve
            bands={eqBands}
            selectedBand={selectedBand}
            onBandDrag={handleBandDrag}
            onBandDragEnd={handleBandDragEnd}
            onBandSelect={setSelectedBand}
            onBandQScroll={handleBandQScroll}
          />
        </div>

        {/* Band strips — horizontal row of vertical controls */}
        {mode === "input" && (
          <div className="flex gap-0 px-4 pb-2 shrink-0" style={{ borderBottom: "1px solid var(--border-default)" }}>
            {eqBands.map((band, idx) => (
              <BandStrip
                key={idx}
                index={idx}
                band={band}
                isSelected={selectedBand === idx}
                disabled={!eqEnabled}
                onSelect={() => setSelectedBand(idx)}
                onTypeChange={(t) => updateBand(idx, "band_type", t)}
                onFreqChange={(v) => updateBand(idx, "frequency", v)}
                onGainChange={(v) => updateBand(idx, "gain_db", v)}
                onQChange={(v) => updateBand(idx, "q", v)}
              />
            ))}
          </div>
        )}

        {/* Bottom section: Gate/De-esser (input) or Compressor/Limiter (output) */}
        <div className="flex gap-4 px-4 py-3 overflow-x-auto shrink-0">
          {mode === "input" ? (
            <>
              <DspGroup title="Noise Gate" enabled={gate.enabled} onToggle={(v) => {
                setGate({ ...gate, enabled: v });
                if (channelId) DspApi.setInputGateEnabled(channelId, v);
              }}>
                <Param label="Threshold" value={gate.threshold_db} suffix="dB" min={-80} max={0} step={0.5} defaultValue={-40} disabled={!gate.enabled}
                  onChange={(v) => { setGate({ ...gate, threshold_db: v }); if (channelId) DspApi.setInputGate(channelId, v, gate.attack_ms, gate.release_ms, gate.hold_ms); }} />
                <Param label="Attack" value={gate.attack_ms} suffix="ms" min={0.1} max={100} step={0.5} defaultValue={1} disabled={!gate.enabled}
                  onChange={(v) => { setGate({ ...gate, attack_ms: v }); if (channelId) DspApi.setInputGate(channelId, gate.threshold_db, v, gate.release_ms, gate.hold_ms); }} />
                <Param label="Release" value={gate.release_ms} suffix="ms" min={1} max={2000} step={5} defaultValue={50} disabled={!gate.enabled}
                  onChange={(v) => { setGate({ ...gate, release_ms: v }); if (channelId) DspApi.setInputGate(channelId, gate.threshold_db, gate.attack_ms, v, gate.hold_ms); }} />
                <Param label="Hold" value={gate.hold_ms} suffix="ms" min={0} max={500} step={1} defaultValue={5} disabled={!gate.enabled}
                  onChange={(v) => { setGate({ ...gate, hold_ms: v }); if (channelId) DspApi.setInputGate(channelId, gate.threshold_db, gate.attack_ms, gate.release_ms, v); }} />
              </DspGroup>
              <DspGroup title="De-esser" enabled={deesser.enabled} onToggle={(v) => {
                setDeesser({ ...deesser, enabled: v });
                if (channelId) DspApi.setInputDeesserEnabled(channelId, v);
              }}>
                <Param label="Frequency" value={deesser.frequency} suffix="Hz" min={2000} max={16000} step={100} defaultValue={6000} disabled={!deesser.enabled}
                  onChange={(v) => { setDeesser({ ...deesser, frequency: v }); if (channelId) DspApi.setInputDeesser(channelId, v, deesser.threshold_db, deesser.ratio); }} />
                <Param label="Threshold" value={deesser.threshold_db} suffix="dB" min={-60} max={0} step={0.5} defaultValue={-20} disabled={!deesser.enabled}
                  onChange={(v) => { setDeesser({ ...deesser, threshold_db: v }); if (channelId) DspApi.setInputDeesser(channelId, deesser.frequency, v, deesser.ratio); }} />
                <Param label="Ratio" value={deesser.ratio} suffix=":1" min={1} max={20} step={0.5} defaultValue={4} disabled={!deesser.enabled}
                  onChange={(v) => { setDeesser({ ...deesser, ratio: v }); if (channelId) DspApi.setInputDeesser(channelId, deesser.frequency, deesser.threshold_db, v); }} />
              </DspGroup>
            </>
          ) : (
            <>
              <DspGroup title="Compressor" enabled={compressor.enabled} onToggle={(v) => {
                setCompressor({ ...compressor, enabled: v });
                if (channelId) DspApi.setOutputCompressorEnabled(channelId, v);
              }}>
                <Param label="Threshold" value={compressor.threshold_db} suffix="dB" min={-60} max={0} step={0.5} defaultValue={-18} disabled={!compressor.enabled}
                  onChange={(v) => { const c = { ...compressor, threshold_db: v }; setCompressor(c); if (channelId) DspApi.setOutputCompressor(channelId, c.threshold_db, c.ratio, c.attack_ms, c.release_ms, c.makeup_gain_db, c.knee_db); }} />
                <Param label="Ratio" value={compressor.ratio} suffix=":1" min={1} max={20} step={0.5} defaultValue={4} disabled={!compressor.enabled}
                  onChange={(v) => { const c = { ...compressor, ratio: v }; setCompressor(c); if (channelId) DspApi.setOutputCompressor(channelId, c.threshold_db, c.ratio, c.attack_ms, c.release_ms, c.makeup_gain_db, c.knee_db); }} />
                <Param label="Attack" value={compressor.attack_ms} suffix="ms" min={0.1} max={200} step={0.5} defaultValue={10} disabled={!compressor.enabled}
                  onChange={(v) => { const c = { ...compressor, attack_ms: v }; setCompressor(c); if (channelId) DspApi.setOutputCompressor(channelId, c.threshold_db, c.ratio, c.attack_ms, c.release_ms, c.makeup_gain_db, c.knee_db); }} />
                <Param label="Release" value={compressor.release_ms} suffix="ms" min={1} max={2000} step={5} defaultValue={100} disabled={!compressor.enabled}
                  onChange={(v) => { const c = { ...compressor, release_ms: v }; setCompressor(c); if (channelId) DspApi.setOutputCompressor(channelId, c.threshold_db, c.ratio, c.attack_ms, c.release_ms, c.makeup_gain_db, c.knee_db); }} />
                <Param label="Makeup" value={compressor.makeup_gain_db} suffix="dB" min={0} max={30} step={0.5} defaultValue={0} disabled={!compressor.enabled}
                  onChange={(v) => { const c = { ...compressor, makeup_gain_db: v }; setCompressor(c); if (channelId) DspApi.setOutputCompressor(channelId, c.threshold_db, c.ratio, c.attack_ms, c.release_ms, c.makeup_gain_db, c.knee_db); }} />
                <Param label="Knee" value={compressor.knee_db} suffix="dB" min={0} max={12} step={0.5} defaultValue={0} disabled={!compressor.enabled}
                  onChange={(v) => { const c = { ...compressor, knee_db: v }; setCompressor(c); if (channelId) DspApi.setOutputCompressor(channelId, c.threshold_db, c.ratio, c.attack_ms, c.release_ms, c.makeup_gain_db, c.knee_db); }} />
              </DspGroup>
              <DspGroup title="Limiter" enabled={limiter.enabled} onToggle={(v) => {
                setLimiter({ ...limiter, enabled: v });
                if (channelId) DspApi.setOutputLimiterEnabled(channelId, v);
              }}>
                <Param label="Ceiling" value={limiter.ceiling_db} suffix="dB" min={-12} max={0} step={0.1} defaultValue={-0.5} disabled={!limiter.enabled}
                  onChange={(v) => { setLimiter({ ...limiter, ceiling_db: v }); if (channelId) DspApi.setOutputLimiter(channelId, v, limiter.release_ms); }} />
                <Param label="Release" value={limiter.release_ms} suffix="ms" min={1} max={500} step={1} defaultValue={50} disabled={!limiter.enabled}
                  onChange={(v) => { setLimiter({ ...limiter, release_ms: v }); if (channelId) DspApi.setOutputLimiter(channelId, limiter.ceiling_db, v); }} />
              </DspGroup>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Band Strip — vertical column per EQ band
// ---------------------------------------------------------------------------

const BAND_COLORS = [
  "#e74c3c", "#e67e22", "#f1c40f", "#2ecc71",
  "#1abc9c", "#3498db", "#9b59b6", "#e91e63",
];

interface BandStripProps {
  index: number;
  band: EqBandInfo;
  isSelected: boolean;
  disabled: boolean;
  onSelect: () => void;
  onTypeChange: (type: string) => void;
  onFreqChange: (freq: number) => void;
  onGainChange: (gain: number) => void;
  onQChange: (q: number) => void;
}

function BandStrip({ index, band, isSelected, disabled, onSelect, onTypeChange, onFreqChange: _onFreqChange, onGainChange, onQChange: _onQChange }: BandStripProps) {
  const color = BAND_COLORS[index % BAND_COLORS.length];
  const ratio = (band.gain_db + 24) / 48; // 0 at -24dB, 1 at +24dB

  const trackRef = React.useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = React.useState(false);

  const posToGain = React.useCallback((clientY: number) => {
    const track = trackRef.current;
    if (!track) return band.gain_db;
    const rect = track.getBoundingClientRect();
    const r = 1 - (clientY - rect.top) / rect.height;
    return Math.max(-24, Math.min(24, Math.round((-24 + r * 48) * 10) / 10));
  }, [band.gain_db]);

  React.useEffect(() => {
    if (!dragging) return;
    const onMove = (e: MouseEvent) => onGainChange(posToGain(e.clientY));
    const onUp = () => setDragging(false);
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => { window.removeEventListener("mousemove", onMove); window.removeEventListener("mouseup", onUp); };
  }, [dragging, posToGain, onGainChange]);

  return (
    <div
      className="flex flex-col items-center flex-1 py-2 cursor-pointer transition-all duration-[120ms]"
      style={{
        opacity: disabled ? 0.35 : 1,
        background: isSelected ? `${color}12` : "transparent",
        borderLeft: index > 0 ? "1px solid var(--border-subtle)" : "none",
      }}
      onClick={onSelect}
    >
      {/* Band number + type */}
      <div className="flex items-center gap-1 mb-1.5">
        <span className="text-[10px] font-bold" style={{ color }}>{index + 1}</span>
        <select
          className="text-[9px] rounded px-0.5 cursor-pointer"
          style={{ background: "var(--bg-surface-2)", border: "none", color: "var(--text-muted)" }}
          value={BAND_TYPES.indexOf(band.band_type)}
          onChange={(e) => onTypeChange(BAND_TYPES[parseInt(e.target.value)])}
          onClick={(e) => e.stopPropagation()}
          disabled={disabled}
        >
          {BAND_TYPE_LABELS.map((label, i) => (
            <option key={i} value={i}>{label}</option>
          ))}
        </select>
      </div>

      {/* Custom vertical gain fader */}
      <div
        ref={trackRef}
        className="relative flex-1 min-h-[70px]"
        style={{ width: 20, borderRadius: 4, background: "var(--bg-surface-2)", cursor: disabled ? "default" : "pointer" }}
        onMouseDown={(e) => {
          if (disabled) return;
          e.stopPropagation();
          setDragging(true);
          onGainChange(posToGain(e.clientY));
        }}
        onDoubleClick={(e) => {
          if (disabled) return;
          e.stopPropagation();
          onGainChange(0); // Reset gain to 0dB
        }}
      >
        {/* Fill from center (0dB) */}
        {band.gain_db >= 0 ? (
          <div style={{
            position: "absolute", left: 2, right: 2, borderRadius: 2,
            bottom: "50%",
            height: `${(band.gain_db / 24) * 50}%`,
            background: color, opacity: 0.5,
          }} />
        ) : (
          <div style={{
            position: "absolute", left: 2, right: 2, borderRadius: 2,
            top: "50%",
            height: `${(-band.gain_db / 24) * 50}%`,
            background: color, opacity: 0.5,
          }} />
        )}
        {/* Center line (0dB) */}
        <div style={{ position: "absolute", left: 3, right: 3, top: "50%", height: 1, background: "var(--border-strong)" }} />
        {/* Thumb */}
        <div style={{
          position: "absolute", left: -2, right: -2, height: 8, borderRadius: 3,
          bottom: `calc(${ratio * 100}% - 4px)`,
          background: "#d4d4dc",
          boxShadow: "0 1px 3px rgba(0,0,0,0.5)",
          cursor: dragging ? "grabbing" : "grab",
        }} />
      </div>

      {/* Value readouts */}
      <div className="flex flex-col items-center gap-0.5 mt-1.5">
        <span className="text-[10px] font-medium tabular-nums" style={{ fontFamily: "var(--font-mono)", color }}>
          {band.gain_db >= 0 ? "+" : ""}{band.gain_db.toFixed(1)}
        </span>
        <span className="text-[9px] tabular-nums" style={{ fontFamily: "var(--font-mono)", color: "var(--text-muted)" }}>
          {band.frequency >= 1000 ? `${(band.frequency / 1000).toFixed(1)}k` : `${Math.round(band.frequency)}`}Hz
        </span>
        <span className="text-[9px] tabular-nums" style={{ fontFamily: "var(--font-mono)", color: "var(--text-muted)" }}>
          Q {band.q.toFixed(1)}
        </span>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// DSP Group — card wrapping a set of parameters
// ---------------------------------------------------------------------------

function DspGroup({ title, enabled, onToggle, children }: {
  title: string;
  enabled: boolean;
  onToggle: (v: boolean) => void;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2 p-3 rounded-lg flex-1 min-w-[200px]" style={{ background: "var(--bg-surface-1)", border: "1px solid var(--border-default)" }}>
      <div className="flex items-center gap-2">
        <label className="flex items-center gap-1.5 text-[11px] font-semibold cursor-pointer" style={{ color: "var(--text-secondary)" }}>
          <input type="checkbox" checked={enabled} onChange={(e) => onToggle(e.target.checked)} />
          {title}
        </label>
      </div>
      <div className="flex flex-wrap gap-x-4 gap-y-1.5" style={{ opacity: enabled ? 1 : 0.4 }}>
        {children}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Parameter — label + horizontal slider + value
// ---------------------------------------------------------------------------

function Param({ label, value, suffix, min, max, step, defaultValue, disabled, onChange }: {
  label: string;
  value: number;
  suffix: string;
  min: number;
  max: number;
  step: number;
  defaultValue?: number;
  disabled?: boolean;
  onChange: (v: number) => void;
}) {
  return (
    <div className="flex items-center gap-2 min-w-[180px]" style={{ opacity: disabled ? 0.4 : 1 }}>
      <span className="w-16 text-[10px] text-right shrink-0" style={{ color: "var(--text-muted)" }}>{label}</span>
      <input
        type="range" min={min} max={max} step={step} value={value} disabled={disabled}
        className="flex-1 h-1"
        style={{ accentColor: "var(--accent-info)" }}
        onChange={(e) => onChange(parseFloat(e.target.value))}
        onDoubleClick={() => { if (!disabled && defaultValue !== undefined) onChange(defaultValue); }}
      />
      <span className="w-14 text-[10px] text-right shrink-0 tabular-nums" style={{ fontFamily: "var(--font-mono)", color: "var(--text-secondary)" }}>
        {Math.round(value * 10) / 10} {suffix}
      </span>
    </div>
  );
}
