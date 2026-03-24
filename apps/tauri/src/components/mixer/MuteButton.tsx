interface MuteButtonProps {
  muted: boolean;
  size?: number;
  onClick: () => void;
}

export default function MuteButton({ muted, size = 28, onClick }: MuteButtonProps) {
  return (
    <button
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className="shrink-0 rounded-md flex items-center justify-center transition-colors duration-[120ms] cursor-pointer"
      style={{
        width: size,
        height: size,
        background: muted ? "var(--accent-danger)" : "var(--bg-surface-2)",
        border: muted ? "none" : "1px solid var(--border-default)",
      }}
      title={muted ? "Unmute" : "Mute"}
    >
      <svg
        width={size * 0.5}
        height={size * 0.5}
        viewBox="0 0 16 16"
        fill="none"
        stroke={muted ? "var(--text-inverse)" : "var(--text-secondary)"}
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        {muted ? (
          <>
            {/* Speaker body */}
            <path d="M2 5.5h2.5L8 2.5v11l-3.5-3H2a.5.5 0 0 1-.5-.5V6a.5.5 0 0 1 .5-.5z" />
            {/* X mark */}
            <line x1="11" y1="5.5" x2="15" y2="10.5" />
            <line x1="15" y1="5.5" x2="11" y2="10.5" />
          </>
        ) : (
          <>
            {/* Speaker body */}
            <path d="M2 5.5h2.5L8 2.5v11l-3.5-3H2a.5.5 0 0 1-.5-.5V6a.5.5 0 0 1 .5-.5z" />
            {/* Sound waves */}
            <path d="M11 5.5a3.5 3.5 0 0 1 0 5" />
            <path d="M13 3.5a6.5 6.5 0 0 1 0 9" />
          </>
        )}
      </svg>
    </button>
  );
}
