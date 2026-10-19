# Twitch Pikker

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-2-24C8DB.svg)](https://tauri.app/)
[![CI](https://github.com/remmody/twitch-pikker/actions/workflows/ci.yml/badge.svg)](https://github.com/remmody/twitch-pikker/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/remmody/twitch-pikker)](https://github.com/remmody/twitch-pikker/releases)

Twitch Pikker is a cross-platform desktop Twitch chat reader built with Tauri and Rust. It reads chat in real time, can speak every message aloud with fully offline Russian text-to-speech, and can play a notification sound for each new message.

- Reads Twitch chat messages in real time
- Fully offline Russian TTS (Silero and Piper), male and female voices
- Configurable notification sound for every new message, with custom sound support
- Modern desktop interface (React + Tailwind in a Tauri shell)
- No cloud TTS and no telemetry: chat text never leaves your machine

## Screenshots

<!-- Screenshots have not been added yet. Add them to docs/screenshots/ after the first UI build, then uncomment:
![Chat](docs/screenshots/chat.png)
![Voice settings](docs/screenshots/voice.png)
![Notifications](docs/screenshots/notifications.png)
-->

*Screenshots will be added to [`docs/screenshots/`](docs/screenshots/) after the first UI build.*

## Features

- Twitch IRC chat connection (anonymous by default, optional account)
- Automatic reconnection with exponential backoff and jitter
- Message sanitization (links, emotes, control characters, repeated characters, length limit)
- TTS queue with bounded memory usage (oldest pending speech is dropped under load)
- Silero TTS and Piper TTS support
- Russian male and female voices
- Notification sound mode with built-in sounds, cooldown and volume
- Custom WAV / MP3 / OGG notification sounds
- Volume controls and speech speed control
- Dark and light themes
- Settings persistence
- Offline operation after model installation

## Requirements

- Rust (stable)
- Node.js (LTS) and npm
- Platform prerequisites for Tauri 2:
  - **Linux:** `sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev`
  - **Windows:** WebView2 (preinstalled on Windows 11) and the MSVC C++ build tools
  - **macOS:** Xcode Command Line Tools (`xcode-select --install`)
- For TTS: Python with PyTorch (Silero) and/or the Piper executable. See [Offline TTS setup](#offline-tts-setup).

## Installation

```bash
git clone https://github.com/remmody/twitch-pikker.git
cd twitch-pikker
npm install
npm run tauri dev
```

If you have the Tauri CLI for Cargo (`cargo install tauri-cli --locked`), `cargo tauri dev` works as well. The project uses npm; pnpm is not configured.

## Building release versions

```bash
npm run tauri build
```

Artifacts are written to `src-tauri/target/release/bundle/`:

| OS | Location |
|---|---|
| Linux | `bundle/appimage/*.AppImage`, `bundle/deb/*.deb`, `bundle/rpm/*.rpm` |
| Windows | `bundle/msi/*.msi`, `bundle/nsis/*.exe` |
| macOS | `bundle/dmg/*.dmg`, `bundle/macos/*.app` |

Builds are unsigned unless you configure signing yourself.

## Offline TTS setup

On first launch, if no voices are installed, a wizard offers to download them. **Nothing is downloaded until you give explicit consent and click Download.** Files are SHA-256 verified; downloads can be paused, resumed, retried and cancelled.

- **Silero v4_ru** (about 38 MB, five voices): `models/silero/v4_ru.pt`. Silero runs through a local Python helper that needs Python and PyTorch. The wizard can install both: Python via `winget` on Windows (on Linux and macOS install Python 3.10+ yourself), and the CPU build of PyTorch into a private environment in `models/runtime/venv` (about 200 MB), leaving your system Python untouched. The model is loaded from disk; nothing is fetched at runtime.
- **Piper** (about 61 MB per voice): `models/piper/<voice>.onnx` plus `<voice>.onnx.json`. The wizard also downloads the official Piper program (about 25 MB, SHA-256 verified) into `models/runtime/piper`. A `piper` already on `PATH` is used instead if present.
- Whenever a required model or program is missing, the wizard opens by itself and asks for confirmation before installing anything. Enabling TTS or pressing Test voice while something is missing reopens it.
- Manual install: place the files in the layout above, or use **Voice → Choose models folder**. Extra Piper `.onnx` + `.json` pairs in `models/piper` appear as voices automatically.

**Female and male Russian voices:** open **Voice**, pick the engine, filter by gender, and select a voice.

| Engine | Female | Male |
|---|---|---|
| Silero | `kseniya`, `xenia`, `baya` | `aidar`, `eugene` |
| Piper | `irina` | `denis`, `dmitri`, `ruslan` |

**Where models are stored** (inside the app data folder):

| OS | Path |
|---|---|
| Windows | `%LOCALAPPDATA%\twitch-chat-reader\models` |
| Linux | `~/.local/share/twitch-chat-reader/models` |
| macOS | `~/Library/Application Support/twitch-chat-reader/models` |

**Verify offline operation:** install the models, disconnect from the network, then use **Voice → Test voice** and **Preview**. Speech and notification sounds keep working; only the Twitch connection needs internet. The only HTTP client in the backend is the model downloader, and it runs only from the setup wizard.

## Configuration

Settings are edited in the app and saved to `config.toml` in the app data folder (the same folder as above, without `models`). Use **Settings → Open config folder** to find it. Main options:

| Section | Keys |
|---|---|
| `[twitch]` | `channel`, optional `username` and `oauth_token` (stored locally only) |
| `[tts]` | `engine` (`silero` / `piper`), `voice`, `speed` (0.7–1.5), `volume` (0–100), `max_message_chars`, `announce_username`, `username_template` |
| `[audio]` | `sound` (`default_ping` / `soft_pop` / `telegram` / `custom`), `custom_sound_path`, `notification_volume`, `cooldown_ms`, `output_device` |
| `[notifications]` | `enabled` |

An invalid file is moved to `config.toml.bad` and defaults are used.

## Development

```bash
npm install
npm run tauri dev
npm run typecheck
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --all --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
```

There is no frontend lint script yet. Tests that need an audio device or Piper are ignored by default; run them with `cargo test --features integration-tests -- --ignored`.

### Adding a TTS engine

1. Add `src-tauri/src/tts/<name>.rs` implementing `TextToSpeech` (return a WAV).
2. Add a `TtsEngine` variant and its voices in `tts/voices.rs`, and install detection in `tts/models.rs`.
3. Add the match arm in `Pipeline::engine` and the runtime selection in `pipeline.rs`.

## CI/CD

Every push to `main` and every pull request runs formatting, lint, tests, a frontend typecheck and a Tauri build on Linux, Windows and macOS. Pushing a tag that starts with `v` builds installers for all three platforms and attaches them to a GitHub Release (created as a draft because the builds are unsigned).

To publish a release:

```bash
git tag v0.1.0
git push origin main --tags
```

## License

This project is licensed under the [MIT License](LICENSE).
