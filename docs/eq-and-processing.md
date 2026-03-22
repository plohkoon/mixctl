# EQ and Audio Processing

mixctl includes per-channel audio processing that runs inline in the real-time audio path. All processors default to **disabled** — enable only what you need. When disabled, a processor has zero CPU cost.

## Per-Input Processing Chain

Each input channel has three toggleable processors applied in order:

```
Raw audio → [8-band Parametric EQ] → [Noise Gate] → [De-esser] → processed audio
```

### 8-Band Parametric EQ

Shape the frequency response of any input channel with 8 independent filter bands.

**Band types:**
- `low_shelf` — boost/cut everything below the frequency
- `peaking` — boost/cut a narrow band around the frequency
- `high_shelf` — boost/cut everything above the frequency
- `bypass` — band does nothing

**Default frequencies:** 80, 250, 800, 2500, 5000, 8000, 12000, 16000 Hz

```bash
# Enable EQ on input 1
mixctl-cli input eq enable 1

# Boost bass, cut muddy mids, add presence
mixctl-cli input eq set 1 0 low_shelf 80 3.0 0.7
mixctl-cli input eq set 1 2 peaking 400 -2.0 1.4
mixctl-cli input eq set 1 5 peaking 8000 2.0 1.4

# Reset all bands to flat
mixctl-cli input eq reset 1

# Disable EQ (zero CPU)
mixctl-cli input eq disable 1
```

### Noise Gate

Silences the input when the audio level drops below a threshold. Useful for eliminating background noise between phrases.

**Parameters:**
- `threshold_db` (-80 to 0): Level below which the gate closes
- `attack_ms` (0.1 to 100): How fast the gate opens
- `release_ms` (1 to 2000): How fast the gate closes
- `hold_ms` (0 to 500): How long the gate stays open after signal drops

```bash
# Enable gate on input 4 (Chat)
mixctl-cli input gate enable 4
mixctl-cli input gate set 4 -35 1.0 100 50
```

### De-esser

Reduces harsh sibilance ("s" and "t" sounds) on voice channels. Works by compressing only the high-frequency content.

```bash
# Enable de-esser on input 4
mixctl-cli input deesser enable 4
mixctl-cli input deesser set 4 6000 -20 4.0
```

## Per-Output Processing Chain

Each output channel has two toggleable processors:

```
Mixed audio → [Compressor] → [Limiter] → final output
```

### Compressor

Reduces dynamic range — makes quiet parts louder and loud parts quieter. Essential for consistent stream audio.

**Parameters:**
- `threshold_db`: Level where compression starts
- `ratio`: Compression ratio (4:1 = every 4dB over threshold becomes 1dB)
- `attack_ms`: How fast compression kicks in
- `release_ms`: How fast compression releases
- `makeup_gain_db`: Volume boost to compensate for compression
- `knee_db`: Soft knee width (gradual onset)

```bash
# Enable compressor on output 5 (Personal Mix)
mixctl-cli output compressor enable 5
mixctl-cli output compressor set 5 -18 4.0 10 100 6.0 6.0
```

### Limiter

Brick-wall limiter that prevents clipping. No audio will exceed the ceiling level.

```bash
# Enable limiter on output 5
mixctl-cli output limiter enable 5
mixctl-cli output limiter set 5 -0.5 50
```

## Using External Tools Instead

If you prefer to use EasyEffects, Carla, or other PipeWire effects tools, simply leave all mixctl processors disabled (the default). mixctl acts as a pure router with zero DSP overhead. External tools can insert themselves into the PipeWire graph between mixctl's nodes.
