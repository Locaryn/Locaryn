# Locaryn Plugin: Remote & Tunnels (`plugin-remote`)

Official Locaryn extension for remote access through encrypted outbound tunnels (Cloudflare, ngrok, or devtunnel) with QR pairing.

The plugin is optional. Without it, Locaryn only exposes the local-network pairing mode. When it is enabled, it adds the Remote settings, tunnel mode, and explicit port-open pairing mode.

## Features

- **Remote access**: connect the Locaryn mobile app from outside the local network.
- **Encrypted tunnels**: use Cloudflare, ngrok, or devtunnel providers.
- **Port-open pairing**: generate a signed pairing code when the user has deliberately exposed a port.
- **Settings integration**: adds the "Remote & Tunnels" settings section in Locaryn.

## Installation

The GitHub repository keeps its historical slug until the organization rename is completed:

```bash
locaryn plugin install Locaryn/plugin-travel-tunnel
```

## Development

```bash
cargo test
```
