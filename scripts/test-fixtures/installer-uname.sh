#!/bin/sh
set -eu
case "$1" in
    -s) printf '%s\n' "$AIRTYPE_MOCK_OS" ;;
    -m) printf '%s\n' "$AIRTYPE_MOCK_ARCH" ;;
    *) exit 64 ;;
esac
