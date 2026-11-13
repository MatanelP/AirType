#!/bin/sh
# Test double: read local API fixtures; always reject installer downloads.
set -eu
url=''
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o|--output|-H|--header|--connect-timeout|--max-time|--max-filesize) shift 2 ;;
        https://*) url="$1"; shift ;;
        *) shift ;;
    esac
done
printf '%s\n' "$url" >> "$AIRTYPE_INSTALLER_CURL_LOG"
case "$url" in
    */releases/latest) fixture=latest.json ;;
    */releases\?*) fixture=releases.json ;;
    */releases/download/*) exit 22 ;;
    *) exit 66 ;;
esac
[ -f "$AIRTYPE_INSTALLER_FIXTURES/$fixture" ] || exit 22
cat "$AIRTYPE_INSTALLER_FIXTURES/$fixture"
