# AirType 🎤✨

**Live Voice-to-Text Desktop App**

AirType is a lightweight, cross-platform desktop application that transcribes your voice to text and inserts it directly at your cursor position. Works anywhere on your computer with a simple global hotkey.

## Features

- 🎙️ **Global Hotkey Recording** - Press a hotkey anywhere to start recording
- 🌍 **Dual Language Support** - English and Hebrew (more coming soon)
- ⚡ **Two Modes**:
  - **Batch Mode**: Record → Transcribe → Insert (more accurate)
  - **Live Mode**: Real-time transcription as you speak
- 🔒 **100% Offline** - Uses Whisper locally, no data sent to cloud
- 🪶 **Lightweight** - Near-zero CPU/RAM when idle
- 🖥️ **System Tray** - Runs quietly in the background
- 🚀 **Fast** - Optimized for quick transcription

## Installation

### Prerequisites

#### Linux (Ubuntu/Debian)
```bash
# Install system dependencies
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

# Optional: For GPU acceleration (CUDA)
# sudo apt install nvidia-cuda-toolkit
```

#### macOS
```bash
# Install Xcode Command Line Tools
xcode-select --install

# Install Homebrew dependencies (optional)
brew install cmake
```

#### Windows
- Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- Install [CMake](https://cmake.org/download/)

### Building from Source

1. **Clone the repository**
```bash
git clone https://github.com/yourusername/AirType.git
cd AirType
```

2. **Install Node.js dependencies**
```bash
npm install
```

3. **Download a Whisper model**
```bash
# Create models directory
mkdir -p ~/.config/airtype/models

# Download base model (recommended, ~150MB)
curl -L -o ~/.config/airtype/models/ggml-base.bin \
    https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin

# Or download tiny model for faster but less accurate transcription (~75MB)
curl -L -o ~/.config/airtype/models/ggml-tiny.bin \
    https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin
```

4. **Build and run**
```bash
# Development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Usage

### Default Hotkeys

| Hotkey | Action |
|--------|--------|
| `Ctrl+Shift+Space` | Hold to record |
| `Ctrl+Shift+R` | Toggle recording on/off |
| `Ctrl+Shift+L` | Toggle language (EN/HE) |

### Quick Start

1. Launch AirType
2. Place your cursor where you want text inserted
3. Press and hold `Ctrl+Shift+Space` while speaking
4. Release to transcribe and insert text

### Settings

Click the ⚙️ icon in the app to configure:
- Language (English/Hebrew)
- Recording mode (Batch/Live)
- Whisper model size
- Hotkey configuration
- Start on login

## Model Selection

| Model | Size | Speed | Accuracy | Languages |
|-------|------|-------|----------|-----------|
| Tiny | ~75MB | Fastest | Good | EN only (tiny.en) or all |
| Base | ~150MB | Fast | Better | All |
| Small | ~500MB | Medium | Great | All |
| Medium | ~1.5GB | Slow | Excellent | All |
| Large | ~3GB | Slowest | Best | All |

**Recommendation**: Start with `base` for a good balance of speed and accuracy.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     AirType Desktop App                      │
├─────────────────────────────────────────────────────────────┤
│  Frontend (Svelte)           │  Backend (Rust/Tauri)        │
│  ├─ Settings UI              │  ├─ Global Hotkey Manager    │
│  ├─ Recording Status         │  ├─ Audio Capture (cpal)     │
│  ├─ Mode Toggle              │  ├─ Whisper Engine           │
│  └─ Transcription Display    │  ├─ Text Injector (enigo)    │
│                              │  └─ Settings Store           │
├─────────────────────────────────────────────────────────────┤
│                      System Tray                             │
└─────────────────────────────────────────────────────────────┘
```

## Tech Stack

- **Framework**: [Tauri v2](https://tauri.app/) - Lightweight desktop app framework
- **Backend**: Rust
- **Frontend**: [Svelte 5](https://svelte.dev/)
- **Speech-to-Text**: [whisper-rs](https://github.com/tazz4843/whisper-rs) (whisper.cpp bindings)
- **Audio Capture**: [cpal](https://github.com/RustAudio/cpal)
- **Text Injection**: [enigo](https://github.com/enigo-rs/enigo)

## Troubleshooting

### Linux: Global hotkeys not working
- Wayland has limited global hotkey support. Consider using X11.
- Some desktop environments require accessibility permissions.

### macOS: Permission denied
- Go to System Preferences → Security & Privacy → Privacy
- Enable AirType in both "Accessibility" and "Input Monitoring"

### Model not loading
- Ensure the model file exists in `~/.config/airtype/models/`
- Check file permissions
- Try a smaller model (tiny) to test

### No audio input
- Check your microphone permissions
- Verify the correct input device is selected
- Test with another audio recording app

## Development

```bash
# Run in development mode with hot reload
npm run tauri dev

# Run Rust tests
cd src-tauri && cargo test

# Build for production
npm run tauri build
```

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).

## License

MIT License
