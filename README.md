# cosmic-applet-db-ice

A COSMIC desktop panel applet that shows live speed and trip data from Deutsche Bahn ICE trains.

The applet is hidden when you are not connected to an ICE WiFi network (`WIFIonICE` or `WIFI@DB`). When connected, the panel shows the train type and current speed (e.g. `ICE 201 km/h`).

Click the panel text to open a popup with:

- Train number and destination
- Current speed and internet quality
- A speed graph of the last 15 minutes
- Next stop with delay
- All remaining stops

![Screenshot](screenshot.png)

## Requirements

- COSMIC desktop (epoch 1.0+)
- NetworkManager (`nmcli`) for WiFi detection
- Connection to an ICE onboard WiFi network

## Install with Nix (flake)

```sh
nix build github:smephite/cosmic-applet-db-ice
cp result/bin/cosmic-applet-db-ice ~/.local/bin/
cp result/share/applications/dev.smephite.CosmicAppletDbIce.desktop ~/.local/share/applications/
```

Then add `"dev.smephite.CosmicAppletDbIce"` to your panel plugins in
`~/.config/cosmic/com.system76.CosmicPanel.Panel/v1/plugins_wings`
and restart the panel (`pkill -USR1 cosmic-panel`).

## Build from source

```sh
nix-shell --run "cargo build --release"
```

Or with the flake dev shell:

```sh
nix develop
cargo build --release
```

## Disclaimer

This code was generated with the help of AI (Claude Code).

## License

MIT
