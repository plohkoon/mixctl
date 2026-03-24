import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { SystemApi } from "../../lib/api";

export default function AddChannelMenu() {
  const [open, setOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (
        dropdownRef.current && !dropdownRef.current.contains(e.target as Node) &&
        buttonRef.current && !buttonRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    };
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleEscape);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [open]);

  const handleAddInput = useCallback(() => {
    SystemApi.openChannelEditor("input");
    setOpen(false);
  }, []);

  const handleAddOutput = useCallback(() => {
    SystemApi.openChannelEditor("output");
    setOpen(false);
  }, []);

  const rect = buttonRef.current?.getBoundingClientRect();

  return (
    <div className="h-full flex items-center justify-center">
      <button
        ref={buttonRef}
        onClick={() => setOpen((v) => !v)}
        className="w-7 h-7 flex items-center justify-center rounded text-sm font-bold cursor-pointer transition-all duration-[120ms]"
        style={{
          background: "var(--bg-surface-2)",
          border: "1px solid var(--border-default)",
          color: "var(--text-secondary)",
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.background = "var(--bg-surface-3)";
          e.currentTarget.style.color = "var(--text-primary)";
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.background = "var(--bg-surface-2)";
          e.currentTarget.style.color = "var(--text-secondary)";
        }}
        title="Add channel"
      >
        +
      </button>

      {open && rect && createPortal(
        <div
          ref={dropdownRef}
          className="fixed z-[9999] flex flex-col rounded-md overflow-hidden"
          style={{
            top: rect.bottom + 4,
            left: rect.left,
            background: "var(--bg-surface-3)",
            border: "1px solid var(--border-strong)",
            boxShadow: "0 4px 16px rgba(0,0,0,0.5)",
            minWidth: 130,
          }}
        >
          <button
            onClick={handleAddInput}
            className="px-3 py-2 text-[12px] text-left font-medium cursor-pointer"
            style={{ color: "var(--text-secondary)" }}
            onMouseEnter={(e) => { e.currentTarget.style.background = "var(--bg-surface-2)"; e.currentTarget.style.color = "var(--text-primary)"; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-secondary)"; }}
          >
            Add Input
          </button>
          <button
            onClick={handleAddOutput}
            className="px-3 py-2 text-[12px] text-left font-medium cursor-pointer"
            style={{ color: "var(--text-secondary)" }}
            onMouseEnter={(e) => { e.currentTarget.style.background = "var(--bg-surface-2)"; e.currentTarget.style.color = "var(--text-primary)"; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-secondary)"; }}
          >
            Add Output
          </button>
        </div>,
        document.body
      )}
    </div>
  );
}
