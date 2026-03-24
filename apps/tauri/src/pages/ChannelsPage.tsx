import { useCallback, useEffect, useRef, useState } from "react";
import { ChannelsApi, MixerApi } from "../lib/api";
import type { InputInfo, OutputInfo } from "../lib/types";

export default function ChannelsPage() {
  const [inputs, setInputs] = useState<InputInfo[]>([]);
  const [outputs, setOutputs] = useState<OutputInfo[]>([]);
  const [newInputName, setNewInputName] = useState("");
  const [newOutputName, setNewOutputName] = useState("");

  const refresh = useCallback(async () => {
    const [inp, out] = await Promise.all([
      MixerApi.listInputs(),
      MixerApi.listOutputs(),
    ]);
    setInputs(inp);
    setOutputs(out);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const dragSrcIdx = useRef<{ type: "input" | "output"; idx: number } | null>(null);
  const [dragOverTarget, setDragOverTarget] = useState<{ type: string; idx: number } | null>(null);

  return (
    <div className="flex flex-col p-3 gap-2 h-screen overflow-y-auto">
      {/* Inputs section */}
      <span className="font-bold text-sm">Inputs</span>
      <div className="flex flex-col gap-1">
        {inputs.map((inp, idx) => (
          <ChannelRow
            key={inp.id}
            id={inp.id}
            name={inp.name}
            color={inp.color}
            index={idx}
            total={inputs.length}
            dragOver={dragOverTarget?.type === "input" && dragOverTarget.idx === idx}
            onRename={async (name) => {
              await ChannelsApi.setInputName(inp.id, name);
              refresh();
            }}
            onMoveUp={async () => {
              if (idx > 0) {
                await ChannelsApi.moveInput(inp.id, idx - 1);
                refresh();
              }
            }}
            onMoveDown={async () => {
              if (idx < inputs.length - 1) {
                await ChannelsApi.moveInput(inp.id, idx + 1);
                refresh();
              }
            }}
            onRemove={async () => {
              await ChannelsApi.removeInput(inp.id);
              refresh();
            }}
            onDragStart={() => {
              dragSrcIdx.current = { type: "input", idx };
            }}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOverTarget({ type: "input", idx });
            }}
            onDragLeave={() => setDragOverTarget(null)}
            onDrop={async () => {
              if (dragSrcIdx.current?.type === "input" && dragSrcIdx.current.idx !== idx) {
                const src = inputs[dragSrcIdx.current.idx];
                if (src) {
                  await ChannelsApi.moveInput(src.id, idx);
                  refresh();
                }
              }
              dragSrcIdx.current = null;
              setDragOverTarget(null);
            }}
          />
        ))}
      </div>
      <div className="flex gap-1">
        <input
          className="flex-1 bg-[#ffffff10] border border-[#333] rounded px-2 py-1 text-xs outline-none"
          placeholder="New input name"
          value={newInputName}
          onChange={(e) => setNewInputName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && newInputName.trim()) {
              ChannelsApi.addInput(newInputName.trim(), "#4A90D9").then(() => {
                setNewInputName("");
                refresh();
              });
            }
          }}
        />
        <button
          disabled={!newInputName.trim()}
          onClick={async () => {
            await ChannelsApi.addInput(newInputName.trim(), "#4A90D9");
            setNewInputName("");
            refresh();
          }}
          className="px-3 py-1 text-xs rounded bg-[var(--accent-blue)] text-white disabled:opacity-40 cursor-pointer"
        >
          Add Input
        </button>
      </div>

      <div className="h-px bg-[var(--border-subtle)]" />

      {/* Outputs section */}
      <span className="font-bold text-sm">Outputs</span>
      <div className="flex flex-col gap-1">
        {outputs.map((out, idx) => (
          <ChannelRow
            key={out.id}
            id={out.id}
            name={out.name}
            color={out.color}
            index={idx}
            total={outputs.length}
            dragOver={dragOverTarget?.type === "output" && dragOverTarget.idx === idx}
            onRename={async (name) => {
              await ChannelsApi.setOutputName(out.id, name);
              refresh();
            }}
            onMoveUp={async () => {
              if (idx > 0) {
                await ChannelsApi.moveOutput(out.id, idx - 1);
                refresh();
              }
            }}
            onMoveDown={async () => {
              if (idx < outputs.length - 1) {
                await ChannelsApi.moveOutput(out.id, idx + 1);
                refresh();
              }
            }}
            onRemove={async () => {
              await ChannelsApi.removeOutput(out.id);
              refresh();
            }}
            onDragStart={() => {
              dragSrcIdx.current = { type: "output", idx };
            }}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOverTarget({ type: "output", idx });
            }}
            onDragLeave={() => setDragOverTarget(null)}
            onDrop={async () => {
              if (dragSrcIdx.current?.type === "output" && dragSrcIdx.current.idx !== idx) {
                const src = outputs[dragSrcIdx.current.idx];
                if (src) {
                  await ChannelsApi.moveOutput(src.id, idx);
                  refresh();
                }
              }
              dragSrcIdx.current = null;
              setDragOverTarget(null);
            }}
          />
        ))}
      </div>
      <div className="flex gap-1">
        <input
          className="flex-1 bg-[#ffffff10] border border-[#333] rounded px-2 py-1 text-xs outline-none"
          placeholder="New output name"
          value={newOutputName}
          onChange={(e) => setNewOutputName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && newOutputName.trim()) {
              ChannelsApi.addOutput(newOutputName.trim(), "#E74C3C", 0).then(() => {
                setNewOutputName("");
                refresh();
              });
            }
          }}
        />
        <button
          disabled={!newOutputName.trim()}
          onClick={async () => {
            await ChannelsApi.addOutput(newOutputName.trim(), "#E74C3C", 0);
            setNewOutputName("");
            refresh();
          }}
          className="px-3 py-1 text-xs rounded bg-[var(--accent-blue)] text-white disabled:opacity-40 cursor-pointer"
        >
          Add Output
        </button>
      </div>
    </div>
  );
}

