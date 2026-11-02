#!/usr/bin/env sh
set -eu

lib_dir="${XDG_DATA_HOME:-$HOME/.local/share}/entropy/ibus"
component_dir="${XDG_DATA_HOME:-$HOME/.local/share}/ibus/component"
component_path="$component_dir/entropy-universal-symbols.xml"

if [ -f "$component_path" ]; then
    rm -f "$component_path"
    printf '%s\n' "Removed Entropy IBus component: $component_path"
else
    printf '%s\n' "Entropy IBus component was not installed: $component_path"
fi

if [ -d "$lib_dir" ]; then
    rm -rf "$lib_dir"
    printf '%s\n' "Removed Entropy IBus engine files: $lib_dir"
else
    printf '%s\n' "Entropy IBus engine files were not installed: $lib_dir"
fi

if command -v ibus >/dev/null 2>&1; then
    ibus_component_path="$component_dir"
    if [ -n "${IBUS_COMPONENT_PATH:-}" ]; then
        ibus_component_path="$ibus_component_path:$IBUS_COMPONENT_PATH"
    else
        ibus_component_path="$ibus_component_path:/usr/share/ibus/component"
    fi
    if IBUS_COMPONENT_PATH="$ibus_component_path" ibus write-cache >/dev/null 2>&1; then
        printf '%s\n' "Updated IBus registry cache."
    else
        printf '%s\n' "Uninstalled, but IBus registry cache update failed."
        printf '%s\n' "Run: IBUS_COMPONENT_PATH=\"$ibus_component_path\" ibus write-cache"
    fi
else
    printf '%s\n' "Uninstalled, but ibus command was not found."
fi
# No `ibus restart` here, for the reason given in install-user.sh.
printf '%s\n' "A running IBus drops the engine once restarted: Entropy restarts it after this step, or log out and back in."
