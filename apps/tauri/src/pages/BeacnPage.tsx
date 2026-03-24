import { useCallback, useEffect, useState } from "react";
import { SystemApi } from "../lib/api";

interface BeacnConfig {
  layout: string;
  dial_sensitivity: number;
  level_decay: number;
}

export default function BeacnPage() {
  const [config, setConfig] = useState<BeacnConfig>({
    layout: "column",
    dial_sensitivity: 5,
    level_decay: 0.5,
  });

  useEffect(() => {
    SystemApi.getBeacnConfig().then((json) => {
      try {
        setConfig(JSON.parse(json));
      } catch {
        // use defaults
      }
    });
  }, []);

  const handleApply = useCallback(async () => {
    await SystemApi.setBeacnConfig(
      config.layout,
      config.dial_sensitivity,
      config.level_decay
    );
  }, [config]);

  return (
    <div className="flex flex-col p-4 gap-3 h-screen">
      <div className="flex items-center gap-2">
        <label className="w-30 text-xs">Layout:</label>
        <select
          className="flex-1 bg-[#ffffff10] border border-[#333] rounded px-2 py-1 text-xs"
          value={config.layout}
          onChange={(e) => setConfig({ ...config, layout: e.target.value })}
        >
          <option value="column">column</option>
          <option value="grid">grid</option>
          <option value="dial">dial</option>
        </select>
      </div>

      <div className="flex items-center gap-2">
        <label className="w-30 text-xs">Dial Sensitivity:</label>
        <input
          type="number"
          min={1}
          max={10}
          className="flex-1 bg-[#ffffff10] border border-[#333] rounded px-2 py-1 text-xs"
          value={config.dial_sensitivity}
          onChange={(e) =>
            setConfig({
              ...config,
              dial_sensitivity: parseInt(e.target.value) || 5,
            })
          }
        />
      </div>

      <div className="flex items-center gap-2">
        <label className="w-30 text-xs">Level Decay (%):</label>
        <input
          type="number"
          min={0}
          max={100}
          className="flex-1 bg-[#ffffff10] border border-[#333] rounded px-2 py-1 text-xs"
          value={Math.round(config.level_decay * 100)}
          onChange={(e) =>
            setConfig({
              ...config,
              level_decay: (parseInt(e.target.value) || 0) / 100,
            })
          }
        />
      </div>

      <div className="flex-1" />

      <div className="flex justify-end">
        <button
          onClick={handleApply}
          className="px-4 py-1.5 text-xs rounded bg-[var(--accent-blue)] text-white cursor-pointer"
        >
          Apply
        </button>
      </div>
    </div>
  );
}
