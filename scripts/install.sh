#!/usr/bin/env sh
# AirType installer — Linux & macOS
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/MatanelP/AirType/main/scripts/install.sh | sh
#
# Env overrides:
#   AIRTYPE_VERSION              Exact tag (default: newest compatible stable release)
#   AIRTYPE_REPO                 owner/name (default: MatanelP/AirType)
#   AIRTYPE_RESET_PERMISSIONS=1  macOS only — reset TCC grants for AirType
#                                so the OS re-prompts for mic/apple-events
#                                on first launch (simulates a brand-new Mac)

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
need awk

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  PLATFORM=linux ;;
    Darwin) PLATFORM=macos ;;
    *)      err "unsupported OS: $OS (this script handles Linux and macOS; for Windows use install.ps1)" ;;
esac

# Choose the exact installer before resolving a release. A partial release may
# have another platform ready while this package is still building.
case "$PLATFORM/$ARCH" in
    macos/arm64|macos/aarch64) ASSET_PREFIX=AirType_; ASSET_SUFFIX=_aarch64.dmg ;;
    macos/x86_64) err "macOS x86_64 is not published. Use an Apple Silicon Mac or build from source." ;;
    linux/x86_64|linux/amd64)
        if command -v apt-get >/dev/null 2>&1 || command -v dpkg >/dev/null 2>&1; then
            ASSET_PREFIX=AirType_; ASSET_SUFFIX=_amd64.deb
        elif command -v dnf >/dev/null 2>&1 || command -v rpm >/dev/null 2>&1; then
            ASSET_PREFIX=AirType-; ASSET_SUFFIX=-1.x86_64.rpm
        else
            ASSET_PREFIX=AirType_; ASSET_SUFFIX=_amd64.AppImage
        fi ;;
    *) err "unsupported $PLATFORM arch: $ARCH" ;;
esac

release_api() {
    curl -fsSL --connect-timeout 15 --max-time 60 --max-filesize 16777216 \
        "https://api.github.com/repos/$REPO/$1"
}