interface ChannelRowProps {
  id: number;
  name: string;
  color: string;
  index: number;
  total: number;
  dragOver: boolean;
  onRename: (name: string) => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onRemove: () => void;
  onDragStart: () => void;
  onDragOver: (e: React.DragEvent) => void;
  onDragLeave: () => void;
  onDrop: () => void;
}

function ChannelRow({
  name,
  color,
  index,
  total,
  dragOver,
  onRename,
  onMoveUp,
  onMoveDown,
  onRemove,
  onDragStart,
  onDragOver,
  onDragLeave,
  onDrop,
}: ChannelRowProps) {
  const [editing, setEditing] = useState(false);
  const [editValue, setEditValue] = useState(name);

  return (
    <div
      draggable
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
    >
      {dragOver && <div className="h-0.5 bg-[var(--accent-blue)] rounded-full" />}
      <div className="flex items-center gap-1 h-9 rounded bg-[#ffffff10] px-2">
        <span
          className="w-3 h-3 rounded-full shrink-0"
          style={{ background: color }}
        />
        {editing ? (
          <input
            className="flex-1 bg-transparent border-b border-[var(--accent-blue)] outline-none text-xs"
            value={editValue}
            autoFocus
            onChange={(e) => setEditValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                onRename(editValue);
                setEditing(false);
              }
              if (e.key === "Escape") {
                setEditValue(name);
                setEditing(false);
              }
            }}
            onBlur={() => {
              if (editValue !== name) onRename(editValue);
              setEditing(false);
            }}
          />
        ) : (
          <span
            className="flex-1 text-xs truncate cursor-text"
            onDoubleClick={() => {
              setEditValue(name);
              setEditing(true);
            }}
          >
            {name}
          </span>
        )}
        <button
          disabled={index === 0}
          onClick={onMoveUp}
          className="w-6 h-6 text-[10px] rounded bg-[#ffffff10] disabled:opacity-30 cursor-pointer"
        >
          {"\u25B2"}
        </button>
        <button
          disabled={index === total - 1}
          onClick={onMoveDown}
          className="w-6 h-6 text-[10px] rounded bg-[#ffffff10] disabled:opacity-30 cursor-pointer"
        >
          {"\u25BC"}
        </button>
        <button
          onClick={onRemove}
          className="w-6 h-6 text-[10px] rounded bg-[#ffffff10] hover:bg-[var(--accent-red)] transition-colors cursor-pointer"
        >
          {"\u2715"}
        </button>
      </div>
    </div>
  );
}
