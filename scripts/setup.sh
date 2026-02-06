#!/bin/bash
# AirType Setup Script
# Helps install dependencies and download Whisper models

set -e

echo "🎤 AirType Setup Script"
echo "========================"
echo ""

# Detect OS
OS="unknown"
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    OS="linux"
elif [[ "$OSTYPE" == "darwin"* ]]; then
    OS="macos"
elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "cygwin" ]]; then
    OS="windows"
fi

echo "Detected OS: $OS"
echo ""

# Install system dependencies
install_linux_deps() {
    echo "📦 Installing Linux dependencies..."
    if command -v apt &> /dev/null; then
        sudo apt update
        sudo apt install -y \
            libgtk-3-dev \
            libwebkit2gtk-4.1-dev \
            libayatana-appindicator3-dev \
            librsvg2-dev \
            patchelf \
            libasound2-dev \
            libssl-dev \
            pkg-config \
            build-essential \
            cmake \
            curl \
            libxdo-dev
        echo "✅ Linux dependencies installed"
    elif command -v dnf &> /dev/null; then
        sudo dnf install -y \
            gtk3-devel \
            webkit2gtk4.1-devel \
            libappindicator-gtk3-devel \
            librsvg2-devel \
            patchelf \
            alsa-lib-devel \
            openssl-devel \
            pkgconf-pkg-config \
            cmake \
            curl
        echo "✅ Linux dependencies installed (Fedora/RHEL)"
    elif command -v pacman &> /dev/null; then
        sudo pacman -Syu --noconfirm \
            gtk3 \
            webkit2gtk-4.1 \
            libappindicator-gtk3 \
            librsvg \
            patchelf \
            alsa-lib \
            openssl \
            pkgconf \
            cmake \
            curl
        echo "✅ Linux dependencies installed (Arch)"
    else
        echo "⚠️  Could not detect package manager. Please install dependencies manually."
        echo "See: https://tauri.app/start/prerequisites/#linux"
    fi
}

install_macos_deps() {
    echo "📦 Installing macOS dependencies..."
    if ! command -v xcode-select &> /dev/null; then
        xcode-select --install
    else
        echo "Xcode Command Line Tools already installed"
    fi
    
    if command -v brew &> /dev/null; then
        brew install cmake || true
    else
        echo "⚠️  Homebrew not found. Install from https://brew.sh/"
    fi
    echo "✅ macOS dependencies ready"
}

# Create config directories
setup_directories() {
    echo ""
    echo "📁 Setting up directories..."
    
    CONFIG_DIR=""
    if [[ "$OS" == "linux" ]]; then
        CONFIG_DIR="$HOME/.config/airtype"
    elif [[ "$OS" == "macos" ]]; then
        CONFIG_DIR="$HOME/Library/Application Support/airtype"
    elif [[ "$OS" == "windows" ]]; then
        CONFIG_DIR="$APPDATA/airtype"
    fi
    
    mkdir -p "$CONFIG_DIR/models"
    echo "✅ Created $CONFIG_DIR/models"
}

# Download Whisper model
download_model() {
    echo ""
    echo "🤖 Whisper Model Download"
    echo "------------------------"
    echo "Available models:"
    echo "  1) tiny   (~75MB)  - Fastest, less accurate"
    echo "  2) base   (~150MB) - Recommended balance"
    echo "  3) small  (~500MB) - Better accuracy, slower"
    echo "  4) Skip download"
    echo ""
    
    read -p "Select model [2]: " choice
    choice=${choice:-2}
    
    MODEL=""
    case $choice in
        1) MODEL="ggml-tiny.bin" ;;
        2) MODEL="ggml-base.bin" ;;
        3) MODEL="ggml-small.bin" ;;
        4) echo "Skipping model download"; return ;;
        *) echo "Invalid choice, using base"; MODEL="ggml-base.bin" ;;
    esac
    
    CONFIG_DIR=""
    if [[ "$OS" == "linux" ]]; then
        CONFIG_DIR="$HOME/.config/airtype"
    elif [[ "$OS" == "macos" ]]; then
        CONFIG_DIR="$HOME/Library/Application Support/airtype"
    elif [[ "$OS" == "windows" ]]; then
        CONFIG_DIR="$APPDATA/airtype"
    fi
    
    MODEL_PATH="$CONFIG_DIR/models/$MODEL"
    
    if [[ -f "$MODEL_PATH" ]]; then
        echo "✅ Model already exists at $MODEL_PATH"
        return
    fi
    
    echo "⬇️  Downloading $MODEL..."
    curl -L -# -o "$MODEL_PATH" \
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$MODEL"
    
    echo "✅ Model downloaded to $MODEL_PATH"
}

# Install Node.js dependencies
install_npm_deps() {
    echo ""
    echo "📦 Installing Node.js dependencies..."
    
    if command -v npm &> /dev/null; then
        npm install
        echo "✅ Node.js dependencies installed"
    else
        echo "⚠️  npm not found. Please install Node.js first."
        echo "See: https://nodejs.org/"
    fi
}

# Main
main() {
    # Install system deps based on OS
    if [[ "$OS" == "linux" ]]; then
        read -p "Install Linux system dependencies? [Y/n]: " install_deps
        install_deps=${install_deps:-Y}
        if [[ "$install_deps" =~ ^[Yy] ]]; then
            install_linux_deps
        fi
    elif [[ "$OS" == "macos" ]]; then
        read -p "Install macOS dependencies? [Y/n]: " install_deps
        install_deps=${install_deps:-Y}
        if [[ "$install_deps" =~ ^[Yy] ]]; then
            install_macos_deps
        fi
    fi
    
    setup_directories
    download_model
    install_npm_deps
    
    echo ""
    echo "🎉 Setup complete!"
    echo ""
    echo "Next steps:"
    echo "  1. Run in development mode:  npm run tauri dev"
    echo "  2. Build for production:     npm run tauri build"
    echo ""
    echo "Default hotkeys:"
    echo "  Ctrl+Shift+Space - Hold to record"
    echo "  Ctrl+Shift+R     - Toggle recording"
    echo "  Ctrl+Shift+L     - Toggle language (EN/HE)"
    echo ""
}

main "$@"
