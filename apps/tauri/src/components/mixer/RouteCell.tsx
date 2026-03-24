import { useCallback, useEffect, useRef, useState } from "react";
import { MixerApi } from "../../lib/api";
import VolumeFader from "./VolumeFader";
import MuteButton from "./MuteButton";
import type { RouteInfo } from "../../lib/types";

interface RouteCellProps {
  route: RouteInfo;
  inputColor: string;
}

export default function RouteCell({ route, inputColor }: RouteCellProps) {
  const [localVolume, setLocalVolume] = useState(route.volume);
  const isDragging = useRef(false);

  useEffect(() => {
    if (!isDragging.current) setLocalVolume(route.volume);
  }, [route.volume]);

  const handleChange = useCallback(
    (vol: number) => {
      isDragging.current = true;
      setLocalVolume(vol);
      MixerApi.setRouteVolume(route.input_id, route.output_id, vol);
    },
    [route.input_id, route.output_id]
  );

  const handleChangeEnd = useCallback(
    (vol: number) => {
      isDragging.current = false;
      setLocalVolume(vol);
      MixerApi.setRouteVolume(route.input_id, route.output_id, vol);
    },
    [route.input_id, route.output_id]
  );

  const handleMute = useCallback(() => {
    MixerApi.setRouteMute(route.input_id, route.output_id, !route.muted);
  }, [route.input_id, route.output_id, route.muted]);

  return (
    <div
      className="flex items-center gap-2 px-3 h-full"
      style={{
        opacity: route.muted ? 0.55 : 1,
        transition: "opacity 120ms ease",
      }}
    >
      <VolumeFader
        value={localVolume}
        color={inputColor}
        onChange={handleChange}
        onChangeEnd={handleChangeEnd}
      />
      <span
        className="w-8 text-right shrink-0 tabular-nums"
        style={{
          fontFamily: "var(--font-mono)",
          fontSize: 11,
          color: "var(--text-secondary)",
        }}
      >
        {localVolume}
      </span>
      <MuteButton muted={route.muted} size={24} onClick={handleMute} />
    </div>
  );
}
