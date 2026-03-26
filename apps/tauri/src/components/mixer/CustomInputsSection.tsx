import { useCallback, useEffect, useRef, useState } from "react";
import { useMixerStore } from "../../lib/stores/mixer-store";
import { SystemApi } from "../../lib/api";

const CUSTOM_TYPES = ["brightness", "media", "scroll", "generic"];

const TYPE_COLORS: Record<string, string> = {
  brightness: "#f59e0b",
  media: "#8b5cf6",
  scroll: "#3b82f6",
  generic: "#6b7280",
};

export default function CustomInputsSection() {
  const customInputs = useMixerStore((s) => s.customInputs);
  const refreshCustomInputs = useMixerStore((s) => s.refreshCustomInputs);
  const [collapsed, setCollapsed] = useState(false);
  const [showForm, setShowForm] = useState(false);

  return (
    <div
      className="shrink-0"
      style={{
        background: "var(--bg-surface-0)",
        borderTop: "1px solid var(--border-default)",
      }}
    >
      {/* Section header */}
      <div
        className="flex items-center gap-2 px-4 py-1.5 cursor-pointer select-none"
        onClick={() => setCollapsed((v) => !v)}
        style={{ borderBottom: collapsed ? "none" : "1px solid var(--border-subtle)" }}
      >
        <svg
          width="10"
          height="10"
          viewBox="0 0 16 16"
          fill="none"
          stroke="var(--text-muted)"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          style={{
            transform: collapsed ? "rotate(-90deg)" : "rotate(0deg)",
            transition: "transform 120ms",
          }}
        >
          <polyline points="4 6 8 10 12 6" />
        </svg>
        <span
          className="text-[11px] font-semibold uppercase tracking-wide"
          style={{ color: "var(--text-muted)", letterSpacing: "0.04em" }}
        >
          Custom Inputs ({customInputs.length})
        </span>
        <div className="flex-1" />
        {!collapsed && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              setShowForm((v) => !v);
            }}
            className="px-2 py-0.5 rounded text-[10px] cursor-pointer transition-opacity hover:opacity-80"
            style={{
              background: "var(--accent-primary)",
              color: "var(--text-on-accent)",
              border: "none",
            }}
          >
            + Add
          </button>
        )}
      </div>

      {/* Content */}
      {!collapsed && (
        <div className="px-4 py-2 flex flex-col gap-1.5">
          {/* Add form */}
          {showForm && (
            <AddCustomInputForm
              onClose={() => setShowForm(false)}
              onAdded={() => {
                refreshCustomInputs();
                setShowForm(false);
              }}
            />
          )}

          {/* Custom input rows */}
          {customInputs.length === 0 && !showForm && (
            <span
              className="text-[11px] text-center py-1"
              style={{ color: "var(--text-disabled)" }}
            >
              No custom inputs
            </span>
          )}
          {customInputs.map((ci) => (
            <CustomInputRow
              key={ci.id}
              id={ci.id}
              name={ci.name}
              color={ci.color}
              customType={ci.custom_type}
              value={ci.value}
              onRemoved={refreshCustomInputs}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Custom Input Row
// ---------------------------------------------------------------------------

function CustomInputRow({
  id,
  name,
  color,
  customType,
  value,
  onRemoved,
}: {
  id: number;
  name: string;
  color: string;
  customType: string;
  value: number;
  onRemoved: () => void;
}) {
  const [localValue, setLocalValue] = useState(value);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Sync external value changes
  useEffect(() => {
    setLocalValue(value);
  }, [value]);

  const handleValueChange = useCallback(
    (newValue: number) => {
      setLocalValue(newValue);
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        SystemApi.setCustomInputValue(id, newValue).catch(console.error);
      }, 50);
    },
    [id],
  );

  const handleRemove = useCallback(async () => {
    await SystemApi.removeCustomInput(id);
    onRemoved();
  }, [id, onRemoved]);

  const typeColor = TYPE_COLORS[customType] ?? TYPE_COLORS.generic;

  return (
    <div
      className="flex items-center gap-2 px-3 py-1.5 rounded-md text-[11px]"
      style={{
        background: "var(--bg-surface-1)",
        border: "1px solid var(--border-subtle)",
      }}
    >
      {/* Color dot + name */}
      <span
        className="w-2 h-2 rounded-full shrink-0"
        style={{ background: color }}
      />
      <span className="truncate min-w-0 font-medium" style={{ maxWidth: 100 }}>
        {name}
      </span>

      {/* Type badge */}
      <span
        className="px-1.5 py-0.5 rounded text-[9px] font-semibold uppercase shrink-0"
        style={{
          background: `${typeColor}20`,
          color: typeColor,
          letterSpacing: "0.03em",
        }}
      >
        {customType}
      </span>

      {/* Value slider */}
      <input
        type="range"
        min={0}
        max={255}
        value={localValue}
        onChange={(e) => handleValueChange(Number(e.target.value))}
        className="flex-1 h-1 cursor-pointer"
        style={{ accentColor: color, minWidth: 60 }}
      />
      <span
        className="w-8 text-right tabular-nums shrink-0"
        style={{ color: "var(--text-muted)", fontSize: 10 }}
      >
        {localValue}
      </span>

      {/* Remove button */}
      <button
        onClick={handleRemove}
        className="w-4 h-4 flex items-center justify-center rounded-full opacity-40 hover:opacity-100 transition-opacity cursor-pointer shrink-0"
        style={{
          color: "var(--accent-danger)",
          fontSize: 12,
          background: "none",
          border: "none",
        }}
        title="Remove custom input"
      >
        {"\u00D7"}
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Add Custom Input Form
// ---------------------------------------------------------------------------

function AddCustomInputForm({
  onClose,
  onAdded,
}: {
  onClose: () => void;
  onAdded: () => void;
}) {
  const [name, setName] = useState("");
  const [color, setColor] = useState("#4A90D9");
  const [customType, setCustomType] = useState(CUSTOM_TYPES[0]);
  const formRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (formRef.current && !formRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleEscape);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [onClose]);

  const handleSubmit = useCallback(async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    await SystemApi.addCustomInput(trimmed, color, customType, "{}");
    onAdded();
  }, [name, color, customType, onAdded]);

  return (
    <div
      ref={formRef}
      className="flex items-center gap-2 px-3 py-2 rounded-md text-[11px]"
      style={{
        background: "var(--bg-surface-2)",
        border: "1px solid var(--border-default)",
      }}
    >
      <input
        type="text"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") handleSubmit();
        }}
        placeholder="Name"
        autoFocus
        className="w-24 px-1.5 py-0.5 rounded text-[11px] outline-none"
        style={{
          background: "var(--bg-surface-0)",
          border: "1px solid var(--border-default)",
          color: "var(--text-primary)",
        }}
      />
      <input
        type="color"
        value={color}
        onChange={(e) => setColor(e.target.value)}
        className="w-6 h-5 rounded cursor-pointer border-0 p-0"
        title="Color"
      />
      <select
        value={customType}
        onChange={(e) => setCustomType(e.target.value)}
        className="px-1 py-0.5 rounded text-[11px] outline-none cursor-pointer"
        style={{
          background: "var(--bg-surface-0)",
          border: "1px solid var(--border-default)",
          color: "var(--text-primary)",
        }}
      >
        {CUSTOM_TYPES.map((t) => (
          <option key={t} value={t}>
            {t}
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
      <button
        onClick={onClose}
        className="px-1.5 py-0.5 rounded text-[11px] cursor-pointer"
        style={{
          background: "none",
          border: "1px solid var(--border-default)",
          color: "var(--text-muted)",
        }}
      >
        Cancel
      </button>
    </div>
  );
}