# A bounded JSON reader, embedded so `curl | sh` needs no fetched code or extra
# runtime. Read only release metadata and uploaded asset names; skip other JSON
# structurally, including escaped strings. Never evaluate API data as shell code.
inspect_releases() {
    awk -v mode="$1" -v ceiling="${2:-}" -v prefix="$ASSET_PREFIX" -v suffix="$ASSET_SUFFIX" '
    function bad() { print "invalid GitHub release metadata" > "/dev/stderr"; exit 2 }
    function next_token(    c,e,h,n,i) {
        while (substr(json,pos,1) ~ /[ \t\r\n]/ && pos <= length(json)) pos++
        value=""; c=substr(json,pos++,1)
        if (c=="") { token="eof"; return }
        if (c=="{" || c=="}" || c=="[" || c=="]" || c=="," || c==":") { token=c; return }
        if (c=="\"") {
            token="string"
            while (pos <= length(json)) {
                c=substr(json,pos++,1)
                if (c=="\"") return
                if (c=="\\") {
                    e=substr(json,pos++,1)
                    if (e=="u") {
                        h=substr(json,pos,4); if (length(h)!=4 || h ~ /[^0-9a-fA-F]/) bad()
                        n=0
                        for (i=1;i<=4;i++) n=n*16+index("0123456789abcdef",tolower(substr(h,i,1)))-1
                        pos+=4; c=(n<128 ? sprintf("%c",n) : "?")
                    } else if (e=="\"" || e=="\\" || e=="/") c=e
                    else if (e ~ /^[bfnrt]$/) c="?"
                    else bad()
                } else if (c ~ /[[:cntrl:]]/) bad()
                value=value c
            }
            bad()
        }
        pos--
        if (!match(substr(json,pos),/^(-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?|true|false|null)/)) bad()
        token="scalar"; value=substr(json,pos,RLENGTH); pos+=RLENGTH
    }
    function expect(want) { if (token!=want) bad() }
    function skip_value(depth,    closing) {
        if (depth>64) bad()
        if (token=="{" || token=="[") {
            closing=(token=="{" ? "}" : "]"); next_token()
            if (token==closing) { next_token(); return }
            while (1) {
                if (closing=="}") { expect("string"); next_token(); expect(":"); next_token() }
                skip_value(depth+1)
                if (token==closing) { next_token(); return }
                expect(","); next_token()
            }
        }
        if (token!="string" && token!="scalar") bad()
        next_token()
    }
    function read_assets(    key,name,state) {
        expect("["); next_token()
        if (token=="]") { next_token(); return }
        while (1) {
            expect("{"); next_token(); name=""; state=""
            if (token!="}") while (1) {
                expect("string"); key=value; next_token(); expect(":"); next_token()
                if (key=="name" || key=="state") {
                    expect("string")
                    if (key=="name") name=value; else state=value
                    next_token()
                } else skip_value(1)
                if (token=="}") break
                expect(","); next_token()
            }
            if (state=="uploaded") names[name]=1
            next_token()
            if (token=="]") { next_token(); return }
            expect(","); next_token()
        }
    }
    function stable(tag) { return tag ~ /^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/ }
    function compare(a,b,    x,y,i) {
        split(substr(a,2),x,"."); split(substr(b,2),y,".")
        for (i=1;i<=3;i++) {
            if (length(x[i])!=length(y[i])) return length(x[i])>length(y[i]) ? 1 : -1
            if ("x" x[i] != "x" y[i]) return "x" x[i] > "x" y[i] ? 1 : -1
        }
        return 0
    }
    function read_release(    key,tag,draft,pre,assets,found) {
        for (key in names) delete names[key]
        for (key in seen) delete seen[key]
        expect("{"); next_token()
        if (token!="}") while (1) {
            expect("string"); key=value; next_token(); expect(":"); next_token()
            if (key=="tag_name" || key=="draft" || key=="prerelease" || key=="assets") {
                if (seen[key]++) bad()
                if (key=="tag_name") { expect("string"); tag=value; next_token() }
                else if (key=="assets") { assets=1; read_assets() }
                else {
                    expect("scalar"); if (value!="true" && value!="false") bad()
                    if (key=="draft") draft=value; else pre=value
                    next_token()
                }
            } else skip_value(1)
            if (token=="}") break
            expect(","); next_token()
        }
        next_token()
        if (!assets || draft=="" || pre=="" || tag=="") bad()
        found=((prefix substr(tag,2) suffix) in names)
        if (mode=="latest") {
            if (!stable(tag) || draft!="false" || pre!="false") bad()
            selected=tag "|" (found ? "yes" : "no")
        } else if (stable(tag) && draft=="false" && pre=="false" && found && compare(tag,ceiling)<=0) {
            if (selected=="" || compare(tag,selected)>0) selected=tag
        }
    }
    { json=json $0 "\n" }
    END {
        pos=1; next_token()
        if (mode=="latest") read_release()
        else {
            expect("["); next_token()
            if (token!="]") while (1) {
                if (++count>100) bad()
                read_release()
                if (token=="]") break
                expect(","); next_token()
            }
            expect("]"); next_token()
        }
        expect("eof")
        print selected
    }'
}

# Explicit pins remain exact and never fall back, including download failures.
if [ -z "$VERSION" ]; then
    info "Resolving latest compatible stable release…"
    metadata="$(release_api releases/latest)" || err "could not query latest release"
    latest="$(printf '%s\n' "$metadata" | inspect_releases latest)" || err "could not read latest release"
    VERSION="${latest%|*}"
    if [ "${latest#*|}" != yes ]; then
        info "$VERSION does not yet contain this installer; checking up to 100 releases…"
        metadata="$(release_api 'releases?per_page=100&page=1')" || err "could not query compatible releases"
        VERSION="$(printf '%s\n' "$metadata" | inspect_releases fallback "$VERSION")" || err "could not read compatible releases"
        [ -n "$VERSION" ] || err "no compatible stable installer found in the first 100 releases"
    fi
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

    if [ "${AIRTYPE_RESET_PERMISSIONS:-0}" = "1" ] && command -v tccutil >/dev/null 2>&1; then
        info "Resetting macOS TCC permissions for com.airtype.app"
        # Each service is independent; ignore failures for services the
        # app never requested (tccutil returns non-zero in that case).
        for svc in Microphone AppleEvents Accessibility All; do
            tccutil reset "$svc" com.airtype.app >/dev/null 2>&1 || true
        done
    fi

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

    launch_cmd="airtype"
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
