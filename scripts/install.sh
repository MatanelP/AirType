#!/usr/bin/env sh
# AirType installer — Linux & macOS
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/MatanelP/AirType/master/scripts/install.sh | sh
#
# Env overrides:
#   AIRTYPE_VERSION   Tag to install (default: latest release)
#   AIRTYPE_REPO      owner/name (default: MatanelP/AirType)

set -eu

REPO="${AIRTYPE_REPO:-MatanelP/AirType}"
VERSION="${AIRTYPE_VERSION:-}"

info()  { printf '\033[1;34m==>\033[0m %s\n' "$*" >&2; }
warn()  { printf '\033[1;33m!!\033[0m  %s\n' "$*" >&2; }
err()   { printf '\033[1;31mxx\033[0m  %s\n' "$*" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || err "required command not found: $1"
}

need curl
need uname

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  PLATFORM=linux ;;
    Darwin) PLATFORM=macos ;;
    *)      err "unsupported OS: $OS (this script handles Linux and macOS; for Windows use install.ps1)" ;;
esac

# Resolve latest version if not pinned
if [ -z "$VERSION" ]; then
    info "Resolving latest release…"
    VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep -oE '"tag_name":\s*"[^"]+"' \
        | head -n1 \
        | sed -E 's/.*"([^"]+)"/\1/')"
    [ -n "$VERSION" ] || err "could not determine latest release tag"
fi
VERSION_NUM="${VERSION#v}"
info "Installing AirType $VERSION on $PLATFORM/$ARCH"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

download() {
    url="https://github.com/$REPO/releases/download/$VERSION/$1"
    out="$TMPDIR/$1"
    info "Downloading $1"
    curl -fL --progress-bar -o "$out" "$url" >&2 || err "download failed: $url"
    printf '%s' "$out"
}

install_macos() {
    case "$ARCH" in
        arm64|aarch64) asset="AirType_${VERSION_NUM}_aarch64.dmg" ;;
        x86_64)        err "macOS x86_64 is not published. Use an Apple Silicon Mac or build from source." ;;
        *)             err "unsupported macOS arch: $ARCH" ;;
    esac

    dmg="$(download "$asset")"
    mnt="$(mktemp -d)/AirType"
    mkdir -p "$mnt"

    info "Mounting DMG"
    hdiutil attach -nobrowse -quiet -mountpoint "$mnt" "$dmg"
    # shellcheck disable=SC2064
    trap "hdiutil detach -quiet '$mnt' >/dev/null 2>&1 || true; rm -rf '$TMPDIR'" EXIT

    if [ -d "/Applications/AirType.app" ]; then
        info "Removing existing /Applications/AirType.app"
        # Quit any running instance so rm/copy don't race with it.
        osascript -e 'tell application "AirType" to quit' >/dev/null 2>&1 || true
        sleep 1
        rm -rf "/Applications/AirType.app" \
            || err "could not remove existing AirType.app (is it still running?)"
    fi

    info "Copying AirType.app to /Applications"
    cp -R "$mnt/AirType.app" /Applications/ 2>/dev/null \
        || cp -R "$mnt"/*.app /Applications/

    info "Stripping quarantine attribute"
    xattr -cr /Applications/AirType.app 2>/dev/null || true

    info "Unmounting DMG"
    hdiutil detach -quiet "$mnt" >/dev/null 2>&1 || true

    info "Installed to /Applications/AirType.app"
    prompt_launch "open -a AirType"
}

install_linux() {
    case "$ARCH" in
        x86_64|amd64) : ;;
        *)            err "unsupported Linux arch: $ARCH (only x86_64 is published)" ;;
    esac

    launch_cmd="AirType"
    # Prefer package managers if present, else fall back to AppImage.
    # Use reinstall-style flags so re-running this script always results
    # in the downloaded version being the installed version, even when
    # the same version is already present.
    if command -v apt-get >/dev/null 2>&1 || command -v dpkg >/dev/null 2>&1; then
        asset="AirType_${VERSION_NUM}_amd64.deb"
        pkg="$(download "$asset")"
        info "Installing .deb (requires sudo)"
        # dpkg -i always replaces, regardless of version. Fix any missing
        # deps with apt afterwards.
        sudo dpkg -i "$pkg" || sudo apt-get -f install -y || true
    elif command -v dnf >/dev/null 2>&1 || command -v rpm >/dev/null 2>&1; then
        asset="AirType-${VERSION_NUM}-1.x86_64.rpm"
        pkg="$(download "$asset")"
        info "Installing .rpm (requires sudo)"
        # rpm -Uvh --force reinstalls the package even if the same
        # version is present and overwrites owned files.
        sudo rpm -Uvh --force "$pkg" \
            || (command -v dnf >/dev/null 2>&1 && sudo dnf reinstall -y "$pkg") \
            || (command -v dnf >/dev/null 2>&1 && sudo dnf install -y "$pkg")
    else
        asset="AirType_${VERSION_NUM}_amd64.AppImage"
        appimg="$(download "$asset")"
        dest="${HOME}/.local/bin"
        mkdir -p "$dest"
        install -m 0755 "$appimg" "$dest/AirType.AppImage"
        info "Installed AppImage to $dest/AirType.AppImage"
        launch_cmd="$dest/AirType.AppImage"
        case ":$PATH:" in
            *":$dest:"*) ;;
            *) warn "$dest is not on your PATH. Add it to your shell profile to launch from anywhere." ;;
        esac
    fi

    info "Installed."
    prompt_launch "$launch_cmd"
}

# Ask whether to launch now. Reads from /dev/tty so it works when piped
# through `curl | sh` (which has stdin consumed by the pipe).
# Falls back to "no" non-interactively.
prompt_launch() {
    cmd="$1"
    if [ -n "${AIRTYPE_NO_LAUNCH:-}" ]; then
        info "Launch later with: $cmd"
        return
    fi
    if [ ! -r /dev/tty ]; then
        info "Launch later with: $cmd"
        return
    fi
    printf '\033[1;34m==>\033[0m Launch AirType now? [Y/n] ' >&2
    ans=''
    read -r ans < /dev/tty || ans=''
    case "$ans" in
        ''|y|Y|yes|YES|Yes)
            info "Launching…"
            # shellcheck disable=SC2086
            ( eval "$cmd" >/dev/null 2>&1 & ) || warn "could not launch; run: $cmd"
            ;;
        *)
            info "OK. Launch later with: $cmd"
            ;;
    esac
}

case "$PLATFORM" in
    macos) install_macos ;;
    linux) install_linux ;;
esac
