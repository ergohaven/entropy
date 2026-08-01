#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
lib_dir="${XDG_DATA_HOME:-$HOME/.local/share}/entropy/ibus"
component_dir="${XDG_DATA_HOME:-$HOME/.local/share}/ibus/component"
engine_path="$lib_dir/entropy-ibus-engine"
component_path="$component_dir/entropy-universal-symbols.xml"

mkdir -p "$lib_dir" "$component_dir"
install -m 0755 "$script_dir/entropy-ibus-engine" "$engine_path"
sed "s|@ENGINE_PATH@|$engine_path|g" "$script_dir/entropy-universal-symbols.xml.in" > "$component_path"

if "$engine_path" --entropy-check >/dev/null; then
    :
else
    printf '%s\n' "Installed, but Entropy IBus dependency check failed."
    printf '%s\n' "Install python3-gi and gir1.2-ibus-1.0, then run Install IBus again."
    exit 1
fi

printf '%s\n' "Installed Entropy IBus engine: $component_path"
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
        printf '%s\n' "Installed, but IBus registry cache update failed."
        printf '%s\n' "Run: IBUS_COMPONENT_PATH=\"$ibus_component_path\" ibus write-cache"
    fi
    if ibus restart >/dev/null 2>&1; then
        printf '%s\n' "Restarted IBus."
    else
        printf '%s\n' "Installed, but IBus restart failed. Run: ibus restart"
    fi
else
    printf '%s\n' "Installed, but ibus command was not found."
fi
printf '%s\n' "Select the Entropy Text Expander input sources for the layouts you use."
