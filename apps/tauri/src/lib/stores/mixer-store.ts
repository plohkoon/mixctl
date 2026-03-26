import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { MixerApi, SystemApi } from "../api";
import type {
  AppRuleInfo,
  CaptureDeviceInfo,
  CustomInputInfo,
  FullState,
  InputInfo,
  OutputInfo,
  PickingState,
  PlaybackDeviceInfo,
  RouteInfo,
  StreamInfo,
} from "../types";

interface MixerStore {
  // Connection state
  daemonConnected: boolean;
  audioConnected: boolean;
  beacnConnected: boolean;

  // Data
  inputs: InputInfo[];
  outputs: OutputInfo[];
  routes: RouteInfo[];
  streams: StreamInfo[];
  playbackDevices: PlaybackDeviceInfo[];
  captureDevices: CaptureDeviceInfo[];
  customInputs: CustomInputInfo[];
  rules: AppRuleInfo[];
  selectedOutputId: number;
  defaultInputId: number;
  defaultOutputId: number;

  // UI state
  picking: PickingState | null;

  // Actions
  initialize: () => Promise<void>;
  setFullState: (state: FullState) => void;
  updateRoute: (route: RouteInfo) => void;
  setStreams: (streams: StreamInfo[]) => void;
  selectOutput: (outputId: number) => Promise<void>;
  setPicking: (p: PickingState | null) => void;
  setAudioConnected: (connected: boolean) => void;
  setBeacnConnected: (connected: boolean) => void;
  setDaemonConnected: (connected: boolean) => void;
  refreshRules: () => Promise<void>;
  refreshCaptureDevices: () => Promise<void>;
  refreshCustomInputs: () => Promise<void>;
}

export const useMixerStore = create<MixerStore>((set, get) => ({
  daemonConnected: false,
  audioConnected: false,
  beacnConnected: false,

  inputs: [],
  outputs: [],
  routes: [],
  streams: [],
  playbackDevices: [],
  captureDevices: [],
  customInputs: [],
  rules: [],
  selectedOutputId: 0,
  defaultInputId: 0,
  defaultOutputId: 0,

  picking: null,

  initialize: async () => {
    // Escape cancels picking
    window.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && get().picking) set({ picking: null });
    });

    try {
      const state = await MixerApi.getFullState();
      get().setFullState(state);
      set({ daemonConnected: true });
      // Fetch rules, capture devices, and custom inputs separately (not in FullState)
      get().refreshRules();
      get().refreshCaptureDevices();
      get().refreshCustomInputs();
    } catch {
      set({ daemonConnected: false });
    }

    // Subscribe to daemon events
    listen<FullState>("mixer:full-refresh", (event) => {
      get().setFullState(event.payload);
    });

    listen<RouteInfo>("mixer:route-changed", (event) => {
      get().updateRoute(event.payload);
    });

    listen<StreamInfo[]>("mixer:streams-changed", (event) => {
      get().setStreams(event.payload);
    });

    listen<boolean>("mixer:status-changed", (event) => {
      get().setAudioConnected(event.payload);
    });

    listen<boolean>("mixer:beacn-changed", (event) => {
      get().setBeacnConnected(event.payload);
    });

    listen("mixer:connected", () => {
      set({ daemonConnected: true });
      MixerApi.getFullState().then((state) => {
        get().setFullState(state);
      });
      get().refreshRules();
      get().refreshCaptureDevices();
      get().refreshCustomInputs();
    });

    listen("mixer:disconnected", () => {
      set({ daemonConnected: false });
    });

    // Rules and capture devices change signals
    listen("mixer:rules-changed", () => {
      get().refreshRules();
    });

    listen("mixer:capture-devices-changed", () => {
      get().refreshCaptureDevices();
    });

    listen("mixer:custom-inputs-changed", () => {
      get().refreshCustomInputs();
    });
  },

  setFullState: (state: FullState) => {
    set({
      inputs: state.inputs,
      outputs: state.outputs,
      routes: state.routes,
      streams: state.streams,
      playbackDevices: state.playbackDevices,
      selectedOutputId: state.selectedOutputId,
      defaultInputId: state.defaultInputId,
      defaultOutputId: state.defaultOutputId,
      audioConnected: state.audioConnected,
      beacnConnected: state.beacnConnected,
    });
  },

  updateRoute: (route: RouteInfo) => {
    set((state) => ({
      routes: state.routes.map((r) =>
        r.input_id === route.input_id && r.output_id === route.output_id
          ? route
          : r
      ),
    }));
  },

  setStreams: (streams: StreamInfo[]) => {
    set({ streams });
  },

  selectOutput: async (outputId: number) => {
    try {
      const routes = await MixerApi.selectOutput(outputId);
      set({ selectedOutputId: outputId, routes });
    } catch (e) {
      console.error("Failed to select output:", e);
    }
  },

  setPicking: (p: PickingState | null) => {
    set({ picking: p });
  },

  setAudioConnected: (connected: boolean) => {
    set({ audioConnected: connected });
  },

  setBeacnConnected: (connected: boolean) => {
    set({ beacnConnected: connected });
  },

  setDaemonConnected: (connected: boolean) => {
    set({ daemonConnected: connected });
  },

  refreshRules: async () => {
    try {
      const rules = await SystemApi.listAppRules();
      set({ rules: rules.filter((r) => !r.app_name.includes("mixctl.") && !r.app_name.startsWith("output.")) });
    } catch { /* ignore if daemon not ready */ }
  },

  refreshCaptureDevices: async () => {
    try {
      const devices = await SystemApi.listCaptureDevices();
      set({ captureDevices: devices });
    } catch { /* ignore if daemon not ready */ }
  },

  refreshCustomInputs: async () => {
    try {
      const customInputs = await SystemApi.listCustomInputs();
      set({ customInputs });
    } catch { /* ignore if daemon not ready */ }
  },
}));
