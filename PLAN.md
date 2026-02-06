# AirType - Voice-to-Text Desktop App

## Problem Statement
Create a lightweight, cross-platform (Linux, macOS, Windows) desktop app in Rust that provides live voice-to-text functionality. Users can press/hold a global hotkey anywhere on their computer to record voice, which is transcribed and inserted at the cursor position.

## Proposed Approach

### Technology Stack
| Component | Technology | Rationale |
|-----------|------------|-----------|
| **GUI Framework** | **Tauri v2** | Lightweight (tiny binaries), modern web UI, excellent Rust integration, cross-platform |
| **Speech-to-Text** | **whisper-rs** (whisper.cpp bindings) | Offline, fast, supports Hebrew & English, runs locally |
| **Global Hotkeys** | **global-hotkey** (Tauri crate) | Cross-platform, well-maintained, integrates with Tauri |
| **Text Injection** | **enigo** | Cross-platform keyboard simulation, injects text at cursor |
| **Audio Capture** | **cpal** | Cross-platform audio input |
| **Frontend** | **Svelte/SvelteKit** | Lightweight, modern, fast, simple |

### Key Features
1. **Global Hotkey Recording** - Press/hold configurable keys to record from anywhere
2. **Dual Language Support** - Hebrew and English (auto-detect or manual toggle)
3. **Two Modes**:
   - **Live Mode**: Real-time transcription as you speak
   - **Batch Mode**: Transcribe after recording completes
4. **Text Injection** - Insert text directly at cursor position
5. **System Tray** - Minimal footprint when idle, start on login
6. **Minimal Resource Usage** - Near-zero CPU/RAM when idle

---

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

---

## Workplan

### Phase 1: Project Setup
- [ ] Initialize Tauri v2 project with Svelte frontend
- [ ] Set up Rust workspace structure
- [ ] Configure cross-platform build targets
- [ ] Add core dependencies (whisper-rs, global-hotkey, enigo, cpal)

### Phase 2: Core Audio Pipeline
- [ ] Implement audio capture with cpal (microphone input)
- [ ] Set up whisper-rs with downloadable models (tiny/base for speed)
- [ ] Create audio buffer management for recording
- [ ] Test basic transcription (English first)

### Phase 3: Global Hotkey System
- [ ] Implement global hotkey registration
- [ ] Support press-to-toggle and press-and-hold modes
- [ ] Make hotkeys configurable
- [ ] Handle hotkey conflicts gracefully

### Phase 4: Text Injection
- [ ] Implement enigo-based text injection
- [ ] Handle special characters and Unicode (Hebrew)
- [ ] Test injection in various apps (terminal, browser, editors)

### Phase 5: Recording Modes
- [ ] Implement batch mode (record → transcribe → inject)
- [ ] Implement live mode (stream → transcribe → inject continuously)
- [ ] Add mode toggle in UI and via hotkey

### Phase 6: Language Support
- [ ] Add Hebrew language model support
- [ ] Implement language auto-detection or manual toggle
- [ ] Test Hebrew transcription accuracy

### Phase 7: UI Development
- [ ] Create minimal, modern system tray UI
- [ ] Build settings panel (hotkeys, language, mode)
- [ ] Add recording indicator/status
- [ ] Implement transcription preview window (optional)

### Phase 8: System Integration
- [ ] Implement start-on-login functionality
- [ ] Add first-run setup wizard
- [ ] Configure model download on first use
- [ ] Optimize for minimal idle resource usage

### Phase 9: Polish & Testing
- [ ] Test on Linux (X11/Wayland considerations)
- [ ] Test on macOS (permissions handling)
- [ ] Test on Windows
- [ ] Performance optimization
- [ ] Error handling and user feedback

---

## Notes & Considerations

### Platform-Specific Issues
- **Linux/Wayland**: Global hotkeys only work on X11 currently (Wayland is a limitation)
- **macOS**: Requires Accessibility permissions for hotkeys and text injection
- **Windows**: Generally works well, may need admin for some features

### Model Selection
- **tiny.en** (~75MB): Fastest, English only, good for live mode
- **base** (~142MB): Good balance, multilingual including Hebrew
- **small** (~466MB): Better accuracy, slower

### Resource Optimization
- Lazy-load Whisper model only when recording starts
- Unload model after configurable idle timeout
- Use system tray to minimize window footprint

### Hebrew Specifics
- RTL text handling in UI
- Whisper supports Hebrew in multilingual models
- May need special keyboard handling for injection

---

## File Structure (Planned)

```
AirType/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs              # Tauri entry point
│   │   ├── lib.rs               # Module exports
│   │   ├── audio/
│   │   │   ├── mod.rs
│   │   │   ├── capture.rs       # Audio recording
│   │   │   └── buffer.rs        # Audio buffering
│   │   ├── transcription/
│   │   │   ├── mod.rs
│   │   │   ├── whisper.rs       # Whisper integration
│   │   │   └── streaming.rs     # Live transcription
│   │   ├── hotkeys/
│   │   │   ├── mod.rs
│   │   │   └── manager.rs       # Global hotkey handling
│   │   ├── injection/
│   │   │   ├── mod.rs
│   │   │   └── keyboard.rs      # Text injection
│   │   └── settings/
│   │       ├── mod.rs
│   │       └── store.rs         # Persistent settings
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                         # Svelte frontend
│   ├── routes/
│   │   └── +page.svelte
│   ├── lib/
│   │   ├── components/
│   │   └── stores/
│   └── app.html
├── static/
│   └── icons/
├── package.json
└── README.md
```

---

## Getting Started

To start development:
1. Initialize the Tauri v2 + Svelte project
2. Set up the Rust dependencies
3. Create the basic project structure
4. Implement a minimal working prototype (hotkey → record → transcribe → inject)
