import { useMixerStore } from "../../lib/stores/mixer-store";
import OutputHeader from "./OutputHeader";
import InputLabel from "./InputLabel";
import RouteCell from "./RouteCell";
import AddChannelMenu from "./AddChannelMenu";

export default function MatrixGrid() {
  const inputs = useMixerStore((s) => s.inputs);
  const outputs = useMixerStore((s) => s.outputs);
  const routes = useMixerStore((s) => s.routes);

  if (outputs.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <span className="text-[var(--text-muted)] text-sm">
          No outputs configured
        </span>
      </div>
    );
  }

  // Build a lookup: routes[inputId][outputId] = RouteInfo
  const routeMap = new Map<string, (typeof routes)[0]>();
  for (const r of routes) {
    routeMap.set(`${r.input_id}-${r.output_id}`, r);
  }

  const colCount = outputs.length;

  return (
    <div
      className="overflow-auto"
      style={{
        display: "grid",
        gridTemplateColumns: `160px repeat(${colCount}, minmax(0, 1fr))`,
        gridTemplateRows: `auto repeat(${inputs.length}, 36px)`,
        gap: 0,
        minWidth: 160 + colCount * 160,
        alignContent: "start",
      }}
    >
      {/* Top-left corner cell — add channel menu */}
      <div
        className="sticky left-0 z-10"
        style={{
          background: "var(--bg-base)",
          borderBottom: "2px solid var(--border-strong)",
          borderRight: "1px solid var(--border-subtle)",
        }}
      >
        <AddChannelMenu />
      </div>

      {/* Output column headers */}
      {outputs.map((output, idx) => (
        <OutputHeader key={output.id} output={output} index={idx} />
      ))}

      {/* Input rows */}
      {inputs.map((input, rowIdx) => (
        <>
          {/* Row label (sticky left) */}
          <div
            key={`label-${input.id}`}
            className="sticky left-0 z-10"
            style={{
              background: "var(--bg-base)",
              borderRight: "1px solid var(--border-subtle)",
              borderBottom:
                rowIdx < inputs.length - 1
                  ? "1px solid var(--border-subtle)"
                  : "none",
            }}
          >
            <InputLabel
              inputId={input.id}
              name={input.name}
              color={input.color}
              index={rowIdx}
            />
          </div>

          {/* Route cells for this input */}
          {outputs.map((output) => {
            const route = routeMap.get(`${input.id}-${output.id}`);
            return (
              <div
                key={`cell-${input.id}-${output.id}`}
                style={{
                  borderBottom:
                    rowIdx < inputs.length - 1
                      ? "1px solid var(--border-subtle)"
                      : "none",
                  borderRight: "1px solid var(--border-subtle)",
                  background: "var(--bg-surface-0)",
                }}
              >
                {route ? (
                  <RouteCell route={route} inputColor={input.color} />
                ) : (
                  <div className="h-10 flex items-center justify-center">
                    <span className="text-[10px] text-[var(--text-disabled)]">—</span>
                  </div>
                )}
              </div>
            );
          })}
        </>
      ))}
    </div>
  );
}
