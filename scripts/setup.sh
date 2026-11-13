#!/usr/bin/env bash
# Prepare this checkout without changing system packages or downloading models.
set -euo pipefail

usage() {
    cat <<'HELP'
Usage: bash scripts/setup.sh [--frontend-only] [--check]

  --frontend-only  Prepare only the Svelte frontend (no Rust/backend required).
  --check          Check prerequisites without installing dependencies.
  --help           Show this help.

Run from any directory. Installs locked npm dependencies with npm ci.
System packages and toolchains must be installed separately.
HELP
}

frontend_only=false
check_only=false
for argument in "$@"; do
    case "$argument" in
        --frontend-only) frontend_only=true ;;
        --check) check_only=true ;;
        --help|-h) usage; exit 0 ;;
        *) echo "Unknown option: $argument" >&2; usage >&2; exit 2 ;;
    esac
done

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"
failures=0
fail() { echo "ERROR: $*" >&2; failures=$((failures + 1)); }

if command -v node >/dev/null 2>&1; then
    node_major="$(node -p 'process.versions.node.split(".")[0]')"
    if (( node_major < 20 )); then
        fail "Node.js 20 or newer is required; found $(node --version)."
    else
        echo "Node.js: $(node --version)"
    fi
else
    fail "Node.js 20 or newer is required."
fi
command -v npm >/dev/null 2>&1 || fail "npm is required."
[[ -f package-lock.json ]] || fail "package-lock.json is missing; restore it before setup."

if ! "$frontend_only"; then
    for backend_file in Cargo.toml tauri.conf.json; do
        [[ -f "src-tauri/$backend_file" ]] || fail "Missing src-tauri/$backend_file. Restore the desktop backend, or use --frontend-only."
    done
    for tool in cargo rustc; do
        command -v "$tool" >/dev/null 2>&1 || fail "$tool is required for desktop development."
    done
    case "$(uname -s)" in
        Linux)
            command -v cmake >/dev/null 2>&1 || fail "cmake is required for desktop development."
            if command -v pkg-config >/dev/null 2>&1; then
                for library in gtk+-3.0 webkit2gtk-4.1 alsa openssl dbus-1 x11 xi xtst; do
                    pkg-config --exists "$library" || fail "Missing development library: $library (see README.md)."
                done
                if ! pkg-config --exists ayatana-appindicator3-0.1 && ! pkg-config --exists appindicator3-0.1; then
                    fail "An Ayatana AppIndicator or AppIndicator development package is required."
                fi
            else
                fail "pkg-config is required to check Linux development libraries (see README.md)."
            fi
            # Ubuntu's libxdo-dev does not ship a pkg-config file.
            if ! command -v cc >/dev/null 2>&1; then
                fail "A C compiler is required; install build-essential."
            elif [[ ! -f "$(cc -print-file-name=libxdo.so)" ]]; then
                fail "libxdo development library is required; install libxdo-dev."
            fi
            ;;
        Darwin)
            if ! command -v xcode-select >/dev/null 2>&1 || ! xcode-select -p >/dev/null 2>&1; then
                fail "Xcode Command Line Tools are required; install them with xcode-select --install."
            fi
            ;;
    esac
    echo "Desktop builds also require platform libraries; see README.md."
fi

if (( failures > 0 )); then
    echo "Setup stopped: $failures prerequisite check(s) failed. No dependencies installed." >&2
    exit 1
fi
if "$check_only"; then
    echo "Prerequisite checks passed. No dependencies installed."
    exit 0
fi

npm ci --no-audit --no-fund
printf '\nSetup complete.\n'
if "$frontend_only"; then
    echo "Run npm run dev for the frontend. Desktop commands require the Tauri backend."
else
    echo "Run npm run tauri dev for desktop development."
fi
echo "Configure cloud transcription credentials in the desktop app Settings. No local models are needed."
