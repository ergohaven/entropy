#!/usr/bin/env bash

# These constants are consumed by scripts that source this file. Update them
# with `bash scripts/update_appimagetool_pin.sh <immutable-release-tag>`.
# shellcheck disable=SC2034
readonly APPIMAGETOOL_PINNED_RELEASE="12"
readonly APPIMAGETOOL_PINNED_URL="https://github.com/AppImage/AppImageKit/releases/download/12/appimagetool-x86_64.AppImage"
readonly APPIMAGETOOL_PINNED_SHA256="d918b4df547b388ef253f3c9e7f6529ca81a885395c31f619d9aaf7030499a13"
