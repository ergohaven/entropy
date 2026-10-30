# Entropy IBus backend

Entropy uses this IBus backend for Text Expander on Linux. While selected as an input method, it watches typing and commits matching expansions through IBus; this is also the safe input path required by Wayland.

## Install for current user

```sh
./linux/ibus/install-user.sh
```

Then log out and back in so that IBus loads it; Install IBus in Entropy restarts a running IBus
itself. `ibus restart` works too, but since ibus 1.5.30 it also makes IBus the input method KWin
launches in a KDE Plasma Wayland session, replacing the one chosen there.

After that, add/select the two Entropy input sources in the system input-source/input-method settings:

- **English (Entropy)** for the `us` layout
- **Russian (Entropy)** for the `ru` layout

Switch between those system input sources to change language while keeping Entropy Text Expander active.
Entropy also reads the selected source's base layout (`EN` or `RU`) for firmware-native
Universal Symbols, including on KDE and X11 desktops where the normal layout tracker is
not IBus.

On GNOME, registering the engine is not enough: GNOME Shell switches only the sources listed in
`org.gnome.desktop.input-sources`. This puts English (Entropy) and Russian (Entropy) where English (US)
and Russian stand, once IBus lists the engine, and `restore` puts them back:

```sh
./linux/ibus/gnome-input-sources.sh use
```

On KDE Plasma Wayland, IBus runs only when it is chosen under System Settings → Keyboard → Virtual
Keyboard (IBus Wayland); add the Entropy sources in IBus Preferences.

To remove the Entropy IBus sources (on GNOME this also restores the layouts they replaced):

```sh
./linux/ibus/uninstall-user.sh
```

Required distro packages are usually:

- `ibus`
- `python3-gi`
- `gir1.2-ibus-1.0`

## Behavior

- Loads Text Expander settings from `~/.config/entropy/app_settings.json`
- Loads primary and selected extra rules from `~/.config/entropy/text_expander_rules/`
- Passes normal typing through unless a trigger matches
- On match, swallows the last trigger key, removes the already typed trigger text through surrounding-text APIs in GUI clients, uses terminal erase bytes in terminal clients, and commits the replacement
- Does not log keyboard input

## Scope

This is the Linux backend for Text Expander. Universal Symbols are implemented by supported RMK firmware and do not use IBus.
