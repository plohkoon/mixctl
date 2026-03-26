import { useCallback, useEffect, useState } from "react";
import { useMixerStore } from "../lib/stores/mixer-store";
import Header from "../components/layout/Header";
import MatrixGrid from "../components/mixer/MatrixGrid";
import CustomInputsSection from "../components/mixer/CustomInputsSection";
import FooterBar from "../components/mixer/FooterBar";
import ExpandedFooter from "../components/mixer/ExpandedFooter";

export default function MainPage() {
  const initialize = useMixerStore((s) => s.initialize);
  const daemonConnected = useMixerStore((s) => s.daemonConnected);
  const [footerExpanded, setFooterExpanded] = useState(false);

  useEffect(() => {
    initialize();
  }, [initialize]);

  const toggleExpanded = useCallback(() => {
    setFooterExpanded((v) => !v);
  }, []);

  return (
    <div className="flex flex-col h-screen w-screen overflow-hidden relative" style={{ background: "var(--bg-base)" }}>
      <Header />
      <div className="flex-1 overflow-auto">
        <MatrixGrid />
      </div>
      <CustomInputsSection />
      <FooterBar onToggleExpand={toggleExpanded} />

      {/* Expanded footer overlay */}
      {footerExpanded && (
        <>
          {/* Dimmed backdrop */}
          <div
            className="absolute inset-0 z-40"
            style={{ background: "var(--bg-overlay)" }}
            onClick={() => setFooterExpanded(false)}
          />
          {/* Expanded panel */}
          <ExpandedFooter onClose={() => setFooterExpanded(false)} />
        </>
      )}

      {/* Disconnected overlay */}
      {!daemonConnected && (
        <div
          className="absolute inset-0 flex items-center justify-center z-50"
          style={{ background: "var(--bg-overlay)" }}
        >
          <div className="flex flex-col items-center gap-2">
            <div
              className="w-3 h-3 rounded-full animate-pulse"
              style={{ background: "var(--accent-danger)" }}
            />
            <span style={{ color: "var(--text-secondary)", fontSize: 14 }}>
              Waiting for daemon...
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
