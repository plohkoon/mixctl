import { useCallback, useRef, useState, useEffect } from "react";

interface VolumeFaderProps {
  value: number;
  min?: number;
  max?: number;
  color?: string;
  orientation?: "vertical" | "horizontal";
  onChange: (value: number) => void;
  onChangeEnd?: (value: number) => void;
}

export default function VolumeFader({
  value,
  min = 0,
  max = 100,
  color = "var(--accent-primary)",
  orientation = "horizontal",
  onChange,
  onChangeEnd,
}: VolumeFaderProps) {
  const trackRef = useRef<HTMLDivElement>(null);
  const [isDragging, setIsDragging] = useState(false);
  const isVertical = orientation === "vertical";

  const clamp = useCallback(
    (v: number) => Math.max(min, Math.min(max, Math.round(v))),
    [min, max]
  );
  const ratio = (value - min) / (max - min);

  const posToValue = useCallback(
    (clientX: number, clientY: number) => {
      const track = trackRef.current;
      if (!track) return value;
      const rect = track.getBoundingClientRect();
      if (isVertical) {
        const r = 1 - (clientY - rect.top) / rect.height;
        return clamp(min + r * (max - min));
      } else {
        const r = (clientX - rect.left) / rect.width;
        return clamp(min + r * (max - min));
      }
    },
    [min, max, value, isVertical, clamp]
  );

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setIsDragging(true);
      onChange(posToValue(e.clientX, e.clientY));
    },
    [posToValue, onChange]
  );

  useEffect(() => {
    if (!isDragging) return;
    const handleMouseMove = (e: MouseEvent) => onChange(posToValue(e.clientX, e.clientY));
    const handleMouseUp = (e: MouseEvent) => {
      setIsDragging(false);
      onChangeEnd?.(posToValue(e.clientX, e.clientY));
    };
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isDragging, posToValue, onChange, onChangeEnd]);

  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      e.preventDefault();
      const step = e.shiftKey ? 1 : 5;
      const newVal = clamp(value + (e.deltaY < 0 ? step : -step));
      onChange(newVal);
      onChangeEnd?.(newVal);
    },
    [value, onChange, onChangeEnd, clamp]
  );

  const handleDoubleClick = useCallback(() => {
    onChange(100);
    onChangeEnd?.(100);
  }, [onChange, onChangeEnd]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      let newVal = value;
      switch (e.key) {
        case "ArrowUp": case "ArrowRight": newVal = clamp(value + 1); break;
        case "ArrowDown": case "ArrowLeft": newVal = clamp(value - 1); break;
        case "PageUp": newVal = clamp(value + 10); break;
        case "PageDown": newVal = clamp(value - 10); break;
        case "Home": newVal = max; break;
        case "End": newVal = min; break;
        default: return;
      }
      e.preventDefault();
      onChange(newVal);
      onChangeEnd?.(newVal);
    },
    [value, onChange, onChangeEnd, min, max, clamp]
  );

  // Horizontal (default for matrix cells and applet)
  return (
    <div
      className="relative select-none outline-none"
      style={{
        height: isVertical ? "200px" : "14px",
        width: isVertical ? "28px" : undefined,
        flex: isVertical ? undefined : 1,
      }}
      tabIndex={0}
      onWheel={handleWheel}
      onDoubleClick={handleDoubleClick}
      onKeyDown={handleKeyDown}
      role="slider"
      aria-valuenow={value}
      aria-valuemin={min}
      aria-valuemax={max}
    >
      <div
        ref={trackRef}
        className="absolute inset-0 rounded"
        style={{
          background: "var(--bg-surface-2)",
          border: "1px solid var(--border-subtle)",
          cursor: isDragging ? "grabbing" : "pointer",
        }}
        onMouseDown={handleMouseDown}
      >
        {/* Fill */}
        <div
          className="absolute rounded-l"
          style={
            isVertical
              ? {
                  bottom: 0,
                  left: 0,
                  right: 0,
                  height: `${ratio * 100}%`,
                  background: color,
                  opacity: 0.45,
                  borderRadius: "0 0 3px 3px",
                }
              : {
                  top: 0,
                  left: 0,
                  bottom: 0,
                  width: `${ratio * 100}%`,
                  background: color,
                  opacity: 0.45,
                }
          }
        />
        {/* Thumb */}
        <div
          className="absolute"
          style={
            isVertical
              ? {
                  left: -2,
                  right: -2,
                  height: 12,
                  bottom: `calc(${ratio * 100}% - 6px)`,
                  background: "#d4d4dc",
                  borderRadius: 3,
                  boxShadow:
                    "0 1px 3px rgba(0,0,0,0.5), inset 0 1px 0 rgba(255,255,255,0.15)",
                  cursor: isDragging ? "grabbing" : "grab",
                }
              : {
                  top: -1,
                  bottom: -1,
                  width: 8,
                  left: `calc(${ratio * 100}% - 4px)`,
                  background: "#d4d4dc",
                  borderRadius: 2,
                  boxShadow:
                    "0 1px 3px rgba(0,0,0,0.5), inset 0 1px 0 rgba(255,255,255,0.15)",
                  cursor: isDragging ? "grabbing" : "grab",
                }
          }
        />
      </div>
    </div>
  );
}
