#!/usr/bin/env sh
# Puts the Entropy IBus sources into GNOME's input sources, or takes them out.
#
#   use      English (US) and Russian become English (Entropy) and Russian
#            (Entropy) where they stand: the same layouts, with Text Expander.
#            A layout listed only with a variant (ru+winkeys) is kept, and the
#            Entropy source goes right after it.
#   restore  The reverse. A layout `use` replaced comes back where the Entropy
#            source stands; an Entropy source next to a layout still listed
#            goes away.
#
# Both leave every other source where it is and change nothing when run twice.
# Which layouts `use` replaced is kept next to the user copy of the engine,
# since the list alone cannot tell a replaced layout from an added source.
set -eu

schema=org.gnome.desktop.input-sources
key=sources
replaced_file="${XDG_DATA_HOME:-$HOME/.local/share}/entropy/ibus/gnome-replaced-layouts"

action=${1:-}
case $action in
use | restore) ;;
*)
    printf '%s\n' "usage: $0 use|restore" >&2
    exit 2
    ;;
esac

if ! command -v gsettings >/dev/null 2>&1 || ! current=$(gsettings get "$schema" "$key" 2>/dev/null); then
    # Nothing of ours can be listed where GNOME's input sources do not exist.
    [ "$action" = restore ] && exit 0
    printf '%s\n' "GNOME input sources are not available in this session." >&2
    exit 1
fi
if [ "$(gsettings writable "$schema" "$key" 2>/dev/null)" != true ]; then
    printf '%s\n' "GNOME input sources are locked in this session." >&2
    [ "$action" = restore ] && exit 0
    exit 1
fi

# One source per line, as GSettings prints them: ('xkb', 'us')
entries=$(printf '%s\n' "$current" | sed -e 's/^@a(ss) //' -e 's/^\[//' -e 's/\]$//' -e 's/), (/)\n(/g')
replaced=$(cat "$replaced_file" 2>/dev/null || true)

entropy() { printf "('ibus', 'entropy-universal-symbols-%s')" "$1"; }
plain() { printf "('xkb', '%s')" "$1"; }
has() { printf '%s\n' "$entries" | grep -Fqx -- "$1"; }
has_variant() { printf '%s\n' "$entries" | grep -Fq -- "('xkb', '$1+"; }
was_replaced() { printf '%s\n' "$replaced" | grep -Fqx -- "$1"; }
forget_replaced() { replaced=$(printf '%s\n' "$replaced" | grep -Fvx -- "$1" || true); }
swap() { entries=$(printf '%s\n' "$entries" | sed "s/^$1\$/$2/"); }
drop() { entries=$(printf '%s\n' "$entries" | sed "/^$1\$/d"); }

# GNOME drops a source whose engine IBus does not list, and with nothing left
# falls back to English (US): a layout is never traded for such a source.
listed=
if [ "$action" = use ]; then
    listed=$(ibus list-engine --name-only 2>/dev/null || true)
    if ! printf '%s\n' "$listed" | grep -q '^entropy-universal-symbols-'; then
        printf '%s\n' "IBus does not list the Entropy engine; GNOME input sources are left as they are." >&2
        exit 1
    fi
fi
engine_listed() { printf '%s\n' "$listed" | grep -Fqx -- "entropy-universal-symbols-$1"; }

for layout in us ru; do
    # A note outlives its source when that is taken out by hand.
    has "$(entropy "$layout")" || forget_replaced "$layout"
    if [ "$action" = use ]; then
        if has "$(entropy "$layout")" || ! engine_listed "$layout"; then
            continue
        elif has "$(plain "$layout")"; then
            swap "$(plain "$layout")" "$(entropy "$layout")"
            was_replaced "$layout" || replaced=$(printf '%s\n%s' "$replaced" "$layout")
        elif has_variant "$layout"; then
            entries=$(printf '%s\n' "$entries" | awk -v variant="('xkb', '$layout+" -v source="$(entropy "$layout")" \
                '{ print } !added && index($0, variant) == 1 { print source; added = 1 }')
        fi
    elif has "$(entropy "$layout")"; then
        # A source added by hand next to a variant goes; one with nothing
        # else left for its language turns back into the plain layout.
        if has "$(plain "$layout")" || { has_variant "$layout" && ! was_replaced "$layout"; }; then
            drop "$(entropy "$layout")"
        else
            swap "$(entropy "$layout")" "$(plain "$layout")"
        fi
        forget_replaced "$layout"
    fi
done

updated="[$(printf '%s\n' "$entries" | awk 'NF { printf "%s%s", (n++ ? ", " : ""), $0 }')]"
if [ "$updated" != "$(printf '%s\n' "$current" | sed 's/^@a(ss) //')" ]; then
    gsettings set "$schema" "$key" "$updated"
fi
replaced=$(printf '%s\n' "$replaced" | sed '/^$/d')
if [ -n "$replaced" ]; then
    mkdir -p "$(dirname -- "$replaced_file")"
    printf '%s\n' "$replaced" > "$replaced_file"
else
    rm -f "$replaced_file"
fi
printf '%s\n' "GNOME input sources: $updated"
