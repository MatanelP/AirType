# AirType 🎤✨

**Voice-to-Text Desktop App — English & Hebrew**

AirType is a lightweight, cross-platform desktop application that transcribes your voice to text and inserts it directly at your cursor position. Works anywhere on your computer with global hotkeys.

## Features

- 🎙️ **Global Hotkeys** — Record from anywhere with customizable hotkeys
- 🌍 **English & Hebrew** — Separate hotkeys, each with the best model for the language
- ⚡ **Two Engines**:
  - **Local Whisper** (free, offline) — runs on your CPU, no internet needed
  - **Paid API** — OpenAI Realtime (English live) + HuggingFace ivrit-ai (Hebrew)
- 🔴 **Floating Indicator** — Small on-screen dot shows recording/processing state
- 🪶 **Lightweight** — Near-zero CPU/RAM when idle
- 🖥️ **System Tray** — Runs quietly in the background
- 🚀 **Auto-start** — Can launch on login

## Transcription Matrix

### Engine: **Paid (API keys required)**

| | English (`Ctrl+Shift+E`) | Hebrew (`Ctrl+Shift+H`) |
|---|---|---|
| **Service** | OpenAI Realtime API | HuggingFace Inference API |
| **Model** | `gpt-4o-transcribe` | `ivrit-ai/whisper-large-v3` |
| **Mode** | **Live** — text streams as you speak | **Batch** — text after you stop |
| **Key** | OpenAI (`sk-...`) | HuggingFace (`hf_...`) |
| **Quality** | ⭐ Great | ⭐ Great |
| **Speed** | Real-time | ~2-3s after stop |

### Engine: **Local Whisper (free, offline)**

| | English (`Ctrl+Shift+E`) | Hebrew (`Ctrl+Shift+H`) |
|---|---|---|
| **Model** | Selected Whisper model | Same model + Hebrew language hint |
| **Mode** | **Batch** — text after you stop | **Batch** — text after you stop |
| **Key** | None | None |
| **Quality** | 👍 Good (`small`+) | 👍 OK (`small`+), 😐 weak on `base` |
| **Speed** | ~3-10s depending on model size | ~3-10s depending on model size |

### Recording Modes (both engines)

| Mode | How it works |
|------|-------------|
| **Hold** | Hold hotkey to record, release to stop |
| **Toggle** | Press to start, press again to stop |

## Installation

### Prerequisites

#### Linux (Ubuntu/Debian)
```bash
sudo apt update
sudo apt install -y \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    libappindicator3-dev \
    librsvg2-dev \
    patchelf \
    libasound2-dev \
    libssl-dev \
    pkg-config \
    build-essential \
    cmake
```

#### macOS
```bash
xcode-select --install
brew install cmake  # optional
```

#### Windows
- Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- Install [CMake](https://cmake.org/download/)

### Building from Source

```bash
git clone https://github.com/yourusername/AirType.git
cd AirType
npm install
npm run tauri dev      # development
npm run tauri build    # production
```

## Setup

### Free (Local Whisper)

No setup needed — a Whisper model will be downloaded automatically on first use. Choose model size in Settings:

| Model | Size | Speed | Best for |
|-------|------|-------|----------|
| tiny | ~75MB | Fastest | Quick English notes |
| base | ~150MB | Fast | English (default) |
| **small** | **~466MB** | **Medium** | **English + Hebrew (recommended)** |
| medium | ~1.5GB | Slow | Best local accuracy |
| large | ~3GB | Slowest | Maximum accuracy |

### Paid (API Keys)

1. **OpenAI key** (for English live): [platform.openai.com/api-keys](https://platform.openai.com/api-keys) → Create key
2. **HuggingFace key** (for Hebrew): [huggingface.co/settings/tokens](https://huggingface.co/settings/tokens) → Create token with "Make calls to Inference Providers" permission
3. Open AirType Settings → select "OpenAI (paid, live)" → paste both keys

## Usage

1. Launch AirType (appears in system tray)
2. Place your cursor where you want text
3. Press `Ctrl+Shift+E` (English) or `Ctrl+Shift+H` (Hebrew)
4. Speak — text is inserted at cursor when done

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                      AirType Desktop App                      │
├──────────────────────────────────────────────────────────────┤
│  Frontend (Svelte 5)          │  Backend (Rust/Tauri v2)      │
│  ├─ Settings UI               │  ├─ Global Hotkey Manager     │
│  ├─ Recording Indicator       │  ├─ Audio Capture (cpal)      │
│  └─ Floating Status Window    │  ├─ Local Whisper (whisper-rs) │
│                               │  ├─ OpenAI Realtime (WS)      │
│                               │  ├─ HuggingFace Inference API  │
│                               │  ├─ Text Injector (enigo)      │
│                               │  └─ Settings Store (JSON)      │
├──────────────────────────────────────────────────────────────┤
│                       System Tray                              │
└──────────────────────────────────────────────────────────────┘
```

## Tech Stack

- **Framework**: [Tauri v2](https://tauri.app/)
- **Backend**: Rust
- **Frontend**: [Svelte 5](https://svelte.dev/)
- **Local STT**: [whisper-rs](https://github.com/tazz4843/whisper-rs) (whisper.cpp bindings)
- **Live STT**: OpenAI Realtime API (WebSocket)
- **Hebrew STT**: [ivrit-ai/whisper-large-v3](https://huggingface.co/ivrit-ai/whisper-large-v3) via HuggingFace (fal-ai provider)
- **Audio**: [cpal](https://github.com/RustAudio/cpal)
- **Text Injection**: [enigo](https://github.com/enigo-rs/enigo)

## Troubleshooting

### Linux: Global hotkeys not working
- Wayland has limited global hotkey support — use X11
- Some DEs require accessibility permissions

### macOS: Permission denied
- System Preferences → Security & Privacy → Privacy
- Enable AirType in "Accessibility" and "Input Monitoring"

### Model not loading
- Models stored in `~/.config/airtype/models/`
- Try a smaller model (tiny) first
- Check file permissions

### API transcription not working
- Verify keys in Settings (green ✓ = valid)
- Check internet connection
- OpenAI: ensure billing is set up at platform.openai.com
- HuggingFace: ensure "Inference Providers" permission on token

## Development

```bash
npm run tauri dev           # dev mode with hot reload
cd src-tauri && cargo test  # run Rust tests
npm run tauri build         # production build
```

## License

MIT License
