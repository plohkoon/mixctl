export interface InputInfo {
  id: number;
  name: string;
  color: string;
}

export interface OutputInfo {
  id: number;
  name: string;
  color: string;
  volume: number;
  muted: boolean;
  target_device: string;
}

export interface RouteInfo {
  input_id: number;
  output_id: number;
  volume: number;
  muted: boolean;
}

export interface StreamInfo {
  pw_node_id: number;
  app_name: string;
  media_name: string;
  input_id: number;
}

export interface PlaybackDeviceInfo {
  pw_node_id: number;
  name: string;
  device_name: string;
}

export interface CaptureDeviceInfo {
  pw_node_id: number;
  name: string;
  device_name: string;
  is_added: boolean;
  input_id: number;
}

export interface AppRuleInfo {
  app_name: string;
  input_id: number;
}

export interface ComponentInfo {
  bus_name: string;
  component_type: string;
}

export interface CustomInputInfo {
  id: number;
  name: string;
  color: string;
  custom_type: string;
  value: number;
}

export interface EqBandInfo {
  band_type: string;
  frequency: number;
  gain_db: number;
  q: number;
}

export interface GateInfo {
  enabled: boolean;
  threshold_db: number;
  attack_ms: number;
  release_ms: number;
  hold_ms: number;
}

export interface DeesserInfo {
  enabled: boolean;
  frequency: number;
  threshold_db: number;
  ratio: number;
}

export interface CompressorInfo {
  enabled: boolean;
  threshold_db: number;
  ratio: number;
  attack_ms: number;
  release_ms: number;
  makeup_gain_db: number;
  knee_db: number;
}

export interface LimiterInfo {
  enabled: boolean;
  ceiling_db: number;
  release_ms: number;
}

export interface FullState {
  inputs: InputInfo[];
  outputs: OutputInfo[];
  routes: RouteInfo[];
  streams: StreamInfo[];
  playbackDevices: PlaybackDeviceInfo[];
  selectedOutputId: number;
  defaultInputId: number;
  defaultOutputId: number;
  audioConnected: boolean;
  beacnConnected: boolean;
}

export interface PickingState {
  type: "stream" | "capture" | "playback" | "rule";
  id: string;
  data: {
    pwNodeId?: number;
    deviceName?: string;
    appName?: string;
    isAdded?: boolean;
    name?: string;
    inputId?: number;
  };
}

export interface InputDspState {
  eqEnabled: boolean;
  eqBands: EqBandInfo[];
  gate: GateInfo;
  deesser: DeesserInfo;
}

export interface OutputDspState {
  compressor: CompressorInfo;
  limiter: LimiterInfo;
}
