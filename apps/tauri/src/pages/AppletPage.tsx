import { useEffect } from "react";
import { useMixerStore } from "../lib/stores/mixer-store";
import { MixerApi } from "../lib/api";
import { getCurrentWindow } from "@tauri-apps/api/window";
import VolumeFader from "../components/mixer/VolumeFader";

export default function AppletPage() {
  const initialize = useMixerStore((s) => s.initialize);
  const daemonConnected = useMixerStore((s) => s.daemonConnected);
  const outputs = useMixerStore((s) => s.outputs);
  const routes = useMixerStore((s) => s.routes);
  const inputs = useMixerStore((s) => s.inputs);
  const selectedOutputId = useMixerStore((s) => s.selectedOutputId);
  const selectOutput = useMixerStore((s) => s.selectOutput);

  useEffect(() => {
    initialize();

    // Hide on blur (clicking outside the popup)
    const handleBlur = () => {
      getCurrentWindow().hide();
    };
    window.addEventListener("blur", handleBlur);
    return () => window.removeEventListener("blur", handleBlur);
  }, [initialize]);

  const selectedOutput = outputs.find((o) => o.id === selectedOutputId);

  return (
    <div className="flex flex-col p-2 gap-1 text-[12px] h-screen overflow-y-auto">
      {/* Output Levels */}
      <span className="font-bold text-[13px]">Output Levels</span>
      {outputs.map((out) => (
        <div key={out.id} className="flex items-center gap-1.5 h-9">
          <span
            className="w-3 h-3 rounded-full shrink-0"
            style={{ background: out.color }}
          />
          <span className="w-20 truncate text-[12px]">{out.name}</span>
          <VolumeFader
            value={out.volume}
            color={out.color}
            orientation="horizontal"
            onChange={(v) => MixerApi.setOutputVolume(out.id, v)}
          />
          <span className="w-9 text-right text-[11px] text-[var(--text-secondary)]">
            {out.volume}%
          </span>
          <button
            onClick={() => MixerApi.setOutputMute(out.id, !out.muted)}
            className={`w-13 h-6 rounded text-[10px] cursor-pointer ${
              out.muted
                ? "bg-[var(--accent-red)] text-white"
                : "bg-[var(--accent-blue)] text-white"
            }`}
          >
            {out.muted ? "Muted" : "Mute"}
          </button>
          <button
            onClick={() => selectOutput(out.id)}
            className={`w-6 h-6 rounded text-[12px] cursor-pointer ${
              out.id === selectedOutputId
                ? "bg-[var(--accent-blue)] text-white"
                : "bg-[#555] text-[var(--text-secondary)]"
            }`}
          >
            {"\u2192"}
          </button>
        </div>
      ))}

      {/* Separator */}
      <div className="h-px bg-[var(--border-subtle)] my-0.5" />

      {/* Route mix for selected output */}
      <span className="font-bold text-[13px]">
        Input mix for: {selectedOutput?.name ?? "?"}
      </span>
      {routes.filter((r) => r.output_id === selectedOutputId).map((route) => {
        const input = inputs.find((i) => i.id === route.input_id);
        return (
          <div key={`${route.input_id}-${route.output_id}`} className="flex items-center gap-1.5 h-8">
            <span
              className="w-3 h-3 rounded-full shrink-0"
              style={{ background: input?.color ?? "#888" }}
            />
            <span className="w-16 truncate text-[12px]">{input?.name ?? "?"}</span>
            <VolumeFader
              value={route.volume}
              color={input?.color ?? "#888"}
              orientation="horizontal"
              onChange={(v) =>
                MixerApi.setRouteVolume(route.input_id, route.output_id, v)
              }
            />
            <span className="w-9 text-right text-[11px] text-[var(--text-secondary)]">
              {route.volume}%
            </span>
            <button
              onClick={() =>
                MixerApi.setRouteMute(
                  route.input_id,
                  route.output_id,
                  !route.muted
                )
              }
              className={`w-13 h-6 rounded text-[10px] cursor-pointer ${
                route.muted
                  ? "bg-[var(--accent-red)] text-white"
                  : "bg-[var(--accent-blue)] text-white"
              }`}
            >
              {route.muted ? "Muted" : "Mute"}
            </button>
          </div>
        );
      })}

      {/* Open Mixer button */}
      <button
        onClick={async () => {
          const { getAllWindows } = await import("@tauri-apps/api/window");
          const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
          const allWins = await getAllWindows();
          const mainWin = allWins.find((w) => w.label === "main");
          if (mainWin) {
            await mainWin.show();
            await mainWin.setFocus();
          } else {
            const win = new WebviewWindow("main", {
              url: "/",
              title: "MixCtl",
              width: 750,
              height: 450,
            });
            await win.once("tauri://created", () => win.setFocus());
          }
          getCurrentWindow().hide();
        }}
        className="w-full h-8 rounded bg-[var(--accent-blue)] text-white text-[12px] font-medium mt-1 cursor-pointer hover:brightness-110"
      >
        Open Mixer
      </button>

      {/* Disconnected overlay */}
      {!daemonConnected && (
        <div className="absolute inset-0 bg-[#2c3e50ee] flex items-center justify-center">
          <span className="text-[#bbb] text-sm">Waiting for daemon...</span>
        </div>
      )}
    </div>
  );
}
