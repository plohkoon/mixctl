import { invoke } from "@tauri-apps/api/core";
import type {
  AppRuleInfo,
  CaptureDeviceInfo,
  ComponentInfo,
  EqBandInfo,
  FullState,
  InputDspState,
  InputInfo,
  OutputDspState,
  OutputInfo,
  PlaybackDeviceInfo,
  RouteInfo,
  StreamInfo,
} from "./types";

// ---------------------------------------------------------------------------
// Mixer
// ---------------------------------------------------------------------------

export const MixerApi = {
  getFullState: () => invoke<FullState>("get_full_state"),
  listInputs: () => invoke<InputInfo[]>("list_inputs"),
  listOutputs: () => invoke<OutputInfo[]>("list_outputs"),
  getDefaultInput: () => invoke<number>("get_default_input"),
  setDefaultInput: (id: number) => invoke("set_default_input", { id }),
  getDefaultOutput: () => invoke<number>("get_default_output"),
  setDefaultOutput: (id: number) => invoke("set_default_output", { id }),
  selectOutput: (outputId: number) =>
    invoke<RouteInfo[]>("select_output", { outputId }),
  listRoutesForOutput: (outputId: number) =>
    invoke<RouteInfo[]>("list_routes_for_output", { outputId }),
  setRouteVolume: (inputId: number, outputId: number, volume: number) =>
    invoke("set_route_volume", { inputId, outputId, volume }),
  setRouteMute: (inputId: number, outputId: number, muted: boolean) =>
    invoke("set_route_mute", { inputId, outputId, muted }),
  setOutputVolume: (id: number, volume: number) =>
    invoke("set_output_volume", { id, volume }),
  setOutputMute: (id: number, muted: boolean) =>
    invoke("set_output_mute", { id, muted }),
  setOutputTarget: (id: number, deviceName: string) =>
    invoke("set_output_target", { id, deviceName }),
  listStreams: () => invoke<StreamInfo[]>("list_streams"),
  assignStream: (pwNodeId: number, inputId: number, remember: boolean) =>
    invoke("assign_stream", { pwNodeId, inputId, remember }),
  listPlaybackDevices: () =>
    invoke<PlaybackDeviceInfo[]>("list_playback_devices"),
  getAudioStatus: () => invoke<string>("get_audio_status"),
};

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

export const ChannelsApi = {
  addInput: (name: string, color: string) =>
    invoke<number>("add_input", { name, color }),
  removeInput: (id: number) => invoke("remove_input", { id }),
  moveInput: (id: number, position: number) =>
    invoke("move_input", { id, position }),
  setInputName: (id: number, name: string) =>
    invoke("set_input_name", { id, name }),
  setInputColor: (id: number, color: string) =>
    invoke("set_input_color", { id, color }),
  addOutput: (name: string, color: string, sourceOutputId: number) =>
    invoke<number>("add_output", { name, color, sourceOutputId }),
  removeOutput: (id: number) => invoke("remove_output", { id }),
  moveOutput: (id: number, position: number) =>
    invoke("move_output", { id, position }),
  setOutputName: (id: number, name: string) =>
    invoke("set_output_name", { id, name }),
  setOutputColor: (id: number, color: string) =>
    invoke("set_output_color", { id, color }),
};

// ---------------------------------------------------------------------------
// DSP
// ---------------------------------------------------------------------------

