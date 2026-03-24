import { useCallback, useEffect, useState } from "react";
import { MixerApi, SystemApi } from "../lib/api";
import type { AppRuleInfo, InputInfo } from "../lib/types";

export default function RulesPage() {
  const [rules, setRules] = useState<AppRuleInfo[]>([]);
  const [inputs, setInputs] = useState<InputInfo[]>([]);
  const [newAppName, setNewAppName] = useState("");
  const [newInputIdx, setNewInputIdx] = useState(0);

  const refresh = useCallback(async () => {
    const [r, i] = await Promise.all([
      SystemApi.listAppRules(),
      MixerApi.listInputs(),
    ]);
    setRules(r.filter((r) => !r.app_name.includes("mixctl.") && !r.app_name.startsWith("output.")));
    setInputs(i);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const handleAdd = useCallback(async () => {
    if (!newAppName.trim() || !inputs[newInputIdx]) return;
    await SystemApi.setAppRule(newAppName.trim(), inputs[newInputIdx].id);
    setNewAppName("");
    refresh();
  }, [newAppName, newInputIdx, inputs, refresh]);

  const handleRemove = useCallback(
    async (appName: string) => {
      await SystemApi.removeAppRule(appName);
      refresh();
    },
    [refresh]
  );

  return (
    <div className="flex flex-col p-3 gap-2 h-screen">
      <div className="flex-1 overflow-y-auto flex flex-col gap-1">
        {rules.length === 0 && (
          <span className="text-[var(--text-muted)] text-xs text-center py-4">
            No rules configured
          </span>
        )}
        {rules.map((rule, idx) => {
          const input = inputs.find((i) => i.id === rule.input_id);
          return (
            <div
              key={rule.app_name}
              className={`flex items-center gap-2 h-10 rounded px-2 ${
                idx % 2 === 0 ? "bg-[#ffffff08]" : ""
              }`}
            >
              <span className="flex-1 text-xs truncate">{rule.app_name}</span>
              <span className="text-xs text-[var(--text-secondary)]">
                {"\u25B8"} {input?.name ?? "?"}
              </span>
              <button
                onClick={() => handleRemove(rule.app_name)}
                className="w-6 h-6 text-[10px] rounded bg-[#ffffff10] hover:bg-[var(--accent-red)] transition-colors cursor-pointer"
              >
                {"\u2715"}
              </button>
            </div>
          );
        })}
      </div>

      <div className="flex gap-1">
        <input
          className="flex-1 bg-[#ffffff10] border border-[#333] rounded px-2 py-1 text-xs outline-none"
          placeholder="App name"
          value={newAppName}
          onChange={(e) => setNewAppName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAdd()}
        />
        <select
          className="bg-[#ffffff10] border border-[#333] rounded px-1 py-1 text-xs"
          value={newInputIdx}
          onChange={(e) => setNewInputIdx(parseInt(e.target.value))}
        >
          {inputs.map((inp, i) => (
            <option key={inp.id} value={i}>
              {inp.name}
            </option>
          ))}
        </select>
        <button
          disabled={!newAppName.trim()}
          onClick={handleAdd}
          className="px-3 py-1 text-xs rounded bg-[var(--accent-blue)] text-white disabled:opacity-40 cursor-pointer"
        >
          Add
        </button>
      </div>
    </div>
  );
}
