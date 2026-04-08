# mixctl-{{device_name}}-daemon

Device adapter for {{device_name}}, connecting it to the [mixctl](https://github.com/plohkoon/mixctl) audio routing daemon.

## Building

```bash
cargo build --release
```

## Running

```bash
# Start the mixer daemon first
mixctl-daemon

# Then start this adapter
cargo run --release
```

## Installing

```bash
# Install the binary
cargo install --path .

# Install the systemd service
cp dist/systemd/mixctl-device.service ~/.config/systemd/user/mixctl-{{device_name}}-daemon.service
systemctl --user enable --now mixctl-{{device_name}}-daemon

# Install udev rules (if USB device)
sudo cp dist/udev/99-device.rules /etc/udev/rules.d/99-{{device_name}}.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

## Verifying

```bash
mixctl-cli adapter list
```
