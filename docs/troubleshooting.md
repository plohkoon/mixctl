# Troubleshooting

## No Audio Through Mixer

**Symptom:** Apps play but no sound reaches speakers.

**Check the link chain:**
```bash
pw-link -l | grep mixctl
```
You should see: `app → input.playback → input.monitor → mixer.in → mixer.out → output.playback → output.monitor → speaker.playback`

**Check mixer is running:**
```bash
pw-top | grep mixctl
```
The mixer should show `R` (running) state with non-zero BUSY time.

**Check volume matrix:**
```bash
mixctl-cli route list 5  # for output 5
```
Ensure routes have non-zero volume and aren't muted.

## App Not Appearing in Stream List

**Symptom:** A running app doesn't show in `mixctl-cli stream list`.

**Check PipeWire sees it:**
```bash
pw-cli list-objects Node | grep -B5 "your-app-name"
```

The app must have `media.class = "Stream/Output/Audio"`. Some apps (web browsers) don't create PipeWire nodes until they play audio.

**Check app rules:**
```bash
mixctl-cli rule list
```
If there's no matching rule, the app goes to the default input.

## USB DAC Goes Silent

**Symptom:** DAC accepts audio (ALSA shows Running) but no analog output.

This is typically an XMOS USB controller firmware issue. See [SDAC Investigation](../SDAC_INVESTIGATION.md) for details.

**Fix:**
1. Unplug DAC USB cable, wait 30 seconds, replug
2. If that fails, boot into Windows and back
3. Long-term: the daemon defers routing until output chains are ready (prevents abrupt stream interruptions that trigger the bug)

## PipeWire "no more output formats" Error

**Cause:** Format mismatch between mixer ports and null-audio-sink ports.

**Fix:** Ensure the mixer uses `format.dsp = "32 bit float mono audio"` on its ports (this is the default configuration).

## High CPU Usage

**Check which DSP processors are enabled:**
```bash
mixctl-cli input eq get 1
mixctl-cli input gate get 1
mixctl-cli output compressor get 5
```

Disable processors you don't need:
```bash
mixctl-cli input eq disable 1
mixctl-cli input gate disable 1
```

When disabled, processors have zero CPU cost.

## TUI Shows "Waiting for daemon..."

The daemon is not running. Start it:
```bash
systemctl --user start mixctl-daemon
# Or directly:
mixctl-daemon
```

## BEACN Display Frozen

The BEACN daemon lost connection to the mixer daemon. Check:
```bash
systemctl --user status mixctl-daemon
systemctl --user status mixctl-beacn-daemon
```

Restart both if needed:
```bash
systemctl --user restart mixctl-daemon mixctl-beacn-daemon
```

## Checking Daemon Status

```bash
mixctl-cli status
```

Shows: audio connection state, default input, and connected components (CLI, TUI, UI, applet, BEACN).
