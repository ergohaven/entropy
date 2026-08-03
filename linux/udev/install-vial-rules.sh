#!/usr/bin/env sh
set -eu

rule_path="/etc/udev/rules.d/59-vial.rules"

install_rule() {
    user_gid="$1"
    case "$user_gid" in
        *[!0-9]*|"")
            printf '%s\n' "Invalid user group id: $user_gid" >&2
            exit 1
            ;;
    esac

    tmp_file=$(mktemp)
    trap 'rm -f "$tmp_file"' EXIT

    printf '%s\n' \
        "# Entropy Vial hidraw access v2" \
        "KERNEL==\"hidraw*\", SUBSYSTEM==\"hidraw\", ATTRS{serial}==\"*vial:f64c2b3c*\", MODE=\"0660\", GROUP=\"$user_gid\", TAG+=\"uaccess\", TAG+=\"udev-acl\"" \
        "KERNEL==\"hidraw*\", SUBSYSTEM==\"hidraw\", KERNELS==\"0005:E126:*\", MODE=\"0660\", GROUP=\"$user_gid\", TAG+=\"uaccess\", TAG+=\"udev-acl\"" \
        > "$tmp_file"

    install -D -m 0644 "$tmp_file" "$rule_path"

    if command -v udevadm >/dev/null 2>&1; then
        udevadm control --reload-rules
        udevadm trigger
    else
        printf '%s\n' "Installed, but udevadm was not found. Reload udev rules manually." >&2
        exit 1
    fi

    printf '%s\n' "Installed Vial udev rule: $rule_path"
}

if [ "${1:-}" = "--as-root" ]; then
    if [ "$(id -u)" -ne 0 ]; then
        printf '%s\n' "Root privileges are required to install udev rules." >&2
        exit 1
    fi
    install_rule "${2:-}"
    exit 0
fi

user_gid=$(id -g)
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
script_path="$script_dir/$(basename -- "$0")"

if [ "$(id -u)" -eq 0 ]; then
    install_rule "$user_gid"
    exit 0
fi

if command -v pkexec >/dev/null 2>&1; then
    exec pkexec sh "$script_path" --as-root "$user_gid"
fi

if command -v sudo >/dev/null 2>&1; then
    exec sudo sh "$script_path" --as-root "$user_gid"
fi

printf '%s\n' "Install pkexec or sudo, then run this script again." >&2
exit 1