export const DspApi = {
  getInputDsp: (inputId: number) =>
    invoke<InputDspState>("get_input_dsp", { inputId }),
  getOutputDsp: (outputId: number) =>
    invoke<OutputDspState>("get_output_dsp", { outputId }),
  setInputEqEnabled: (inputId: number, enabled: boolean) =>
    invoke("set_input_eq_enabled", { inputId, enabled }),
  setInputEqBand: (
    inputId: number,
    band: number,
    bandType: string,
    freq: number,
    gainDb: number,
    q: number
  ) => invoke("set_input_eq_band", { inputId, band, bandType, freq, gainDb, q }),
  getInputEq: (inputId: number) =>
    invoke<EqBandInfo[]>("get_input_eq", { inputId }),
  resetInputEq: (inputId: number) => invoke("reset_input_eq", { inputId }),
  setInputGateEnabled: (inputId: number, enabled: boolean) =>
    invoke("set_input_gate_enabled", { inputId, enabled }),
  setInputGate: (
    inputId: number,
    thresholdDb: number,
    attackMs: number,
    releaseMs: number,
    holdMs: number
  ) =>
    invoke("set_input_gate", {
      inputId,
      thresholdDb,
      attackMs,
      releaseMs,
      holdMs,
    }),
  setInputDeesserEnabled: (inputId: number, enabled: boolean) =>
    invoke("set_input_deesser_enabled", { inputId, enabled }),
  setInputDeesser: (
    inputId: number,
    frequency: number,
    thresholdDb: number,
    ratio: number
  ) => invoke("set_input_deesser", { inputId, frequency, thresholdDb, ratio }),
  setOutputCompressorEnabled: (outputId: number, enabled: boolean) =>
    invoke("set_output_compressor_enabled", { outputId, enabled }),
  setOutputCompressor: (
    outputId: number,
    thresholdDb: number,
    ratio: number,
    attackMs: number,
    releaseMs: number,
    makeupGainDb: number,
    kneeDb: number
  ) =>
    invoke("set_output_compressor", {
      outputId,
      thresholdDb,
      ratio,
      attackMs,
      releaseMs,
      makeupGainDb,
      kneeDb,
    }),
  setOutputLimiterEnabled: (outputId: number, enabled: boolean) =>
    invoke("set_output_limiter_enabled", { outputId, enabled }),
  setOutputLimiter: (
    outputId: number,
    ceilingDb: number,
    releaseMs: number
  ) => invoke("set_output_limiter", { outputId, ceilingDb, releaseMs }),
  computeEqCurve: (bands: EqBandInfo[]) =>
    invoke<[number, number][]>("compute_eq_curve", { bands }),
};

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

export const SystemApi = {
  listAppRules: () => invoke<AppRuleInfo[]>("list_app_rules"),
  setAppRule: (appName: string, inputId: number) =>
    invoke("set_app_rule", { appName, inputId }),
  removeAppRule: (appName: string) => invoke("remove_app_rule", { appName }),
  listCaptureDevices: () =>
    invoke<CaptureDeviceInfo[]>("list_capture_devices"),
  addCaptureInput: (pwNodeId: number, name: string, color: string) =>
    invoke<number>("add_capture_input", { pwNodeId, name, color }),
  getBeacnConfig: () => invoke<string>("get_beacn_config"),
  setBeacnConfig: (
    layout: string,
    dialSensitivity: number,
    levelDecay: number
  ) => invoke("set_beacn_config", { layout, dialSensitivity, levelDecay }),
  listComponents: () => invoke<ComponentInfo[]>("list_components"),
  registerComponent: (componentType: string) =>
    invoke("register_component", { componentType }),
  bindCaptureToInput: (inputId: number, deviceName: string) =>
    invoke("bind_capture_to_input", { inputId, deviceName }),
  removeCaptureInput: (id: number) => invoke("remove_capture_input", { id }),
  setCaptureVolume: (id: number, volume: number) =>
    invoke("set_capture_volume", { id, volume }),
  setCaptureMute: (id: number, muted: boolean) =>
    invoke("set_capture_mute", { id, muted }),
  openDialog: (dialog: string) => invoke("open_dialog", { dialog }),
  openChannelEditor: (mode: "input" | "output", id?: number) =>
    invoke("open_channel_editor", { mode, id: id ?? null }),
  listProfiles: () => invoke<string[]>("list_profiles"),
  saveProfile: (name: string) => invoke("save_profile", { name }),
  loadProfile: (name: string) => invoke("load_profile", { name }),
  deleteProfile: (name: string) => invoke("delete_profile", { name }),
};
