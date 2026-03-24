import { useCallback, useEffect, useState } from "react";
import { MixerApi, SystemApi } from "../lib/api";
import type { CaptureDeviceInfo, InputInfo } from "../lib/types";

export default function CapturePage() {
  const [devices, setDevices] = useState<CaptureDeviceInfo[]>([]);
  const [inputs, setInputs] = useState<InputInfo[]>([]);

  const refresh = useCallback(async () => {
    const [d, i] = await Promise.all([
      SystemApi.listCaptureDevices(),
      MixerApi.listInputs(),
    ]);
    setDevices(d);
    setInputs(i);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return (
    <div className="flex flex-col p-3 gap-2 h-screen overflow-y-auto">
      {devices.length === 0 && (
        <span className="text-[var(--text-muted)] text-xs text-center py-4">
          No capture devices found
        </span>
      )}
      {devices.map((dev, idx) => (
        <div
          key={dev.pw_node_id}
          className={`flex items-center gap-2 rounded px-2 py-2 ${
            idx % 2 === 0 ? "bg-[#ffffff08]" : ""
          }`}
        >
          <div className="flex-1">
            <div className="text-xs font-bold">{dev.name}</div>
            <div className="text-[11px] text-[var(--text-muted)]">
              {dev.device_name}
            </div>
          </div>
          {dev.is_added ? (
            <div className="flex items-center gap-2">
              <select
                className="text-[11px] rounded bg-[var(--bg-secondary)] text-[var(--text-primary)] border border-[var(--border)] px-1 py-0.5 cursor-pointer"
                defaultValue=""
                onChange={async (e) => {
                  const inputId = Number(e.target.value);
                  if (!inputId) return;
                  await SystemApi.bindCaptureToInput(inputId, dev.device_name);
                  e.target.value = "";
                  refresh();
                }}
              >
                <option value="" disabled>
                  Bind to Input...
                </option>
                {inputs.map((inp) => (
                  <option key={inp.id} value={inp.id}>
                    {inp.name}
                  </option>
                ))}
              </select>
              <button
                onClick={async () => {
                  await SystemApi.removeCaptureInput(dev.input_id);
                  refresh();
                }}
                className="px-2 py-1 text-[11px] rounded bg-[var(--accent-red,#D94A4A)] text-white cursor-pointer"
              >
                Remove
              </button>
            </div>
          ) : (
            <button
              onClick={async () => {
                await SystemApi.addCaptureInput(dev.pw_node_id, dev.name, "#4A90D9");
                refresh();
              }}
              className="px-2 py-1 text-[11px] rounded bg-[var(--accent-blue)] text-white cursor-pointer"
            >
              Add
            </button>
          )}
        </div>
      ))}
    </div>
  );
}
