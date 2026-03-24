import { useCallback, useEffect, useState } from "react";
import { SystemApi } from "../lib/api";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type SimpleAction =
  | "toggle_route_mute"
  | "toggle_global_mute"
  | "mute_all_outputs"
  | "toggle_eq"
  | "toggle_gate"
  | "toggle_deesser"
  | "toggle_compressor"
  | "toggle_limiter"
  | "push_to_mute"
  | "push_to_talk"
  | "next_output"
  | "prev_output"
  | "page_left"
  | "page_right"
  | "none";

type ButtonAction =
  | SimpleAction
  | { mute_output: { output_id: number } }
  | { load_profile: { name: string } };

interface ButtonMapping {
  press: ButtonAction;
  hold: ButtonAction;
}

interface ButtonMappings {
  dial1: ButtonMapping;
  dial2: ButtonMapping;
  dial3: ButtonMapping;
  dial4: ButtonMapping;
  audience1: ButtonMapping;
  audience2: ButtonMapping;
  audience3: ButtonMapping;
  audience4: ButtonMapping;
  mix: ButtonMapping;
  page_left: ButtonMapping;
  page_right: ButtonMapping;
}

interface BeacnConfig {
  layout: string;
  dial_sensitivity: number;
  level_decay: number;
  button_mappings?: ButtonMappings;
  hold_threshold_ms?: number;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const BUTTONS: { key: keyof ButtonMappings; label: string }[] = [
  { key: "dial1", label: "Dial 1" },
  { key: "dial2", label: "Dial 2" },
  { key: "dial3", label: "Dial 3" },
  { key: "dial4", label: "Dial 4" },
  { key: "audience1", label: "Audience 1" },
  { key: "audience2", label: "Audience 2" },
  { key: "audience3", label: "Audience 3" },
  { key: "audience4", label: "Audience 4" },
  { key: "mix", label: "Mix" },
  { key: "page_left", label: "Page Left" },
  { key: "page_right", label: "Page Right" },
];

const SIMPLE_ACTIONS: { value: SimpleAction; label: string }[] = [
  { value: "toggle_route_mute", label: "Mute Route" },
  { value: "toggle_global_mute", label: "Mute Global" },
  { value: "mute_all_outputs", label: "Mute All Outputs" },
  { value: "toggle_eq", label: "Toggle EQ" },
  { value: "toggle_gate", label: "Toggle Gate" },
  { value: "toggle_deesser", label: "Toggle De-esser" },
  { value: "toggle_compressor", label: "Toggle Compressor" },
  { value: "toggle_limiter", label: "Toggle Limiter" },
  { value: "push_to_mute", label: "Push to Mute" },
  { value: "push_to_talk", label: "Push to Talk" },
  { value: "next_output", label: "Next Output" },
  { value: "prev_output", label: "Prev Output" },
  { value: "page_left", label: "Page Left" },
  { value: "page_right", label: "Page Right" },
  { value: "none", label: "None" },
];

const PARAMETERIZED_ACTIONS = [
  { value: "mute_output", label: "Mute Output" },
  { value: "load_profile", label: "Load Profile" },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function actionToSelectValue(action: ButtonAction): string {
  if (typeof action === "string") return action;
  if ("mute_output" in action) return "mute_output";
  if ("load_profile" in action) return "load_profile";
  return "none";
}

function actionParam(action: ButtonAction): string {
  if (typeof action !== "object") return "";
  if ("mute_output" in action) return String(action.mute_output.output_id);
  if ("load_profile" in action) return action.load_profile.name;
  return "";
}

function buildAction(selectValue: string, param: string): ButtonAction {
  if (selectValue === "mute_output") {
    return { mute_output: { output_id: parseInt(param) || 0 } };
  }
  if (selectValue === "load_profile") {
    return { load_profile: { name: param } };
  }
  return selectValue as SimpleAction;
}

const DEFAULT_MAPPING: ButtonMapping = { press: "none", hold: "none" };

function defaultMappings(): ButtonMappings {
  return {
    dial1: { ...DEFAULT_MAPPING },
    dial2: { ...DEFAULT_MAPPING },
    dial3: { ...DEFAULT_MAPPING },
    dial4: { ...DEFAULT_MAPPING },
    audience1: { ...DEFAULT_MAPPING },
    audience2: { ...DEFAULT_MAPPING },
    audience3: { ...DEFAULT_MAPPING },
    audience4: { ...DEFAULT_MAPPING },
    mix: { ...DEFAULT_MAPPING },
    page_left: { ...DEFAULT_MAPPING },
    page_right: { ...DEFAULT_MAPPING },
  };
}

// ---------------------------------------------------------------------------
// Shared styles
// ---------------------------------------------------------------------------

const selectClass =
  "bg-[#ffffff10] border border-[#333] rounded px-2 py-1 text-xs";
const inputClass =
  "bg-[#ffffff10] border border-[#333] rounded px-2 py-1 text-xs";

// ---------------------------------------------------------------------------
// Sub-component: action dropdown + optional param input
// ---------------------------------------------------------------------------

function ActionCell({
  action,
  onChange,
}: {
  action: ButtonAction;
  onChange: (a: ButtonAction) => void;
}) {
  const selected = actionToSelectValue(action);
  const param = actionParam(action);
  const isParameterized = selected === "mute_output" || selected === "load_profile";

  return (
    <div className="flex items-center gap-1">
      <select
        className={selectClass + " flex-1 min-w-0"}
        value={selected}
        onChange={(e) => {
          const v = e.target.value;
          if (v === "mute_output" || v === "load_profile") {
            onChange(buildAction(v, ""));
          } else {
            onChange(v as SimpleAction);
          }
        }}
      >
        {SIMPLE_ACTIONS.map((a) => (
          <option key={a.value} value={a.value}>
            {a.label}
          </option>
        ))}
        {PARAMETERIZED_ACTIONS.map((a) => (
          <option key={a.value} value={a.value}>
            {a.label}
          </option>
        ))}
      </select>

      {isParameterized && (
        <input
          type={selected === "mute_output" ? "number" : "text"}
          placeholder={selected === "mute_output" ? "Output ID" : "Profile name"}
          className={inputClass + " w-24"}
          value={param}
          onChange={(e) => onChange(buildAction(selected, e.target.value))}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function BeacnPage() {
  const [config, setConfig] = useState<BeacnConfig>({
    layout: "column",
    dial_sensitivity: 5,
    level_decay: 0.5,
    button_mappings: defaultMappings(),
    hold_threshold_ms: 500,
  });

  const fetchConfig = useCallback(async () => {
    try {
      const json = await SystemApi.getBeacnConfig();
      const parsed = JSON.parse(json) as BeacnConfig;
      if (!parsed.button_mappings) parsed.button_mappings = defaultMappings();
      if (parsed.hold_threshold_ms == null) parsed.hold_threshold_ms = 500;
      setConfig(parsed);
    } catch {
      // use defaults
    }
  }, []);

  useEffect(() => {
    fetchConfig();
  }, [fetchConfig]);

  const applyConfig = useCallback(
    async (next: BeacnConfig) => {
      setConfig(next);
      await SystemApi.setBeacnConfig(JSON.stringify(next));
      await fetchConfig();
    },
    [fetchConfig]
  );

  const handleApply = useCallback(() => applyConfig(config), [applyConfig, config]);

  const updateMapping = useCallback(
    (
      buttonKey: keyof ButtonMappings,
      slot: "press" | "hold",
      action: ButtonAction
    ) => {
      const next = { ...config };
      const mappings = { ...(next.button_mappings ?? defaultMappings()) };
      mappings[buttonKey] = { ...mappings[buttonKey], [slot]: action };
      next.button_mappings = mappings;
      applyConfig(next);
    },
    [config, applyConfig]
  );

  const mappings = config.button_mappings ?? defaultMappings();

  return (
    <div className="flex flex-col p-4 gap-3 h-screen overflow-y-auto">
      {/* ---- General settings ---- */}
      <div className="flex items-center gap-2">
        <label className="w-30 text-xs">Layout:</label>
        <select
          className={"flex-1 " + selectClass}
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
          className={"flex-1 " + inputClass}
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
          className={"flex-1 " + inputClass}
          value={Math.round(config.level_decay * 100)}
          onChange={(e) =>
            setConfig({
              ...config,
              level_decay: (parseInt(e.target.value) || 0) / 100,
            })
          }
        />
      </div>

      {/* ---- Button mappings ---- */}
      <h2 className="text-xs font-semibold mt-3 text-neutral-400 uppercase tracking-wide">
        Button Mappings
      </h2>

      <table className="w-full text-xs border-collapse">
        <thead>
          <tr className="text-neutral-500 text-left">
            <th className="py-1 pr-2 font-medium">Button</th>
            <th className="py-1 pr-2 font-medium">Press</th>
            <th className="py-1 font-medium">Hold</th>
          </tr>
        </thead>
        <tbody>
          {BUTTONS.map(({ key, label }) => (
            <tr key={key} className="border-t border-[#333]">
              <td className="py-1.5 pr-2 whitespace-nowrap">{label}</td>
              <td className="py-1.5 pr-2">
                <ActionCell
                  action={mappings[key]?.press ?? "none"}
                  onChange={(a) => updateMapping(key, "press", a)}
                />
              </td>
              <td className="py-1.5">
                <ActionCell
                  action={mappings[key]?.hold ?? "none"}
                  onChange={(a) => updateMapping(key, "hold", a)}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      {/* ---- Hold threshold ---- */}
      <div className="flex items-center gap-2 mt-2">
        <label className="w-30 text-xs">Hold Threshold (ms):</label>
        <input
          type="number"
          min={100}
          max={2000}
          step={50}
          className={"flex-1 " + inputClass}
          value={config.hold_threshold_ms ?? 500}
          onChange={(e) =>
            setConfig({
              ...config,
              hold_threshold_ms: parseInt(e.target.value) || 500,
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
